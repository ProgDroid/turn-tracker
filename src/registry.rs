//! In-memory room storage with a per-room broadcast channel for fan-out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::persist::snapshot::{RegistrySnapshot, RoomSnapshot};

use crate::domain::ids::{PlayerId, RoomCode};
use crate::domain::room::Room;
use crate::error::AppError;
use crate::wire::{PublicRoom, ServerMessage};

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
    dirty: AtomicBool,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
            dirty: AtomicBool::new(false),
        }
    }

    /// Mark that state changed since the last snapshot.
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Return whether state changed since the last call, clearing the flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }

    /// Build a serializable snapshot of every live room.
    #[must_use]
    pub fn snapshot(&self) -> RegistrySnapshot {
        RegistrySnapshot {
            rooms: self
                .rooms
                .iter()
                .map(|entry| RoomSnapshot::from(&entry.value().room))
                .collect(),
        }
    }

    /// Rebuild a registry from a snapshot. Each room gets a fresh, empty
    /// broadcast channel; the dirty flag starts clear.
    #[must_use]
    pub fn from_snapshot(snap: RegistrySnapshot) -> Self {
        let rooms = DashMap::new();
        for rs in snap.rooms {
            let room = rs.into_room();
            let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
            rooms.insert(room.code.0.clone(), RoomHandle { room, tx });
        }
        Self {
            rooms,
            dirty: AtomicBool::new(false),
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
            self.mark_dirty();
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

    /// Read-only public snapshot of a room's current state. Does NOT mark the
    /// registry dirty (no mutation occurs). Used to resync a client that fell
    /// behind on its broadcast channel.
    #[must_use]
    pub fn public_room(&self, code: &str) -> Option<PublicRoom> {
        self.rooms.get(code).map(|h| PublicRoom::from(&h.room))
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
    /// The closure returns `(result, outbound, dirty)`. `dirty` MUST be true iff
    /// the closure changed state that is persisted in a snapshot (see
    /// [`RoomSnapshot`]). Read-only refreshes, rejected/unauthorized actions,
    /// connection flips (`connected` is not persisted), and nudges (cooldowns
    /// are not persisted) pass `false` so they don't trigger snapshot churn.
    ///
    /// # Errors
    /// `AppError::RoomNotFound` if the code is unknown.
    pub fn with_room_mut<F, T>(&self, code: &str, f: F) -> Result<T, AppError>
    where
        F: FnOnce(&mut Room) -> (T, Vec<Outbound>, bool),
    {
        let mut handle = self.rooms.get_mut(code).ok_or(AppError::RoomNotFound)?;
        let (result, outbound, dirty) = f(&mut handle.room);
        for msg in outbound {
            // Ignore send errors: a closed channel just means no live receivers.
            let _ = handle.tx.send(msg);
        }
        if dirty {
            self.mark_dirty();
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
        if !expired.is_empty() {
            self.mark_dirty();
        }
        expired.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_flag_set_on_create_and_cleared_on_take() {
        let reg = Registry::new();
        assert!(!reg.take_dirty(), "fresh registry is clean");
        reg.create_room("Host".into(), Instant::now()).unwrap();
        assert!(reg.take_dirty(), "create_room marks dirty");
        assert!(!reg.take_dirty(), "take_dirty clears the flag");
    }

    #[test]
    fn test_snapshot_round_trip_preserves_room_and_token() {
        let reg = Registry::new();
        let now = Instant::now();
        let (code, host_id, token) = reg.create_room("Host".into(), now).unwrap();

        let rebuilt = Registry::from_snapshot(reg.snapshot());

        assert!(rebuilt.contains(&code.0), "room survives snapshot");
        // Host is reconnectable by token, and starts disconnected.
        rebuilt
            .with_room_mut(&code.0, |room| {
                let p = room.player_by_token(&token).expect("token preserved");
                assert_eq!(p.id, host_id);
                assert!(!p.connected, "player starts disconnected after load");
                ((), vec![], false)
            })
            .unwrap();
        // subscribing works → a fresh channel was created
        assert!(rebuilt.subscribe(&code.0).is_ok());
    }

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
    fn test_with_room_mut_marks_dirty_only_when_flag_is_set() {
        let reg = Registry::new();
        let (code, _h, _t) = reg.create_room("Host".into(), Instant::now()).unwrap();
        assert!(reg.take_dirty(), "create marked dirty");

        // dirty = false → no snapshot churn (e.g. a rejected action or nudge).
        reg.with_room_mut(&code.0, |_room| ((), vec![], false))
            .unwrap();
        assert!(
            !reg.take_dirty(),
            "closure returning dirty=false must not mark dirty"
        );

        // dirty = true → a real persisted-state change.
        reg.with_room_mut(&code.0, |_room| ((), vec![], true))
            .unwrap();
        assert!(
            reg.take_dirty(),
            "closure returning dirty=true must mark dirty"
        );
    }

    #[test]
    fn test_public_room_reads_state_without_marking_dirty() {
        let reg = Registry::new();
        let (code, _h, _t) = reg.create_room("Host".into(), Instant::now()).unwrap();
        assert!(reg.take_dirty(), "create marked dirty");
        let public = reg.public_room(&code.0).expect("room exists");
        assert_eq!(public.code, code.0);
        assert!(
            !reg.take_dirty(),
            "public_room is a read: must not mark dirty"
        );
        assert!(reg.public_room("NOPE12").is_none());
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
