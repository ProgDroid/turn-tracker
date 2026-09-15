# Deploy + private beta — design

**Date:** 2026-09-15
**Status:** approved in outline, pending spec review
**Domain:** `whosego.app`
**Related:** `DEPLOY.md` (operator runbook — this spec builds the thing that runbook describes)

## 1. Goal

Get Turn Tracker onto the public internet at `https://whosego.app`, reachable
over `wss://` from real phones, with room creation restricted to invited hosts
for a friends-and-family test window. Afterwards, removing one environment
variable opens it to everyone.

## 2. Non-goals

- **No separate staging environment.** One box, one environment. A staging
  clone cannot test the only things still unproven (real certificate, real
  `wss://` over mobile data, Linux graceful-shutdown snapshot), and it would be
  a second thing to maintain for a solo project with no users yet.
- **No orchestration.** No Kubernetes, no Swarm, no multi-instance. The app
  takes an exclusive file lock and is explicitly single-instance by design.
- **No horizontal scaling or zero-downtime deploys.** A ~3 second restart is
  well inside the client's existing reconnect budget (6 attempts, ~23s).
- **No offsite backups.** See §5.10.

## 3. Findings that shape the design

Established by reading the code during design, not assumed:

1. **The QR encodes a deep link.** `ShareCodeBlock.vue:13` builds
   `${window.location.origin}/room/${code}`. Guests scan or follow a shared
   link; the typed path (bare domain + 6-char code) is the fallback. This is
   why the access gate must sit on *creation*, never on *joining* — gating the
   join path would break the flow the test exists to validate.
2. **Nothing initiates a WebSocket heartbeat.** `connection.rs:70` replies to
   pings with pongs, but neither side sends a periodic ping. An idle connection
   relies entirely on intermediaries not timing it out. See §5.7.
3. **The frontend already handles stale tokens.** `room.ts:156` clears the
   saved token and resets on `not_found` while `joiningWithToken` is set. The
   backend fix in §5.8 only needs to emit that existing error code.
4. **The repo is public.** GitHub's free `ubuntu-24.04-arm` runners are
   therefore available, so the ARM64 image builds natively. A QEMU-emulated
   Rust release build would take 30+ minutes; a native one is a few.
5. **The default `GITHUB_TOKEN` cannot write `.github/workflows/*`** (recorded
   in project memory, hit previously on this repo). Affects rollout — §9.
6. **The SPA is baked into the image at build time.** There is no runtime
   frontend config, which is why the creation token is carried as URL state
   rather than a build-time variable — §5.6.

## 4. Architecture

```
            whosego.app  (Porkbun registration, Cloudflare DNS, grey cloud)
                     |
                     |  A / AAAA -> box IP, DNS-only (not proxied)
                     v
   +----------------------------------------------------+
   |  Hetzner CAX11 (ARM64, 2 vCPU, 4 GB, Ubuntu 24.04)  |
   |  ufw: 22, 80, 443 only                              |
   |                                                     |
   |  +-- docker compose (/opt/turn-tracker) ----------+ |
   |  |                                                | |
   |  |  caddy  :80 :443  --- TLS, HTTP-01, sets XFF   | |
   |  |     |  internal network only                   | |
   |  |     v                                          | |
   |  |  app  :8080  (NOT published to host)           | |
   |  |     |                                          | |
   |  |     +-> volume tt_data -> /data/rooms.json     | |
   |  +------------------------------------------------+ |
   +----------------------------------------------------+
            ^
            |  docker compose pull && up -d  (over SSH)
            |
     GitHub Actions (push to main)
       build arm64 on ubuntu-24.04-arm -> push ghcr.io/progdroid/turn-tracker
```

### Why grey cloud (DNS-only), not Cloudflare's proxy

Recommended for the beta, for three reasons that compound:

- **Trust boundary.** `DEPLOY.md` requires that the rate limiter's client IP be
  unspoofable, which holds only when the app is reachable *solely* through a
  proxy that overwrites `X-Forwarded-For`. With Caddy alone, Caddy is that
  proxy and the app port is never published. Adding Cloudflare's proxy means
  the real client IP arrives in `CF-Connecting-IP`, the origin IP stays
  reachable directly, and the existing `RealIpKeyExtractor` would read a
  spoofable value unless additionally hardened.
