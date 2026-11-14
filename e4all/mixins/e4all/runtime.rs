// `game::e4all`: the relay session's life inside the game.
//
// One rule drives everything: **the session exists exactly while the world is
// actually published to the LAN**, and it is told the LAN port every tick. The
// relay gives a domain as soon as it is asked, so a session started before the
// LAN opens would hand out an address nothing answers.
//
// Two things ask for a session and both are honoured: the LAN opening on its
// own (Open to LAN, or `hostEnabled` on), and `/e4all start`. `/e4all stop`
// closes it for as long as the LAN stays open; closing and reopening the LAN
// publishes again.

use crate::game::Game;
use crate::game::lan::LanSettings;
use crate::text::Text;
use e4all_core::config::Config;
use e4all_core::session::{Session, SessionState, Timeouts};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The version the game and the release both report.
pub const VERSION: &str = "0.1.1";

/// The config's file, in Horizon's config folder.
const FILE: &str = "e4all.json";

/// The config, read once and then kept here: the settings page changes it at
/// run time, so it cannot be a `OnceLock` of `&'static`.
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

/// A copy of the config. Cheap: a handful of fields.
pub fn config() -> Config {
    let mut slot = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(config) = slot.as_ref() {
        return config.clone();
    }
    let loaded = load();
    *slot = Some(loaded.clone());
    loaded
}

/// Reads the file, writing the defaults out on the first run.
fn load() -> Config {
    let Ok(dir) = Game::native_config_dir() else {
        return Config::default();
    };
    let path = dir.join(FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        // First run: write the defaults out, so there is something to edit.
        if let Ok(json) = serde_json::to_string_pretty(&Config::default()) {
            let _ = std::fs::write(&path, json);
        }
        return Config::default();
    };
    match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(e) => {
            log::warn!(
                "e4all: {} doesn't read ({e}); using the defaults",
                path.display()
            );
            Config::default()
        }
    }
}

/// Keeps `config` and writes it out. A write that fails leaves the running
/// game on the new value: the file is a record, not the authority.
pub fn save(config: &Config) {
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(config.clone());
    let Ok(dir) = Game::native_config_dir() else {
        return;
    };
    match serde_json::to_string_pretty(config) {
        Ok(json) => {
            if let Err(e) = std::fs::write(dir.join(FILE), json) {
                log::warn!("e4all: could not write config/{FILE}: {e}");
            }
        }
        Err(e) => log::warn!("e4all: could not encode the config: {e}"),
    }
}

/// "on"/"off", for a settings button's label.
pub fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// Starts hosting: the LAN's online mode is decided while the world opens, long
/// before the first tick, so the flag has to be set here, from the
/// `server_about_to_start` hook that runs just before it.
///
/// Setting it later than this has no effect on the LAN: `OnlineMode::lan()` has
/// already read `auth::current()`, and the `NetConfig` it built is what every
/// login goes through. So the switch in the settings page decides the **next**
/// world, not this one.
pub fn begin_hosting() {
    let offline = config().offline_mode;
    if offline {
        log::info!("e4all: the LAN runs in offline mode (config/{FILE}, offline_mode)");
    }
    crate::auth::e4all_auth_suppress::set_suppress(offline);
}

/// Stops hosting: the account is visible again, so joining an online-mode
/// server from the title screen works as usual.
pub fn end_hosting() {
    crate::auth::e4all_auth_suppress::set_suppress(false);
}

/// The one session, if any: the game thread starts and stops it.
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// The LAN port the session bridges to, updated every tick.
static PORT: Mutex<Option<u16>> = Mutex::new(None);
/// The last domain told to chat, so it is said once.
static ANNOUNCED: Mutex<Option<String>> = Mutex::new(None);
/// Set by `/e4all stop`: the player closed the address while the LAN stays open,
/// so the tick must not dial the relay again until they ask (`/e4all start`) or
/// the LAN is closed and opened once more.
static CLOSED: AtomicBool = AtomicBool::new(false);

/// The port supplier handed to the session: it reads the shared cell, so a LAN
/// port that appears after the session started is picked up.
fn port_source() -> Arc<dyn Fn() -> Option<u16> + Send + Sync> {
    Arc::new(|| *PORT.lock().unwrap_or_else(|e| e.into_inner()))
}

fn set_port(port: Option<u16>) {
    *PORT.lock().unwrap_or_else(|e| e.into_inner()) = port;
}

/// Whether the relay session is up and has a domain.
fn running() -> bool {
    SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|session| session.domain().is_some())
}

/// Whether the world must keep ticking while it is hosted: a paused world
/// freezes every guest until they are dropped, and the host's own pause menu
/// would do that to everybody.
pub fn prevent_pause() -> bool {
    config().prevent_pause_while_hosting && running()
}

/// The timeouts the session is started with.
fn timeouts(config: &Config) -> Timeouts {
    Timeouts {
        domain: config.relay_connect_timeout(),
        connect: config.relay_connect_timeout(),
        keepalive: config.keepalive(),
    }
}

