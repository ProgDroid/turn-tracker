# Room State Persistence — Design

**Date:** 2026-06-17
**Status:** Approved (brainstorming) — pending implementation plan
**Component:** backend (`turn-tracker`, Rust / Actix)

## Problem

All room state lives in process memory: `Registry` holds a
`DashMap<String, RoomHandle>`, where each `RoomHandle` is a `Room` plus a
per-room `tokio::sync::broadcast::Sender`. When the server process stops — a
deploy, a reboot, or a crash — every live game is lost. Hosts must re-create
rooms and re-add players from scratch.

The goal is to persist room state across server restarts so that in-progress
games survive, and players can reconnect by token after the server comes back.

## Durability Model

**Periodic full-registry snapshot to disk + a final snapshot on graceful
shutdown.**

- Periodic: every `TT_SNAPSHOT_INTERVAL_SECS` (default 60), and only when state
  has changed since the last write.
- Shutdown: a final snapshot after the HTTP server has stopped and connections
  have drained.

This yields **zero loss on planned restarts/deploys** (the common case on a
single VPS) and **bounded, self-healing loss on the rare hard crash** (at most
the seconds since the last periodic write — and the turn pointer being off by
one or two is correctable in-app via the existing `end_turn` / `undo_turn`).

Rejected alternative — per-mutation writes (embedded KV such as `redb`): adds a
hard dependency and forces I/O (or a writer channel) into the synchronous
mutation path, to protect against a loss that is already trivial and
self-healing for this domain. Reconsider only if crashes prove frequent or the
data becomes something worth mourning.

**All disk I/O stays off the mutation hot path.** Mutations only flip an
in-memory dirty flag; a background task does the writing.

## Scope

In scope:

- Persist room state to disk and reload it at startup.
- A `Store` abstraction with a file-backed implementation.
- Background periodic writer + shutdown writer.
- Configuration via environment variables.

Out of scope (explicitly):

- Per-mutation durability / write-ahead logging.
- Horizontal scaling (sharding, Redis pub/sub). The `Store` trait is the seam
  that keeps that path open, but no scaling code is written now.
- Wall-clock preservation of room age / TTL across restarts (see Time Policy).

## Module Layout

New `src/persist/` module, mirroring the existing pure-domain / wire-DTO
separation so the domain stays serde-free:

- `persist/mod.rs` — the `Store` trait and `StoreError`.
- `persist/snapshot.rs` — serde DTOs (`RegistrySnapshot`, `RoomSnapshot`,
  `PlayerSnapshot`) and conversions to/from the domain types.
- `persist/file.rs` — `FileStore` (the only implementation for now).

```rust
pub trait Store: Send + Sync {
    fn save(&self, snap: &RegistrySnapshot) -> Result<(), StoreError>;
    fn load(&self) -> Result<RegistrySnapshot, StoreError>;
}
```

The trait is the boundary that keeps the future scale step (swap `FileStore`
for a Redis-backed `Store`) a contained change rather than surgery across the
codebase.

## Snapshot DTOs and Time Policy

The domain types (`Room`, `Player`) remain free of serde derives. Persistence
uses dedicated DTOs, consistent with how `wire::PublicRoom` / `PublicPlayer`
already separate wire types from the domain.

The DTOs **exclude every `std::time::Instant`** and exclude live runtime state.

Persisted per room:

- `code`
- `state` (`Lobby` / `Active`)
- `players`, in order (order **is** the turn order), each with:
  `id`, `name`, `token`, `is_host`, `skip_next`
- `current_player_id`
- `previous_player_id`

`token` is persisted because it is the reconnection credential
(`Room::player_by_token` → flip `connected = true`). See Security.

### Time policy on load

`Instant` is a monotonic clock reading with no absolute meaning across a process
restart and cannot be reconstructed. Rather than serialize wall-clock time, all
time-derived state is reset deterministically on load:

- `created_at` → `Instant::now()`
- `last_active` → `Instant::now()` (rooms get a fresh 24h idle TTL after a
  restart — harmless; documented)
- `last_nudge_at` → empty (a player may nudge slightly early once after a
  restart — trivial)
