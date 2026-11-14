//! The Dialtone address the relay hands to the players.
//!
//! A ticket is the host's own peer-to-peer endpoint: the relay keeps it and
//! gives it to every guest, so a guest can reach the host directly instead of
//! through the relay. e4all's Java mod sends it as `v1_<ticket>`, and hides
//! the host's direct address from the relay (`dialtoneSanitizeTicket`), so the
//! relay never learns where the host is.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What the relay expects in front of a ticket.
pub const TICKET_PREFIX: &str = "v1_";

/// How long a ticket stays usable: the Java mod's cache keeps them for a
/// minute, because an endpoint that is gone is worse than no address at all.
pub const TICKET_TTL: Duration = Duration::from_secs(60);

/// How many hosts are remembered at once.
pub const TICKET_MAX_ENTRIES: usize = 64;

/// A ticket without its direct address: the part before the first colon is
/// the node's own id, the rest is where it can be reached right now.
///
/// An address that is already bare is returned as it is.
pub fn sanitize(raw: &str) -> String {
    match raw.split_once(':') {
        Some((node, rest)) if !rest.is_empty() && !node.is_empty() => node.to_owned(),
        _ => raw.to_owned(),
    }
}

/// The ticket as the relay wants it: `v1_` in front, the direct address
/// dropped when `sanitize` says so.
pub fn message(raw: &str, sanitize_address: bool) -> String {
    let body = if sanitize_address {
        sanitize(raw)
    } else {
        raw.to_owned()
    };
    format!("{TICKET_PREFIX}{body}")
}

/// What is known about a host's ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    /// The ticket, when the host has one.
    pub ticket: Option<String>,
    /// The host is known and has no ticket: the relay must be told so.
    pub no_ticket: bool,
}

impl Lookup {
    pub fn has_ticket(&self) -> bool {
        self.ticket.is_some()
    }
}

#[derive(Debug)]
struct Entry {
    ticket: Option<String>,
    at: Instant,
}

/// The hosts' tickets, each for a minute.
#[derive(Debug)]
pub struct TicketCache {
    entries: Mutex<HashMap<String, Entry>>,
    ttl: Duration,
    max: usize,
}

impl Default for TicketCache {
    fn default() -> Self {
        Self::new(TICKET_TTL, TICKET_MAX_ENTRIES)
    }
}

impl TicketCache {
    pub fn new(ttl: Duration, max: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            max,
        }
    }

    /// The host's ticket, when it is still fresh.
    pub fn fresh(&self, host: &str) -> Option<Lookup> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let entry = entries.get(host)?;
        if entry.at.elapsed() > self.ttl {
            entries.remove(host);
            return None;
        }
        Some(Lookup {
            ticket: entry.ticket.clone(),
            no_ticket: entry.ticket.is_none(),
        })
    }

    /// The host has a ticket.
    pub fn put_ticket(&self, host: &str, ticket: &str) {
        self.put(host, Some(ticket.to_owned()));
    }

    /// The host has no ticket: the relay must be told so.
    pub fn put_no_ticket(&self, host: &str) {
        self.put(host, None);
    }

    fn put(&self, host: &str, ticket: Option<String>) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= self.max {
            let now = Instant::now();
            entries.retain(|_, entry| now.duration_since(entry.at) <= self.ttl);
        }
        entries.insert(
            host.to_owned(),
            Entry {
                ticket,
                at: Instant::now(),
            },
        );
    }

    /// How many hosts are remembered.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_keeps_its_version_prefix() {
        assert_eq!(message("secret", true), "v1_secret");
    }

    #[test]
    fn the_direct_address_is_dropped_when_asked_for() {
        // An iroh ticket: the node's id, then where it can be reached.
        assert_eq!(sanitize("abc123:192.168.1.5:41641"), "abc123");
        assert_eq!(message("abc123:192.168.1.5:41641", true), "v1_abc123");
    }

    #[test]
    fn the_direct_address_is_kept_when_asked_for() {
        assert_eq!(
            message("abc123:192.168.1.5:41641", false),
            "v1_abc123:192.168.1.5:41641"
        );
    }

    #[test]
    fn a_bare_ticket_is_left_alone() {
        assert_eq!(sanitize("abc123"), "abc123");
        assert_eq!(message("abc123", true), "v1_abc123");
    }

    #[test]
    fn a_ticket_is_remembered_for_its_host() {
        let cache = TicketCache::default();
        cache.put_ticket("one.example", "v1_secret");
        let lookup = cache.fresh("one.example").unwrap();
        assert_eq!(lookup.ticket.as_deref(), Some("v1_secret"));
        assert!(lookup.has_ticket());
        assert!(!lookup.no_ticket);
    }

    #[test]
    fn a_host_without_a_ticket_is_known_to_have_none() {
        let cache = TicketCache::default();
        cache.put_no_ticket("one.example");
        let lookup = cache.fresh("one.example").unwrap();
        assert_eq!(lookup.ticket, None);
        assert!(lookup.no_ticket);
        assert!(!lookup.has_ticket());
    }

    #[test]
    fn an_unknown_host_has_no_lookup() {
        let cache = TicketCache::default();
        assert_eq!(cache.fresh("one.example"), None);
    }

    #[test]
    fn a_ticket_outlives_its_minute() {
        let cache = TicketCache::new(Duration::from_millis(1), 64);
        cache.put_ticket("one.example", "v1_secret");
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(cache.fresh("one.example"), None);
    }

    #[test]
    fn the_cache_forgets_its_oldest_hosts() {
        let cache = TicketCache::new(Duration::from_secs(60), 2);
        cache.put_ticket("one.example", "v1_a");
        cache.put_ticket("two.example", "v1_b");
        cache.put_ticket("three.example", "v1_c");
        assert!(cache.len() <= 3);
        assert!(cache.fresh("three.example").is_some());
    }
}
