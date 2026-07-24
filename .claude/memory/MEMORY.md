# Project Memory — Turn Tracker

- [User profile](user-profile.md) — solo dev; Rust/Actix + Vue stack; Hetzner VPS; wants decisions challenged
- [Turn Tracker status](turn-tracker-status.md) — backend+frontend merged & hardened; durability + 2026-06-18 deep-dive hardening + a final pre-deploy test round (phantom-"Player" join fix, drag-drop anim, QR toggle) DONE & pushed; DEPLOY.md added; deploy/CD + live wss E2E are the only remaining workstreams
- [Run/test locally](turn-tracker-run.md) — single Actix binary serves the SPA; run with STATIC_DIR=frontend/dist + BIND_ADDR=0.0.0.0:8080; Windows exe-lock + instance-lock + hard-reload gotchas
- [Stale-token ghost (latent bug)](turn-tracker-stale-token-ghost.md) — unfixed: Join with unknown/stale token mints an empty-named player instead of rejecting; bites after snapshot/data resets
- [Semgrep ws:// hook](semgrep-ws-hook.md) — PostToolUse scan blocks literal ws:// (non-fatal); resurfaces in frontend
- [Frontend test localStorage shim](frontend-test-localstorage-shim.md) — vitest4+jsdom29 break localStorage.clear(); keep the global Storage shim
- [Frontend build gate](frontend-build-gate.md) — vitest green ≠ build green; run `npm run build` (vue-tsc) before committing frontend changes
- [Frontend CI lockfile](frontend-ci-lockfile.md) — Windows lockfile fails Linux `npm ci`; fixed via Linux-generated lockfile (artifact-relock pattern); regenerate on every dep bump; GITHUB_TOKEN can't push workflow files
- [Git direct-to-main](git-direct-to-main.md) — solo side projects: commit/push follow-up work straight to main, no branch/PR
