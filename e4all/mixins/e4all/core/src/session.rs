//! The relay session: one QUIC connection to the relay, whose control stream
//! carries `kind`-tagged JSON and whose bidirectional streams *are* the
//! joining players.
//!
//! The relay (`e4mc-quiclime`) takes the connection, reads the control stream,
//! and for every Minecraft client that dials the assigned domain it opens a
//! fresh bidirectional stream here. Bridging each of those to a local TCP
//! socket is all the tunnelling there is.
//!
//! Two rules the Java client taught us:
//!
//! * **one control stream**, opened once and used for the whole session. A
//!   `SendStream::finish()` on it closes the QUIC connection with it, so the
//!   control stream is never finished: the session ends by closing the
//!   connection, not the stream.
//! * **the relay is asked again** when the link goes quiet. Every
//!   `keepalive` seconds the session sends a capability probe on the same
//!   control stream; three probes the relay does not answer mean the link is
//!   dead, and the session dials the relay again for a new domain.

use crate::broker::{Broker, Relay};
use crate::protocol::{FromRelay, MAX_CONTROL_MESSAGE, ToRelay};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;

/// The ALPN the relay requires on the QUIC handshake.
pub const ALPN: &[u8] = b"quiclime";

/// How many keepalive probes the relay may leave unanswered before the link
/// is called dead and the session dials the relay again.
const MISSED_KEEPALIVES: u64 = 3;

/// How long to wait before dialling the relay again.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// How long the session waits for a joining player before it looks at the
/// stop flag again.
const PLAYER_WAIT: Duration = Duration::from_millis(250);

/// What the session is doing, for the game thread to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Stopped,
    Starting,
    Started { domain: String },
}

/// How long each wait of the session may take.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// The relay may take this long to answer a control message.
    pub domain: Duration,
    /// The connection to the relay may take this long.
    pub connect: Duration,
    /// How often the relay is probed.
    pub keepalive: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            domain: Duration::from_secs(15),
            connect: Duration::from_secs(15),
            keepalive: Duration::from_secs(15),
        }
    }
}

/// A running relay session, seen from the game thread.
pub struct Session {
    state: Arc<Mutex<SessionState>>,
    stop: Arc<AtomicBool>,
    /// The live connection, so the game thread can close it at once.
    connection: Arc<Mutex<Option<quinn::Connection>>>,
    /// The domain, or the error that ended the attempt, as soon as it is known.
    events: Receiver<Result<String, String>>,
    /// The session thread. It is never joined: it notices the stop flag
    /// within a quarter of a second and ends on its own, and the game thread
    /// must not wait for a network round trip.
    join: Option<JoinHandle<()>>,
    relay: Relay,
}

impl Session {
    /// Dials the broker's relay on a thread of its own and asks for a domain.
    ///
    /// `local_port` is asked for every joining player, so a LAN port that
    /// changes is picked up without restarting the session.
    pub fn start(
        broker: Broker,
        local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    ) -> Session {
        Session::launch(broker, local_port, Timeouts::default())
    }

    #[doc(hidden)]
    pub fn launch(
        broker: Broker,
        local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
        timeouts: Timeouts,
    ) -> Session {
        let relay = broker.relay();
        let state = Arc::new(Mutex::new(SessionState::Starting));
        let stop = Arc::new(AtomicBool::new(false));
        let connection = Arc::new(Mutex::new(None));
        let (tx, events) = mpsc::channel();
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread_connection = Arc::clone(&connection);
        let thread_relay = relay.clone();
        let join = std::thread::Builder::new()
            .name("e4all-relay".into())
            .spawn(move || {
                run(
                    thread_relay,
                    local_port,
                    thread_state,
                    thread_stop,
                    thread_connection,
                    tx,
                    timeouts,
                )
            })
            .ok();
        Session {
            state,
            stop,
            connection,
            events,
            join,
            relay,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn relay(&self) -> &Relay {
        &self.relay
    }

    /// The domain, once the relay has given one.
    pub fn domain(&self) -> Option<String> {
        match self.state() {
            SessionState::Started { domain } => Some(domain),
            _ => None,
        }
    }

    /// Blocks until the domain arrives, or the error that stopped the attempt.
    pub fn wait_for_domain(&self, timeout: Duration) -> Result<String, String> {
        match self.events.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => Err("e4all: no domain from the relay in time".into()),
        }
    }

    /// Ends the session. The relay drops the domain once the connection closes.
    ///
    /// The connection is closed right here, so the domain is gone at once; the
    /// session thread notices and ends by itself.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(connection) = self
            .connection
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            connection.close(0u32.into(), b"e4all stopped");
        }
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = SessionState::Stopped;
        // The thread is left to end on its own: joining it here would make the
        // game thread wait for the network.
        self.join.take();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(connection) = self
            .connection
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            connection.close(0u32.into(), b"e4all stopped");
        }
    }
}

