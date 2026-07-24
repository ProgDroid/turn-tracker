---
name: frontend-ci-lockfile
description: "turn-tracker frontend: Windows-generated lockfile fails Linux `npm ci`; fixed via a Linux-generated lockfile + `npm ci` (regenerate on every dep bump)"
metadata:
  node_type: memory
  type: project
  originSessionId: 3426f0ae-0be2-4336-b82c-36775044293f
---

The Turn Tracker `frontend/package-lock.json` resolves **different optional-dep versions on Windows vs Linux**. The native-binary toolchain (Vite 8 / rolldown / lightningcss, via `@napi-rs/wasm-runtime` → `@emnapi/core`+`@emnapi/runtime`) makes a Windows-generated lockfile pin e.g. `@emnapi/core@1.10.0` while Linux `npm ci` demands `1.11.1` and hard-fails: `npm error Missing: @emnapi/core@1.11.1 from lock file`. A fresh `npm install` on Windows still resolves `1.10.0` — **no Windows-side regeneration fixes it.** Also: `.dockerignore` must NOT exclude `frontend/` (it broke the Docker frontend `COPY` → "/frontend: not found"); ignore `frontend/node_modules` + `frontend/dist` instead.

**RESOLVED (2026-06-17, commit ea2ab85):** the committed lockfile is now **Linux-generated** and CI + the Docker frontend stage use strict **`npm ci`** again (CI run 27697351034 green; `@emnapi/core@1.11.1`). The integrity gate is restored.

**How the Linux lockfile was produced without local Linux** (WSL won't start — virtualisation disabled, HCS_E_HYPERV_NOT_INSTALLED; no Docker daemon): a throwaway `on: push` workflow `relock-frontend.yml` ran `npm install` on an ubuntu runner, verified it with `npm ci && npm run build && npm run test`, and **uploaded `package-lock.json` as an artifact**. Then locally: `gh run download <id> -n frontend-package-lock`, copy it over `frontend/package-lock.json`, flip `npm install`→`npm ci` in `ci.yml`+`Dockerfile`, delete the workflow, commit, push. The workflow was deleted after use but is recoverable from git history (it lived at commit `85ee5e8`).

**KEY GOTCHA:** the default `GITHUB_TOKEN` **cannot create/update `.github/workflows/*` files** even with `permissions: contents: write` (`remote rejected ... without 'workflows' permission`). So a CI bot can't auto-commit the `npm ci` flip or self-delete a workflow — only a human push (or a PAT with `workflow` scope) can. That's why the relock workflow only produces an **artifact** and a human assembles the final commit.

**How to apply (recurs on EVERY frontend dep bump):** the committed lockfile is now Linux-based, so a local `npm install` on Windows will re-introduce the drift and break CI `npm ci` again. To change frontend deps: edit `package.json`, then re-run the artifact-relock pattern above (resurrect the workflow from `85ee5e8`) to regenerate the Linux lockfile — do NOT commit a Windows-regenerated lockfile. Local Windows `npm ci` may now fail (the mirror image); use `npm install` / `npm run dev` locally. Related: [[frontend-build-gate]], [[turn-tracker-status]], [[git-direct-to-main]].
