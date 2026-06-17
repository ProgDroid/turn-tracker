# Room State Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist room state to disk so in-progress games survive server restarts, with players able to reconnect by token after the server comes back.

**Architecture:** A `persist` module defines a `Store` trait with a file-backed JSON implementation. The domain stays serde-free; persistence uses dedicated DTOs (`RegistrySnapshot`/`RoomSnapshot`/`PlayerSnapshot`) that exclude all `Instant`s and live runtime state. The `Registry` gains an `AtomicBool` dirty flag plus `snapshot()`/`from_snapshot()`. A background task writes a full snapshot periodically (only when dirty) and `main` writes a final snapshot on graceful shutdown. All disk I/O runs off the synchronous mutation path.

**Tech Stack:** Rust 2024, Actix-Web 4, `serde`/`serde_json` (already present), `tokio` (already present), `dashmap`, `thiserror`. **No new dependencies.**

## Global Constraints

- Rust edition **2024**; toolchain pinned by `rust-toolchain.toml`.
- **No new crate dependencies** — use `serde`, `serde_json`, `tokio`, `std::fs`, `thiserror`, all already in `Cargo.toml`.
- Clippy runs **pedantic + nursery at `-D warnings`** (canonical table in `Cargo.toml`). Every public function returning `Result` needs a `# Errors` doc section; add `#[must_use]` to pure getters; if `clippy::module_name_repetitions` fires on the snapshot DTOs, add `#![allow(clippy::module_name_repetitions)]` at the top of `src/persist/snapshot.rs`.
- Remediate lints with `cargo clippy --fix --allow-dirty --all-targets`; enforce with `cargo clippy --all-targets -- -D warnings`.
- **Make git commits through the Bash tool, not PowerShell** (PowerShell prepends a UTF-8 BOM to the commit subject).
- Dev machine is **Windows**; deploy target is **Linux**. Atomic file replace via rename is only atomic on Linux — the `FileStore` includes a Windows fallback so tests pass locally.
- Env var names are fixed: `TT_SNAPSHOT_PATH` (default `./data/rooms.json`), `TT_SNAPSHOT_INTERVAL_SECS` (default `60`).
- Run `cargo test` and `cargo build` after changes (vitest/vue-tsc are frontend-only and not relevant here).

## File Structure

- Create `src/persist/mod.rs` — `Store` trait, `StoreError`, module re-exports.
- Create `src/persist/snapshot.rs` — `RegistrySnapshot`, `RoomSnapshot`, `PlayerSnapshot`, `SnapshotState` DTOs + conversions to/from `Room`.
- Create `src/persist/file.rs` — `FileStore` (atomic write, corrupt-recovery load, `0600` perms).
- Create `src/persist/writer.rs` — `spawn_snapshotter` + the testable `snapshot_once` helper.
- Modify `src/lib.rs` — add `pub mod persist;`.
- Modify `src/registry.rs` — add `dirty: AtomicBool`, `mark_dirty`, `take_dirty`, `snapshot`, `from_snapshot`; mark dirty in `create_room`, `with_room_mut`, `sweep_expired`.
- Modify `src/main.rs` — load snapshot at startup, spawn snapshotter, final save on shutdown.
- Create `tests/persistence.rs` — lib-level full round-trip integration test.
- Modify `.gitignore` — ignore the data directory.

---

### Task 1: Snapshot DTOs and domain conversions

**Files:**
- Create: `src/persist/snapshot.rs`
- Create: `src/persist/mod.rs` (DTO re-exports only in this task; trait added in Task 2)
- Modify: `src/lib.rs` (add `pub mod persist;`)

**Interfaces:**
- Consumes: `crate::domain::room::{Room, RoomState}`, `crate::domain::player::Player`, `crate::domain::ids::{PlayerId, RoomCode}` (all fields are `pub`; `PlayerId`/`RoomCode` already derive `Serialize`/`Deserialize`).
- Produces:
  - `RegistrySnapshot { pub rooms: Vec<RoomSnapshot> }` (derives `Default`)
  - `RoomSnapshot { code: String, state: SnapshotState, players: Vec<PlayerSnapshot>, current_player_id: Option<PlayerId>, previous_player_id: Option<PlayerId> }`
  - `PlayerSnapshot { id: PlayerId, name: String, token: String, is_host: bool, skip_next: bool }`
  - `SnapshotState { Lobby, Active }`
  - `impl From<&Room> for RoomSnapshot`
  - `RoomSnapshot::into_room(self) -> Room` — resets `created_at`/`last_active` to `Instant::now()`, empties `last_nudge_at`, forces every player `connected = false`, preserves `skip_next`.