/// The session thread: a tokio runtime of its own, then the relay loop.
#[allow(clippy::too_many_arguments)]
fn run(
    relay: Relay,
    local_port: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    state: Arc<Mutex<SessionState>>,
    stop: Arc<AtomicBool>,
    connection: Arc<Mutex<Option<quinn::Connection>>>,
    events: Sender<Result<String, String>>,
    timeouts: Timeouts,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = events.send(Err(format!("e4all: no runtime: {e}")));
            *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Stopped;
            return;
        }
    };
    let mut answered = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Starting;
        let outcome = runtime.block_on(serve(
            &relay,
            &local_port,
            &state,
            &stop,
            &connection,
            &events,
            &timeouts,
            &mut answered,
        ));
        match outcome {
            Ok(()) => log::info!("e4all: the relay closed the session"),
            Err(ref e) => log::warn!("e4all: {e}"),
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // The first failure is the one the game is told about; a later one
        // is only logged, because the session is already up.
        if !answered {
            let message = match outcome {
                Ok(()) => "e4all: the relay closed the session".to_string(),
                Err(e) => e,
            };
            let _ = events.send(Err(message));
            answered = true;
        }
        // The relay may be restarting: wait a little, then dial it again.
        if !sleep_unless_stopped(&stop, RECONNECT_DELAY) {
            break;
        }
    }
    *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Stopped;
}

/// Waits, but gives up as soon as the session is stopped.
fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) -> bool {
    let step = Duration::from_millis(100);
    let mut left = total;
    while !stop.load(Ordering::SeqCst) {
        if left.is_zero() {
            return true;
        }
        let nap = left.min(step);
        std::thread::sleep(nap);
        left -= nap;
    }
    false
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    relay: &Relay,
    local_port: &Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    state: &Arc<Mutex<SessionState>>,
    stop: &Arc<AtomicBool>,
    connection_slot: &Arc<Mutex<Option<quinn::Connection>>>,
    events: &Sender<Result<String, String>>,
    timeouts: &Timeouts,
    answered: &mut bool,
) -> Result<(), String> {
    let connection = connect(relay, timeouts.connect).await?;
    *connection_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(connection.clone());

    // The one control stream, for the whole session. It is never finished:
    // finishing it would close the QUIC connection with it.
    let (control_send, control_recv) = connection.open_bi().await.map_err(|e| {
        format!(
            "e4all: the relay {}:{} opened no control stream: {e}",
            relay.host, relay.port
        )
    })?;
    let control = Arc::new(AsyncMutex::new(control_send));
    let incoming = Arc::new(AsyncMutex::new(control_recv));

    // e4mc's handshake: capabilities, then a domain.
    write_control(&control, &ToRelay::ProbeCapabilities).await?;
    // The answer is the relay's capabilities. A relay too old to know the
    // probe still answers a domain, so its silence is not fatal.
    let _ = read_control(&incoming, Some(timeouts.domain)).await;

    write_control(&control, &ToRelay::RequestDomainAssignment).await?;
    let domain = loop {
        match read_control(&incoming, Some(timeouts.domain))
            .await
            .map_err(|e| format!("e4all: no domain: {e}"))?
        {
            Some(Some(FromRelay::DomainAssignmentComplete { domain })) => break domain,
            Some(Some(_)) => continue,
            Some(None) => return Err("e4all: the relay closed the control stream".into()),
            None => return Err("e4all: no answer from the relay in time".into()),
        }
    };

    *state.lock().unwrap_or_else(|x| x.into_inner()) = SessionState::Started {
        domain: domain.clone(),
    };
    let _ = events.send(Ok(domain));
    *answered = true;

    // From here the relay pushes one stream per joining player. The control
    // stream is drained on its own task so the loop below only waits for
    // players; losing it ends the whole session, because a relay that closed
    // it has forgotten the domain.
    let (control_closed_tx, mut control_closed_rx) = tokio::sync::mpsc::channel::<()>(1);
    let missed = Arc::new(AtomicU64::new(0));
    {
        let incoming = Arc::clone(&incoming);
        let missed = Arc::clone(&missed);
        tokio::spawn(async move {
            // A broadcast (the relay's `/broadcast`) or an unknown kind: the
            // game has nothing to do with either, so read past them. Every
            // answer the relay gives clears the missed-keepalive count.
            while let Ok(Some(Some(message))) = read_control(&incoming, None).await {
                missed.store(0, Ordering::Relaxed);
                if let FromRelay::RequestMessageBroadcast { message } = message {
                    log::info!("e4all: the relay says: {message}");
                }
            }
            let _ = control_closed_tx.send(()).await;
        });
    }

    // The keepalive: the relay is probed on the same control stream. Three
    // probes it does not answer mean the link is dead, and the session dials
    // the relay again for a new domain.
    {
        let control = Arc::clone(&control);
        let missed = Arc::clone(&missed);
        let stop = Arc::clone(stop);
        let connection = connection.clone();
        let keepalive = timeouts.keepalive;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(keepalive);
            // The first tick of an interval is immediate; this one is eaten.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if write_control(&control, &ToRelay::ProbeCapabilities)
                    .await
                    .is_err()
                {
                    break;
                }
                let count = missed.fetch_add(1, Ordering::SeqCst) + 1;
                if count >= MISSED_KEEPALIVES {
                    log::warn!(
                        "e4all: the relay link went quiet ({count} probes unanswered); \
                         dialing the relay again"
                    );
                    connection.close(0u32.into(), b"e4all: the relay link went quiet");
                    break;
                }
            }
        });
    }

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // `try_recv` first: a closed control stream must not be missed while
        // waiting for a player that never comes.
        if control_closed_rx.try_recv().is_ok() {
            break;
        }
        // A bounded wait, so `Session::stop` is noticed: a session with no
        // players would otherwise sit in `accept_bi` until the relay hung up.
        let Some(player) = wait_for_player(&connection).await else {
            continue;
        };
        let (send, recv) = player;
        // A joining player with the LAN closed under us gets a stream that
        // ends at once, which their client reads as a disconnect.
        let Some(port) = local_port() else {
            continue;
        };
        tokio::spawn(async move {
            if let Err(e) = bridge(send, recv, port).await {
                log::debug!("e4all: a joining player's stream ended: {e}");
            }
        });
    }
    connection.close(0u32.into(), b"e4all: the session ended");
    Ok(())
}

