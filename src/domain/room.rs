//! The Room aggregate and all turn-coordination logic. Pure — no Actix, WS,
//! serde-wire, or wall-clock dependencies.

use std::time::Instant;

use crate::domain::ids::{PlayerId, RoomCode, generate_token};
use crate::domain::player::Player;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomState {
    Lobby,
    Active,
}

/// Errors returned by turn-coordination operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnError {
    NotAuthorized,
    NotFound,
    WrongState,
    NotYourTurn,
    NudgeOnCooldown,
}

pub struct Room {
    pub code: RoomCode,
    pub created_at: Instant,
    pub last_active: Instant,
    pub state: RoomState,
    /// Order is significant: this Vec IS the turn order.
    pub players: Vec<Player>,
    pub current_player_id: Option<PlayerId>,
    pub previous_player_id: Option<PlayerId>,
    pub last_nudge_at: std::collections::HashMap<PlayerId, Instant>,
}

impl Room {
    /// Create a new room in the Lobby state with the host as its first player.
    /// Returns the room plus the host's player id and secret token.
    #[must_use]
    pub fn create(code: RoomCode, host_name: String, now: Instant) -> (Self, PlayerId, String) {
        let host_id = PlayerId(generate_token());
        let token = generate_token();
        let host = Player::new(host_id.clone(), host_name, token.clone(), true);
        let room = Self {
            code,
            created_at: now,
            last_active: now,
            state: RoomState::Lobby,
            players: vec![host],
            current_player_id: None,
            previous_player_id: None,
            last_nudge_at: std::collections::HashMap::new(),
        };
        (room, host_id, token)
    }

    /// Add a new (non-host) player. Returns the new player's id and token.
    pub fn add_player(&mut self, name: String, now: Instant) -> (PlayerId, String) {
        let id = PlayerId(generate_token());
        let token = generate_token();
        self.players
            .push(Player::new(id.clone(), name, token.clone(), false));
        self.last_active = now;
        (id, token)
    }

    /// Find a player by their secret token.
    #[must_use]
    pub fn player_by_token(&self, token: &str) -> Option<&Player> {
        self.players.iter().find(|p| p.token == token)
    }

    #[must_use]
    pub fn is_host(&self, id: &PlayerId) -> bool {
        self.players.iter().any(|p| &p.id == id && p.is_host)
    }

    /// Transition Lobby -> Active. The first player in order becomes current.
    ///
    /// # Errors
    /// `TurnError::WrongState` if not in Lobby.
    pub fn start_game(&mut self, now: Instant) -> Result<(), TurnError> {
        if self.state != RoomState::Lobby {
            return Err(TurnError::WrongState);
        }
        self.state = RoomState::Active;
        self.current_player_id = self.players.first().map(|p| p.id.clone());
        self.previous_player_id = None;
        self.last_active = now;
        Ok(())
    }

    /// Index of `current_player_id` in `players`, if present.
    fn current_index(&self) -> Option<usize> {
        let cur = self.current_player_id.as_ref()?;
        self.players.iter().position(|p| &p.id == cur)
    }

    /// Compute the next eligible player's index, walking forward (wrapping)
    /// from `from_index`, skipping disconnected players and consuming one
    /// `skip_next` flag per skipped player. Returns None if nobody is eligible.
    ///
    /// `consume_skips` mutates `skip_next` flags as it walks, so call it only
    /// when actually advancing.
    fn advance_from(&mut self, from_index: usize) -> Option<usize> {
        let n = self.players.len();
        if n == 0 {
            return None;
        }
        for offset in 1..=n {
            let idx = (from_index + offset) % n;
            let player = &mut self.players[idx];
            if !player.connected {
                continue;
            }
            if player.skip_next {
                player.skip_next = false; // consume the one-turn skip
                continue;
            }
            return Some(idx);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn test_create_starts_in_lobby_with_host_as_only_player() {
        let (room, host_id, _tok) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert_eq!(room.state, RoomState::Lobby);
        assert_eq!(room.players.len(), 1);
        assert!(room.players[0].is_host);
        assert_eq!(room.players[0].id, host_id);
        assert!(room.current_player_id.is_none());
    }

    #[test]
    fn test_add_player_appends_non_host_to_order() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (pid, _tok) = room.add_player("Bob".into(), t0());
        assert_eq!(room.players.len(), 2);
        assert_eq!(room.players[1].id, pid);
        assert!(!room.players[1].is_host);
    }

    #[test]
    fn test_player_by_token_finds_correct_player() {
        let (room, host_id, tok) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert_eq!(room.player_by_token(&tok).map(|p| &p.id), Some(&host_id));
        assert!(room.player_by_token("nope").is_none());
    }

    #[test]
    fn test_start_game_activates_and_sets_first_player_current() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        assert!(room.start_game(t0()).is_ok());
        assert_eq!(room.state, RoomState::Active);
        assert_eq!(room.current_player_id, Some(host_id));
    }

    #[test]
    fn test_start_game_twice_returns_wrong_state() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.start_game(t0()).unwrap();
        assert_eq!(room.start_game(t0()), Err(TurnError::WrongState));
    }

    #[test]
    fn test_advance_from_skips_disconnected_player() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0()); // index 1
        room.add_player("Cara".into(), t0()); // index 2
        room.players[1].connected = false;
        assert_eq!(room.advance_from(0), Some(2));
    }

    #[test]
    fn test_advance_from_consumes_skip_next_flag() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0()); // index 1
        room.add_player("Cara".into(), t0()); // index 2
        room.players[1].skip_next = true;
        assert_eq!(room.advance_from(0), Some(2)); // Bob skipped
        assert!(!room.players[1].skip_next); // flag consumed
    }

    #[test]
    fn test_advance_from_wraps_around() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        let host_idx = room.players.iter().position(|p| p.id == host_id).unwrap();
        assert_eq!(room.advance_from(1), Some(host_idx));
    }
}
