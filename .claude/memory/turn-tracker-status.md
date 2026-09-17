---
name: turn-tracker-status
description: "Turn Tracker project — backend MVP + Vue 3 frontend both built & merged to main; v1 follow-up gaps noted"
metadata: 
  node_type: memory
  type: project
  originSessionId: 86aa50b1-333c-404b-bee8-9bb1bf11a075
---

Turn Tracker: a no-install, Jackbox-style web app for tracking whose turn it is in physical board games — each player joins on their own phone via a shared code, real-time over WebSockets.

**Status (2026-06-16):** Backend MVP **complete and merged to `main`** (commit `faddd52`) — pure domain turn engine, `actix-ws` WebSocket server, in-memory `DashMap` rooms with a per-room `tokio::sync::broadcast` channel, 24h idle cleanup, Docker + GitHub Actions CI. 46 unit + 1 integration test, clippy pedantic+nursery clean. Remote: `github.com:ProgDroid/turn-tracker`.

**Frontend (2026-06-17):** the **Vue 3 + Vite + TS frontend is built and merged to `main`** (merge `1314604`) — lives in `frontend/`. Pinia `room` store as single source of truth, vue-router (history) + an Actix SPA-fallback (`spa_files` in `server.rs`), vue-i18n (en, all strings wrapped), device integrations (vibration/sound/wake-lock/QR/reduced-motion), token reconnection. 43 Vitest unit tests + 3 Playwright multi-player E2E (ran live). Stack majors are modern: Vite 8, Pinia 3, vue-router 5, vue-i18n 11, vitest 4, TS 6 (see package.json). Spec/plan: `docs/superpowers/specs/2026-06-16-frontend-vue-design.md` and `docs/superpowers/plans/2026-06-16-frontend-vue.md`.

**v1 follow-up gaps — frontend ones RESOLVED 2026-06-17 (session 3426f0ae):**
- ✅ **Reconnect-to-vanished-room loop** — fixed: `socket.ts` now caps reconnects at `MAX_RECONNECT_ATTEMPTS = 6` (~23s of backoff), then emits a terminal `'gone'` ConnStatus; `room.ts` `_setSocket` maps `'gone'` → `roomGone = true`, which `RoomView.vue:47` already routes to landing. (404 still happens at the WS handshake, so the cap — not a frame — is the trigger.) Tested in `socket.spec.ts` + `room.spec.ts`.
- ✅ **Drag-to-reorder** — fixed: wired `@dragstart/@dragover.prevent/@drop` on host `PlayerRow`s in `LobbySubview.vue` → local `dragIndex` ref → `store.setOrder(reordered ids)`. Tested in `LobbySubview.spec.ts`.
- ✅ **"Wait my turn" ghost button** (screen 07 Claim) — fixed: wired as a local dismiss (`claimDismissed` ref gates the claim branch; resets on each `current_player_id` change). No backend. Tested in `ActiveSubview.spec.ts`.
- ✅ **Cleanup hygiene** — consolidated dual vite/client decl into one `src/env.d.ts` (deleted root `env.d.ts` + `src/vite-env.d.ts`); added `strict` to `tsconfig.node.json`; added LandingView join + create-error unit tests. Frontend suite now 49/49, build clean.
- ✅ **Server-side durability DONE (2026-06-17, session 533c8ae8, pushed to `main` `5263908`..`f16dd12`)** — snapshot-to-disk persistence: periodic full-registry JSON snapshot (only when dirty) + a final snapshot on graceful shutdown, reloaded at startup. New `src/persist/` module (`Store` trait + `FileStore`: atomic temp+fsync+rename, corrupt-file→`.corrupt`-backup recovery, `0600`); `Registry` gained a `dirty: AtomicBool` + `snapshot`/`from_snapshot`; background snapshotter off the mutation hot path. Env: `TT_SNAPSHOT_PATH` (default `./data/rooms.json`), `TT_SNAPSHOT_INTERVAL_SECS` (default 60). Suite 79, clippy clean. Spec/plan: `docs/superpowers/specs/2026-06-17-room-state-persistence-design.md`, `docs/superpowers/plans/2026-06-17-room-state-persistence.md`. **Two non-obvious gotchas (read before touching this code):** (1) `std::time::Instant` can't survive a process restart, so the load policy deliberately resets `created_at`/`last_active` to now, empties `last_nudge_at`, and forces every player `connected=false` (clients reconnect by token) — NO wall-clock is serialized; (2) the shutdown writer uses a `tokio::sync::Notify` (NOT `JoinHandle::abort()`) because `abort()` does NOT cancel an in-flight `spawn_blocking` save — that bug would race the final save on the shared `rooms.json.tmp`.
- ⚠️ **Deploy-side persistence TODO:** point `TT_SNAPSHOT_PATH` at a mounted-volume path on the Hetzner box (else container recreation wipes it). The graceful-shutdown final save was verified by code only — it can't be smoke-tested on Windows (SIGTERM hard-kills before Actix drains); it fires on the Linux deploy target.

