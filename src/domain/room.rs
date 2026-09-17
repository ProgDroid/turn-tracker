//! The Room aggregate and all turn-coordination logic. Pure — no Actix, WS,
//! serde-wire, or wall-clock dependencies.

use std::time::{Duration, Instant};

/// Minimum interval between nudges from the same sender.
pub const NUDGE_COOLDOWN: Duration = Duration::from_secs(10);

/// Maximum number of players allowed in a single room.
pub const MAX_PLAYERS: usize = 16;

use crate::domain::ids::{PlayerId, RoomCode, generate_token};
use crate::domain::player::{ConnEpoch, Player};

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
    RoomFull,
    RoomLocked,
}

pub struct Room {
    pub code: RoomCode,
    pub created_at: Instant,
    pub last_active: Instant,
    pub state: RoomState,
    /// When true, only existing players (rejoin-by-token) may connect; brand-new
    /// players are turned away. Host-toggled; survives restarts.
    pub locked: bool,
    /// Order is significant: this Vec IS the turn order.
    pub players: Vec<Player>,
    pub current_player_id: Option<PlayerId>,
    pub previous_player_id: Option<PlayerId>,
    /// When the CURRENT player's turn began. `None` in Lobby (no turn is in
    /// progress). Deliberately an `Instant`, like every other time field here:
    /// no wall clock is kept in the domain, so this cannot be persisted and a
    /// restart restarts the clock.
    pub turn_started_at: Option<Instant>,
    pub last_nudge_at: std::collections::HashMap<PlayerId, Instant>,
    /// Hands out the next [`ConnEpoch`]. Starts at 1 so [`ConnEpoch::NONE`] is
    /// never issued. Process-local; deliberately NOT persisted, because every
    /// player is disconnected after a reload so no epoch can still be owned.
    pub next_conn_epoch: u64,
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
            locked: false,
            players: vec![host],
            current_player_id: None,
            previous_player_id: None,
            turn_started_at: None,
            last_nudge_at: std::collections::HashMap::new(),
            next_conn_epoch: 1,
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

    /// True once the room holds [`MAX_PLAYERS`] players (no more may join).
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.players.len() >= MAX_PLAYERS
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

    /// Attach a live connection to `id`: mark the player connected and return
    /// the epoch identifying this connection as the player's current owner.
    ///
    /// A later attach supersedes an earlier one. Nothing prevents two live
    /// sockets for the same token (a half-open connection the server has not
    /// noticed yet, plus the phone that already reconnected), so ownership is
    /// what [`Self::detach_connection`] tests before clearing `connected`.
    ///
    /// The epoch is burned even when `id` is unknown, so a stale caller can
    /// never be handed an epoch that matches some other player.
    pub fn attach_connection(&mut self, id: &PlayerId) -> ConnEpoch {
        let epoch = ConnEpoch(self.next_conn_epoch);
        self.next_conn_epoch += 1;
        if let Some(p) = self.players.iter_mut().find(|p| &p.id == id) {
            p.connected = true;
            p.conn_epoch = epoch;
        }
        epoch
    }

    /// Detach a departing connection: clear `connected` only if `epoch` is
    /// still the player's owner.
    ///
    /// Returns true if the flag was actually cleared — false means a newer
    /// connection has taken over and the caller is a ghost whose departure
    /// must change nothing (and must not be broadcast).
    pub fn detach_connection(&mut self, id: &PlayerId, epoch: ConnEpoch) -> bool {
        self.players
            .iter_mut()
            .find(|p| &p.id == id)
            .is_some_and(|p| {
                let owns = p.conn_epoch == epoch;
                if owns {
                    p.connected = false;
                }
                owns
            })
    }

    /// Transition Lobby -> Active. Host-only. The first player in order becomes current.
    ///
    /// # Errors
    /// `NotAuthorized` if `actor` is not the host; `WrongState` if not in Lobby.
    pub fn start_game(&mut self, actor: &PlayerId, now: Instant) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        if self.state != RoomState::Lobby {
            return Err(TurnError::WrongState);
        }
        self.state = RoomState::Active;
        self.current_player_id = self.players.first().map(|p| p.id.clone());
        self.previous_player_id = None;
        self.turn_started_at = Some(now);
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