- [ ] **Step 1: Add the module to the crate**

In `src/lib.rs`, add after `pub mod error;`:

```rust
pub mod persist;
```

Create `src/persist/mod.rs`:

```rust
//! Durable room-state persistence: a `Store` trait and a file-backed snapshot.

pub mod snapshot;

pub use snapshot::{PlayerSnapshot, RegistrySnapshot, RoomSnapshot, SnapshotState};
```

- [ ] **Step 2: Write the failing test**

Create `src/persist/snapshot.rs` with only the tests first:

```rust
//! Serde DTOs for persisting room state. The domain types stay serde-free;
//! these mirror the wire-DTO pattern in `crate::wire`.
#![allow(clippy::module_name_repetitions)]

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

        assert!(rebuilt.players.iter().all(|p| !p.connected), "connected must reset to false");
        assert!(rebuilt.last_nudge_at.is_empty(), "nudge cooldowns must clear");
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
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib persist::snapshot`
Expected: FAIL — `RoomSnapshot` / `SnapshotState` not found (compile error).

- [ ] **Step 4: Write the DTOs and conversions**

At the top of `src/persist/snapshot.rs` (above the `#[cfg(test)]` block, below the `#![allow(...)]`):

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib persist::snapshot`
Expected: PASS (3 tests).

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/lib.rs src/persist/mod.rs src/persist/snapshot.rs
git commit -m "feat(persist): add snapshot DTOs and domain conversions"
```

---

### Task 2: Store trait and FileStore

**Files:**
- Modify: `src/persist/mod.rs` (add `Store` trait, `StoreError`, `file` module + re-export)
- Create: `src/persist/file.rs`

**Interfaces:**
- Consumes: `RegistrySnapshot` from Task 1.
- Produces:
  - `enum StoreError { Io(std::io::Error), Serde(serde_json::Error) }` (derives `thiserror::Error`)
  - `trait Store: Send + Sync { fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError>; fn load(&self) -> Result<RegistrySnapshot, StoreError>; }`
  - `struct FileStore { path: PathBuf }` (derives `Clone`); `FileStore::new(path: impl Into<PathBuf>) -> Self`
  - `FileStore::load` returns an empty `RegistrySnapshot` on a missing file and on a parse error (after renaming the bad file to `<path>.corrupt`).

- [ ] **Step 1: Add the trait and error to `src/persist/mod.rs`**

Replace the body of `src/persist/mod.rs` with:

```rust
//! Durable room-state persistence: a `Store` trait and a file-backed snapshot.

pub mod file;
pub mod snapshot;

pub use file::FileStore;
pub use snapshot::{PlayerSnapshot, RegistrySnapshot, RoomSnapshot, SnapshotState};

use thiserror::Error;

/// Errors raised while saving or loading a snapshot.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("snapshot io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// A backend that can persist and restore the full room registry.
pub trait Store: Send + Sync {
    /// Persist a snapshot.
    ///
    /// # Errors
    /// `StoreError::Io` on a filesystem failure; `StoreError::Serde` if the
    /// snapshot cannot be serialized.
    fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError>;

    /// Load the most recent snapshot, or an empty one if none exists.
    ///
    /// # Errors
    /// `StoreError::Io` on an unreadable file. A corrupt/unparseable file is
    /// NOT an error: it is backed up and an empty snapshot is returned.
    fn load(&self) -> Result<RegistrySnapshot, StoreError>;
}
```

- [ ] **Step 2: Write the failing test**

Create `src/persist/file.rs` with the tests first:

