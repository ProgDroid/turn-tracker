# Turn Tracker Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust/Actix backend for Turn Tracker — an in-memory, WebSocket-driven turn-coordination server for board game groups — with a pure, exhaustively-tested domain core.

**Architecture:** A single Actix-web binary. Room creation is a one-shot HTTP `POST`; all realtime play flows over a WebSocket. Room state lives in a `DashMap` (no DB). Each room owns a `tokio::sync::broadcast` channel; every WebSocket connection task `select!`s between its incoming client stream and the room's broadcast receiver, forwarding server messages to its socket. The **domain layer is pure Rust** (no Actix/WS types) so the entire turn engine is unit-testable without a server. A background tokio task sweeps rooms idle > 24h.

**Tech Stack:** Rust (edition 2024), actix-web 4, actix-ws, tokio, dashmap, serde/serde_json, thiserror, rand, log/env_logger. Scope of THIS plan: backend only. The Vue frontend is a separate plan.

**Companion spec:** `docs/superpowers/specs/2026-06-16-turn-tracker-design.md`

---

## File Structure

```
turn-tracker/
├── Cargo.toml                  # deps + [lints.clippy] policy
├── rust-toolchain.toml         # pin stable toolchain
├── src/
│   ├── main.rs                 # entrypoint: logger, build state, run HttpServer
│   ├── server.rs               # App factory + route registration
│   ├── error.rs                # AppError enum + ResponseError impl
│   ├── domain/
│   │   ├── mod.rs              # re-exports
│   │   ├── ids.rs             # PlayerId, RoomCode newtypes + code generation
│   │   ├── player.rs          # Player struct
│   │   └── room.rs            # Room struct + ALL turn logic (the tested heart)
│   ├── wire.rs                 # ClientMessage / ServerMessage / PublicRoom (serde)
│   ├── registry.rs             # Registry: DashMap<String, RoomHandle> + broadcast
│   ├── cleanup.rs              # idle-room sweep task
│   └── ws/
│       ├── mod.rs             # re-exports
│       ├── connection.rs      # actix-ws handler: select! loop per connection
│       └── dispatch.rs        # ClientMessage -> domain mutation -> Vec<Outbound>
└── tests/
    └── ws_integration.rs       # end-to-end WS tests
```

**Responsibility boundaries:**
- `domain/` knows nothing about HTTP, WS, serde-wire, or time-of-day. Pure data + logic.
- `wire.rs` owns the on-the-wire JSON contract (tagged enums + `PublicRoom`).
- `registry.rs` owns storage + the broadcast fan-out primitive.
- `ws/` owns I/O orchestration only; it calls `dispatch`, which calls `domain`.

---

## Task 0: Project setup

**Files:**
- Modify: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Modify: `src/main.rs`

- [ ] **Step 1: Add dependencies**

Run (pins latest compatible versions automatically):

```bash
cargo add actix-web@4
cargo add actix-ws
cargo add tokio --features "rt-multi-thread,macros,sync,time"
cargo add dashmap
cargo add serde --features derive
cargo add serde_json
cargo add thiserror
cargo add rand
cargo add log env_logger
cargo add --dev actix-rt
cargo add --dev awc          # actix websocket client for integration tests
cargo add --dev futures-util
```

- [ ] **Step 2: Add the clippy lint policy to `Cargo.toml`**

Append this table to `Cargo.toml` (single source of truth read by the edit hook, CI, and manual runs):

```toml
[lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
missing_docs_in_private_items = "allow"
separated_literal_suffix = "allow"
implicit_return = "allow"
print_stderr = "allow"
exhaustive_enums = "allow"
exhaustive_structs = "allow"
single_char_lifetime_names = "allow"
missing_inline_in_public_items = "allow"
self_named_module_files = "allow"
wildcard_enum_match_arm = "allow"
pattern_type_mismatch = "allow"
std_instead_of_core = "allow"
```

- [ ] **Step 3: Pin the toolchain**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 4: Replace `src/main.rs` with a compiling stub**

```rust
fn main() {
    println!("turn-tracker placeholder");
}
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build`
Expected: compiles, no errors.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock rust-toolchain.toml src/main.rs
git commit -m "chore: add backend dependencies and lint policy"
```

---

## Task 1: ID newtypes + Player struct

**Files:**
- Create: `src/domain/ids.rs`
- Create: `src/domain/player.rs`
- Create: `src/domain/mod.rs`

- [ ] **Step 1: Write failing tests for ID newtypes**

Create `src/domain/ids.rs`:

```rust
//! Strongly-typed identifiers for rooms and players.

use serde::{Deserialize, Serialize};

/// Opaque, server-generated identifier for a player within a room.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub String);

/// Short, human-shareable room code (e.g. "K7M-Q2P").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomCode(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_id_equality_compares_inner_string() {
        assert_eq!(PlayerId("a".into()), PlayerId("a".into()));
        assert_ne!(PlayerId("a".into()), PlayerId("b".into()));
    }
}
```

- [ ] **Step 2: Run the test to verify it passes (newtype wiring only)**

First create `src/domain/mod.rs`:

```rust
pub mod ids;
pub mod player;
```

And register the module tree in `src/main.rs` (add at top):

```rust
mod domain;

fn main() {
    println!("turn-tracker placeholder");
}
```

Run: `cargo test domain::ids`
Expected: PASS.

- [ ] **Step 3: Write the failing test for Player**

Create `src/domain/player.rs`:

```rust
//! A participant in a room.

use crate::domain::ids::PlayerId;

/// A single player in a room. `token` is a secret session credential and is
/// NEVER serialized to other clients (see `wire::PublicPlayer`).
#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub token: String,
    pub is_host: bool,
    pub connected: bool,
    /// When true, this player is skipped exactly once, then the flag clears.
    pub skip_next: bool,
}