    /// Advance the turn. Authorized for the current player OR the host.
    ///
    /// # Errors
    /// `WrongState` if not Active; `NotAuthorized` if `actor` is neither the
    /// current player nor the host.
    pub fn end_turn(&mut self, actor: &PlayerId, now: Instant) -> Result<(), TurnError> {
        if self.state != RoomState::Active {
            return Err(TurnError::WrongState);
        }
        let is_current = self.current_player_id.as_ref() == Some(actor);
        if !is_current && !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        self.advance_turn(now)
    }

    /// "Pull" the turn: the NEXT eligible player claims it early.
    ///
    /// # Errors
    /// `WrongState` if not Active; `NotYourTurn` if `actor` is not the next
    /// eligible player.
    pub fn claim_turn(&mut self, actor: &PlayerId, now: Instant) -> Result<(), TurnError> {
        if self.state != RoomState::Active {
            return Err(TurnError::WrongState);
        }
        let cur_idx = self.current_index().ok_or(TurnError::WrongState)?;
        let next_idx = self.peek_next(cur_idx).ok_or(TurnError::NotYourTurn)?;
        if self.players[next_idx].id != *actor {
            return Err(TurnError::NotYourTurn);
        }
        self.advance_turn(now)
    }

    /// Shared advance: move current -> next eligible, recording previous.
    fn advance_turn(&mut self, now: Instant) -> Result<(), TurnError> {
        let cur_idx = self.current_index().ok_or(TurnError::WrongState)?;
        let next_idx = self.advance_from(cur_idx).ok_or(TurnError::NotFound)?;
        self.previous_player_id = self.current_player_id.take();
        self.current_player_id = Some(self.players[next_idx].id.clone());
        self.turn_started_at = Some(now);
        self.last_active = now;
        Ok(())
    }

    /// Like `advance_from` but does NOT consume skip flags (read-only peek).
    fn peek_next(&self, from_index: usize) -> Option<usize> {
        let n = self.players.len();
        if n == 0 {
            return None;
        }
        for offset in 1..=n {
            let idx = (from_index + offset) % n;
            let p = &self.players[idx];
            if p.connected && !p.skip_next {
                return Some(idx);
            }
        }
        None
    }