- **WebSocket idle timeouts.** Cloudflare's proxy closes idle WebSocket
  connections. Given finding #2 (no heartbeat), that is an unnecessary failure
  mode to take on during a test.
- **Certificates.** Grey cloud lets Caddy complete HTTP-01 directly. Orange
  cloud intercepts ports 80/443, which forces either DNS-01 with a Cloudflare
  API token or Cloudflare Origin Certificates — more moving parts, no benefit
  at this scale.

Flipping to orange later is a deliberate, separate change requiring: the
heartbeat from §5.7, reading `CF-Connecting-IP`, and blocking non-Cloudflare
traffic to the origin.

## 5. Components

### 5.1 Domain and DNS

- Register `whosego.app` at Porkbun.
- Add the site to Cloudflare (Free plan); set Porkbun's authoritative
  nameservers to the two `*.ns.cloudflare.com` Cloudflare issues.
- Records: `A whosego.app -> <box IPv4>` and `AAAA whosego.app -> <box IPv6>`,
  both **DNS-only (grey cloud)**.
- `www` is redirected to the apex by Caddy, and is deliberately *not* added to
  `ALLOWED_ORIGINS`.
- Registration stays at Porkbun. A transfer to Cloudflare Registrar is blocked
  for 60 days by ICANN policy regardless, and the pricing difference on `.app`
  does not justify it afterwards.

### 5.2 The box

- Hetzner CAX11, Ubuntu 24.04 LTS, Falkenstein (closest EU region to the UK).
- Bootstrap (documented as a script in `deploy/bootstrap.sh`, run once):
  - non-root `deploy` user, SSH public-key only, password auth disabled
  - `ufw` default deny inbound; allow 22, 80, 443
  - `unattended-upgrades` enabled
  - Docker Engine + Compose plugin
  - `/opt/turn-tracker` owned by `deploy`

### 5.3 Compose

`deploy/docker-compose.yml`, checked into the repo, copied to
`/opt/turn-tracker/` on the box:

- **`caddy`** — `caddy:2-alpine`, publishes `80:80` and `443:443`, mounts
  `./Caddyfile` read-only plus named volumes `caddy_data` (certificates — must
  persist or you will re-issue on every deploy and hit Let's Encrypt rate
  limits) and `caddy_config`.
- **`app`** — `ghcr.io/progdroid/turn-tracker:main`. **No `ports:` stanza**;
  reachable only on the internal compose network. `restart: unless-stopped`.
  Healthcheck hits `GET /health`. Env from `/opt/turn-tracker/.env`. Mounts
  named volume `tt_data` at `/data`. `stop_grace_period: 30s` — the default of
  10s can kill the container before Actix drains and the final snapshot is
  written (see `DEPLOY.md` §7).

Compose's default recreate is stop-then-start, which is exactly what the
single-instance lock in §5.5 requires. No rolling-update configuration.

### 5.4 Caddy

```
whosego.app {
	encode zstd gzip
	reverse_proxy app:8080 {
		header_up X-Forwarded-For {remote_host}
	}
}

www.whosego.app {
	redir https://whosego.app{uri} permanent
}
```

`header_up X-Forwarded-For {remote_host}` is required, not decorative. A bare
`reverse_proxy` **appends** the client address to any `X-Forwarded-For` the
client already sent, and the app's rate limiter reads the **leftmost** entry —
so without the override an attacker can pick their own rate-limit bucket by
sending a header, rotate it, and get unlimited `POST /api/rooms` attempts. The
override replaces the header with the actual peer address, which is what
`DEPLOY.md` §2 requires. Caddy obtains/renews the certificate over HTTP-01
automatically.
It handles the HTTP→HTTPS redirect itself, which `.app` requires anyway as an
HSTS-preloaded TLD. WebSocket upgrades pass through `reverse_proxy` without
extra configuration.