```rust
//! File-backed `Store`: atomic JSON snapshot with corrupt-file recovery.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::snapshot::{RoomSnapshot, SnapshotState};
    use crate::persist::Store;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("tt-persist-{}-{}.json", std::process::id(), tag))
    }

    fn sample() -> RegistrySnapshot {
        RegistrySnapshot {
            rooms: vec![RoomSnapshot {
                code: "ABC123".into(),
                state: SnapshotState::Lobby,
                players: vec![],
                current_player_id: None,
                previous_player_id: None,
            }],
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let store = FileStore::new(&path);
        store.save(&sample()).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.rooms.len(), 1);
        assert_eq!(loaded.rooms[0].code, "ABC123");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let path = tmp_path("missing");
        let _ = std::fs::remove_file(&path);
        let store = FileStore::new(&path);
        let loaded = store.load().unwrap();
        assert!(loaded.rooms.is_empty());
    }

    #[test]
    fn load_corrupt_file_backs_up_and_returns_empty() {
        let path = tmp_path("corrupt");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let store = FileStore::new(&path);
        let loaded = store.load().unwrap();
        assert!(loaded.rooms.is_empty());

        let mut corrupt = path.clone().into_os_string();
        corrupt.push(".corrupt");
        assert!(std::path::Path::new(&corrupt).exists(), "corrupt backup not created");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(std::path::PathBuf::from(corrupt));
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib persist::file`
Expected: FAIL — `FileStore` not found.

- [ ] **Step 4: Implement `FileStore`**

At the top of `src/persist/file.rs` (above the `#[cfg(test)]` block):

```rust
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::snapshot::RegistrySnapshot;
use super::{Store, StoreError};

/// Snapshot persisted as a single JSON file. Writes go to a temp file that is
/// fsync'd then renamed over the target (atomic on Linux) to avoid torn files.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

/// Append a suffix to a path's filename (e.g. `rooms.json` -> `rooms.json.tmp`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.to_path_buf().into_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

impl FileStore {
    /// Create a store backed by the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Store for FileStore {
    fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(snap)?;
        let tmp = with_suffix(&self.path, ".tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        restrict_permissions(&tmp)?;
        // Windows rename fails if the destination exists; remove first there.
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn load(&self) -> Result<RegistrySnapshot, StoreError> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RegistrySnapshot::default());
            }
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<RegistrySnapshot>(&bytes) {
            Ok(snap) => Ok(snap),
            Err(e) => {
                log::error!(
                    "snapshot parse failed ({e}); backing up to .corrupt and starting fresh"
                );
                let corrupt = with_suffix(&self.path, ".corrupt");
                let _ = fs::rename(&self.path, &corrupt);
                Ok(RegistrySnapshot::default())
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib persist::file`
Expected: PASS (3 tests).

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/persist/mod.rs src/persist/file.rs
git commit -m "feat(persist): add Store trait and atomic FileStore"
```

---

### Task 3: Registry persistence hooks

**Files:**
- Modify: `src/registry.rs`

**Interfaces:**
- Consumes: `RegistrySnapshot`, `RoomSnapshot` from Task 1; `BROADCAST_CAPACITY` and `RoomHandle` (same module).
- Produces (on `Registry`):
  - `fn snapshot(&self) -> RegistrySnapshot`
  - `fn from_snapshot(snap: RegistrySnapshot) -> Self` (rebuilds the `DashMap`, fresh broadcast channels)
  - `fn take_dirty(&self) -> bool` (check-and-clear)
  - private `fn mark_dirty(&self)`
  - `create_room`, `with_room_mut`, `sweep_expired` now call `mark_dirty` on success.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `src/registry.rs`:

```rust
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
                ((), vec![])
            })
            .unwrap();
        // subscribing works → a fresh channel was created
        assert!(rebuilt.subscribe(&code.0).is_ok());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib registry::`
Expected: FAIL — `take_dirty` / `snapshot` / `from_snapshot` not found.

- [ ] **Step 3: Add the dirty flag field and imports**

In `src/registry.rs`, update the imports near the top (after the existing `use` lines):

```rust
use std::sync::atomic::{AtomicBool, Ordering};

use crate::persist::snapshot::{RegistrySnapshot, RoomSnapshot};
```

Change the `Registry` struct and `new` to carry the flag:

```rust
#[derive(Default)]
pub struct Registry {
    rooms: DashMap<String, RoomHandle>,
    dirty: AtomicBool,
}
```

```rust
    #[must_use]
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
            dirty: AtomicBool::new(false),
        }
    }
```

- [ ] **Step 4: Add the persistence methods**

Add these methods inside `impl Registry` (place them after `new`):

```rust
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
```

- [ ] **Step 5: Mark dirty in the mutation funnel**

In `create_room`, immediately after the `self.rooms.insert(...)` line (before the `log::info!`):

```rust
            self.mark_dirty();
```

In `with_room_mut`, after the `for msg in outbound { ... }` loop and before `drop(handle);`:

```rust
        self.mark_dirty();
