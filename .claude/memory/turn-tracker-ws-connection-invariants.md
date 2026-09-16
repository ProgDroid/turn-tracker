---
name: turn-tracker-ws-connection-invariants
description: "Three invariants the WebSocket connection lifecycle depends on — read before touching src/ws/connection.rs or the heartbeat"
metadata:
  node_type: memory
  type: project
  originSessionId: 9ef2eb60-98fe-5a01-9457-2dd80719116a
---

**Supersedes the old `turn-tracker-stale-token-ghost` memory — that bug is FIXED
(2026-09-16).** Three related invariants now hold in `src/ws/connection.rs`. All
three were real bugs; each is cheap to reintroduce by "simplifying" the code.

### 1. An unknown player token is REJECTED, not minted

A `Join` carrying a non-empty token that matches nobody returns the wire error
`not_found`. It used to fall through to `add_player("")`, creating a blank-named
ghost. **Why `not_found` specifically:** the frontend already listens for it
(`room.ts:156`, gated on `joiningWithToken`) to clear the stale token and
re-prompt. Any other code silently degrades to a generic error.

Note the branch ORDER: the unknown-token check sits ahead of the locked/full
checks, so an unknown token against a locked room returns `not_found` rather than
`room_locked`. Intentional — "your identity is gone" is the more actionable
answer, and it drives the client down the clear-token path, after which a fresh
join correctly gets `room_locked`.

### 2. A connection owns at most one player, for its whole life

A second `Join` on an already-joined connection is refused with `wrong_state`.
Without that guard, `me` was overwritten and the end-of-loop cleanup could only
ever detach the LAST player held — stranding the earlier one at
`connected: true` with no owner. Because stranded players look connected,
`advance_from`/`peek_next` do NOT skip them, so turns land on a phantom nobody
can claim. The reachable path was a tokenless join (which mints a fresh player)
repeated up to `MAX_PLAYERS`, i.e. a trivial denial-of-play against any room code
a stranger could read over your shoulder.

A join that was REFUSED leaves `me` as `None`, so a legitimate retry still works.
There is a regression test pinning exactly that.

### 3. Only the OWNING connection may mark a player disconnected

`Player.conn_epoch` + `Room::attach_connection`/`detach_connection`. Cleanup
clears `connected` only if the departing connection still owns the player.

**The non-obvious part, and the reason this is worth reading:** the race
pre-dated the heartbeat and was harmless. A half-open socket lingered on TCP
timeouts for minutes-to-hours and essentially never fired mid-session. **Adding
the heartbeat converted it into a deterministic event ~120s after a drop —
reliably AFTER the client had already reconnected**, which is the worst possible
window. A reliability feature promoted a latent race into a live bug. Worth
remembering as a pattern: when adding a timeout to anything, ask what previously
never fired that now will.

Epochs are per-room, start at 1, only increment, and `ConnEpoch::NONE` (0) is
never issued — so a restored-from-snapshot player (which carries `NONE`) matches
no live connection. `conn_epoch`/`next_conn_epoch` are deliberately NOT in
`PlayerSnapshot`/`RoomSnapshot`; the DTOs are hand-built, so the on-disk JSON is
unchanged and old snapshots still load.

**How to apply:** before editing `handle_join`, `run_connection`, or the
heartbeat, re-read these three. The integration tests
(`test_join_with_unknown_token_is_rejected`,
`test_second_join_on_one_connection_is_rejected`,
`test_stale_connection_cleanup_does_not_disconnect_a_reattached_player`) are the
guardrails. `handle_join`'s `#[allow(clippy::too_many_lines)]` is load-bearing —
measured at 110 lines against a threshold of 100.

**Still open (pre-existing, not a regression):** a socket sending `join` twice
with DIFFERENT tokens strands the first player at `connected: true`. The SPA
never does this. Parked deliberately.