### 5.5 App configuration

`/opt/turn-tracker/.env`, mode `0600`, owned by `deploy`, **never committed**
(the repo is public):

| Variable | Value |
|---|---|
| `BIND_ADDR` | `0.0.0.0:8080` (image default) |
| `STATIC_DIR` | `/home/app/static` (image default) |
| `ALLOWED_ORIGINS` | `https://whosego.app` |
| `TT_SNAPSHOT_PATH` | `/data/rooms.json` |
| `TT_SNAPSHOT_INTERVAL_SECS` | `60` |
| `TT_CREATE_TOKEN` | a random 32-char string (§5.6) |

The lock file lives at `/data/.lock` alongside the snapshot, inside the
persistent volume.

### 5.6 Creation gate — `TT_CREATE_TOKEN`

**Behaviour.** Unset or empty ⇒ no gate, exactly mirroring how
`ALLOWED_ORIGINS` already degrades for development, so local runs and the
Playwright E2E suite are unaffected. When set, `POST /api/rooms` requires
header `X-Create-Token` to match, and otherwise returns `403` with the standard
HTTP error envelope this codebase already uses — `{"error": "Hosting is
invite-only during the beta"}`. There is deliberately no machine-readable
`code` field: the HTTP envelope has never carried one (codes are the WebSocket
`ServerMessage::Error` shape), and the SPA branches on the `403` status.

The token is resolved from the environment once at startup and injected as
application state, so the handler does not read the process environment per
request and the enabled gate is testable without mutating the environment.

**Why a header, not a query parameter:** query strings land in proxy access
logs and leak via `Referer`. The SPA holds the token and sends it as a header.

**Why after the rate limiter:** `actix-governor` must run first so that
guessing the token is itself rate-limited.

**Frontend.** On landing, if `?k=<token>` is present, store it under
`tt.createToken` in `localStorage` and strip it from the URL via
`history.replaceState`. The create-room call attaches the header when a stored
token exists. A `403` response renders a plain message — "Hosting is
invite-only during the beta" — reusing the existing error display.
**The join path is untouched**, so guests scanning a QR see no difference.

**Distribution.** Hosts get `https://whosego.app/?k=<token>` once; the token
persists on their device. Everyone else just gets a room link.

### 5.7 WebSocket heartbeat

**Decision: in scope for the first deploy** (confirmed 2026-09-15).

During active play the server broadcasts on every turn advance, so sockets are
rarely idle — which initially made this look cuttable. The case that changes it
is the **lobby**: people arriving late, fumbling with phones, wandering off to
make tea. A friends-and-family test is disproportionately lobby time, so idle
sockets are over-represented in exactly the scenario being tested. It is also a
prerequisite for ever enabling Cloudflare's proxy, and it cleans up half-open
connections.

**Design.** Per connection, a 30s interval sends a WebSocket Ping. Browsers
answer Ping frames automatically at protocol level (the JS API cannot send
pings, which is why this must be server-initiated). Track the last Pong; close
the connection once more than 90s has passed without one. Staleness is only
evaluated on the same 30s ticks, so the actual close lands somewhere in the
**90-120s** range — typically near 120s, since in steady state the last pong
arrives just after a tick. That looseness is fine: the point is to reap
half-open sockets eventually, not promptly.

30s sits comfortably under every intermediary timeout we might meet, including
Cloudflare's. Note the direction of that requirement — because the server pings
every 30s the connection is never idle for longer than that, so an
intermediary's idle timeout only needs to exceed 30s with margin.

### 5.8 Stale-token fix

In `handle_join` (`src/ws/connection.rs`): when `player_token` is present and
non-empty but matches no player in the room, send the existing `not_found`
error instead of falling through to `add_player("")`. The frontend path at
`room.ts:156` already clears the token and re-prompts for a name, so no
frontend change is needed.

Trigger is narrower than first assumed — a full data wipe removes the room, so
the handshake 404s onto the existing `gone` path. This needs *room present,
token unknown*: corrupt-snapshot recovery onto an older snapshot, or an edited
token. Low probability, ~10 line fix, and an empty-named ghost player appearing
mid-demo is disproportionately bad for a first impression.