**Fidelity + hardening pass (2026-06-17, session 3426f0ae) — DONE, merged to `main` (`ad16619` frontend, `c7ad40c` backend), CI green.** Ran a 4-agent audit (design tokens/screens, component states/motion/a11y, backend security/ops, i18n) then fixed everything the user approved:
- Frontend: mounted the orphaned `HostSheet` via a host-only corner button + added `ReorderSheet` (set_order works mid-game — no state guard at room.rs:297); rendered screen-06 "Up next" strip; wired nudge→emblem 1.6s speed-up; hero radial-wipe enter anim (reduced-motion crossfade); pruned dead `useReducedMotion` + 4 i18n keys; misc fidelity polish. vitest 49→61, vue-tsc build green.
- Backend: `GET /health`; input caps `MAX_NAME_LEN=40` / `MAX_PLAYERS=16` / `MAX_ROOMS=10_000` (503 at cap); token-free connect/disconnect/join logging; **new env `ALLOWED_ORIGINS`** (comma-sep WS Origin allowlist; **unset = allow-all + startup warn**, so dev/E2E unaffected — but **MUST be set in prod**). New wire error codes **`room_full`** + **`bad_name`** (mapped frontend).

**Deep-dive review hardening (2026-06-18, session 7-findings) — DONE, pushed to `main` (`4533754` backend, `350a168` frontend, `aaef935` docs).** A review surfaced ~7 threads; all addressed + tested (backend 95 tests, frontend 78, clippy/fmt/build green):
- **Reconnect-rejoin bug (the one real functional bug):** `RoomSocket` reconnects internally on a blip but the store never re-sent `join`, so the fresh server-side connection stayed `me=None` and every action failed `not_joined` until a full page reload. Fix: `room.ts` `_setSocket` re-sends `join` on every `'open'` **once `me` is set** (guard avoids double-join on the view-driven first connect).
- **Lagged broadcast:** a client >64 msgs behind was force-disconnected (`Err(_) => break`). Now `Lagged` resyncs via new read-only `Registry::public_room`; only `Closed` ends the socket.
- **Rate limiting:** `POST /api/rooms` is now per-client-IP via **`actix-governor` 0.10** (burst 10, 20/min). Custom `RealIpKeyExtractor` reads XFF (no built-in `SmartIpKeyExtractor` in 0.10). **Trust boundary:** container must NOT be exposed directly + proxy MUST set XFF — see `DEPLOY.md`. (Gotcha: don't impl the `KeyExtractor` `name`/`key_name` methods unless you enable the crate's `log` feature — they're `#[cfg(feature="log")]` and trip `unexpected_cfgs`.)
- **Snapshot write-amplification:** `with_room_mut` now takes an explicit `dirty: bool`; rejected actions / nudges / rejoins / disconnects (none persisted) no longer mark dirty.
- **Single-instance guard:** startup takes an exclusive OS advisory lock on `<data-dir>/.lock` (`src/persist/lock.rs`), so a 2nd instance refuses to start. **Big gotcha: Rust std has NATIVE file locking since 1.89 (`File::try_lock` → `Result<(), std::fs::TryLockError>` with `WouldBlock`/`Error(io)` variants); toolchain here is stable 1.95 — do NOT add `fs4`/`fs2`.** (I added fs4, clippy flagged the trait import as unused → std had shadowed it → removed the dep.)
- **Host room-lock (new feature; user chose host-toggleable over hard-block-on-Active):** `Room.locked` + `SetLocked` msg + host-only `set_locked`; a locked room turns away NEW joiners but **token rejoin always works**. Persisted (`#[serde(default)]` for old snapshots). UI: lobby toggle + HostSheet item + a "Can't join" rejection screen so a refused joiner isn't stuck on a spinner.
- **Security headers:** app-wide `DefaultHeaders` — CSP tuned to the Vite build (external-only scripts ⇒ `script-src 'self'`; `style-src 'unsafe-inline'` for Vue's runtime inline styles; `connect-src 'self'` covers same-origin wss) + nosniff/DENY/no-referrer; Dockerfile builds `--locked`.
- Added **`DEPLOY.md`** operator runbook (env vars + the proxy/single-instance/origin requirements).
- **`undo_turn` lossiness deliberately left as-is (user agreed):** single-level undo doesn't restore consumed `skip_next` flags — low blast radius; a lossless fix needs a transition-delta journal that isn't worth the complexity for a casual game.

**Final pre-deploy manual-test round (2026-06-18, session 533c8ae8) — pushed to `main` (`16d531e` fix, `99aea7a` feat).** A live test surfaced/polished three things:
- **Phantom default "Player" on join (real bug, fixed):** the auto-rejoin from `4533754` (line 29 above — `_setSocket` re-sends `join` on every `'open'` once `me` is set) fired across an in-app room switch because `connect()` never reset the store's `me`/`room`; with no token for the new room it sent `{type:'join'}` (no name, no token) → server minted a default-named **"Player"**, then the real named join followed → 2 users became 3 players. Fix: `connect()` now clears `me`/`room` on room entry, and the auto-rejoin is **token-gated** (only fires when a saved token exists — rejoin is always token-based). +2 regression tests in `room.spec.ts`.
- **Drag-to-reorder animation:** lifted row + other rows glide apart to reveal the drop slot (`DraggablePlayerList.vue`); live target + drop both resolve against rects captured at drag-start.
- **QR button toggles label** Show QR ⇄ Hide QR (new `lobby.hideQr`).
- Also spotted a separate **unfixed** latent backend bug → [[turn-tracker-stale-token-ghost]].

✅ **DEPLOYED AND LIVE (2026-09-16) at https://whosego.app — the deploy/CD and
live-wss workstreams are DONE.** Runs on **Fly.io**, not Hetzner (see
[[user-profile]] — the old "ships to a Hetzner CAX11" note was wrong and cost a
design cycle): one Machine in `lhr`, `shared-cpu-1x`/256mb, a 1 GB volume at
`/data`, ~$2.20/mo. Config is `fly.toml` at the repo root. CD is
`.github/workflows/ci.yml` — test/frontend/docker then `flyctl deploy
--remote-only --strategy immediate`, gated to pushes on `main`.

**Fly invariants that are correctness requirements, not preferences:** exactly
ONE Machine (each gets its own volume, so two would serve different rooms under
the same codes — hence `--strategy immediate`, `auto_stop_machines = false`,
`min_machines_running = 1`); `kill_timeout = "30s"` must stay TOP-LEVEL in the
TOML or it silently nests and the app is killed mid-drain, losing the final
snapshot; `TT_CLIENT_IP_HEADER=Fly-Client-IP` because X-Forwarded-For is
client-spoofable on Fly — unset it if ever moving to the VPS path, where Caddy
overwrites XFF and trusting a platform header would be the same hole reversed.

**The VPS path is preserved, not deleted** — `deploy/vps/` plus
`deploy/README.md` document swapping back. Same Dockerfile and binary; only
infrastructure differs.

**Shipped in the pre-deploy round (2026-09-16):** unknown-token rejection,
server-initiated WebSocket heartbeat (30s ping, closes at 90-120s), the
`TT_CREATE_TOKEN` closed-beta gate on room creation only (joining is never
gated), the SPA `?k=` token capture, and a Critical connection-ownership fix —
see [[turn-tracker-ws-connection-invariants]] before touching
`src/ws/connection.rs`. Suite: 114 lib + 1 persistence + 12 integration + 89
frontend.

**Remaining:** the live phone tests (Task 4 of
`docs/superpowers/plans/2026-09-16-deploy-fly.md`) — specifically the 5-minute
backgrounding test for the heartbeat and the aeroplane-mode reconnect test for
the epoch fix, both needing two phones on mobile data with wifi OFF. Then
`flyctl secrets unset TT_CREATE_TOKEN` to open it to everyone.

**Turn clock + shuffle, and three bug classes behind them (2026-09-17, session
`ccf10503`) — branch `claude/timer-shuffle-players-wwg967`, 5 commits, merged to
`main`.** Prompted by a friend's playtest ("needs a timer, could use a shuffle").
Suite now 140 Rust / 136 vitest / 5 Playwright E2E; clippy, fmt, vue-tsc clean.
- **Turn clock** — server stamps `turn_started_at` and broadcasts
  `turn_elapsed_secs`; client extrapolates. Count-up only. **A per-turn time
  LIMIT was deliberately deferred** (user's call): the stopwatch is descriptive,
  a limit is normative and the right value is unknown until real games are
  watched. The server-side stamp is the only piece a countdown needs, so the
  follow-up is a room field + a host setting + colouring the existing number.
  See [[turn-tracker-turn-engine-invariants]].
- **Shuffle** — Fisher-Yates on the CLIENT, riding the existing `set_order`
  message. No new wire variant and no RNG in the pure domain: the host can
  already drag the list into any order, so a server-side draw protects nothing.
- **`remove_player` could wedge a room** (pre-existing, found in passing). Root
  cause and the two follow-on rules are in
  [[turn-tracker-turn-engine-invariants]] — read that before touching
  `src/domain/room.rs`.
- **i18n error path** — `RoomView` rendered the server's raw English, and
  `errorKey`'s hand-kept code list had drifted (`room_locked` was translated but
  unreachable). The store now keeps the CODE (`joinRejectedCode`), and
  `errorKey` tests `en.errors` itself rather than a list. **Not `te()`** — it
  only consults the ACTIVE locale, so once `pt` exists a code it had not covered
  would fall to the generic message instead of vue-i18n's own fallback to
  English. A drift test now pins every actionable code to its own message.
  Note the server's code set spans BOTH `ws/dispatch.rs` and `ws/connection.rs`
  (an audit that scanned only the former missed three).
- **E2E suite was silently broken** and is repaired — see [[turn-tracker-run]].
  It caught a gap unit tests missed: the claim screen had no clock.
- **Still not done: the live two-phone test.** The E2E proves three browser
  clients agree within a tick, including one joining mid-turn, but it cannot
  prove it on two real phones on mobile data.
