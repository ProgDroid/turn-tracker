# Fly Deployment Implementation Plan

> **For agentic workers:** almost none of this is agent-executable. It creates
> cloud resources, handles secrets and touches `.github/workflows/`, which an
> agent session is refused by GitHub. Treat it as a human runbook. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve Turn Tracker at `https://whosego.app` from a single Fly Machine, deploying automatically from `main`.

**Architecture:** One Fly Machine in `lhr` running the existing `Dockerfile`, with a 1 GB volume at `/data` for the room snapshot and lock. Fly terminates TLS and provides the shared anycast IPv4/IPv6. There is no reverse proxy of ours, no OS to maintain, and no SSH surface. The app stays deliberately single-instance.

**Tech Stack:** Fly.io (Machines, Volumes, Certificates), `flyctl`, GitHub Actions, Cloudflare DNS, Porkbun registration.

**Supersedes:** `docs/superpowers/plans/2026-09-15-deploy-infrastructure.md`, which targeted a VPS. That path is **not deleted** — it is preserved under `deploy/vps/` and documented in `deploy/README.md` so it can be swapped back in.

**Spec:** `docs/superpowers/specs/2026-09-15-deploy-and-private-beta-design.md`

## Global Constraints

- **One Machine. Never two.** The app takes an exclusive OS lock on its data directory, and on Fly each Machine gets its own volume. A second Machine would not merely fail the lock — it would serve a **different set of rooms under the same codes**, so two players scanning one QR could land in separate games. `auto_start_machines = false`, `min_machines_running = 1`, and deploys use `--strategy immediate`.
- **`TT_CLIENT_IP_HEADER=Fly-Client-IP` on Fly.** `X-Forwarded-For` is client-spoofable there; Fly-Client-IP is set from the real TCP connection with any client value stripped. Leaving it unset means the rate limiter keys on a header anyone can rotate, which also removes the throttling that makes guessing `TT_CREATE_TOKEN` impractical. **Never set it on the VPS path** — Caddy forwards unknown headers untouched, which is the same hole in reverse.
- **`TT_CREATE_TOKEN` is a Fly secret, never in `fly.toml`.** The repository is public.
- **`kill_timeout = "30s"` must stay top-level in `fly.toml`.** Placed after a table header, TOML silently nests it and it has no effect — the app would be killed mid-drain at Fly's 5s default and lose the final snapshot.
- **Actions logs on a public repository are world-readable.** Secret values are masked, but `set -x` or an echoed interpolation leaks. No shell tracing in the deploy job.
- **Never commit a filled-in `.env`.** `.gitignore` matches `.env` at any depth.
- **Supply chain: the deploy job is pinned; the rest are not, deliberately.**
  Only `deploy` carries a secret (`FLY_API_TOKEN`), so it is the only job where
  a compromised action gains something worth stealing — its `setup-flyctl` ref
  must be an immutable SHA and the workflow refuses to run until it is.
  The other jobs still use mutable refs, including the branch ref
  `dtolnay/rust-toolchain@stable` and tags like `Swatinem/rust-cache@v2`. That
  is a **knowingly accepted residual**, not an oversight. Those jobs hold no
  secrets and run with `contents: read`, so the realistic attack is tampering
  with test results to turn CI green and let a bad commit through to `deploy` —
  indirect, and it still cannot reach the token. Pinning all six to SHAs is the
  correct end state; do it via Dependabot for `github-actions`, which bumps
  pinned SHAs through reviewable PRs. Hand-pinning them now would mean six
  unresolvable placeholders in a file you have to apply by hand, which trades a
  small risk for a large chance of the workflow never being applied at all.

---

### Task 1: Create the Fly app and volume

**Files:** none (uses the committed `fly.toml`).

- [ ] **Step 1: Install flyctl and sign in**

```bash
curl -L https://fly.io/install.sh | sh
flyctl auth login
```

**If `flyctl auth login` fails with "connection refused"** — which it does on
Termux/Android, because flyctl opens a loopback listener for the OAuth redirect
that the browser cannot reach — authenticate with an environment variable
instead. It is the same mechanism the CI job uses, so it is well-travelled:

```bash
# Create a token at https://fly.io/user/personal_access_tokens
export FLY_API_TOKEN="<token>"
flyctl auth whoami          # should print your email
```

There is no `flyctl auth token` subcommand; the env var is the supported path.
Persist it in your shell profile if you want it across sessions, and `chmod 600`
that profile since it now holds a credential. Use a PERSONAL token here — the
deploy-scoped token from Task 3 is narrower and belongs only in CI.

- [ ] **Step 2: Create the app WITHOUT letting flyctl rewrite fly.toml**

`fly.toml` is already written and carries decisions that matter (single Machine, `kill_timeout`, the trusted header). `flyctl launch` will offer to regenerate it — decline.

