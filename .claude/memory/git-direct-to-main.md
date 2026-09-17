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

**Collides with the cloud-session memory hook (found 2026-09-17).** In a cloud
session `sync-memory.sh` exits at once (it is the local-only path) and
`auto-commit.sh` is the ONLY thing that copies
`~/.claude/projects/<key>/memory/` into the repo's `.claude/memory/` — but it
`exit 0`s on `main`/`master` before reaching the copy. So a cloud session that
follows this note, merges to main and ends there loses every memory it wrote,
silently, when the VM is reclaimed.

**How to apply:** in a cloud session, sync and commit memory on the FEATURE
branch before merging — `cp -a ~/.claude/projects/<key>/memory/. .claude/memory/`
then commit — so the merge carries it. (Upstream fix worth making in the
`personal` plugin: move the memory copy in `auto-commit.sh` above the
default-branch gate, so the copy always happens and only the auto-commit is
skipped.)
