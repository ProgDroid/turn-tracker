---
name: turn-tracker-run
description: How to run/reboot Turn Tracker locally for manual testing (single Actix binary serves the SPA) + Windows rebuild gotchas
metadata: 
  node_type: memory
  type: project
  originSessionId: 150ac4db-514f-4f99-a7d0-aa9cf6bdca23
---

Local run/test recipe (there is **no run script, no `.env`, no vite dev server** in normal use — one Actix binary serves the SPA).

**Run command** (bind `0.0.0.0` so phones on the LAN can hit it for QR/join testing; default bind is `127.0.0.1:8080`):
```
BIND_ADDR=0.0.0.0:8080 STATIC_DIR=frontend/dist ./target/release/turn-tracker.exe
```
`STATIC_DIR` defaults to `./static`, **which does not exist** in this repo — you must point it at `frontend/dist` (the Dockerfile copies dist → `/home/app/static`; locally use `frontend/dist`).

**To test the latest code**, rebuild BOTH:
- Frontend: `cd frontend && npm run build` → writes `frontend/dist` (run `vitest run` + this build per [[frontend-build-gate]]).
- Backend: `cargo build --release`.

**Gotchas (all hit this session 2026-06-18):**
- **Backend serves dist files per-request**, so a rebuilt bundle is picked up **without restarting the server** — a frontend-only fix needs no backend restart. BUT browsers cache the old bundle, so always **hard-reload / clear-site-data (mobile especially)** or you keep running stale JS.
- **Windows exe lock:** `cargo build` fails with `error: failed to remove file ...turn-tracker.exe / Access is denied (os error 5)` while the server is running — **stop the process first** (`Stop-Process -Name turn-tracker -Force`).
- **Single-instance lock:** the server takes an exclusive OS lock on `data/.lock`; a 2nd instance pointed at the same data dir refuses to start. Stop the old one before relaunching.
- **Snapshot:** rooms persist to `data/rooms.json` and reload at startup (log: `restored N room(s)`). To wipe accumulated test rooms for a clean slate, stop the server, `rm data/rooms.json`, restart.

**Playwright E2E** (`cd frontend && npm run e2e`) — the config's `webServer`
builds the SPA and runs `cargo run --release` itself, so no manual server is
needed. Three gotchas, all hit 2026-09-17:
- **In a cloud session the pinned browser is usually absent.** The image ships
  Chromium build 1194; `@playwright/test` 1.61 wants 1228, and the run dies with
  "Executable doesn't exist". **Do NOT run `npx playwright install`.** Write a
  throwaway `frontend/playwright.local.config.ts` that spreads the real config
  and adds `use.launchOptions.executablePath = '/opt/pw-browsers/chromium'`
  (a symlink to the browser that IS there), run with `-c`, then delete it. It
  must live under `frontend/` — from `/tmp` it cannot resolve `@playwright/test`.
- **Joiners go through the name gate.** `RoomView` reads the handed-over name
  from `history.state.name`, never from a `?name=` query. The `joinRoom` helper
  used to pass one, so joiners silently sat on the gate and both multi-player
  tests failed for months. The helper now fills the gate — that is also the real
  path a newcomer on a shared link takes.
- **A run leaves `frontend/data/rooms.json`**, because the webServer runs the
  backend with `cwd: frontend` and `TT_SNAPSHOT_PATH` defaults to `./data/`.
  It holds player **tokens**. `.gitignore` now matches `data/` at any depth
  (it was root-anchored, so this was stageable in a public repo).

See [[turn-tracker-status]], [[turn-tracker-turn-engine-invariants]].