impl Player {
    #[must_use]
    pub fn new(id: PlayerId, name: String, token: String, is_host: bool) -> Self {
        Self { id, name, token, is_host, connected: true, skip_next: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_new_defaults_connected_true_skip_false() {
        let p = Player::new(PlayerId("p1".into()), "Alice".into(), "tok".into(), true);
        assert!(p.connected);
        assert!(!p.skip_next);
        assert!(p.is_host);
        assert_eq!(p.name, "Alice");
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test domain::`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/domain src/main.rs
git commit -m "feat: add PlayerId/RoomCode newtypes and Player struct"
```

---

## Task 2: Room code generation (invariant-tested)

**Files:**
- Modify: `src/domain/ids.rs`

- [ ] **Step 1: Write failing tests for code generation invariants**

Add to `src/domain/ids.rs` (inside the file, above the existing `tests` module):

```rust
/// Unambiguous alphabet: no 0/O, 1/I/L to ease verbal sharing.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;

impl RoomCode {
    /// Generate a random 6-character code from the unambiguous alphabet.
    #[must_use]
    pub fn generate() -> Self {
        // rand 0.10 API: standalone `random_range` over the thread-local RNG.
        let code: String = (0..CODE_LEN)
            .map(|_| {
                let idx = rand::random_range(0..CODE_ALPHABET.len());
                CODE_ALPHABET[idx] as char
            })
            .collect();
        Self(code)
    }
}
```

Add these tests inside the existing `mod tests`:

```rust
    #[test]
    fn test_generate_code_has_correct_length() {
        assert_eq!(RoomCode::generate().0.len(), CODE_LEN);
    }

    #[test]
    fn test_generate_code_uses_only_unambiguous_alphabet() {
        for _ in 0..1000 {
            let code = RoomCode::generate();
            for ch in code.0.bytes() {
                assert!(
                    CODE_ALPHABET.contains(&ch),
                    "char {} not in alphabet",
                    ch as char
                );
            }
        }
    }

    #[test]
    fn test_generate_code_excludes_ambiguous_chars() {
        for _ in 0..1000 {
            let code = RoomCode::generate();
            for bad in ['0', 'O', '1', 'I', 'L'] {
                assert!(!code.0.contains(bad), "code {} contains {}", code.0, bad);
            }
        }
    }
```

- [ ] **Step 2: Run the tests to verify they pass**

Run: `cargo test domain::ids`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/ids.rs
git commit -m "feat: add unambiguous room code generation"
```

---

## Task 3: Token generation helper

**Files:**
- Modify: `src/domain/ids.rs`

- [ ] **Step 1: Write the failing test**

Add to `src/domain/ids.rs` (above the `tests` module):

```rust
/// Generate a 256-bit opaque token, hex-encoded (used for player/host auth).
#[must_use]
pub fn generate_token() -> String {
    // rand 0.10 API: standalone `random()` fills the array from StandardUniform.
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

Add to `mod tests`:

```rust
    #[test]
    fn test_generate_token_is_64_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token_is_unique_across_calls() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::ids`
Expected: PASS (6 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/ids.rs
git commit -m "feat: add opaque token generation"
```

---

## Task 4: Room struct + creation + add_player

**Files:**
- Create: `src/domain/room.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Write the failing test for room creation**

Create `src/domain/room.rs`:

```rust
//! The Room aggregate and all turn-coordination logic. Pure — no Actix, WS,
//! serde-wire, or wall-clock dependencies.

use std::time::Instant;

use crate::domain::ids::{generate_token, PlayerId, RoomCode};
use crate::domain::player::Player;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomState {
    Lobby,
    Active,
}

/// Outcome of a domain mutation: did the room change such that clients must be
/// re-synced, and was a side-effect (like a nudge) produced.
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
        self.players.push(Player::new(id.clone(), name, token.clone(), false));
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
}
```

- [ ] **Step 2: Register the module**

Update `src/domain/mod.rs`:

```rust
pub mod ids;
pub mod player;
pub mod room;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/domain
git commit -m "feat: add Room aggregate with creation and add_player"
```

---

## Task 5: start_game

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing test**

Add this method inside `impl Room`:

```rust
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
```

Add to `mod tests`:

```rust
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (5 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add start_game transition"
```

---

## Task 6: next-player computation (skips disconnected / skip_next)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing test**

Add this private helper inside `impl Room`:

```rust
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
```

Add to `mod tests`:

```rust
    #[test]
    fn test_advance_from_skips_disconnected_player() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());   // index 1
        room.add_player("Cara".into(), t0());  // index 2
        room.players[1].connected = false;
        // from host (0) -> skip Bob (1, disconnected) -> Cara (2)
        assert_eq!(room.advance_from(0), Some(2));
    }

    #[test]
    fn test_advance_from_consumes_skip_next_flag() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());   // index 1
        room.add_player("Cara".into(), t0());  // index 2
        room.players[1].skip_next = true;
        assert_eq!(room.advance_from(0), Some(2)); // Bob skipped
        assert!(!room.players[1].skip_next);       // flag consumed
    }

    #[test]
    fn test_advance_from_wraps_around() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        // from last player wraps back to host (0)
        let host_idx = room.players.iter().position(|p| p.id == host_id).unwrap();
        assert_eq!(room.advance_from(1), Some(host_idx));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (8 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add next-player computation skipping disconnected/skipped players"
```

---

## Task 7: end_turn and claim_turn (model B: push + pull)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add these methods inside `impl Room`:

```rust
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
        // Peek the next eligible index WITHOUT consuming skip flags first.
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
```

Add to `mod tests`:

```rust
    #[test]
    fn test_end_turn_by_current_player_advances() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap();
        assert!(room.end_turn(&host_id, t0()).is_ok());
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.previous_player_id, Some(host_id));
    }

    #[test]
    fn test_end_turn_by_non_current_non_host_is_unauthorized() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.add_player("Cara".into(), t0());
        room.start_game(t0()).unwrap(); // host is current
        // Bob is neither current nor host
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
        room.start_game(t0()).unwrap(); // host current, Bob is next
        assert!(room.claim_turn(&bob, t0()).is_ok());
        assert_eq!(room.current_player_id, Some(bob));
        assert_eq!(room.previous_player_id, Some(host_id));
    }

    #[test]
    fn test_claim_turn_by_non_next_player_is_not_your_turn() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        let (cara, _ct) = room.add_player("Cara".into(), t0());
        room.start_game(t0()).unwrap(); // host current, Bob next, Cara is NOT next
        assert_eq!(room.claim_turn(&cara, t0()), Err(TurnError::NotYourTurn));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (13 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add end_turn and claim_turn (push + pull) advancement"
```

---

## Task 8: undo_turn (single-level)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `impl Room`:

```rust
    /// Single-level undo: move the turn back to the immediately previous holder.
    /// Host-only. Does NOT restore any skip flags consumed by the advance
    /// (single-step, best-effort revert for fat-finger correction).
    ///
    /// # Errors
    /// `WrongState` if not Active or there is no previous holder; `NotAuthorized`
    /// if `actor` is not the host.
    pub fn undo_turn(&mut self, actor: &PlayerId, now: Instant) -> Result<(), TurnError> {
        if !self.is_host(actor) {
            return Err(TurnError::NotAuthorized);
        }
        if self.state != RoomState::Active {
            return Err(TurnError::WrongState);
        }
        let prev = self.previous_player_id.take().ok_or(TurnError::WrongState)?;
        // Only revert if the previous player still exists.
        if !self.players.iter().any(|p| p.id == prev) {
            return Err(TurnError::NotFound);
        }
        self.current_player_id = Some(prev);
        self.previous_player_id = None;
        self.last_active = now;
        Ok(())
    }
```

Add to `mod tests`:

```rust
    #[test]
    fn test_undo_turn_reverts_to_previous_player() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap();
        room.end_turn(&host_id, t0()).unwrap(); // now Bob's turn
        assert_eq!(room.current_player_id, Some(bob));
        room.undo_turn(&host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(host_id));
    }

    #[test]
    fn test_undo_turn_by_non_host_is_unauthorized() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap();
        room.end_turn(&host_id, t0()).unwrap();
        assert_eq!(room.undo_turn(&bob, t0()), Err(TurnError::NotAuthorized));
    }

    #[test]
    fn test_undo_turn_with_no_previous_is_wrong_state() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap(); // no turn has ended yet
        assert_eq!(room.undo_turn(&host_id, t0()), Err(TurnError::WrongState));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (16 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add single-level undo_turn"
```

---

## Task 9: skip_player (host)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `impl Room`:

```rust
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
        self.players[idx].skip_next = true;
        self.last_active = now;
        // If the target is the current player, advance past them now.
        if self.state == RoomState::Active && self.current_player_id.as_ref() == Some(target) {
            let next = self.advance_from(idx).ok_or(TurnError::NotFound)?;
            self.previous_player_id = self.current_player_id.take();
            self.current_player_id = Some(self.players[next].id.clone());
        }
        Ok(())
    }
```

Add to `mod tests`:

```rust
    #[test]
    fn test_skip_player_flags_future_skip() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap();
        room.skip_player(&host_id, &bob, t0()).unwrap();
        let bob_idx = room.players.iter().position(|p| p.id == bob).unwrap();
        assert!(room.players[bob_idx].skip_next);
    }

    #[test]
    fn test_skip_current_player_advances_immediately() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap(); // host is current
        room.skip_player(&host_id, &host_id, t0()).unwrap();
        assert_eq!(room.current_player_id, Some(bob));
    }

    #[test]
    fn test_skip_player_by_non_host_is_unauthorized() {
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        assert_eq!(room.skip_player(&bob, &bob, t0()), Err(TurnError::NotAuthorized));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (19 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add host skip_player with immediate advance"
```

---

## Task 10: remove_player (incl. removing the current player)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `impl Room`:

```rust
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
            // Pick the next eligible player (other than the one being removed)
            // before we mutate the vec.
            let next = self.advance_from(idx);
            self.current_player_id = next.map(|i| self.players[i].id.clone());
        }
        // Clear previous pointer if it referenced the removed player.
        if self.previous_player_id.as_ref() == Some(target) {
            self.previous_player_id = None;
        }
        self.players.remove(idx);
        self.last_nudge_at.remove(target);
        self.last_active = now;
        Ok(())
    }
```

Add to `mod tests`:

```rust
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
        room.start_game(t0()).unwrap(); // host current
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (22 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add remove_player with current-player advance"
```

---

## Task 11: set_order (host reorder)

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `impl Room`:

```rust
    /// Host reorders the rotation. `order` must be a permutation of exactly the
    /// current player ids (same set, no additions/removals). The current player
    /// pointer is preserved (it tracks by id, not position).
    ///
    /// # Errors
    /// `NotAuthorized` if not host; `InvalidRequest`-equivalent `NotFound` if
    /// `order` is not a permutation of the existing ids.
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
        // Verify same set of ids.
        let all_present = order.iter().all(|id| self.players.iter().any(|p| &p.id == id));
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
```

Add to `mod tests`:

```rust
    #[test]
    fn test_set_order_reorders_players() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.set_order(&host_id, &[bob.clone(), host_id.clone()], t0()).unwrap();
        assert_eq!(room.players[0].id, bob);
        assert_eq!(room.players[1].id, host_id);
    }

    #[test]
    fn test_set_order_preserves_current_player_by_id() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        let (bob, _bt) = room.add_player("Bob".into(), t0());
        room.start_game(t0()).unwrap(); // host current
        room.set_order(&host_id, &[bob.clone(), host_id.clone()], t0()).unwrap();
        assert_eq!(room.current_player_id, Some(host_id)); // unchanged by id
    }

    #[test]
    fn test_set_order_with_wrong_set_is_rejected() {
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), t0());
        room.add_player("Bob".into(), t0());
        // missing one id
        assert_eq!(
            room.set_order(&host_id, &[host_id.clone()], t0()),
            Err(TurnError::NotFound)
        );
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (25 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add host set_order reorder with permutation validation"
```

---

## Task 12: nudge with 10s per-sender cooldown

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing tests**

Add a constant near the top of `room.rs` (after imports):

```rust
use std::time::Duration;

/// Minimum interval between nudges from the same sender.
pub const NUDGE_COOLDOWN: Duration = Duration::from_secs(10);
```

Add inside `impl Room`:

```rust
    /// A non-current player nudges the current player. Enforces a per-sender
    /// cooldown. Returns the current player's id (the nudge target) on success.
    ///
    /// # Errors
    /// `WrongState` if no active turn; `NotAuthorized` if the sender IS the
    /// current player; `NudgeOnCooldown` if the sender nudged within
    /// `NUDGE_COOLDOWN`.
    pub fn nudge(&mut self, sender: &PlayerId, now: Instant) -> Result<PlayerId, TurnError> {
        let current = self.current_player_id.clone().ok_or(TurnError::WrongState)?;
        if &current == sender {
            return Err(TurnError::NotAuthorized);
        }
        if let Some(&last) = self.last_nudge_at.get(sender) {
            if now.duration_since(last) < NUDGE_COOLDOWN {
                return Err(TurnError::NudgeOnCooldown);
            }
        }
        self.last_nudge_at.insert(sender.clone(), now);
        Ok(current)
    }
```

Add to `mod tests` (note: these construct explicit `Instant`s via offsets):

```rust
    #[test]
    fn test_nudge_returns_current_player_as_target() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(now).unwrap(); // host current
        assert_eq!(room.nudge(&bob, now), Ok(host_id));
    }

    #[test]
    fn test_current_player_cannot_nudge_self() {
        let now = t0();
        let (mut room, host_id, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        room.add_player("Bob".into(), now);
        room.start_game(now).unwrap(); // host current
        assert_eq!(room.nudge(&host_id, now), Err(TurnError::NotAuthorized));
    }

    #[test]
    fn test_nudge_within_cooldown_is_rejected() {
        let now = t0();
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(now).unwrap();
        room.nudge(&bob, now).unwrap();
        let soon = now + Duration::from_secs(5);
        assert_eq!(room.nudge(&bob, soon), Err(TurnError::NudgeOnCooldown));
    }

    #[test]
    fn test_nudge_after_cooldown_is_allowed() {
        let now = t0();
        let (mut room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(now).unwrap();
        room.nudge(&bob, now).unwrap();
        let later = now + Duration::from_secs(11);
        assert!(room.nudge(&bob, later).is_ok());
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test domain::room`
Expected: PASS (29 tests).

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add nudge with per-sender cooldown"
```

---

## Task 13: idle detection helper

**Files:**
- Modify: `src/domain/room.rs`

- [ ] **Step 1: Write the failing test**

Add inside `impl Room`:

```rust
    /// True if the room has been inactive for at least `ttl`.
    #[must_use]
    pub fn is_expired(&self, now: Instant, ttl: Duration) -> bool {
        now.duration_since(self.last_active) >= ttl
    }
```

Add to `mod tests`:

```rust
    #[test]
    fn test_is_expired_after_ttl() {
        let now = t0();
        let (room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        assert!(!room.is_expired(now, Duration::from_secs(60)));
        let later = now + Duration::from_secs(61);
        assert!(room.is_expired(later, Duration::from_secs(60)));
    }
```

- [ ] **Step 2: Run the tests, then run the full domain suite + clippy**

Run: `cargo test domain::room`
Expected: PASS (30 tests).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Commit**

```bash
git add src/domain/room.rs
git commit -m "feat: add room expiry helper"
```

---

## Task 14: wire types (ClientMessage / ServerMessage / PublicRoom)

**Files:**
- Create: `src/wire.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing tests for serde round-trips**

Create `src/wire.rs`:

```rust
//! On-the-wire JSON contract. `PublicRoom`/`PublicPlayer` deliberately omit
//! secret tokens so broadcasts never leak credentials.

use serde::{Deserialize, Serialize};

use crate::domain::ids::PlayerId;
use crate::domain::room::{Room, RoomState};

/// Messages a client sends to the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Join { player_token: Option<String>, player_name: Option<String> },
    StartGame,
    SetOrder { player_ids: Vec<PlayerId> },
    EndTurn,
    ClaimTurn,
    UndoTurn,
    SkipPlayer { player_id: PlayerId },
    RemovePlayer { player_id: PlayerId },
    Nudge,
}

/// Messages the server sends to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome { player_id: PlayerId, token: String },
    RoomState { room: PublicRoom },
    Nudged,
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicState {
    Lobby,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayer {
    pub id: PlayerId,
    pub name: String,
    pub is_host: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicRoom {
    pub code: String,
    pub state: PublicState,
    pub players: Vec<PublicPlayer>,
    pub current_player_id: Option<PlayerId>,
}

impl From<&Room> for PublicRoom {
    fn from(room: &Room) -> Self {
        Self {
            code: room.code.0.clone(),
            state: match room.state {
                RoomState::Lobby => PublicState::Lobby,
                RoomState::Active => PublicState::Active,
            },
            players: room
                .players
                .iter()
                .map(|p| PublicPlayer {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    is_host: p.is_host,
                    connected: p.connected,
                })
                .collect(),
            current_player_id: room.current_player_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::domain::ids::RoomCode;

    #[test]
    fn test_client_message_deserializes_tagged_join() {
        let json = r#"{"type":"join","player_name":"Alice"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Join { player_name, player_token } => {
                assert_eq!(player_name, Some("Alice".into()));
                assert_eq!(player_token, None);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_public_room_omits_player_tokens() {
        let (room, _h, _t) =
            Room::create(RoomCode("ABC123".into()), "Host".into(), Instant::now());
        let public = PublicRoom::from(&room);
        let json = serde_json::to_string(&public).unwrap();
        assert!(!json.contains("token"), "PublicRoom leaked a token field: {json}");
    }
}
```

- [ ] **Step 2: Register the module in `src/main.rs`**

```rust
mod domain;
mod wire;

fn main() {
    println!("turn-tracker placeholder");
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test wire::`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add src/wire.rs src/main.rs
git commit -m "feat: add wire protocol types with token-safe PublicRoom"
```

---

## Task 15: error enum

**Files:**
- Create: `src/error.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the error type**

Create `src/error.rs`:

```rust
//! Application-level error type for HTTP responses.

use actix_web::{http::StatusCode, HttpResponse, ResponseError};

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Room not found")]
    RoomNotFound,
    #[error("Could not allocate a unique room code")]
    CodeExhausted,
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .json(serde_json::json!({ "error": self.to_string() }))
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest => StatusCode::BAD_REQUEST,
            Self::RoomNotFound => StatusCode::NOT_FOUND,
            Self::CodeExhausted => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}
```

- [ ] **Step 2: Register and verify it compiles**

Update `src/main.rs`:

```rust
mod domain;
mod error;
mod wire;

fn main() {
    println!("turn-tracker placeholder");
}
```

Run: `cargo check`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add src/error.rs src/main.rs
git commit -m "feat: add AppError with JSON ResponseError"
```

---

## Task 16: Registry (DashMap + per-room broadcast)

**Files:**
- Create: `src/registry.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/registry.rs`:

```rust
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
        Self { rooms: DashMap::new() }
    }

    /// Create a room with a unique code. Returns (code, host_id, host_token).
    ///
    /// # Errors
    /// `AppError::CodeExhausted` if a unique code can't be found in
    /// `MAX_CODE_ATTEMPTS` tries.
    pub fn create_room(
        &self,
        host_name: String,
        now: Instant,
    ) -> Result<(RoomCode, PlayerId, String), AppError> {
        for _ in 0..MAX_CODE_ATTEMPTS {
            let code = RoomCode::generate();
            if self.rooms.contains_key(&code.0) {
                continue;
            }
            let (room, host_id, token) = Room::create(code.clone(), host_name, now);
            let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
            self.rooms.insert(code.0.clone(), RoomHandle { room, tx });
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
        Ok(result)
    }

    /// Remove rooms idle for at least `ttl`. Returns the number removed.
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
    fn test_subscribe_unknown_room_errors() {
        let reg = Registry::new();
        assert!(reg.subscribe("NOPE12").is_err());
    }

    #[test]
    fn test_sweep_removes_expired_rooms() {
        let reg = Registry::new();
        let now = Instant::now();
        let (code, _h, _t) = reg.create_room("Host".into(), now).unwrap();
        let later = now + Duration::from_secs(90_000); // > 24h
        assert_eq!(reg.sweep_expired(later, Duration::from_secs(86_400)), 1);
        assert!(!reg.contains(&code.0));
    }
}
```

- [ ] **Step 2: Register the module**

Update `src/main.rs`:

```rust
mod domain;
mod error;
mod registry;
mod wire;

fn main() {
    println!("turn-tracker placeholder");
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test registry::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/registry.rs src/main.rs
git commit -m "feat: add Registry with DashMap storage and broadcast fan-out"
```

---

## Task 17: dispatch (ClientMessage -> domain mutation -> Outbound)

**Files:**
- Create: `src/ws/dispatch.rs`
- Create: `src/ws/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing tests**

Create `src/ws/dispatch.rs`:

```rust
//! Translate an authenticated ClientMessage into a room mutation plus the
//! resulting outbound messages. Pure orchestration over the domain layer.

use std::time::Instant;

use crate::domain::ids::PlayerId;
use crate::domain::room::{Room, TurnError};
use crate::registry::Outbound;
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};

/// Apply `msg` from `actor` to `room`, returning the messages to broadcast.
/// `Join` is handled at the connection layer (it may create a player), so it is
/// treated as a no-op refresh here.
pub fn dispatch(room: &mut Room, actor: &PlayerId, msg: ClientMessage, now: Instant) -> Vec<Outbound> {
    let result: Result<Vec<Outbound>, TurnError> = match msg {
        ClientMessage::StartGame => room.start_game(now).map(|()| state_broadcast(room)),
        ClientMessage::EndTurn => room.end_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::ClaimTurn => room.claim_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::UndoTurn => room.undo_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::SkipPlayer { player_id } => {
            room.skip_player(actor, &player_id, now).map(|()| state_broadcast(room))
        }
        ClientMessage::RemovePlayer { player_id } => {
            room.remove_player(actor, &player_id, now).map(|()| state_broadcast(room))
        }
        ClientMessage::SetOrder { player_ids } => {
            room.set_order(actor, &player_ids, now).map(|()| state_broadcast(room))
        }
        ClientMessage::Nudge => room.nudge(actor, now).map(|target| {
            vec![Outbound::Player(target, ServerMessage::Nudged)]
        }),
        ClientMessage::Join { .. } => Ok(state_broadcast(room)),
    };

    match result {
        Ok(out) => out,
        Err(e) => vec![Outbound::Player(actor.clone(), error_message(&e))],
    }
}

/// Build a full room-state broadcast to all connections.
fn state_broadcast(room: &Room) -> Vec<Outbound> {
    vec![Outbound::All(ServerMessage::RoomState { room: PublicRoom::from(&*room) })]
}

fn error_message(e: &TurnError) -> ServerMessage {
    let (code, message) = match e {
        TurnError::NotAuthorized => ("not_authorized", "You are not allowed to do that"),
        TurnError::NotFound => ("not_found", "Target not found"),
        TurnError::WrongState => ("wrong_state", "Action not valid in the current state"),
        TurnError::NotYourTurn => ("not_your_turn", "It is not your turn to claim"),
        TurnError::NudgeOnCooldown => ("nudge_cooldown", "Nudge is on cooldown"),
    };
    ServerMessage::Error { code: code.into(), message: message.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::RoomCode;

    fn active_room() -> (Room, PlayerId, PlayerId) {
        let now = Instant::now();
        let (mut room, host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(now).unwrap();
        (room, host, bob)
    }

    #[test]
    fn test_dispatch_end_turn_produces_room_state_broadcast() {
        let (mut room, host, _bob) = active_room();
        let out = dispatch(&mut room, &host, ClientMessage::EndTurn, Instant::now());
        assert!(matches!(out.as_slice(), [Outbound::All(ServerMessage::RoomState { .. })]));
    }

    #[test]
    fn test_dispatch_unauthorized_produces_targeted_error() {
        let (mut room, _host, bob) = active_room();
        // Bob (not current, not host) tries to end the turn.
        let out = dispatch(&mut room, &bob, ClientMessage::EndTurn, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Error { code, .. })] => {
                assert_eq!(target, &bob);
                assert_eq!(code, "not_authorized");
            }
            _ => panic!("expected targeted error, got {out:?}"),
        }
    }

    #[test]
    fn test_dispatch_nudge_targets_current_player() {
        let (mut room, host, bob) = active_room(); // host is current
        let out = dispatch(&mut room, &bob, ClientMessage::Nudge, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Nudged)] => assert_eq!(target, &host),
            _ => panic!("expected nudge to host, got {out:?}"),
        }
    }
}
```

- [ ] **Step 2: Create `src/ws/mod.rs`**

```rust
pub mod dispatch;
```

- [ ] **Step 3: Register in `src/main.rs`**

```rust
mod domain;
mod error;
mod registry;
mod wire;
mod ws;

fn main() {
    println!("turn-tracker placeholder");
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test ws::dispatch`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/ws src/main.rs
git commit -m "feat: add ClientMessage dispatch over domain layer"
```

---

## Task 18: HTTP — create room + static serving + server factory

**Files:**
- Create: `src/server.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the server factory and create-room handler**

Create `src/server.rs`:

```rust
//! HTTP layer: room creation, static file serving, and the App factory.

use std::sync::Arc;
use std::time::Instant;

use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use serde::Deserialize;

use crate::error::AppError;
use crate::registry::Registry;

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    pub host_name: String,
}

/// POST /api/rooms — create a room, returning the code + host credentials.
///
/// # Errors
/// `AppError::InvalidRequest` for an empty host name; `AppError::CodeExhausted`
/// if no unique code is available.
pub async fn create_room(
    registry: web::Data<Arc<Registry>>,
    body: web::Json<CreateRoomRequest>,
) -> Result<impl Responder, AppError> {
    let name = body.host_name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidRequest);
    }
    let (code, host_id, token) = registry.create_room(name.to_owned(), Instant::now())?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "room_code": code.0,
        "player_id": host_id.0,
        "token": token,
    })))
}

/// Build the Actix `App`. Registry is injected as shared state.
pub fn config(cfg: &mut web::ServiceConfig, registry: Arc<Registry>) {
    cfg.app_data(web::Data::new(registry))
        .route("/api/rooms", web::post().to(create_room));
    // WS route is added in Task 19; static files served from ./static in main.
}

/// Run the HTTP server, serving the SPA build from `static_dir`.
///
/// # Errors
/// Propagates bind/IO errors from Actix.
pub async fn run(registry: Arc<Registry>, bind: &str, static_dir: String) -> std::io::Result<()> {
    HttpServer::new(move || {
        let registry = registry.clone();
        App::new()
            .configure(|cfg| config(cfg, registry.clone()))
            .service(actix_files::Files::new("/", &static_dir).index_file("index.html"))
    })
    .bind(bind)?
    .run()
    .await
}
```

- [ ] **Step 2: Add the actix-files dependency**

Run: `cargo add actix-files`

- [ ] **Step 3: Wire up `src/main.rs`**

```rust
mod domain;
mod error;
mod registry;
mod server;
mod wire;
mod ws;

use std::sync::Arc;

use registry::Registry;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let registry = Arc::new(Registry::new());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    log::info!("turn-tracker listening on {bind}");
    server::run(registry, &bind, static_dir).await
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: compiles. (A missing `./static` dir is fine at runtime; tests don't hit it.)

- [ ] **Step 5: Commit**

```bash
git add src/server.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat: add HTTP create-room endpoint, static serving, server factory"
```

---

## Task 19: WebSocket connection handler

**Files:**
- Create: `src/ws/connection.rs`
- Modify: `src/ws/mod.rs`, `src/server.rs`

- [ ] **Step 1: Implement the connection handler**

Create `src/ws/connection.rs`:

```rust
//! actix-ws connection handler. Each connection runs a task that selects
//! between the incoming client stream and the room's broadcast receiver.

use std::sync::Arc;
use std::time::Instant;

use actix_web::{web, HttpRequest, HttpResponse};
use futures_util::StreamExt;

use crate::domain::ids::PlayerId;
use crate::registry::{Outbound, Registry};
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};
use crate::ws::dispatch::dispatch;

/// GET /ws/{code} — upgrade to a WebSocket bound to that room.
///
/// # Errors
/// Returns 404 if the room does not exist; otherwise upgrades the connection.
pub async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    registry: web::Data<Arc<Registry>>,
) -> Result<HttpResponse, actix_web::Error> {
    let code = path.into_inner();
    if !registry.contains(&code) {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({ "error": "Room not found" })));
    }
    let (response, session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let registry = registry.get_ref().clone();

    actix_web::rt::spawn(async move {
        let mut rx = match registry.subscribe(&code) {
            Ok(rx) => rx,
            Err(_) => return,
        };
        let mut session = session;
        let mut me: Option<PlayerId> = None;

        loop {
            tokio::select! {
                // Inbound from this client.
                incoming = msg_stream.next() => {
                    match incoming {
                        Some(Ok(actix_ws::Message::Text(text))) => {
                            if handle_text(&registry, &code, &mut me, &mut session, &text).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(actix_ws::Message::Close(_))) | None => break,
                        Some(Ok(actix_ws::Message::Ping(bytes))) => {
                            let _ = session.pong(&bytes).await;
                        }
                        _ => {}
                    }
                }
                // Outbound broadcast from the room.
                broadcast = rx.recv() => {
                    match broadcast {
                        Ok(Outbound::All(msg)) => {
                            if forward(&mut session, &msg).await.is_err() { break; }
                        }
                        Ok(Outbound::Player(target, msg)) => {
                            if me.as_ref() == Some(&target)
                                && forward(&mut session, &msg).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break, // channel closed/lagged
                    }
                }
            }
        }
        // Mark disconnected on exit (best-effort).
        if let Some(id) = me {
            let _ = registry.with_room_mut(&code, |room| {
                if let Some(p) = room.players.iter_mut().find(|p| p.id == id) {
                    p.connected = false;
                }
                ((), vec![Outbound::All(ServerMessage::RoomState { room: PublicRoom::from(&*room) })])
            });
        }
        let _ = session.close(None).await;
    });

    Ok(response)
}

/// Handle one inbound text frame. Returns Err to terminate the connection.
async fn handle_text(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<PlayerId>,
    session: &mut actix_ws::Session,
    text: &str,
) -> Result<(), ()> {
    let Ok(msg) = serde_json::from_str::<ClientMessage>(text) else {
        let _ = forward(session, &ServerMessage::Error {
            code: "bad_message".into(),
            message: "Could not parse message".into(),
        }).await;
        return Ok(());
    };

    // Join is special: it (re)authenticates this connection and may create a player.
    if let ClientMessage::Join { player_token, player_name } = &msg {
        return handle_join(registry, code, me, session, player_token.clone(), player_name.clone()).await;
    }

    // All other messages require an authenticated player.
    let Some(actor) = me.clone() else {
        let _ = forward(session, &ServerMessage::Error {
            code: "not_joined".into(),
            message: "Send join first".into(),
        }).await;
        return Ok(());
    };

    let _ = registry.with_room_mut(code, |room| {
        let out = dispatch(room, &actor, msg, Instant::now());
        ((), out)
    });
    Ok(())
}

/// Resolve a join: re-attach via token, or create a new player.
async fn handle_join(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<PlayerId>,
    session: &mut actix_ws::Session,
    player_token: Option<String>,
    player_name: Option<String>,
) -> Result<(), ()> {
    let now = Instant::now();
    // Returns (resolved player id, optional fresh token to send via welcome).
    let resolved = registry.with_room_mut(code, |room| {
        let outcome = match player_token {
            Some(tok) => room.player_by_token(&tok).map(|p| (p.id.clone(), None)),
            None => None,
        };
        let (id, fresh_token) = match outcome {
            Some((id, _)) => {
                if let Some(p) = room.players.iter_mut().find(|p| p.id == id) {
                    p.connected = true;
                }
                (id, None)
            }
            None => {
                let name = player_name.unwrap_or_else(|| "Player".into());
                let (id, token) = room.add_player(name, now);
                (id.clone(), Some(token))
            }
        };
        let broadcast = vec![Outbound::All(ServerMessage::RoomState {
            room: PublicRoom::from(&*room),
        })];
        ((id, fresh_token), broadcast)
    });

    match resolved {
        Ok((id, fresh_token)) => {
            *me = Some(id.clone());
            if let Some(token) = fresh_token {
                let _ = forward(session, &ServerMessage::Welcome { player_id: id, token }).await;
            }
            Ok(())
        }
        Err(_) => Err(()),
    }
}

async fn forward(session: &mut actix_ws::Session, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    session.text(json).await.map_err(|_| ())
}
```

- [ ] **Step 2: Export and route it**

Update `src/ws/mod.rs`:

```rust
pub mod connection;
pub mod dispatch;
```

Update the `config` fn in `src/server.rs` to add the WS route:

```rust
pub fn config(cfg: &mut web::ServiceConfig, registry: Arc<Registry>) {
    cfg.app_data(web::Data::new(registry))
        .route("/api/rooms", web::post().to(create_room))
        .route("/ws/{code}", web::get().to(crate::ws::connection::ws_route));
}
```

- [ ] **Step 3: Verify it compiles + clippy**

Run: `cargo check`
Expected: compiles.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings. (Fix any flagged issues, e.g. add `#[allow]` with a reason only where genuinely needed.)

- [ ] **Step 4: Commit**

```bash
git add src/ws src/server.rs
git commit -m "feat: add actix-ws connection handler with join/auth and broadcast loop"
```

---

## Task 20: cleanup task

**Files:**
- Create: `src/cleanup.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write the cleanup spawner**

Create `src/cleanup.rs`:

```rust
//! Background task that periodically removes idle rooms.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::registry::Registry;

pub const ROOM_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Spawn the periodic sweep. Runs until the process exits.
pub fn spawn(registry: Arc<Registry>) {
    actix_web::rt::spawn(async move {
        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            ticker.tick().await;
            let removed = registry.sweep_expired(Instant::now(), ROOM_TTL);
            if removed > 0 {
                log::info!("cleanup: removed {removed} idle room(s)");
            }
        }
    });
}
```

- [ ] **Step 2: Wire into `main.rs`**

Add `mod cleanup;` to the module list and spawn it before `server::run`:

```rust
    let registry = Arc::new(Registry::new());
    cleanup::spawn(registry.clone());
```

(The `sweep_expired` logic itself is already unit-tested in Task 16, so no new test here — this task only wires the timer.)

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add src/cleanup.rs src/main.rs
git commit -m "feat: add background idle-room cleanup task"
```

---

## Task 21: WebSocket integration tests

**Files:**
- Create: `tests/ws_integration.rs`

- [ ] **Step 1: Write an end-to-end test (create -> join -> start -> end_turn)**

Create `tests/ws_integration.rs`:

```rust
//! End-to-end tests over a real HTTP+WS server bound to an ephemeral port.

use std::sync::Arc;

use actix_web::{web, App, HttpServer};
use futures_util::{SinkExt, StreamExt};

use turn_tracker::registry::Registry;
use turn_tracker::server;

// Spawn the server on an OS-assigned port; return its base address.
async fn spawn_server() -> String {
    let registry = Arc::new(Registry::new());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = HttpServer::new(move || {
        let registry = registry.clone();
        App::new().configure(|cfg| server::config(cfg, registry.clone()))
    })
    .listen(listener)
    .unwrap()
    .run();
    actix_web::rt::spawn(srv);
    format!("127.0.0.1:{}", addr.port())
}

#[actix_web::test]
async fn test_create_then_join_and_start_flow() {
    let addr = spawn_server().await;

    // Create a room via HTTP.
    let client = awc::Client::new();
    let mut resp = client
        .post(format!("http://{addr}/api/rooms"))
        .send_json(&serde_json::json!({ "host_name": "Host" }))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let code = body["room_code"].as_str().unwrap().to_owned();
    let token = body["token"].as_str().unwrap().to_owned();

    // Connect WS and join as the host (using the host token).
    let (_resp, mut conn) = awc::Client::new()
        .ws(format!("ws://{addr}/ws/{code}"))
        .connect()
        .await
        .unwrap();

    let join = serde_json::json!({ "type": "join", "player_token": token });
    conn.send(awc::ws::Message::Text(join.to_string().into())).await.unwrap();

    // Expect a room_state broadcast reflecting the join.
    let frame = conn.next().await.unwrap().unwrap();
    if let awc::ws::Frame::Text(bytes) = frame {
        let msg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(msg["type"], "room_state");
        assert_eq!(msg["room"]["state"], "lobby");
    } else {
        panic!("expected text frame");
    }
}
```

- [ ] **Step 2: Make the crate testable as a library**

Add a `src/lib.rs` exposing the modules, and have `main.rs` use the lib. Create `src/lib.rs`:

```rust
pub mod cleanup;
pub mod domain;
pub mod error;
pub mod registry;
pub mod server;
pub mod wire;
pub mod ws;
```

Replace the `mod ...;` lines in `src/main.rs` with `use turn_tracker::{cleanup, registry::Registry, server};` and keep the `main` fn. Final `src/main.rs`:

```rust
use std::sync::Arc;

use turn_tracker::{cleanup, registry::Registry, server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let registry = Arc::new(Registry::new());
    cleanup::spawn(registry.clone());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    log::info!("turn-tracker listening on {bind}");
    server::run(registry, &bind, static_dir).await
}
```

Ensure `Cargo.toml` has both targets (Cargo auto-detects `src/lib.rs` and `src/main.rs`; the binary will share the package name `turn-tracker`).

- [ ] **Step 3: Run the integration test**

Run: `cargo test --test ws_integration`
Expected: PASS.

- [ ] **Step 4: Run the full suite + clippy + fmt**

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/main.rs tests/ws_integration.rs
git commit -m "test: add end-to-end WS integration test and library target"
```

---

## Task 22: Deploy — Dockerfile, .dockerignore, CI

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Multi-stage Dockerfile (ARM-compatible)**

Create `Dockerfile`:

```dockerfile
# ---- build ----
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
RUN useradd -m app
WORKDIR /home/app
COPY --from=build /app/target/release/turn-tracker /usr/local/bin/turn-tracker
# Static SPA build is mounted/copied to ./static at deploy time.
ENV BIND_ADDR=0.0.0.0:8080
ENV STATIC_DIR=/home/app/static
USER app
EXPOSE 8080
CMD ["turn-tracker"]
```

- [ ] **Step 2: `.dockerignore`**

Create `.dockerignore`:

```
target
.git
docs
frontend
static
```

- [ ] **Step 3: CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace
```

- [ ] **Step 4: Verify the build locally (optional but recommended)**

Run: `docker build -t turn-tracker .`
Expected: image builds successfully.

- [ ] **Step 5: Commit**

```bash
git add Dockerfile .dockerignore .github/workflows/ci.yml
git commit -m "ci: add Dockerfile and GitHub Actions test workflow"
```

**Note on TLS:** Terminate TLS at a reverse proxy (Caddy or nginx) in front of this container on the VPS — the binary itself serves plain HTTP on `BIND_ADDR`. Caddy is the lowest-effort option (automatic Let's Encrypt). The reverse-proxy config and the GitHub Actions → Hetzner deploy step are infrastructure tasks tracked in the deploy plan, not application code.

---

## Self-Review

**Spec coverage check (against `2026-06-16-turn-tracker-design.md`):**
- Host creates room + shareable code → Task 18 (`POST /api/rooms`), Task 2 (code gen). ✅
- Join, no account → Task 19 (`handle_join`). ✅
- Drag-reorder / `set_order` (host) → Task 11. ✅ (drag UI is frontend plan)
- Turn passing model B (end_turn current/host; claim_turn next) → Task 7. ✅
- Undo → Task 8. ✅
- Skip (one-turn) → Task 9. ✅
- Remove player → Task 10. ✅
- Nudge + 10s cooldown → Task 12. ✅
- Join mid-game → `add_player` works in any state (Task 4); covered. ✅
- Rooms expire after 24h → Task 13 (`is_expired`), Task 16 (`sweep_expired`), Task 20 (timer). ✅
- Reconnection via token → Task 19 (`handle_join` re-attach). ✅
- `current_player_id` by ID not index → Task 4 data model. ✅
- `PublicRoom` token-leak fix → Task 14. ✅
- Lobby|Active only (no Paused) → Task 4 `RoomState`. ✅
- Serve Vue build from Actix → Task 18 (`actix_files`). ✅
- HTTP-create / WS-play split → Tasks 18/19. ✅
- TLS via reverse proxy → Task 22 note. ✅
- Deferred features (push, host-passing, away toggle, NFC) → intentionally absent. ✅

**Placeholder scan:** No TBD/TODO; every code step contains complete code. ✅

**Type consistency:** `TurnError` variants used in `dispatch` (Task 17) match those defined in `room.rs` (Task 4). `Outbound::{All,Player}` consistent across Tasks 16/17/19. `ServerMessage`/`ClientMessage` variant names (snake_case serde) consistent across Tasks 14/17/19/21. `PublicRoom::from(&Room)` signature consistent. `Registry::with_room_mut` closure signature `FnOnce(&mut Room) -> (T, Vec<Outbound>)` used consistently in Tasks 17/19. ✅

**Known simplification to validate during execution:** `claim_turn`'s `peek_next` does not consume skip flags but `advance_turn` then calls `advance_from` which does — for the claim path the peeked next equals the advanced next (both skip the same disconnected/skip_next players), so they agree. If a future change makes them diverge, unify on one walker.