- every player's `connected` → `false` (no live sockets; clients reconnect by
  token and get flipped back to `true`)
- each room's `broadcast::Sender` → freshly created, empty

This means **no wall-clock serialization is needed at all** — the simplest
correct policy.

Upgrade path (not now): if TTL precision across restarts ever matters, persist
`last_active` as a Unix epoch and rebase against a startup `Instant` reference.

## Registry Changes

- Add `dirty: AtomicBool`, set to `true` in `create_room`, `with_room_mut`, and
  `sweep_expired`.
- Add `fn snapshot(&self) -> RegistrySnapshot` — iterates the `DashMap` and
  builds DTOs (briefly taking shard read locks).
- Add a constructor that rebuilds the `DashMap` from a `RegistrySnapshot`,
  applying the load-time policy above (fresh channels, `connected = false`,
  reset time fields, empty nudge map).

## Startup / Shutdown Wiring (`main.rs`)

```text
let store    = FileStore::new(path_from_env);
let registry = Arc::new(Registry::from_snapshot(store.load()?));   // empty on first run
let writer   = tokio::spawn(periodic_snapshot(registry.clone(), store.clone(), interval));
let srv      = HttpServer::new(...).run();
srv.await?;                            // returns on SIGTERM/SIGINT after connections drain
writer.abort();
store.save(&registry.snapshot())?;     // final, zero-loss save
```

- The periodic task: on each tick, check-and-clear `dirty`; if it was set, build
  a snapshot and write it via `tokio::task::spawn_blocking` (file I/O is
  blocking).
- The final save runs after `srv.await` returns, i.e. after handlers have
  finished — capturing fully-drained state.

## FileStore Behavior

- **Atomic write:** serialize JSON → write to `rooms.json.tmp` → fsync → rename
  over `rooms.json`. Atomic on the Linux deploy target; prevents torn files.
  *(Impl note: rename-over-existing is not atomic on Windows — acceptable, since
  the deploy target is Linux and tests use fresh temp dirs.)*
- **Load:**
  - missing file → empty registry (first boot)
  - parse error → rename the bad file to `rooms.json.corrupt`, log loudly, start
    fresh. The server must never fail to boot because of a bad snapshot.
- **Security:** the file contains player tokens (session secrets), so it is
  created with `0600` permissions, lives in a data directory that is **not**
  served by `actix-files`, and the data directory is git-ignored.

## Configuration

Environment variables, matching the existing config style:

- `TT_SNAPSHOT_PATH` — snapshot file path (default `./data/rooms.json`)
- `TT_SNAPSHOT_INTERVAL_SECS` — periodic interval in seconds (default `60`)

## Error Handling Summary

- Load failure (corrupt/unreadable): back up the file, start fresh, log — never
  block boot.
- Save failure (transient disk issue): log and keep running; the atomic
  temp+rename guarantees the previous good snapshot is never corrupted.

## Testing (TDD)

- **DTO round-trip:** domain → snapshot → JSON → snapshot → domain preserves
  player order, `current`/`previous` turn pointers, skip flags, tokens,
  `is_host`, room state.
- **Load policy:** post-load, every player `connected == false`, the nudge map
  is empty, and a room's fresh broadcast channel is usable.
- **FileStore:** save→load round-trip; corrupt file → fresh start + `.corrupt`
  backup; the `.tmp` file is cleaned up / replaced.
- **Regression:** the existing suite stays green; a room created → snapshot →
  rebuilt registry is still findable and reconnectable by token.

## Future Scaling Notes (informational)

The persistence design is the foundation of the first horizontal scale step,
not an obstacle to it:

- Tier 0 — vertical scale (bigger VPS, raise FD/`sysctl` limits). The
  human-paced protocol leaves large headroom here.
- Tier 1 — shard by room code (LB stickiness or hash `roomCode → node`). Each
  node runs today's in-process code unchanged and snapshots only the rooms it
  owns. This design drops straight in.
- Tier 2 — externalize state to Redis + pub/sub for full statelessness. A
  genuine rewrite behind the `Store` trait; only warranted at large scale or
  multi-region.

The trigger for Tier 1/2 is usually zero-downtime deploys or multi-region
latency, not raw CPU.
