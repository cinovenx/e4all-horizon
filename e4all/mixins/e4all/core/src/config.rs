//! e4all's settings, with the defaults its Java mod ships.
//!
//! The file is `e4all.json` in Horizon's config folder. Every field falls
//! back to the Java default, so an older file (or none at all) still reads.

use crate::broker::Broker;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// e4all's own configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    /// Whether to use the broker to get the best relay based on location or
    /// use a hard-coded relay.
    pub use_broker: bool,
    pub broker_url: String,
    pub relay_host: String,
    pub relay_port: u16,

    /// Allows use of certain dedicated server commands such as /ban and
    /// /whitelist.
    pub restore_dedicated_commands: bool,
    /// Whether to use whitelists on LAN worlds.
    pub use_white_list: bool,

    /// Whether to enable sharing LAN worlds with e4all.
    pub host_enabled: bool,
    /// Keep the world ticking while e4all is hosting it. Vanilla pauses a
    /// singleplayer world on ESC and on window focus loss, and a paused world
    /// freezes every connected guest until they are dropped. Dedicated servers
    /// never pause.
    pub prevent_pause_while_hosting: bool,

    /// Whether to enable Dialtone peer-to-peer connections as the host.
    pub dialtone_host_enabled: bool,
    /// Whether to enable Dialtone peer-to-peer connections as the player.
    pub dialtone_player_enabled: bool,
    /// The URL to get the list of Iroh relays to use.
    pub dialtone_relay_map: String,
    /// Whether to hide direct IP addresses from the relay.
    pub dialtone_sanitize_ticket: bool,

    /// Keepalive probe interval in seconds to keep the relay session alive.
    pub keepalive_interval_seconds: u64,
    /// How long (in ms) a guest login may wait for a starting world to finish
    /// its first tick before being rejected.
    pub login_ready_timeout_ms: u64,
    /// How long (in ms) the connection to the relay may take before retrying.
    pub relay_connect_timeout_ms: u64,

    /// Whether to enable offline mode (disables Microsoft authentication for
    /// ALL LAN connections, including both tunneled and direct).
    pub offline_mode: bool,
    /// Whether the offline mode warning has already been shown to the user.
    pub offline_warning_shown: bool,
    /// Whether the welcome message has already been shown to the user.
    pub welcome_shown: bool,
    /// Whether to hide the domain in the chat message (click to copy still
    /// works).
    pub hide_domain_in_chat: bool,

    /// Master switch for SVC P2P voice support. When disabled, voice will not
    /// be negotiated.
    pub voice_p2p_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            use_broker: true,
            broker_url: "https://broker.e4mc.link/getBestRelay".into(),
            relay_host: "test.e4mc.link".into(),
            relay_port: 25575,
            restore_dedicated_commands: true,
            use_white_list: false,
            host_enabled: true,
            prevent_pause_while_hosting: true,
            dialtone_host_enabled: true,
            dialtone_player_enabled: true,
            dialtone_relay_map: "https://natives.e4mc.link/relaymap.json".into(),
            dialtone_sanitize_ticket: true,
            keepalive_interval_seconds: 15,
            login_ready_timeout_ms: 15000,
            relay_connect_timeout_ms: 15000,
            offline_mode: false,
            offline_warning_shown: false,
            welcome_shown: false,
            hide_domain_in_chat: false,
            voice_p2p_enabled: true,
        }
    }
}

impl Config {
    /// The broker that names the relay to dial.
    pub fn broker(&self) -> Broker {
        Broker {
            use_broker: self.use_broker,
            url: self.broker_url.clone(),
            fallback_host: self.relay_host.clone(),
            fallback_port: self.relay_port,
        }
    }

    /// How often the session probes the relay.
    pub fn keepalive(&self) -> Duration {
        Duration::from_secs(self.keepalive_interval_seconds.max(1))
    }

    /// How long a connection to the relay may take.
    pub fn relay_connect_timeout(&self) -> Duration {
        Duration::from_millis(self.relay_connect_timeout_ms.max(1))
    }

    /// How long a guest login may wait for the world's first tick.
    pub fn login_ready_timeout(&self) -> Duration {
        Duration::from_millis(self.login_ready_timeout_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_java_mods_own() {
        let config = Config::default();
        assert!(config.use_broker);
        assert_eq!(config.broker_url, "https://broker.e4mc.link/getBestRelay");
        assert_eq!(config.relay_host, "test.e4mc.link");
        assert_eq!(config.relay_port, 25575);
        assert!(config.restore_dedicated_commands);
        assert!(!config.use_white_list);
        assert!(config.host_enabled);
        assert!(config.prevent_pause_while_hosting);
        assert!(config.dialtone_host_enabled);
        assert!(config.dialtone_player_enabled);
        assert_eq!(
            config.dialtone_relay_map,
            "https://natives.e4mc.link/relaymap.json"
        );
        assert!(config.dialtone_sanitize_ticket);
        assert_eq!(config.keepalive_interval_seconds, 15);
        assert_eq!(config.login_ready_timeout_ms, 15000);
        assert_eq!(config.relay_connect_timeout_ms, 15000);
        assert!(!config.offline_mode);
        assert!(!config.offline_warning_shown);
        assert!(!config.welcome_shown);
        assert!(!config.hide_domain_in_chat);
        assert!(config.voice_p2p_enabled);
    }

    #[test]
    fn an_empty_file_reads_as_the_defaults() {
        let config: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_file_with_one_field_keeps_the_other_defaults() {
        let config: Config = serde_json::from_str(r#"{"offline_mode":true}"#).unwrap();
        assert!(config.offline_mode);
        assert_eq!(config.relay_host, Config::default().relay_host);
        assert_eq!(config.keepalive_interval_seconds, 15);
    }

    #[test]
    fn the_config_survives_a_round_trip() {
        let config = Config {
            offline_mode: true,
            relay_port: 25576,
            ..Config::default()
        };
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&text).unwrap(), config);
    }

    #[test]
    fn the_broker_carries_the_settings() {
        let config = Config {
            use_broker: false,
            relay_host: "eu.e4mc.link".into(),
            relay_port: 25574,
            ..Config::default()
        };
        let broker = config.broker();
        assert!(!broker.use_broker);
        assert_eq!(broker.fallback_host, "eu.e4mc.link");
        assert_eq!(broker.fallback_port, 25574);
    }

    #[test]
    fn the_timeouts_never_become_zero() {
        let config = Config {
            keepalive_interval_seconds: 0,
            relay_connect_timeout_ms: 0,
            ..Config::default()
        };
        assert_eq!(config.keepalive(), Duration::from_secs(1));
        assert_eq!(config.relay_connect_timeout(), Duration::from_millis(1));
    }
}