```

In `sweep_expired`, replace the final `expired.len()` return with:

```rust
        if !expired.is_empty() {
            self.mark_dirty();
        }
        expired.len()
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib registry::`
Expected: PASS (existing tests + 2 new).

- [ ] **Step 7: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/registry.rs
git commit -m "feat(persist): add registry snapshot/restore and dirty tracking"
```

---

### Task 4: Background snapshotter task

**Files:**
- Create: `src/persist/writer.rs`
- Modify: `src/persist/mod.rs` (add `pub mod writer;`)

**Interfaces:**
- Consumes: `Arc<Registry>` (`take_dirty`, `snapshot` from Task 3), `Arc<dyn Store>` (Task 2).
- Produces:
  - `async fn snapshot_once(registry: &Registry, store: &Arc<dyn Store>)` — writes a snapshot via `spawn_blocking` iff the registry is dirty; logs and swallows save errors.
  - `fn spawn_snapshotter(registry: Arc<Registry>, store: Arc<dyn Store>, interval: Duration) -> actix_web::rt::task::JoinHandle<()>` — ticks every `interval` and calls `snapshot_once`.

- [ ] **Step 1: Register the module**

In `src/persist/mod.rs`, add to the module declarations:

```rust
pub mod writer;
```

- [ ] **Step 2: Write the failing test**

Create `src/persist/writer.rs` with the tests first:

```rust
//! Background task that periodically snapshots the registry when it is dirty.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::{RegistrySnapshot, Store, StoreError};
    use crate::registry::Registry;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// In-memory store that records every save.
    struct MockStore {
        saves: Mutex<Vec<RegistrySnapshot>>,
    }

    impl Store for MockStore {
        fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError> {
            self.saves.lock().unwrap().push(snap.clone());
            Ok(())
        }
        fn load(&self) -> Result<RegistrySnapshot, StoreError> {
            Ok(RegistrySnapshot::default())
        }
    }

    #[actix_web::test]
    async fn snapshot_once_writes_only_when_dirty() {
        let registry = Arc::new(Registry::new());
        let mock = Arc::new(MockStore { saves: Mutex::new(vec![]) });
        let store: Arc<dyn Store> = mock.clone();

        // Clean registry → no save.
        snapshot_once(&registry, &store).await;
        assert_eq!(mock.saves.lock().unwrap().len(), 0);

        // Mutate → dirty → one save capturing the room.
        let (code, _h, _t) = registry.create_room("Host".into(), Instant::now()).unwrap();
        snapshot_once(&registry, &store).await;
        let saves = mock.saves.lock().unwrap();
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].rooms.len(), 1);
        assert_eq!(saves[0].rooms[0].code, code.0);

        drop(saves);
        // Flag cleared → no further save.
        snapshot_once(&registry, &store).await;
        assert_eq!(mock.saves.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib persist::writer`
Expected: FAIL — `snapshot_once` not found.

- [ ] **Step 4: Implement the snapshotter**

At the top of `src/persist/writer.rs` (above the `#[cfg(test)]` block):

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::registry::Registry;

use super::Store;

/// Write a snapshot iff the registry changed since the last write. Save errors
/// are logged and swallowed so a transient disk fault never crashes the server.
pub async fn snapshot_once(registry: &Registry, store: &Arc<dyn Store>) {
    if !registry.take_dirty() {
        return;
    }
    let snap = registry.snapshot();
    let store = store.clone();
    match tokio::task::spawn_blocking(move || store.save(&snap)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::error!("snapshot save failed: {e}"),
        Err(e) => log::error!("snapshot task join failed: {e}"),
    }
}

/// Spawn the periodic snapshotter. Returns the task handle so the caller can
/// abort it on shutdown before taking the final snapshot.
#[must_use]
pub fn spawn_snapshotter(
    registry: Arc<Registry>,
    store: Arc<dyn Store>,
    interval: Duration,
) -> actix_web::rt::task::JoinHandle<()> {
    actix_web::rt::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            snapshot_once(&registry, &store).await;
        }
    })
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib persist::writer`
Expected: PASS (1 test).

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/persist/mod.rs src/persist/writer.rs
git commit -m "feat(persist): add periodic snapshotter task"
```

---

### Task 5: End-to-end persistence integration test

**Files:**
- Create: `tests/persistence.rs`

**Interfaces:**
- Consumes the public lib API: `turn_tracker::persist::{FileStore, Store}`, `turn_tracker::registry::Registry`.