```bash
flyctl apps create whosego
```

Then confirm the committed config is what Fly will use:

```bash
flyctl config validate
```

Expected: `Configuration is valid`. If the app name in `fly.toml` is taken, change it there and re-commit rather than letting flyctl edit the file.

- [ ] **Step 3: Create the volume**

```bash
flyctl volumes create tt_data --region lhr --size 1 --yes
flyctl volumes list
```

Expected: one volume, `lhr`, 1 GB. **Create exactly one.** A second volume invites a second Machine, which is the split-rooms failure above.

- [ ] **Step 4: Set the beta token as a secret**

```bash
flyctl secrets set TT_CREATE_TOKEN="$(openssl rand -hex 16)"
```

Record the value locally — you need it for host links, and Fly will not show it again. It is deliberately absent from `fly.toml`.

---

### Task 2: First deploy and certificate

- [ ] **Step 1: Deploy by hand once**

```bash
flyctl deploy --remote-only --strategy immediate
```

Doing the first one manually makes a later CI failure easy to interpret.

- [ ] **Step 2: Confirm exactly one Machine is running**

```bash
flyctl status
flyctl machines list
```

Expected: **one** Machine, state `started`, with the volume attached. If there are two, destroy one now — before any real games exist.

- [ ] **Step 3: Check it serves on the Fly hostname before touching DNS**

```bash
curl -sS https://whosego.fly.dev/health
```

Expected: `{"status":"ok"}`. This isolates app problems from DNS and certificate problems.

- [ ] **Step 4: Add the custom domain**

```bash
flyctl certs add whosego.app
flyctl certs show whosego.app
```

It prints the DNS records it wants. Add them in Cloudflare as **DNS-only (grey cloud)**: an `A` to Fly's shared IPv4 and an `AAAA` to the app's IPv6, plus any ownership record shown.

Orange cloud would put a second proxy in front of Fly, breaking certificate validation and — more quietly — making `Fly-Client-IP` reflect Cloudflare rather than the visitor.

- [ ] **Step 5: Wait for issuance, then verify the public path**

```bash
flyctl certs show whosego.app
curl -sS -o /dev/null -w '%{http_code} %{scheme}\n' https://whosego.app/health
curl -sS -o /dev/null -w '%{http_code} -> %{redirect_url}\n' http://whosego.app/
```

Expected: certificate issued, `200 https`, and HTTP redirecting to HTTPS (`force_https = true`).

- [ ] **Step 6: Verify the gate is on**

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST https://whosego.app/api/rooms \
  -H 'content-type: application/json' -d '{"host_name":"Nope"}'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST https://whosego.app/api/rooms \
  -H 'content-type: application/json' -H "X-Create-Token: <TOKEN>" -d '{"host_name":"Host"}'
```

Expected: `403`, then `200`.

- [ ] **Step 7: Verify the startup log shows the gate and origin check active**

```bash
flyctl logs
```

Expected: `restored 0 room(s) from snapshot` and a listening line. There must be **no** `ALLOWED_ORIGINS is unset` warning and **no** `TT_CREATE_TOKEN is unset` line. Either means the config is not reaching the app.

---

### Task 3: Wire up continuous deployment

**Files:** `.github/workflows/ci.yml` — **you must apply this yourself.**

- [ ] **Step 1: Create a deploy token**

```bash
flyctl tokens create deploy
```

Add the output as repository secret `FLY_API_TOKEN` (Settings → Secrets and variables → Actions). A deploy-scoped token is narrower than a personal one — use it.

- [ ] **Step 2: Pin the flyctl action — the workflow fails until you do**

`deploy/fly/ci.yml` ships with `superfly/flyctl-actions/setup-flyctl@PIN_ME_SEE_HEADER`,
which is deliberately unresolvable. A mutable ref (`@master`, or even a tag)
resolves at deploy time, so any future compromise of that ref would execute
inside the one job carrying `FLY_API_TOKEN` — full deploy access to the app.
Step-scoping the secret does not help: a malicious setup step can plant a
`flyctl` binary that exfiltrates the token when the next step runs.

```bash
git ls-remote https://github.com/superfly/flyctl-actions HEAD
```

Substitute that 40-character SHA, keeping the version as a trailing comment:

```yaml
- uses: superfly/flyctl-actions/setup-flyctl@<sha>  # v1.5
```

Worth doing once while you are here: enable Dependabot for `github-actions`
so pinned SHAs get PR-based updates rather than silently rotting.

- [ ] **Step 3: Apply the workflow**

The intended file is committed at `deploy/fly/ci.yml`. It is not live, because GitHub refuses a workflow push from this session: *"refusing to allow an OAuth App to create or update workflow `.github/workflows/ci.yml` without `workflow` scope."*

From a clone with your own credentials:

```bash
cp deploy/fly/ci.yml .github/workflows/ci.yml
git add .github/workflows/ci.yml
git commit -m "ci: deploy to Fly on main"
git push
```

Then delete `deploy/fly/ci.yml`, or keep it deliberately in step — a stale duplicate is worse than none.

- [ ] **Step 4: Watch the first automated run**

Actions → the run for that push. Expect `test`, `frontend`, `docker`, then `deploy`.

- [ ] **Step 5: Confirm CI actually deployed, rather than reporting success**

```bash
flyctl status
```

Expected: the Machine's created/updated time matches the CI run, not your manual deploy from Task 2.

---

### Task 4: Live end-to-end verification

The point of the exercise. None of it can be tested any other way.

- [ ] **Step 1: Certificate and redirect from a phone.** Open `https://whosego.app`. No warning; `http://` upgrades.