/// One control message.
///
/// `Ok(Some(Some(message)))` is a message, `Ok(Some(None))` the end of the
/// stream and `Ok(None)` a wait that ran out. `None` as the limit waits for
/// as long as the stream takes.
async fn read_control(
    recv: &Arc<AsyncMutex<quinn::RecvStream>>,
    limit: Option<Duration>,
) -> Result<Option<Option<FromRelay>>, String> {
    let mut recv = recv.lock().await;
    let read = read_message_async(&mut recv);
    match limit {
        None => read.await.map(Some),
        Some(limit) => match tokio::time::timeout(limit, read).await {
            Ok(result) => result.map(Some),
            Err(_) => Ok(None),
        },
    }
}

/// A framed JSON message; `None` when the relay closed the stream cleanly.
async fn read_message_async(recv: &mut quinn::RecvStream) -> Result<Option<FromRelay>, String> {
    let mut length = 0usize;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        match recv.read(&mut byte).await {
            Ok(None) | Ok(Some(0)) => {
                return if shift == 0 {
                    Ok(None)
                } else {
                    Err("control message's length was cut off".into())
                };
            }
            Ok(Some(_)) => {}
            Err(e) => return Err(format!("control stream: {e}")),
        }
        length |= ((byte[0] & 0x7f) as usize) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return Err("control message's length is over five bytes".into());
        }
    }
    if length > MAX_CONTROL_MESSAGE {
        return Err(format!("control message of {length} bytes is too long"));
    }
    let mut buf = vec![0u8; length];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| format!("control message body: {e}"))?;
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(|e| format!("control message: {e}"))
}

/// The next joining player, `None` when none arrived within a tick of the
/// stop flag's clock.
async fn wait_for_player(
    connection: &quinn::Connection,
) -> Option<(quinn::SendStream, quinn::RecvStream)> {
    match tokio::time::timeout(PLAYER_WAIT, connection.accept_bi()).await {
        Ok(Ok(pair)) => Some(pair),
        Ok(Err(_)) => None,
        Err(_) => None,
    }
}