- [ ] **Step 1: Write the failing test**

Create `tests/persistence.rs`:

```rust
//! Full persistence pathway: save a live registry, reload it through the
//! FileStore, and confirm a player can reconnect by token.

use std::time::Instant;

use turn_tracker::persist::{FileStore, Store};
use turn_tracker::registry::Registry;

fn tmp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("tt-e2e-{}-{}.json", std::process::id(), tag))
}

#[test]
fn room_survives_save_and_reload_and_is_reconnectable() {
    let path = tmp_path("reload");
    let _ = std::fs::remove_file(&path);
    let store = FileStore::new(&path);

    // Build state and start a game so turn pointers are non-trivial.
    let original = Registry::new();
    let now = Instant::now();
    let (code, host_id, host_token) = original.create_room("Host".into(), now).unwrap();
    original
        .with_room_mut(&code.0, |room| {
            room.add_player("Bob".into(), now);
            room.start_game(&host_id, now).unwrap();
            ((), vec![])
        })
        .unwrap();

    // Persist, then reload into a brand-new registry.
    store.save(&original.snapshot()).unwrap();
    let restored = Registry::from_snapshot(store.load().unwrap());

    assert!(restored.contains(&code.0), "room must survive reload");
    restored
        .with_room_mut(&code.0, |room| {
            assert_eq!(room.players.len(), 2, "both players persisted");
            assert_eq!(room.current_player_id, Some(host_id.clone()), "turn pointer persisted");
            let host = room.player_by_token(&host_token).expect("host reconnectable by token");
            assert!(!host.connected, "players start disconnected after reload");
            ((), vec![])
        })
        .unwrap();

    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: Run test to verify it fails (or passes — see note)**

Run: `cargo test --test persistence`
Expected: PASS — every piece it exercises already exists (Tasks 1–3). This test is the regression guard for the wiring done in Task 6; if it fails, an earlier task is broken. (If you are practicing strict red-green, note that the behavior is already implemented; this test documents and locks the contract.)

- [ ] **Step 3: Commit**

```bash
cargo clippy --all-targets -- -D warnings
git add tests/persistence.rs
git commit -m "test(persist): add end-to-end save/reload integration test"
```

---

### Task 6: Wire persistence into `main.rs`

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `FileStore`, `Store`, `RegistrySnapshot`, `writer::spawn_snapshotter` (Tasks 1–4); `Registry::{from_snapshot, snapshot}` (Task 3); `server::run` (existing).

- [ ] **Step 1: Replace `src/main.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use turn_tracker::persist::{writer, FileStore, RegistrySnapshot, Store};
use turn_tracker::{cleanup, registry::Registry, server};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let snapshot_path =
        std::env::var("TT_SNAPSHOT_PATH").unwrap_or_else(|_| "./data/rooms.json".into());
    let interval_secs = std::env::var("TT_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);

    let store: Arc<dyn Store> = Arc::new(FileStore::new(snapshot_path));
    let snap = store.load().unwrap_or_else(|e| {
        log::error!("snapshot load failed: {e}; starting with no rooms");
        RegistrySnapshot::default()
    });
    let restored_rooms = snap.rooms.len();
    let registry = Arc::new(Registry::from_snapshot(snap));
    log::info!("restored {restored_rooms} room(s) from snapshot");

    cleanup::spawn(registry.clone());
    let snapshotter = writer::spawn_snapshotter(
        registry.clone(),
        store.clone(),
        Duration::from_secs(interval_secs),
    );

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into());
    if turn_tracker::ws::origin::allowed_origins_from_env().is_empty() {
        log::warn!(
            "ALLOWED_ORIGINS is unset/empty: WebSocket Origin checking is DISABLED (all origins allowed)"
        );
    }
    log::info!("turn-tracker listening on {bind}");

    // Runs until SIGTERM/SIGINT; Actix drains connections before returning.
    let result = server::run(registry.clone(), &bind, static_dir).await;

    // Graceful shutdown: stop the periodic writer, then take a final snapshot
    // so a planned restart loses nothing.
    snapshotter.abort();
    if let Err(e) = store.save(&registry.snapshot()) {
        log::error!("final snapshot save failed: {e}");
    } else {
        log::info!("final snapshot written on shutdown");
    }

    result
}
```

- [ ] **Step 2: Build and verify the full suite**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo test`
Expected: PASS — all lib, doc, and integration tests including `tests/persistence.rs`.