    /// Single-level undo: move the turn back to the immediately previous holder.
    /// Host-only. Does NOT restore any skip flags consumed by the advance
    /// (single-step, best-effort revert for fat-finger correction).
    ///
    /// # Errors
    /// `WrongState` if not Active or there is no previous holder; `NotAuthorized`
    /// if `actor` is not the host; `NotFound` if the previous player no longer exists.
    pub fn undo_turn(&mut self, actor: &PlayerId, now: Instant) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        if self.state != RoomState::Active {
            return Err(TurnError::WrongState);
        }
        let prev = self
            .previous_player_id
            .take()
            .ok_or(TurnError::WrongState)?;
        if !self.players.iter().any(|p| p.id == prev) {
            return Err(TurnError::NotFound);
        }
        self.current_player_id = Some(prev);
        self.previous_player_id = None;
        self.turn_started_at = Some(now);
        self.last_active = now;
        Ok(())
    }

    /// Host flags a player to be skipped on their next turn. If that player is
    /// currently active, the turn advances immediately past them.
    ///
    /// # Errors
    /// `NotAuthorized` if `actor` is not the host; `NotFound` if `target` is
    /// not in the room.
    pub fn skip_player(
        &mut self,
        actor: &PlayerId,
        target: &PlayerId,
        now: Instant,
    ) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        let idx = self
            .players
            .iter()
            .position(|p| &p.id == target)
            .ok_or(TurnError::NotFound)?;
        self.last_active = now;
        if self.state == RoomState::Active && self.current_player_id.as_ref() == Some(target) {
            // Target is currently active: set the flag, then advance past them.
            self.players[idx].skip_next = true;
            let next = self.advance_from(idx).ok_or(TurnError::NotFound)?;
            self.previous_player_id = self.current_player_id.take();
            self.current_player_id = Some(self.players[next].id.clone());
            self.turn_started_at = Some(now);
        } else {
            // Future skip only; no advance needed.
            self.players[idx].skip_next = true;
        }
        Ok(())
    }

    /// Host removes a player from the room and the rotation. If the removed
    /// player was current, the turn advances to the next eligible player first
    /// (computed before removal so order is preserved).
    ///
    /// # Errors
    /// `NotAuthorized` if `actor` is not the host; `NotFound` if `target` is
    /// not present.
    pub fn remove_player(
        &mut self,
        actor: &PlayerId,
        target: &PlayerId,
        now: Instant,
    ) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        let idx = self
            .players
            .iter()
            .position(|p| &p.id == target)
            .ok_or(TurnError::NotFound)?;

        let removing_current =
            self.state == RoomState::Active && self.current_player_id.as_ref() == Some(target);

        if removing_current {
            let next = self.advance_from(idx);
            self.current_player_id = next.map(|i| self.players[i].id.clone());
            // No successor means no turn in progress, so no clock to run.
            self.turn_started_at = self.current_player_id.as_ref().map(|_| now);
        }
        if self.previous_player_id.as_ref() == Some(target) {
            self.previous_player_id = None;
        }
        self.players.remove(idx);
        self.last_nudge_at.remove(target);
        self.last_active = now;
        Ok(())
    }

    /// Host reorders the rotation. `order` must be a permutation of exactly the
    /// current player ids (same set, no additions/removals). The current player
    /// pointer is preserved (it tracks by id, not position).
    ///
    /// # Errors
    /// `NotAuthorized` if not host; `NotFound` if `order` is not a permutation
    /// of the existing ids.
    pub fn set_order(
        &mut self,
        actor: &PlayerId,
        order: &[PlayerId],
        now: Instant,
    ) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        if order.len() != self.players.len() {
            return Err(TurnError::NotFound);
        }
        let all_present = order
            .iter()
            .all(|id| self.players.iter().any(|p| &p.id == id));
        if !all_present {
            return Err(TurnError::NotFound);
        }
        let mut reordered = Vec::with_capacity(self.players.len());
        for id in order {
            if let Some(pos) = self.players.iter().position(|p| &p.id == id) {
                reordered.push(self.players.remove(pos));
            }
        }
        self.players = reordered;
        self.last_active = now;
        Ok(())
    }

    /// Host toggles whether brand-new players may join. Allowed in any state.
    /// Does NOT affect reconnection by token — a dropped player can always
    /// return. Idempotent: setting the current value is a successful no-op.
    ///
    /// # Errors
    /// `NotAuthorized` if `actor` is not the host.
    pub fn set_locked(
        &mut self,
        actor: &PlayerId,
        locked: bool,
        now: Instant,
    ) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        self.locked = locked;
        self.last_active = now;
        Ok(())
    }

    /// A non-current player nudges the current player. Enforces a per-sender
    /// cooldown. Returns the current player's id (the nudge target) on success.
    ///
    /// # Errors
    /// `WrongState` if no active turn; `NotAuthorized` if the sender IS the
    /// current player; `NudgeOnCooldown` if the sender nudged within
    /// `NUDGE_COOLDOWN`.
    pub fn nudge(&mut self, sender: &PlayerId, now: Instant) -> Result<PlayerId, TurnError> {
        let current = self
            .current_player_id
            .clone()
            .ok_or(TurnError::WrongState)?;
        if &current == sender {
            return Err(TurnError::NotAuthorized);
        }
        if self
            .last_nudge_at
            .get(sender)
            .is_some_and(|&last| now.duration_since(last) < NUDGE_COOLDOWN)
        {
            return Err(TurnError::NudgeOnCooldown);
        }
        self.last_nudge_at.insert(sender.clone(), now);
        Ok(current)
    }

    /// True if the room has been inactive for at least `ttl`.
    #[must_use]
    pub fn is_expired(&self, now: Instant, ttl: Duration) -> bool {
        now.duration_since(self.last_active) >= ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    /// An Active two-player room (host is current) with the turn clock running.
    fn clock_room() -> (Room, PlayerId, PlayerId) {
        let (mut room, host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.attach_connection(&host);
        room.attach_connection(&bob);
        room.start_game(&host, t0()).unwrap();
        (room, host, bob)
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
    fn test_is_full_at_max_players() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        // Host counts as player #1; add up to MAX_PLAYERS.
        while room.players.len() < MAX_PLAYERS {
            room.add_player("P".into(), t0());
        }
        assert!(room.is_full());
        assert_eq!(room.players.len(), MAX_PLAYERS);
    }

    #[test]
    fn test_not_full_below_max_players() {
        let (room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert!(!room.is_full());
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
        assert!(room.start_game(&host_id, t0()).is_ok());
        assert_eq!(room.state, RoomState::Active);
        assert_eq!(room.current_player_id, Some(host_id));
    }

    #[test]
    fn test_start_game_twice_returns_wrong_state() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert_eq!(room.start_game(&host_id, t0()), Err(TurnError::WrongState));
    }

    #[test]
    fn test_start_game_by_non_host_is_unauthorized() {
        let (mut room, _host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        assert_eq!(room.start_game(&bob, t0()), Err(TurnError::NotAuthorized));
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

    // --- connection ownership (per-connection epoch) ---

    #[test]
    fn test_attach_connection_marks_connected_and_issues_a_fresh_epoch() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.players[0].connected = false;
        let first = room.attach_connection(&host_id);
        assert!(room.players[0].connected);
        assert_ne!(first, ConnEpoch::NONE, "NONE must never be issued");
        let second = room.attach_connection(&host_id);
        assert_ne!(first, second, "each attach gets a distinct epoch");
    }

    #[test]
    fn test_detach_by_the_owning_connection_clears_connected() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let epoch = room.attach_connection(&host_id);
        assert!(room.detach_connection(&host_id, epoch));
        assert!(!room.players[0].connected);
    }

    #[test]
    fn test_detach_by_a_superseded_connection_leaves_the_player_connected() {
        // The exact heartbeat race: connection A is half-open, the player
        // re-attaches on connection B, then A's cleanup finally fires.
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let conn_a = room.attach_connection(&host_id);
        let conn_b = room.attach_connection(&host_id);
        assert!(
            !room.detach_connection(&host_id, conn_a),
            "a superseded connection must report that it changed nothing"
        );
        assert!(
            room.players[0].connected,
            "the live connection B still owns the player"
        );
        assert!(room.detach_connection(&host_id, conn_b));
        assert!(!room.players[0].connected);
    }

    #[test]
    fn test_detach_of_an_unknown_player_is_a_no_op() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let epoch = room.attach_connection(&host_id);
        assert!(!room.detach_connection(&PlayerId("ghost".into()), epoch));
    }

    #[test]
    fn test_a_never_attached_player_cannot_be_detached_by_a_live_epoch() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        let host_epoch = room.attach_connection(&room.players[0].id.clone());
        assert!(
            !room.detach_connection(&bob, host_epoch),
            "Bob carries ConnEpoch::NONE and matches no issued epoch"
        );
    }

    // --- Task 7: end_turn + claim_turn ---

    #[test]
    fn test_end_turn_by_current_player_advances() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert!(room.end_turn(&host_id, t0()).is_ok());
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.previous_player_id, Some(host_id));
    }

    #[test]
    fn test_end_turn_by_non_current_non_host_is_unauthorized() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.add_player("Cara".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert_eq!(room.end_turn(&bob, t0()), Err(TurnError::NotAuthorized));
    }

    #[test]
    fn test_end_turn_in_lobby_is_wrong_state() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert_eq!(room.end_turn(&host_id, t0()), Err(TurnError::WrongState));
    }

    #[test]
    fn test_claim_turn_by_next_player_succeeds() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert!(room.claim_turn(&bob, t0()).is_ok());
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.previous_player_id, Some(host_id));
    }

    #[test]
    fn test_claim_turn_by_non_next_player_is_not_your_turn() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        let (cara, _ct) = room.add_player("Cara".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert_eq!(room.claim_turn(&cara, t0()), Err(TurnError::NotYourTurn));
    }

    // --- Task 8: undo_turn ---

    #[test]
    fn test_undo_turn_reverts_to_previous_player() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.end_turn(&host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
        room.undo_turn(&host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(host_id));
    }

    #[test]
    fn test_undo_turn_by_non_host_is_unauthorized() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.end_turn(&host_id, t0()).unwrap();
        assert_eq!(room.undo_turn(&bob, t0()), Err(TurnError::NotAuthorized));
    }

    #[test]
    fn test_undo_turn_with_no_previous_is_wrong_state() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        assert_eq!(room.undo_turn(&host_id, t0()), Err(TurnError::WrongState));
    }

    // --- Task 9: skip_player ---

    #[test]
    fn test_skip_player_flags_future_skip() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.skip_player(&host_id, &bob, t0()).unwrap();
        let bob_idx = room.players.iter().position(|p| p.id == bob).unwrap();
        assert!(room.players[bob_idx].skip_next);
    }

    #[test]
    fn test_skip_current_player_advances_immediately() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.skip_player(&host_id, &host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
    }

    #[test]
    fn test_skip_player_by_non_host_is_unauthorized() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        assert_eq!(
            room.skip_player(&bob, &bob, t0()),
            Err(TurnError::NotAuthorized)
        );
    }

    // --- Task 10: remove_player ---

    #[test]
    fn test_remove_player_drops_from_order() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.remove_player(&host_id, &bob, t0()).unwrap();
        assert_eq!(room.players.len(), 1);
        assert!(!room.players.iter().any(|p| p.id == bob));
    }

    #[test]
    fn test_remove_current_player_advances_turn() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.remove_player(&host_id, &host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.players.len(), 1);
    }

    #[test]
    fn test_remove_unknown_player_is_not_found() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert_eq!(
            room.remove_player(&host_id, &PlayerId("ghost".into()), t0()),
            Err(TurnError::NotFound)
        );
    }

    // --- Task 11: set_order ---

    #[test]
    fn test_set_order_reorders_players() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.set_order(&host_id, &[bob.clone(), host_id.clone()], t0())
            .unwrap();
        assert_eq!(room.players[0].id, bob);
        assert_eq!(room.players[1].id, host_id);
    }

    #[test]
    fn test_set_order_preserves_current_player_by_id() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(&host_id, t0()).unwrap();
        room.set_order(&host_id, &[bob, host_id.clone()], t0())
            .unwrap();
        assert_eq!(room.current_player_id, Some(host_id));
    }

    #[test]
    fn test_set_order_with_wrong_set_is_rejected() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        assert_eq!(
            room.set_order(&host_id, std::slice::from_ref(&host_id), t0()),
            Err(TurnError::NotFound)
        );
    }

    // --- set_locked ---

    #[test]
    fn test_set_locked_by_host_toggles_flag() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert!(!room.locked, "rooms start unlocked");
        room.set_locked(&host_id, true, t0()).unwrap();
        assert!(room.locked);
        room.set_locked(&host_id, false, t0()).unwrap();
        assert!(!room.locked);
    }

    #[test]
    fn test_set_locked_by_non_host_is_unauthorized() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        assert_eq!(
            room.set_locked(&bob, true, t0()),
            Err(TurnError::NotAuthorized)
        );
        assert!(!room.locked, "a rejected toggle must not change the flag");
    }

    // --- Task 12: nudge ---

    #[test]
    fn test_nudge_returns_current_player_as_target() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(&host_id, now).unwrap();
        assert_eq!(room.nudge(&bob, now), Ok(host_id));
    }

    #[test]
    fn test_current_player_cannot_nudge_self() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        room.add_player("Bob".into(), now);
        room.start_game(&host_id, now).unwrap();
        assert_eq!(room.nudge(&host_id, now), Err(TurnError::NotAuthorized));
    }

    #[test]
    fn test_nudge_within_cooldown_is_rejected() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(&host_id, now).unwrap();
        room.nudge(&bob, now).unwrap();
        let soon = now + Duration::from_secs(5);
        assert_eq!(room.nudge(&bob, soon), Err(TurnError::NudgeOnCooldown));
    }

    #[test]
    fn test_nudge_after_cooldown_is_allowed() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(&host_id, now).unwrap();
        room.nudge(&bob, now).unwrap();
        let later = now + Duration::from_secs(11);
        assert!(room.nudge(&bob, later).is_ok());
    }

    // --- Task 13: is_expired ---

    #[test]
    fn test_is_expired_after_ttl() {
        let now = t0();
        let (room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        assert!(!room.is_expired(now, Duration::from_mins(1)));
        let later = now + Duration::from_secs(61);
        assert!(room.is_expired(later, Duration::from_mins(1)));
    }

    // --- turn clock ---

    #[test]
    fn test_lobby_room_has_no_turn_start() {
        let (room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        assert!(
            room.turn_started_at.is_none(),
            "a lobby room has no turn in progress, so no clock"
        );
    }

    #[test]
    fn test_start_game_stamps_turn_start() {
        let (mut room, host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        let start = t0();
        room.start_game(&host, start).unwrap();
        assert_eq!(room.turn_started_at, Some(start));
    }

    #[test]
    fn test_end_turn_restamps_turn_start() {
        let (mut room, host, bob) = clock_room();
        let later = room.turn_started_at.unwrap() + Duration::from_secs(45);
        room.end_turn(&host, later).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(
            room.turn_started_at,
            Some(later),
            "the clock restarts for the player who just received the turn"
        );
    }

    #[test]
    fn test_claim_turn_restamps_turn_start() {
        let (mut room, _host, bob) = clock_room();
        let later = room.turn_started_at.unwrap() + Duration::from_secs(12);
        room.claim_turn(&bob, later).unwrap();
        assert_eq!(room.turn_started_at, Some(later));
    }

    #[test]
    fn test_skipping_the_current_player_restamps_turn_start() {
        let (mut room, host, bob) = clock_room();
        let later = room.turn_started_at.unwrap() + Duration::from_secs(30);
        room.skip_player(&host, &host, later).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.turn_started_at, Some(later));
    }

    #[test]
    fn test_skipping_a_future_player_leaves_turn_start_alone() {
        let (mut room, host, bob) = clock_room();
        let started = room.turn_started_at.unwrap();
        room.skip_player(&host, &bob, started + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            room.turn_started_at,
            Some(started),
            "flagging a future skip does not change whose turn it is, so the clock runs on"
        );
    }

    #[test]
    fn test_undo_turn_restamps_turn_start() {
        let (mut room, host, _bob) = clock_room();
        let ended = room.turn_started_at.unwrap() + Duration::from_secs(20);
        room.end_turn(&host, ended).unwrap();
        let undone = ended + Duration::from_secs(3);
        room.undo_turn(&host, undone).unwrap();
        assert_eq!(room.current_player_id, Some(host));
        assert_eq!(
            room.turn_started_at,
            Some(undone),
            "undo hands the turn back and the clock starts fresh"
        );
    }

    #[test]
    fn test_set_order_leaves_turn_start_alone() {
        let (mut room, host, bob) = clock_room();
        let started = room.turn_started_at.unwrap();
        let order = [bob, host.clone()];
        room.set_order(&host, &order, started + Duration::from_secs(9))
            .unwrap();
        assert_eq!(
            room.turn_started_at,
            Some(started),
            "reordering (and so shuffling) must not hand the current player a fresh clock"
        );
    }

    #[test]
    fn test_rejected_end_turn_leaves_turn_start_alone() {
        let (mut room, _host, bob) = clock_room();
        let started = room.turn_started_at.unwrap();
        // Bob is neither current nor host: rejected, and a rejection changes nothing.
        room.end_turn(&bob, started + Duration::from_secs(4))
            .unwrap_err();
        assert_eq!(room.turn_started_at, Some(started));
    }

    #[test]
    fn test_removing_the_current_player_restamps_turn_start() {
        let (mut room, host, bob) = clock_room();
        let later = room.turn_started_at.unwrap() + Duration::from_secs(8);
        room.remove_player(&host, &host, later).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.turn_started_at, Some(later));
    }

    #[test]
    fn test_removing_the_current_player_with_nobody_eligible_clears_turn_start() {
        let (mut room, host, bob) = clock_room();
        // Everyone has dropped off: `advance_from` finds no successor at all.
        room.detach_connection(&host, room.players[0].conn_epoch);
        room.detach_connection(&bob, room.players[1].conn_epoch);
        let later = room.turn_started_at.unwrap() + Duration::from_secs(8);
        room.remove_player(&host, &host, later).unwrap();
        assert!(room.current_player_id.is_none());
        assert!(
            room.turn_started_at.is_none(),
            "no current player means no turn to be clocking"
        );
    }
}
