//! Serde DTOs for persisting room state. The domain types stay serde-free;
//! these mirror the wire-DTO pattern in `crate::wire`.
#![allow(clippy::module_name_repetitions)]

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::domain::ids::{PlayerId, RoomCode};
use crate::domain::player::Player;
use crate::domain::room::{Room, RoomState};

/// Persisted room lifecycle state. Mirrors `RoomState` (kept serde-free).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotState {
    Lobby,
    Active,
}

/// One persisted player. `token` IS persisted — it is the reconnection
/// credential. `connected` is intentionally omitted: it is always `false`
/// after a restart (no live sockets).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub id: PlayerId,
    pub name: String,
    pub token: String,
    pub is_host: bool,
    pub skip_next: bool,
}

/// One persisted room. Excludes all `Instant`s and the broadcast channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub code: String,
    pub state: SnapshotState,
    /// Defaulted so snapshots written before the lock feature still load.
    #[serde(default)]
    pub locked: bool,
    pub players: Vec<PlayerSnapshot>,
    pub current_player_id: Option<PlayerId>,
    pub previous_player_id: Option<PlayerId>,
}

/// The full persisted registry: every live room.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    pub rooms: Vec<RoomSnapshot>,
}

impl From<&Room> for RoomSnapshot {
    fn from(room: &Room) -> Self {
        Self {
            code: room.code.0.clone(),
            state: match room.state {
                RoomState::Lobby => SnapshotState::Lobby,
                RoomState::Active => SnapshotState::Active,
            },
            locked: room.locked,
            players: room
                .players
                .iter()
                .map(|p| PlayerSnapshot {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    token: p.token.clone(),
                    is_host: p.is_host,
                    skip_next: p.skip_next,
                })
                .collect(),
            current_player_id: room.current_player_id.clone(),
            previous_player_id: room.previous_player_id.clone(),
        }
    }
}

impl RoomSnapshot {
    /// Rebuild a domain `Room`, applying the load-time policy: time fields reset
    /// to now, nudge cooldowns cleared, every player marked disconnected.
    #[must_use]
    pub fn into_room(self) -> Room {
        let now = Instant::now();
        Room {
            code: RoomCode(self.code),
            created_at: now,
            last_active: now,
            state: match self.state {
                SnapshotState::Lobby => RoomState::Lobby,
                SnapshotState::Active => RoomState::Active,
            },
            locked: self.locked,
            players: self
                .players
                .into_iter()
                .map(|p| Player {
                    id: p.id,
                    name: p.name,
                    token: p.token,
                    is_host: p.is_host,
                    connected: false,
                    skip_next: p.skip_next,
                })
                .collect(),
            current_player_id: self.current_player_id,
            previous_player_id: self.previous_player_id,
            last_nudge_at: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::RoomCode;
    use crate::domain::room::Room;
    use std::time::Instant;

    #[test]
    fn round_trip_preserves_turn_state_order_and_tokens() {
        let now = Instant::now();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(&host_id, now).unwrap();
        room.end_turn(&host_id, now).unwrap(); // current=Bob, previous=Host

        let snap = RoomSnapshot::from(&room);
        let json = serde_json::to_string(&snap).unwrap();
        let back: RoomSnapshot = serde_json::from_str(&json).unwrap();
        let rebuilt = back.into_room();

        assert_eq!(rebuilt.code.0, "ABC123");
        assert_eq!(rebuilt.players.len(), 2);
        assert_eq!(rebuilt.players[0].id, host_id);
        assert_eq!(rebuilt.players[1].id, bob);
        assert_eq!(rebuilt.current_player_id, Some(bob));
        assert_eq!(rebuilt.previous_player_id, Some(host_id));
        // tokens survive (reconnection credential)
        assert!(rebuilt.players.iter().all(|p| !p.token.is_empty()));
    }

    #[test]
    fn into_room_applies_load_policy() {
        let now = Instant::now();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        room.players[0].connected = true;
        room.last_nudge_at.insert(host_id, now);

        let rebuilt = RoomSnapshot::from(&room).into_room();

        assert!(
            rebuilt.players.iter().all(|p| !p.connected),
            "connected must reset to false"
        );
        assert!(
            rebuilt.last_nudge_at.is_empty(),
            "nudge cooldowns must clear"
        );
    }

    #[test]
    fn locked_flag_survives_round_trip() {
        let now = Instant::now();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        room.set_locked(&host_id, true, now).unwrap();

        let rebuilt = RoomSnapshot::from(&room).into_room();
        assert!(rebuilt.locked, "locked flag must persist across reload");
    }

    #[test]
    fn snapshot_without_locked_field_defaults_to_unlocked() {
        // A snapshot written before the lock feature has no `locked` key.
        let json = r#"{
            "code": "ABC123",
            "state": "lobby",
            "players": [],
            "current_player_id": null,
            "previous_player_id": null
        }"#;
        let snap: RoomSnapshot = serde_json::from_str(json).unwrap();
        assert!(!snap.locked, "missing locked must default to false");
    }

    #[test]
    fn skip_next_flag_survives_round_trip() {
        let now = Instant::now();
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        let idx = room.players.iter().position(|p| p.id == bob).unwrap();
        room.players[idx].skip_next = true;

        let rebuilt = RoomSnapshot::from(&room).into_room();
        let idx = rebuilt.players.iter().position(|p| p.id == bob).unwrap();
        assert!(rebuilt.players[idx].skip_next);
    }
}