### 5.9 CD pipeline

Modify `.github/workflows/ci.yml`: the existing `docker` job currently builds
with `push: false`. On `push` to `main` only, it becomes a build-and-push job
running on `ubuntu-24.04-arm`, targeting `linux/arm64`, pushing to
`ghcr.io/progdroid/turn-tracker` tagged `main` and `sha-<sha>`. On pull
requests it keeps `push: false`. It stays gated behind the `test` and
`frontend` jobs via `needs:`.

New `deploy` job, `needs: [docker]`, `main` only: SSH to the box using
`secrets.DEPLOY_SSH_KEY` / `DEPLOY_HOST` / `DEPLOY_USER` and run
`docker compose pull && docker compose up -d` in `/opt/turn-tracker`, then poll
`https://whosego.app/health` and fail the job if it does not return 200.

The package is public (the repo is), so the box pulls without registry
authentication.

### 5.10 Backups and monitoring

Deliberately minimal. `rooms.json` holds ephemeral board-game state; losing it
costs an in-progress game, not data anyone will mourn.

- A nightly cron on the box copying `rooms.json` to `rooms.json.bak`, covering
  the one realistic failure (a corrupt write) without any offsite machinery.
- **No offsite backup.** If this ever holds anything worth keeping, revisit.
- Monitoring is `restart: unless-stopped` plus the compose healthcheck. An
  UptimeRobot check against `/health` is two minutes of setup if you want a
  push notification when it dies — recommended after the test, not during it,
  since you will be watching it live anyway.

## 6. Request flow

1. Guest scans QR → `https://whosego.app/room/ABC123`.
2. Cloudflare DNS resolves to the box; TLS terminates at Caddy.
3. Caddy serves the SPA (Actix `spa_files` fallback handles the client route).
4. SPA opens `wss://whosego.app/ws/ABC123`; Caddy upgrades and proxies to the
   app, which checks `Origin` against `ALLOWED_ORIGINS`.
5. Host creating a room sends `POST /api/rooms` with `X-Create-Token`; the rate
   limiter runs first, then the token check, keyed on the `X-Forwarded-For`
   value Caddy set.

## 7. Failure modes

| Failure | Behaviour | Mitigation |
|---|---|---|
| Deploy restart | ~3s outage; rooms reload from snapshot, players marked disconnected and rejoin by token | Within the client's ~23s reconnect budget |
| Second instance started | Refuses to start, exits 1 | By design; compose recreate is stop-then-start |
| Corrupt snapshot | Backed up to `.corrupt`, starts empty | Nightly `.bak` copy |
| Certificate renewal failure | Site down at expiry | `caddy_data` volume persists ACME state; renewal is automatic at 30 days' remaining |
| Token brute force | Rate limited to 20/min per IP | Governor runs ahead of the token check |
| Idle WS dropped by carrier NAT | Client reconnects, capped at 6 attempts | Heartbeat (§5.7) |

## 8. Testing

**Automated (pre-deploy):** `cargo test`, `cargo clippy --all-targets -D
warnings`, `cargo fmt --check`, `npm run test`, and `npm run build` — the
vue-tsc build gate matters, vitest passing is not sufficient. New unit tests
cover the token check (unset ⇒ open, set+correct ⇒ 200, set+wrong ⇒ 403),
unknown-token join ⇒ `not_found`, and the heartbeat timeout.

**Live, on real phones, after deploy** — this is the point of the exercise and
none of it can be tested any other way:

1. Certificate valid, HTTP redirects to HTTPS, `www` redirects to apex.
2. Two phones on **mobile data, not wifi**, join one room; turns advance in
   real time both ways.
3. QR scan from a third device lands directly in the room.
4. Lock the room; a new joiner sees the rejection screen; a locked-out player
   rejoins by token successfully.
5. Background a phone for 5 minutes, return, confirm it is still live (this is
   the heartbeat test).
