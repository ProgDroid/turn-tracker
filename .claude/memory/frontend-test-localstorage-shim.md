---
name: frontend-test-localstorage-shim
description: vitest4+jsdom29 breaks localStorage.clear(); frontend tests need the global Storage shim in vitest.setup.ts
metadata: 
  node_type: memory
  type: project
  originSessionId: 72c824bf-3329-4d2b-a304-65f80ea1e0ef
---

In the Turn Tracker `frontend/` test suite, **vitest 4.1.9 + jsdom 29.1.1 expose a `localStorage` whose `.clear()` is not callable** (`TypeError: localStorage.clear is not a function`; a `--localstorage-file ... without a valid path` warning accompanies it). Native jsdom localStorage cannot be relied on here.

Fix in place (commit 85f3ef0): `frontend/vitest.setup.ts` installs a spec-correct in-memory `Storage` (Map-backed, `getItem` uses `m.has(key)` so an empty-string value isn't coerced to null), wired via `setupFiles` in `vite.config.ts`. Vitest's default per-file isolation recreates it per test file, so no cross-file leak.

**How to apply:** Do NOT remove vitest.setup.ts or "simplify" to native localStorage — the tests will break. Any new test that uses localStorage (Pinia room store, LandingView, RoomView) relies on this shim; it just works because the shim is global. A naive Map polyfill using `store[key] || null` is wrong (empty-string bug) — keep the `has`-based version. Related: [[turn-tracker-status]].
