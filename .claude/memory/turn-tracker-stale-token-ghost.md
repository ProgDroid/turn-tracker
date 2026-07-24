---
name: turn-tracker-stale-token-ghost
description: "Latent backend bug: a Join with an unknown/stale token mints an EMPTY-named player instead of rejecting — relevant to deploy/snapshot resets"
metadata: 
  node_type: memory
  type: project
  originSessionId: 150ac4db-514f-4f99-a7d0-aa9cf6bdca23
---

**Latent backend bug (spotted 2026-06-18, NOT fixed).** In `handle_join` (`src/ws/connection.rs`): a Join carrying a non-empty `player_token` is treated as a rejoin (`is_rejoin = true` → `new_name = String::new()`). If that token matches **no** player in the room, the code falls through to the `else` branch and calls `room.add_player(new_name, …)` with the **empty string** — creating a player named `""` and sending it a Welcome, rather than rejecting or treating it as a fresh join.

**Why it matters for deploy:** the frontend only sends tokens it got from a Welcome (saved per-room), so it isn't hit in normal play. But after any event that **invalidates saved tokens while the room code still resolves** — a snapshot wipe/reset, data-dir migration, or a hand-typed/edited token — a returning client sends a stale token and silently becomes an empty-named ghost in the room. The server never emits `not_found` on join, so the client's stale-token recovery (`joiningWithToken` + `not_found` → clear token) never triggers for this path.

**How to apply:** before/around the deploy, decide the intended behavior for an unknown token on join — most likely reject (so the client clears the token and re-prompts for a name) rather than minting an empty player. Distinct from the phantom **"Player"** bug fixed this session (that was nameless+tokenless → default `"Player"`); this is non-empty-token-but-unknown → empty name.

See [[turn-tracker-status]].
