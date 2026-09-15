//! Liveness tracking for a WebSocket connection.

use std::time::{Duration, Instant};

/// How often the server sends a Ping frame. Chosen to sit well under the
/// shortest idle timeout we expect to meet — carrier NAT, and Cloudflare's
/// proxy if it is ever enabled in front of the app.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// How long a connection may go without a Pong before it is closed. Two
/// intervals plus slack, so one dropped Pong does not kill a live connection.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(90);

/// Tracks when a connection last proved it was alive.
///
/// Browsers answer Ping frames automatically at protocol level (the JS
/// WebSocket API cannot send pings), so the server pings and the client's
/// Pong is the liveness signal.
#[derive(Debug)]
pub struct Heartbeat {
    timeout: Duration,
    last_pong: Instant,
}

impl Heartbeat {
    /// Start tracking, treating `now` as the last known-good moment.
    #[must_use]
    pub const fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            timeout,
            last_pong: now,
        }
    }

    /// Record that the peer answered a Ping.
    pub const fn record_pong(&mut self, now: Instant) {
        self.last_pong = now;
    }

    /// Whether the peer has been silent for longer than the timeout.
    #[must_use]
    pub fn is_stale(&self, now: Instant) -> bool {
        now.duration_since(self.last_pong) > self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn fresh_connection_is_not_stale() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(!hb.is_stale(now));
    }

    #[test]
    fn connection_is_stale_once_the_timeout_elapses() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(
            hb.is_stale(now + Duration::from_millis(51)),
            "must be stale strictly after the timeout"
        );
    }

    #[test]
    fn connection_is_not_stale_exactly_at_the_timeout() {
        let now = Instant::now();
        let hb = Heartbeat::new(Duration::from_millis(50), now);
        assert!(!hb.is_stale(now + Duration::from_millis(50)));
    }

    #[test]
    fn recording_a_pong_resets_staleness() {
        let now = Instant::now();
        let mut hb = Heartbeat::new(Duration::from_millis(50), now);
        let later = now + Duration::from_millis(40);
        hb.record_pong(later);
        assert!(!hb.is_stale(later + Duration::from_millis(40)));
        assert!(hb.is_stale(later + Duration::from_millis(51)));
    }

    #[test]
    fn timeout_is_a_multiple_of_the_interval_so_a_single_lost_pong_is_tolerated() {
        assert!(
            CLIENT_TIMEOUT >= HEARTBEAT_INTERVAL * 2,
            "one dropped pong must not close a healthy connection"
        );
    }
}
