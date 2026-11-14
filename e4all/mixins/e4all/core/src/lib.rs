//! e4all's core: everything the mod does that is not the game itself.
//!
//! Written from the protocol `vgskye/e4mc-quiclime` implements (Apache-2.0)
//! and from e4all's Java client (MIT, vgskye and contributors). No game type
//! lives here, so the whole crate is tested with `cargo test` alone.
//!
//! The shape of the session:
//!
//! * the **broker** names the nearest relay (`GET https://broker.e4mc.link/
//!   getBestRelay`), and a hard-coded relay is the fallback;
//! * the **protocol** is every control message: a length and that many bytes
//!   of JSON, on one stream that lives as long as the session;
//! * the **session** dials the relay over QUIC (ALPN `quiclime`), asks for a
//!   domain, and carries every joining player's stream to a TCP socket on
//!   this computer;
//! * the **config** is e4all's own settings, with the defaults its Java mod
//!   ships;
//! * the **ticket** is the Dialtone address the relay hands to the players.

pub mod broker;
pub mod config;
pub mod protocol;
pub mod session;
pub mod ticket;

pub use broker::{Broker, Relay};
pub use config::Config;
pub use protocol::{FromRelay, MAX_CONTROL_MESSAGE, ToRelay};
pub use session::{Session, SessionState, Timeouts};
pub use ticket::TicketCache;