/// Starts the relay for the world, opening the LAN first when it isn't open.
/// `Err` when the LAN can't open or the relay refuses.
pub(super) fn start(game: &mut Game<'_>) -> Result<String, String> {
    let config = config();
    CLOSED.store(false, Ordering::Relaxed);
    if game.lan_port().is_none() {
        open_lan(game)?;
    }
    let port = game
        .lan_port()
        .ok_or_else(|| "e4all: the world isn't shared".to_string())?;
    set_port(Some(port));

    {
        let mut slot = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = slot.as_ref()
            && let Some(domain) = existing.domain()
        {
            return Ok(domain);
        }
        if let Some(mut old) = slot.take() {
            old.stop();
        }
        slot.replace(Session::launch(
            config.broker(),
            port_source(),
            timeouts(&config),
        ));
    }

    // `/e4all start` is an explicit request, so it may wait a bounded time.
    let session = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|session| session.wait_for_domain(config.relay_connect_timeout()));
    match session {
        Some(Ok(domain)) => {
            announce(game, &domain);
            Ok(domain)
        }
        Some(Err(e)) => {
            stop();
            Err(e)
        }
        None => Err("e4all: no session".into()),
    }
}

/// Opens the world to the LAN the way the game's own screen does.
fn open_lan(game: &mut Game<'_>) -> Result<(), String> {
    let settings = LanSettings {
        port: crate::lan::available_port(),
        guest_commands: false,
        force_game_mode: false,
    };
    game.lan_open(settings)
        .map_err(|e| format!("e4all: could not open the LAN: {e}"))
}

/// Ends the session; the relay forgets the domain once the connection closes.
pub fn stop() {
    if let Some(mut session) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() {
        session.stop();
    }
    set_port(None);
    *ANNOUNCED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// `/e4all stop`: closes the address and keeps it closed, so the tick does not
/// dial the relay again for the LAN that is still open.
pub fn stop_and_hold() {
    CLOSED.store(true, Ordering::Relaxed);
    stop();
}

/// The config and what the session is doing.
pub fn status() -> (Config, SessionState) {
    let state = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(Session::state)
        .unwrap_or(SessionState::Stopped);
    (config(), state)
}

/// The domain of the running session, if it has one.
pub fn domain() -> Option<String> {
    SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(Session::domain)
}

/// Called at the end of every server tick: follows the LAN, and starts or stops
/// the session to match. Never waits for the network.
pub(super) fn tick(game: &mut Game<'_>) {
    let shared = game.lan_port();
    set_port(shared);
    let running = running();
    let config = config();

    // The player closed the address (`/e4all stop`) but the LAN is still open:
    // leave it closed. Closing the LAN clears this, so opening it again publishes.
    if shared.is_none() {
        CLOSED.store(false, Ordering::Relaxed);
    }
    // The world stopped being shared (or the mod was switched off): close.
    if running && (shared.is_none() || !config.host_enabled) {
        stop();
        return;
    }
    // The world is shared and the relay isn't up: dial it, without waiting.
    if !running && shared.is_some() && config.host_enabled && !CLOSED.load(Ordering::Relaxed) {
        let mut slot = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            slot.replace(Session::launch(
                config.broker(),
                port_source(),
                timeouts(&config),
            ));
        }
        return;
    }
    // A domain that arrived between two ticks gets its line now.
    if let Some(domain) = domain() {
        announce(game, &domain);
    }
}

/// The domain in chat, clickable to copy, once per domain.
///
/// `hideDomainInChat` keeps the address out of the public chat: a guest asks
/// for it privately with `/e4all domain` instead.
fn announce(game: &mut Game<'_>, domain: &str) {
    if config().hide_domain_in_chat {
        return;
    }
    let mut told = ANNOUNCED.lock().unwrap_or_else(|e| e.into_inner());
    if told.as_deref() == Some(domain) {
        return;
    }
    let line = Text::literal("e4all: ")
        .append(Text::copyable(domain))
        .append(Text::literal("  (click to copy)"));
    game.native_broadcast(line);
    *told = Some(domain.to_string());
}

/// The usages `/e4all` answers.
pub fn commands() -> Vec<crate::game::native_server::NativeCommand> {
    vec![
        crate::game::native_server::NativeCommand::new("e4all"),
        crate::game::native_server::NativeCommand::new("e4all status"),
        crate::game::native_server::NativeCommand::new("e4all start"),
        crate::game::native_server::NativeCommand::new("e4all stop"),
        crate::game::native_server::NativeCommand::new("e4all domain"),
    ]
}

/// Runs one `/e4all ...`, answering the player who typed it.
pub(super) fn command(game: &mut Game<'_>, player: u128, words: &[&str]) -> bool {
    let (config, state) = status();
    let message = match words.get(1).copied() {
        None | Some("status") => match &state {
            SessionState::Started { domain } => format!(
                "e4all: open — {domain} (LAN port {})",
                game.lan_port()
                    .map_or_else(|| "?".into(), |p| p.to_string())
            ),
            SessionState::Starting => "e4all: asking the relay for an address…".to_string(),
            SessionState::Stopped => {
                if !config.host_enabled {
                    "e4all: turned off in config/e4all.json".to_string()
                } else if CLOSED.load(Ordering::Relaxed) {
                    "e4all: closed. /e4all start gets an address again.".to_string()
                } else {
                    "e4all: closed. /e4all start opens the LAN and gets an address.".to_string()
                }
            }
        },
        Some("start") => match start(game) {
            Ok(domain) => format!("e4all: open — {domain}"),
            Err(e) => e,
        },
        Some("stop") => {
            stop_and_hold();
            "e4all: address closed (the LAN stays open).".to_string()
        }
        Some("domain") => match domain() {
            Some(domain) => domain,
            None => "e4all: no address yet.".to_string(),
        },
        Some(other) => format!("e4all: unknown subcommand {other:?}"),
    };
    game.native_tell(player, Text::literal(message));
    true
}
