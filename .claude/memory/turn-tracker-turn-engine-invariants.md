---
name: turn-tracker-turn-engine-invariants
description: "Invariants the turn rotation and turn clock depend on — read before touching src/domain/room.rs"
metadata:
  node_type: memory
  type: project
  originSessionId: ccf10503-d374-5f1e-80fc-8a568a795613
---

Companion to [[turn-tracker-ws-connection-invariants]], for the pure domain
layer. All of these were real bugs or deliberate decisions made 2026-09-17; each
is cheap to reintroduce by "tidying" `src/domain/room.rs`.

### 1. `advance_from` can return `from_index` ITSELF

The scan is `for offset in 1..=n`, so on the final offset it wraps a full lap
back to where it started. **That is correct for `end_turn`/`claim_turn`** — a
lone connected player keeps the turn rather than losing it — and `skip_player`
is immune by accident, because it sets `skip_next` on the target *before*
walking, so the wrap consumes the flag and moves on.

**It is wrong for any caller whose seat is disappearing.** `remove_player` used
to walk the pre-removal list, so removing the current player when nobody else
was eligible named the departing player as their own successor. The result was
not just a stale pointer: `current_index()` stops resolving, so `end_turn` and
`claim_turn` are refused as `WrongState` for everyone, no host action recovers
it, and the room is wedged until the 24h cleanup. `remove_player` now removes
first and resumes the walk one seat *before* the vacated index (so the player
who shifted into it is considered first).

**Before adding a caller, ask whether its starting seat survives the call.**

### 2. An Active room must never hold a turn nobody can advance

Two rules keep that true, and both look like over-engineering until you hit the
wedge above:

- With no *connected* successor, the turn **parks** on a remaining player
  (connected or not) instead of being dropped. `current_player_id` is `None`
  only when `players` is empty.
- Removing the host **promotes** a remaining player, preferring a connected one.
  A room with nobody flagged host can never again be skipped, reordered, undone
  or locked — and the host sheet's "Remove from room" targets the *current*
  player, so a host could do this to their own room in one tap.

### 3. Reordering must NOT restamp the turn clock

`turn_started_at: Option<Instant>` is stamped on every change of holder
(`start_game`, `advance_turn`, the `skip_player` immediate advance, `undo_turn`,
and the `remove_player` advance) — but deliberately **not** by `set_order`.
Shuffling mid-game would otherwise hand the current player a fresh clock. There
is a test pinning this.

### 4. Elapsed time goes on the wire as a COUNT, never a timestamp

`PublicRoom::new(room, now)` resolves `turn_elapsed_secs` at broadcast time and
the client extrapolates from it. Two phones with disagreeing clocks therefore
still show the same turn length, and a player who joins or reconnects mid-turn
sees the real elapsed time rather than zero. `From<&Room>` still exists for
callers with no `now` (the lagged-broadcast resync) and just supplies
`Instant::now()`.

Being an `Instant`, `turn_started_at` is **not persisted** — a restart restarts
the clock, consistent with the existing snapshot load policy for `created_at`
and `last_active`.

### 5. An empty room is CLOSED, not kept

`state_broadcast` sends `ServerMessage::RoomClosed` in place of a player-less
`RoomState`, and the connection layer then drops the room via
`Registry::remove_room`. Centralised in `state_broadcast` so every mutation that
can empty a room inherits it. Without this, removing the last player left an
Active, player-less room that clients rendered as a turn belonging to nobody.

**The trap worth remembering:** the removal MUST happen after `with_room_mut`
returns, never inside its closure — that call holds the room's DashMap shard
lock for the closure's duration, so removing from within deadlocks on the same
shard. It is safe there because `tokio::sync::broadcast` delivers whatever was
sent before the sender dropped, so `RoomClosed` still reaches every client and
the channel closing behind it ends their sockets. `remove_room` marks the
registry dirty, or the snapshot resurrects a closed room on restart.

**How to apply:** the guardrail tests live in `src/domain/room.rs`'s test module
— `test_removing_the_current_player_never_leaves_the_turn_on_them`,
`test_a_room_is_never_left_pointing_at_an_absent_current_player`,
`test_set_order_leaves_turn_start_alone`, and the `trio()` ordering tests that
pin the post-removal index arithmetic.
