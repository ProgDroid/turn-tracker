---
name: frontend-build-gate
description: "In turn-tracker frontend, run `npm run build` before committing — vitest passing does NOT mean the build passes"
metadata: 
  node_type: memory
  type: project
  originSessionId: 72c824bf-3329-4d2b-a304-65f80ea1e0ef
---

In the Turn Tracker `frontend/`, a green `npx vitest run` does **not** guarantee a green build. Vitest transforms TS with esbuild (no type-checking), but `npm run build` runs `vue-tsc --noEmit` over **all** `src/**` including test files, and `tsconfig.json` has `noUnusedLocals`/`noUnusedParameters`. So an unused import/variable in a spec file passes vitest but **fails the build** (TS6133).

**Why:** this bit us in Task 5 — an unused `vi` import in `socket.spec.ts` passed the focused vitest run but would have broken `npm run build` later. The two tools check different things.

**How to apply:** before committing any frontend change, run BOTH `npx vitest run` AND `npm run build`. Treat the build as the real gate. Watch especially for unused imports/locals left behind after a refactor or a removed mock. Related: [[turn-tracker-status]], [[frontend-test-localstorage-shim]].