6. Redeploy mid-game; confirm clients reconnect and state survives.
7. `docker compose down` and back up; confirm the graceful-shutdown snapshot
   was written — the Linux-only path that Windows could never exercise.
8. Attempt `POST /api/rooms` without the token; expect 403.

## 9. Rollout order

1. Buy `whosego.app`; point nameservers at Cloudflare. **Do this first** — NS
   propagation is the only multi-hour step.
2. Land the code changes (§5.6, §5.7, §5.8) on `main`, CI green.
3. Create the CAX11; run `deploy/bootstrap.sh`; add the A/AAAA records.
4. Copy `docker-compose.yml`, `Caddyfile` and `.env` to `/opt/turn-tracker`;
   bring it up manually once and verify the certificate.
5. Add repo secrets; enable the CD workflow.
   **Blocker to expect:** the default `GITHUB_TOKEN` cannot write
   `.github/workflows/*`, so the workflow changes need a push from your machine
   or a PAT with `workflow` scope. An agent session cannot land them.
6. Run the §8 live checklist.
7. Distribute `https://whosego.app/?k=<token>` to the test hosts.

## 10. Going public

Remove `TT_CREATE_TOKEN` from `/opt/turn-tracker/.env` and recreate the app
container. The gate code can stay (it is inert when unset) or be removed in one
commit. Consider UptimeRobot at this point, and reconsider the orange cloud
once the heartbeat has proven itself.

## 11. Cost

| Item | Cost |
|---|---|
| Hetzner CAX11 | ~€6/mo, billed hourly against a monthly cap |
| Primary IPv4 | €0.50/mo |
| `whosego.app` | ~$14/yr, renewal roughly flat |
| GHCR + Actions | Free (public repo) |
| Cloudflare DNS | Free |

Around €7/month, and roughly €3 for a two-week test if the box is deleted
afterwards. Hetzner prices should be confirmed on their site — they were not
reachable from the session that wrote this.

## 12. Open questions

1. IPv6-only to drop the €0.50 IPv4 charge? **No** — some guests' mobile
   networks are IPv4-only and this is the one thing that must work everywhere.
2. `www` at all? Included as a redirect; costs nothing and stops a typo dead-ending.
3. How many test hosts? Affects nothing technically — one shared token either way.

## 13. Deferred — discuss at deploy time

Both of these are deliberately parked, not forgotten. Neither blocks the plan.

### 13.1 Should the repo stay public after release?

Currently public to support job applications — employers are pointed at it from
a CV, and that need outlives the release. Intended monetisation is a Ko-fi
donation pot, which does not by itself argue for going private: there is no
revenue to protect and no moat to lose, and open source alongside a tip jar is a
common, coherent pairing.

Working assumption: **stay public.** Reasons to revisit would be adding
accounts, payments, or any user data, which would raise the stakes on every
publicly disclosed bug.

Note that going private also has a direct cost to this design: free arm64
runners and unmetered Actions minutes are public-repo benefits, and the box
currently pulls from GHCR unauthenticated because the package is public. A
private repo makes the CD slower, metered, and more complex.

**Carry into the deploy discussion:** Actions logs on a public repo are
world-readable. Secret *values* are masked, but a `set -x` around the SSH step,
or any command echoing an interpolated secret, leaks. Decide on a standing rule
for the deploy job rather than relying on remembering.

### 13.2 Is a non-rolling deploy the right call?

§2 rules out zero-downtime deploys and §5.3 relies on Compose's stop-then-start
recreate. Worth re-examining, but note what the question actually decomposes
into: the app takes an **exclusive OS lock** on the data directory and refuses
to start a second instance, so a rolling deploy is not a deployment-config
change — it requires rethinking single-instance ownership of the snapshot
(external state, or lock hand-off, or accepting two writers).

So the real question is "should this stay single-instance?", which is a much
larger one than the deploy mechanism suggests. Current position: a ~3s restart
sits well inside the client's ~23s reconnect budget, and rooms survive via the
snapshot, so the downtime is close to invisible in play. Revisit if the live
test shows reconnects are uglier than expected, or if uptime ever matters more
than simplicity.