- [ ] **Step 2: Two phones on mobile data, wifi OFF.** Host creates a room, the other joins by code, turns advance both ways in real time. Wifi would mask exactly the carrier-NAT behaviour this is meant to expose.

- [ ] **Step 3: QR scan from a third device** lands straight in the room via the deep link.

- [ ] **Step 4: Room lock.** A new device is refused; a player who force-quit rejoins by token successfully.

- [ ] **Step 5: The heartbeat test.** Background a phone for **5 minutes**, lock the screen, come back. Still live, current state, no reconnect spinner. This is the only real test of the 30s ping / 90-120s timeout.

- [ ] **Step 6: The reconnect-ownership test.** With two phones in a room, put one in aeroplane mode for ~30 seconds, then restore it. It should rejoin and **stay** connected — watch it for a further two minutes. This exercises the connection-epoch fix: before it, the old socket's cleanup would mark the returning player disconnected and they would be silently skipped in turn order.

- [ ] **Step 7: Redeploy mid-game.** Push a trivial commit. Clients show a brief reconnect and resume with state intact.

- [ ] **Step 8: The graceful-shutdown snapshot — never yet executed anywhere.**

```bash
flyctl machines list
flyctl machines stop <machine-id>
flyctl logs
flyctl machines start <machine-id>
flyctl logs
```

Expected: `final snapshot written on shutdown` before the stop, then `restored N room(s) from snapshot` after the start, with N matching. Windows hard-kills on SIGTERM, so this path has only ever been verified by reading the code.

- [ ] **Step 9: Rate-limit keying sanity check.** From one machine:

```bash
for i in $(seq 1 25); do
  curl -sS -o /dev/null -w '%{http_code} ' -X POST https://whosego.app/api/rooms \
    -H 'content-type: application/json' \
    -H 'X-Forwarded-For: 1.2.3.4' \
    -H "X-Create-Token: <TOKEN>" -d '{"host_name":"T"}'
done; echo
```

Expected: the first several return `200`, then `429`. A spoofed `X-Forwarded-For` must **not** reset the bucket — if all 25 succeed, `TT_CLIENT_IP_HEADER` is not taking effect and the limiter is bypassable. Delete the rooms you just made.

- [ ] **Step 10: Set a spend alert** in the Fly dashboard. Usage billing is the one thing a fixed-price VPS could not surprise you with.

---

### Task 5: Run the beta, then open up

- [ ] **Step 1: Distribute host links.** Send each host `https://whosego.app/?k=<TOKEN>` once, noting it only needs opening a single time per device they host from. Everyone else needs nothing.

- [ ] **Step 2: Open to everyone when ready**

```bash
flyctl secrets unset TT_CREATE_TOKEN
flyctl logs | grep TT_CREATE_TOKEN
```

Expected: `TT_CREATE_TOKEN is unset: room creation is OPEN to anyone`. Unsetting a secret triggers a redeploy by itself. The gate code stays in place, inert.

- [ ] **Step 3: Consider uptime monitoring.** UptimeRobot against `/health`, 5-minute interval. Worth doing now rather than during the beta, when you are watching it live anyway.

- [ ] **Step 4: Revisit the parked decisions.** Spec §13 holds two: whether the repo stays public, and whether single-instance remains right. Both are now informed by real operational experience.

---

## Done criteria

- `https://whosego.app` serves the app with a valid certificate; `http://` upgrades.
- A push to `main` deploys with no manual step.
- `flyctl status` shows exactly **one** Machine.
- All ten checks in Task 4 pass, including the spoofed-XFF rate-limit check and the graceful-shutdown snapshot.
- `flyctl logs` shows neither the `ALLOWED_ORIGINS` warning nor the `TT_CREATE_TOKEN` open notice during the beta.

## If Fly disappoints

`deploy/README.md` documents the swap back to the VPS path. Same `Dockerfile`, same binary; the switch is infrastructure, not code. The one setting that must change is `TT_CLIENT_IP_HEADER`, which must be **unset** on the VPS.