/// Writes one control message. The stream is left open: it is the session's
/// only control stream and finishing it would close the connection.
async fn write_control(
    control: &Arc<AsyncMutex<quinn::SendStream>>,
    message: &ToRelay,
) -> Result<(), String> {
    let framed = crate::protocol::frame(message)?;
    let mut send = control.lock().await;
    send.write_all(&framed)
        .await
        .map_err(|e| format!("control stream: {e}"))
}

async fn connect(relay: &Relay, connect_timeout: Duration) -> Result<quinn::Connection, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|e| format!("e4all: TLS: {e}"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];
    let client = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| format!("e4all: QUIC: {e}"))?,
    ));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().map_err(|e| format!("{e}"))?)
        .map_err(|e| format!("e4all: no UDP socket: {e}"))?;
    endpoint.set_default_client_config(client);

    let address = tokio::net::lookup_host((relay.host.as_str(), relay.port))
        .await
        .map_err(|e| format!("e4all: {}: {e}", relay.host))?
        .find(|address| address.is_ipv4())
        .ok_or_else(|| format!("e4all: {} has no IPv4 address", relay.host))?;
    let connecting = endpoint
        .connect(address, relay.host.as_str())
        .map_err(|e| format!("e4all: {}: {e}", relay.host))?;
    match tokio::time::timeout(connect_timeout, connecting).await {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(e)) => Err(format!(
            "e4all: {}:{} didn't answer: {e}",
            relay.host, relay.port
        )),
        Err(_) => Err(format!("e4all: {}:{} timed out", relay.host, relay.port)),
    }
}

