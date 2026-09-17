# Project Memory — Turn Tracker

- [User profile](user-profile.md) — solo dev; Rust/Actix + Vue stack; **Fly.io now, NOT Hetzner** (that note was wrong and cost a design cycle); wants decisions challenged
- [Turn Tracker status](turn-tracker-status.md) — **LIVE at whosego.app on Fly (2026-09-16)**; CD via GitHub Actions; single-Machine + kill_timeout + Fly-Client-IP invariants; VPS path preserved under deploy/vps/; only the live phone tests remain
- [Run/test locally](turn-tracker-run.md) — single Actix binary serves the SPA; run with STATIC_DIR=frontend/dist + BIND_ADDR=0.0.0.0:8080; Windows exe-lock + instance-lock + hard-reload gotchas; **Playwright E2E: cloud sessions need an executablePath override, never `playwright install`**
- [WS connection invariants](turn-tracker-ws-connection-invariants.md) — **read before touching src/ws/connection.rs**: unknown tokens rejected, one player per connection, only the owning connection may disconnect; why the heartbeat promoted a latent race into a live bug
- [Turn engine invariants](turn-tracker-turn-engine-invariants.md) — **read before touching src/domain/room.rs**: `advance_from` can return its own start index; an Active room must never hold a turn nobody can advance; reorder must not restamp the clock; elapsed goes on the wire as a count, not a timestamp
- [Semgrep ws:// hook](semgrep-ws-hook.md) — PostToolUse scan blocks literal ws:// (non-fatal); resurfaces in frontend
- [Frontend test localStorage shim](frontend-test-localstorage-shim.md) — vitest4+jsdom29 break localStorage.clear(); keep the global Storage shim
- [Frontend build gate](frontend-build-gate.md) — vitest green ≠ build green; run `npm run build` (vue-tsc) before committing frontend changes
- [Frontend CI lockfile](frontend-ci-lockfile.md) — Windows lockfile fails Linux `npm ci`; fixed via Linux-generated lockfile (artifact-relock pattern); regenerate on every dep bump; GITHUB_TOKEN can't push workflow files
- [Deploying from a phone](deploying-from-a-phone.md) — Termux: Go binaries can't resolve DNS (install proot-distro FIRST); no `flyctl auth token`, use FLY_API_TOKEN; Fly's web UI can't make volumes/certs; Cloudflare imports registrar parking records as proxied → 525
- [Git direct-to-main](git-direct-to-main.md) — solo side projects: commit/push follow-up work straight to main, no branch/PR
