//! In-memory room storage with a per-room broadcast channel for fan-out.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::domain::ids::{PlayerId, RoomCode};
use crate::domain::room::Room;
use crate::error::AppError;
use crate::wire::ServerMessage;

/// A server message plus its delivery target.
#[derive(Debug, Clone)]
pub enum Outbound {
    /// Deliver to every connection in the room.
    All(ServerMessage),
    /// Deliver only to the connection(s) authenticated as this player.
    Player(PlayerId, ServerMessage),
}

const BROADCAST_CAPACITY: usize = 64;
const MAX_CODE_ATTEMPTS: usize = 10;

/// Maximum number of concurrently live rooms the server will hold.
pub const MAX_ROOMS: usize = 10_000;

/// One room plus the channel used to push messages to its connections.
pub struct RoomHandle {
    pub room: Room,
    pub tx: broadcast::Sender<Outbound>,
}

#[derive(Default)]
pub struct Registry {
    rooms: DashMap<String, RoomHandle>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
        }
    }

    /// Create a room with a unique code. Returns `(code, host_id, host_token)`.
    ///
    /// # Errors
    /// `AppError::RoomCapacityReached` if the server already holds [`MAX_ROOMS`]
    /// rooms; `AppError::CodeExhausted` if a unique code can't be found in
    /// `MAX_CODE_ATTEMPTS` tries.
    pub fn create_room(
        &self,
        host_name: String,
        now: Instant,
    ) -> Result<(RoomCode, PlayerId, String), AppError> {
        if self.rooms.len() >= MAX_ROOMS {
            log::warn!("room create rejected: server at capacity ({MAX_ROOMS} rooms)");
            return Err(AppError::RoomCapacityReached);
        }
        for _ in 0..MAX_CODE_ATTEMPTS {
            let code = RoomCode::generate();
            if self.rooms.contains_key(&code.0) {
                continue;
            }
            let (room, host_id, token) = Room::create(code.clone(), host_name, now);
            let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
            self.rooms.insert(code.0.clone(), RoomHandle { room, tx });
            // CODE only — never the host token.
            log::info!("room created: code={}", code.0);
            return Ok((code, host_id, token));
        }
        Err(AppError::CodeExhausted)
    }

    #[must_use]
    pub fn contains(&self, code: &str) -> bool {
        self.rooms.contains_key(code)
    }

    /// Subscribe to a room's broadcast channel.
    ///
    /// # Errors
    /// `AppError::RoomNotFound` if the code is unknown.
    pub fn subscribe(&self, code: &str) -> Result<broadcast::Receiver<Outbound>, AppError> {
        self.rooms
            .get(code)
            .map(|h| h.tx.subscribe())
            .ok_or(AppError::RoomNotFound)
    }

    /// Run a closure with mutable access to a room, then broadcast any produced
    /// messages. Returns the closure's result.
    ///
    /// # Errors
    /// `AppError::RoomNotFound` if the code is unknown.
    pub fn with_room_mut<F, T>(&self, code: &str, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Room) -> (T, Vec<Outbound>),
    {
        let mut handle = self.rooms.get_mut(code).ok_or(AppError::RoomNotFound)?;
        let (result, outbound) = f(&mut handle.room);
        for msg in outbound {
            // Ignore send errors: a closed channel just means no live receivers.
            let _ = handle.tx.send(msg);
        }
        // Release the DashMap shard lock before returning to reduce contention.
        drop(handle);
        Ok(result)
    }

    /// Remove rooms idle for at least `ttl`. Returns the number removed.
    #[must_use]
    pub fn sweep_expired(&self, now: Instant, ttl: Duration) -> usize {
        let expired: Vec<String> = self
            .rooms
            .iter()
            .filter(|entry| entry.value().room.is_expired(now, ttl))
            .map(|entry| entry.key().clone())
            .collect();
        for code in &expired {
            self.rooms.remove(code);
        }
        expired.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_room_inserts_and_is_findable() {
        let reg = Registry::new();
        let (code, _host, _tok) = reg.create_room("Host".into(), Instant::now()).unwrap();
        assert!(reg.contains(&code.0));
    }

    #[test]
    fn test_create_room_rejects_when_at_capacity() {
        let reg = Registry::new();
        let now = Instant::now();
        for _ in 0..MAX_ROOMS {
            reg.create_room("Host".into(), now).unwrap();
        }
        assert_eq!(reg.rooms.len(), MAX_ROOMS);
        match reg.create_room("Host".into(), now) {
            Err(AppError::RoomCapacityReached) => {}
            other => panic!("expected RoomCapacityReached, got {other:?}"),
        }
    }

    #[test]
    fn test_subscribe_unknown_room_errors() {
        let reg = Registry::new();
        assert!(reg.subscribe("NOPE12").is_err());
    }

    #[test]
    fn test_sweep_removes_expired_rooms() {
        let reg = Registry::new();
        let now = Instant::now();
        let (code, _h, _t) = reg.create_room("Host".into(), now).unwrap();
        let later = now + Duration::from_hours(25); // > 24h
        assert_eq!(reg.sweep_expired(later, Duration::from_hours(24)), 1);
        assert!(!reg.contains(&code.0));
    }
}
