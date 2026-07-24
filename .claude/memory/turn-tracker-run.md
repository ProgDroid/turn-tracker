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

See [[turn-tracker-status]], [[turn-tracker-stale-token-ghost]].