- [ ] **Step 3: Manual smoke test (Linux/WSL or local)**

```bash
TT_SNAPSHOT_PATH=./data/rooms.json TT_SNAPSHOT_INTERVAL_SECS=5 BIND_ADDR=127.0.0.1:8080 cargo run &
sleep 2
curl -s -X POST http://127.0.0.1:8080/api/rooms -H 'content-type: application/json' -d '{"host_name":"Alice"}'
sleep 6   # allow one periodic snapshot
test -f ./data/rooms.json && echo "SNAPSHOT WRITTEN"
grep -q room_code <(cat ./data/rooms.json) || grep -q '"code"' ./data/rooms.json && echo "ROOM PERSISTED"
kill %1   # SIGTERM → triggers final snapshot
```
Expected: `SNAPSHOT WRITTEN`, `ROOM PERSISTED`, and on the killed process a `final snapshot written on shutdown` log line.

- [ ] **Step 4: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs
git commit -m "feat(persist): load on startup, snapshot periodically and on shutdown"
```

---

### Task 7: Ignore the data directory and document configuration

**Files:**
- Modify: `.gitignore`
- Modify: `docs/superpowers/specs/2026-06-17-room-state-persistence-design.md` (add a short "Deployment" note) — or the project README if preferred.

**Interfaces:** none.

- [ ] **Step 1: Ignore the data directory**

Append to `.gitignore`:

```gitignore
/data/
```

- [ ] **Step 2: Document the deploy requirement**

Append this section to the design spec (`docs/superpowers/specs/2026-06-17-room-state-persistence-design.md`):

```markdown
## Deployment Note

The snapshot file holds player tokens, so it must be on persistent, private
storage. On the Hetzner VPS / Docker deploy:

- Set `TT_SNAPSHOT_PATH` to a path on a mounted volume (e.g. `/data/rooms.json`)
  so the snapshot survives container recreation.
- Ensure the process user can write that directory; the file is created `0600`.
- `TT_SNAPSHOT_INTERVAL_SECS` defaults to 60; tune per restart cadence.
- The data directory is git-ignored and must never be served by `actix-files`.
```

- [ ] **Step 3: Commit**

```bash
git add .gitignore docs/superpowers/specs/2026-06-17-room-state-persistence-design.md
git commit -m "chore(persist): git-ignore data dir and document deploy config"
```

---

## Self-Review

**Spec coverage:**

- Durability model (periodic + shutdown, off the hot path) → Tasks 4 (periodic via `snapshot_once`/`spawn_snapshotter`) + 6 (startup load + final save).
- Module layout (`persist/{mod,snapshot,file}.rs`) → Tasks 1, 2 (+ `writer.rs` in Task 4, an intentional addition for the background task glue).
- `Store` trait → Task 2.
- Snapshot DTOs + exclusion of `Instant`/runtime state → Task 1.
- Time/load policy (reset time, empty nudges, `connected = false`, fresh channel) → Task 1 (`into_room`) + Task 3 (`from_snapshot` rebuilds channels) — verified in tests in Tasks 1, 3, 5.
- Registry changes (dirty flag, `snapshot`, `from_snapshot`, mark-dirty in 3 sites) → Task 3.
- Startup/shutdown wiring → Task 6.
- FileStore behavior (atomic write, missing→empty, corrupt→backup+empty, `0600`, parent dir creation, Windows fallback) → Task 2.
- Configuration env vars → Task 6.
- Security (tokens persisted, `0600`, not served, git-ignored) → Tasks 2 (perms) + 7 (git-ignore, doc).
- Testing (DTO round-trip, load policy, FileStore cases, regression/reconnect) → Tasks 1, 2, 3, 5.

No spec requirement is left without a task.

**Placeholder scan:** No TBD/TODO/"handle edge cases"/"similar to Task N"; every code step shows complete code.

**Type consistency:** `Store::{save,load}`, `RegistrySnapshot`, `RoomSnapshot`, `PlayerSnapshot`, `SnapshotState`, `FileStore::new`, `Registry::{snapshot,from_snapshot,take_dirty,mark_dirty}`, `snapshot_once`, `spawn_snapshotter` are named identically everywhere they appear across tasks. `RoomSnapshot::into_room` and `From<&Room> for RoomSnapshot` are the single conversion pair used by both the registry and the writer.