/// One joining player: the relay's stream on one side, a TCP connection to the
/// world's LAN port on the other.
async fn bridge(
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    port: u16,
) -> std::io::Result<()> {
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    // `copy_bidirectional` ends both sides when either closes, and half-closes
    // the other, which is what a Minecraft connection needs on a kick.
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let (mut quic_send, mut quic_recv) = (send, recv);
    // Each direction ends on its own; the first to finish closes the other's
    // side. `tokio::join!` is a macro, and `tokio-macros` is not in Horizon's
    // lock, so the two are driven by polling them in one `poll_fn` below.
    let up = async {
        let copied = tokio::io::copy(&mut quic_recv, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
        copied.map(|_| ())
    };
    let down = async {
        let copied = tokio::io::copy(&mut tcp_read, &mut quic_send).await;
        // A player's stream may be finished: it is that player's connection
        // and nobody else's.
        let _ = quic_send.finish();
        copied.map(|_| ())
    };
    join2(up, down).await
}

/// Awaits two futures at once without `tokio::join!` (a macro, and
/// `tokio-macros` is not in Horizon's lock). Both are driven on the same
/// task; the first error wins, and both are always polled to completion so a
/// half-closed stream still flushes what it got.
async fn join2<A, B>(a: A, b: B) -> std::io::Result<()>
where
    A: std::future::Future<Output = std::io::Result<()>>,
    B: std::future::Future<Output = std::io::Result<()>>,
{
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Poll;

    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let (mut a_done, mut b_done) = (None, None);
    std::future::poll_fn(|cx| {
        if a_done.is_none()
            && let Poll::Ready(result) = Pin::new(&mut a).poll(cx)
        {
            a_done = Some(result);
        }
        if b_done.is_none()
            && let Poll::Ready(result) = Pin::new(&mut b).poll(cx)
        {
            b_done = Some(result);
        }
        if a_done.is_some() && b_done.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;

    match a_done {
        Some(Err(e)) => Err(e),
        _ => match b_done {
            Some(Err(e)) => Err(e),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port supplier that always answers 25565.
    fn fixed_port(port: u16) -> Arc<dyn Fn() -> Option<u16> + Send + Sync> {
        Arc::new(move || Some(port))
    }

    /// Waits a test can afford: a closed loopback port fails in a blink.
    fn quick() -> Timeouts {
        Timeouts {
            domain: Duration::from_millis(300),
            connect: Duration::from_millis(500),
            keepalive: Duration::from_secs(60),
        }
    }

    #[test]
    fn a_session_nobody_answers_reports_an_error_and_stops() {
        let broker = Broker {
            use_broker: false,
            url: String::new(),
            // A closed port on the loopback: the QUIC handshake never starts.
            fallback_host: "127.0.0.1".into(),
            fallback_port: 9,
        };
        let mut session = Session::launch(broker, fixed_port(25565), quick());
        let result = session.wait_for_domain(Duration::from_secs(5));
        assert!(result.is_err(), "{result:?}");
        session.stop();
        assert_eq!(session.state(), SessionState::Stopped);
    }

    #[test]
    fn a_session_that_never_started_has_no_domain() {
        let broker = Broker {
            use_broker: false,
            url: String::new(),
            fallback_host: "127.0.0.1".into(),
            fallback_port: 9,
        };
        let session = Session::launch(broker, fixed_port(25565), quick());
        assert_eq!(session.domain(), None);
    }

    /// The real relay: a domain comes back, and the session keeps it.
    ///
    /// Ignored by default: it needs the network. Run it with
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn the_live_relay_gives_a_domain() {
        let broker = Broker::default();
        let relay = broker.relay();
        assert!(!relay.host.is_empty(), "the broker named no relay");
        let mut session = Session::launch(broker, fixed_port(25565), Timeouts::default());
        let domain = session
            .wait_for_domain(Duration::from_secs(30))
            .expect("the relay gave no domain");
        assert!(domain.contains('.'), "the domain looks wrong: {domain}");
        assert_eq!(session.domain().as_deref(), Some(domain.as_str()));
        session.stop();
    }

    /// The whole way a guest takes: the relay gives the host a domain, a
    /// Minecraft client dials that domain, and its handshake arrives at the
    /// host's game port. The host answers with a server list ping, and the
    /// answer comes back through the relay.
    ///
    /// Ignored by default: it needs the network. Run it with
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn a_minecraft_client_reaches_the_host_through_the_relay() {
        use crate::protocol::write_varint;
        use std::io::{Read, Write};

        /// One length-prefixed packet, the way Minecraft sends them.
        fn read_packet(stream: &mut std::net::TcpStream) -> Vec<u8> {
            let mut length = 0usize;
            let mut shift = 0u32;
            loop {
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).unwrap();
                length |= ((byte[0] & 0x7f) as usize) << shift;
                if byte[0] & 0x80 == 0 {
                    break;
                }
                shift += 7;
            }
            let mut body = vec![0u8; length];
            stream.read_exact(&mut body).unwrap();
            body
        }

        // The host: the TCP port the relay bridges to.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        let host = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let handshake = read_packet(&mut stream);
            if tx.send(handshake).is_err() {
                return;
            }
            // A server list ping: the client asks, the server answers.
            let _request = read_packet(&mut stream);
            let status =
                br#"{"version":{"name":"e4all","protocol":770},"players":{"max":20,"online":0},"description":{"text":"e4all"}}"#;
            let mut body = Vec::new();
            body.push(0x00);
            write_varint(status.len(), &mut body);
            body.extend_from_slice(status);
            let mut framed = Vec::new();
            write_varint(body.len(), &mut framed);
            framed.extend_from_slice(&body);
            let _ = stream.write_all(&framed);
            let _ = stream.flush();
        });

        let broker = Broker::default();
        let mut session = Session::launch(broker, fixed_port(port), Timeouts::default());
        let domain = session
            .wait_for_domain(Duration::from_secs(30))
            .expect("the relay gave no domain");

        // The client: a player types the domain in the server list, so the
        // client dials the domain's own host and sends a handshake with it.
        let mut stream =
            std::net::TcpStream::connect((domain.as_str(), 25565)).expect("no connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let mut body = Vec::new();
        body.push(0x00);
        write_varint(770, &mut body);
        write_varint(domain.len(), &mut body);
        body.extend_from_slice(domain.as_bytes());
        body.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(1, &mut body);
        let mut framed = Vec::new();
        write_varint(body.len(), &mut framed);
        framed.extend_from_slice(&body);
        framed.extend_from_slice(&[0x01, 0x00]);
        stream.write_all(&framed).unwrap();
        stream.flush().unwrap();

        // The host got the client's handshake, unchanged.
        let received = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the host got nothing");
        assert_eq!(received, body);

        // And the client got the host's answer, through the relay.
        let answer = read_packet(&mut stream);
        let text = String::from_utf8_lossy(&answer);
        assert!(
            text.contains("e4all"),
            "the answer was not the host's: {text}"
        );

        session.stop();
        let _ = host.join();
    }

    #[test]
    fn stopping_a_session_ends_it_at_once() {
        let broker = Broker {
            use_broker: false,
            url: String::new(),
            fallback_host: "127.0.0.1".into(),
            fallback_port: 9,
        };
        let mut session = Session::launch(broker, fixed_port(25565), quick());
        session.stop();
        assert_eq!(session.state(), SessionState::Stopped);
        assert!(session.domain().is_none());
    }
}
