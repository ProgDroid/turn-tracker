---
name: git-direct-to-main
description: "On personal side projects the user commits/pushes follow-up work directly to main — no feature branch/PR"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 3426f0ae-0be2-4336-b82c-36775044293f
---

For solo side projects (e.g. Turn Tracker), the user commits and pushes follow-up work **directly to `main`**. When offered "branch + commit, or commit directly to main?" for post-merge bugfixes, they chose direct-to-main.

**Why:** solo developer, pre-launch; feature-branch/PR ceremony adds friction with no reviewer. This overrides the harness default ("if on the default branch, branch first").

**How to apply:** for small follow-up/bugfix work on these personal repos, don't reflexively create a branch or ask branch-vs-main — commit to `main` and push when the user says to. Still split logically distinct concerns into separate conventional commits (did so here: one `fix(frontend):` for the followups, one `ci:` for the pipeline fixes). Initial feature builds may still warrant a branch+merge (the Vue frontend was merged `--no-ff`). See [[user-profile]].
