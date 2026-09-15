# Deploy Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to work through this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Most of this plan cannot be executed by an agent.** It provisions a machine that does not exist yet, handles SSH keys and secrets, and modifies `.github/workflows/`, which the default `GITHUB_TOKEN` is not permitted to write. Tasks 3 and 5's file authoring are agent-executable; everything else is a human runbook.

**Goal:** Serve Turn Tracker at `https://whosego.app` from a single Hetzner CAX11, with automated deploys from `main`.

**Architecture:** One ARM64 box running two containers via Docker Compose — Caddy terminating TLS on 80/443, and the app reachable only on the internal compose network. DNS is on Cloudflare but **not proxied** (grey cloud), so Caddy obtains certificates over HTTP-01 and is itself the only proxy in the path, which is what makes the app's `X-Forwarded-For`-keyed rate limiter sound. GitHub Actions builds a native ARM64 image on a free `ubuntu-24.04-arm` runner, pushes it to GHCR, and deploys over SSH.

**Tech Stack:** Hetzner Cloud, Ubuntu 24.04 LTS (ARM64), Docker Engine + Compose plugin, Caddy 2, GitHub Actions, GHCR, Cloudflare DNS, Porkbun registration.

**Spec:** `docs/superpowers/specs/2026-09-15-deploy-and-private-beta-design.md`

**Depends on:** `docs/superpowers/plans/2026-09-15-deploy-prep-code-changes.md` must be complete and merged to `main` first — the image this plan deploys needs `TT_CREATE_TOKEN` support to exist.

## Global Constraints

- **`ALLOWED_ORIGINS` must be set in production.** Unset disables WebSocket origin checking entirely. Value: `https://whosego.app`.
- **The app container must never publish a port to the host.** The rate limiter trusts `X-Forwarded-For`, which is only unspoofable while Caddy is the sole path in. No `ports:` stanza on the `app` service.
- **Single instance only.** The app takes an exclusive OS lock on the data directory and a second process exits 1. Compose's default recreate is stop-then-start, which is correct; do not introduce any rolling-update behaviour.
- **Nothing secret is ever committed.** The repository is public. `TT_CREATE_TOKEN`, SSH keys, and host details live in `/opt/turn-tracker/.env` (mode `0600`) and GitHub Actions secrets.
- **Actions logs on a public repository are world-readable.** Secret *values* are masked, but `set -x` or any command echoing an interpolated secret leaks. No shell tracing in the deploy job.
- **Caddy's `caddy_data` volume must persist.** It holds ACME account and certificate state; losing it on every deploy would re-issue certificates and hit Let's Encrypt rate limits.
- **`.app` is HSTS-preloaded.** The domain is HTTPS-only in browsers forever. This does not affect ACME HTTP-01, which validates over port 80 outside the browser.

---

## Inputs to collect before starting

Gather these once; later tasks refer back to them.

| Input | Where it comes from |
|---|---|
| Box IPv4 and IPv6 | Hetzner console after Task 2 |
| SSH keypair for deploys | Generated in Task 5, public half added in Task 2 |
| `TT_CREATE_TOKEN` value | Generated in Task 4 |
| Cloudflare nameservers | Cloudflare dashboard during Task 1 |

---

### Task 1: Register the domain and move DNS to Cloudflare

Do this first. Nameserver propagation is the only step measured in hours rather than minutes, and nothing else depends on the box existing.

**Files:** none.

**Interfaces:**
- Produces: `whosego.app` resolving through Cloudflare, ready for A/AAAA records in Task 2.

- [ ] **Step 1: Register the domain**

Buy `whosego.app` at Porkbun. Leave WHOIS privacy on (free, on by default).

Registration stays at Porkbun — ICANN locks a newly registered domain against transfer for 60 days regardless, and Cloudflare Registrar's at-cost pricing is not meaningfully cheaper than Porkbun for `.app`.

- [ ] **Step 2: Add the site to Cloudflare**

Cloudflare dashboard → *Add a site* → `whosego.app` → **Free** plan. Cloudflare will scan for existing records (there will be none) and then show two assigned nameservers, of the form `xxxx.ns.cloudflare.com`.

- [ ] **Step 3: Point Porkbun at Cloudflare**

Porkbun → the domain → *Authoritative Nameservers* → *Edit* → replace Porkbun's entries with the two Cloudflare nameservers.

- [ ] **Step 4: Verify**

Cloudflare's overview page for the domain shows **Active** once propagation completes — usually minutes for a newly registered domain. Do not proceed to Task 4 until it does.

Verify independently:

```bash
dig +short NS whosego.app
```

Expected: the two `*.ns.cloudflare.com` names.

---

### Task 2: Provision and harden the box

**Files:**
- Create: `deploy/bootstrap.sh`

**Interfaces:**
- Produces: a box with Docker, a non-root `deploy` user, a firewall, and `/opt/turn-tracker` ready to receive configuration.

- [ ] **Step 1: Create the server**

Hetzner Cloud console → new project → *Add Server*:

- Location: **Falkenstein** (closest EU region to the UK)
- Image: **Ubuntu 24.04**
- Type: **CAX11** (Arm64, 2 vCPU, 4 GB)
- Networking: **IPv4 and IPv6** both enabled. Do not drop IPv4 to save €0.50 — some guests' mobile networks are IPv4-only, and universal reachability is the one thing that must not fail.
- SSH key: add your personal public key so you can log in as `root` for bootstrap.

Record the IPv4 and IPv6 addresses.

- [ ] **Step 2: Write the bootstrap script**

Create `deploy/bootstrap.sh`:

```bash
#!/usr/bin/env bash
# One-time host setup for the turn-tracker box. Run as root on a fresh
# Ubuntu 24.04 server:  bash bootstrap.sh <deploy-ssh-public-key>
set -euo pipefail

DEPLOY_KEY="${1:?usage: bootstrap.sh <deploy-ssh-public-key>}"

apt-get update
apt-get upgrade -y
apt-get install -y ca-certificates curl ufw unattended-upgrades

# --- deploy user -----------------------------------------------------------
id -u deploy >/dev/null 2>&1 || useradd -m -s /bin/bash deploy
install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
echo "$DEPLOY_KEY" > /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 600 /home/deploy/.ssh/authorized_keys

# --- docker ----------------------------------------------------------------
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
  > /etc/apt/sources.list.d/docker.list
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
usermod -aG docker deploy

# --- firewall --------------------------------------------------------------
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# --- ssh hardening ---------------------------------------------------------
sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl reload ssh

# --- automatic security updates -------------------------------------------
dpkg-reconfigure -f noninteractive unattended-upgrades

install -d -o deploy -g deploy /opt/turn-tracker

echo "bootstrap complete"
```

- [ ] **Step 3: Generate the deploy keypair**

On your own machine:

```bash
ssh-keygen -t ed25519 -f ~/.ssh/turn-tracker-deploy -C "turn-tracker github actions" -N ""
```

This produces `~/.ssh/turn-tracker-deploy` (private, for GitHub secrets in Task 5) and `~/.ssh/turn-tracker-deploy.pub` (public, for the box now).

- [ ] **Step 4: Run the bootstrap**

```bash
scp deploy/bootstrap.sh root@<BOX_IP>:/tmp/
ssh root@<BOX_IP> "bash /tmp/bootstrap.sh '$(cat ~/.ssh/turn-tracker-deploy.pub)'"
```

- [ ] **Step 5: Verify the deploy user works and root is the only way in by password**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP> "docker version --format '{{.Server.Version}}' && ls -ld /opt/turn-tracker && sudo -n true 2>&1 | head -1"
```

Expected: a Docker server version, `/opt/turn-tracker` owned by `deploy`, and no passwordless sudo (the deploy user deliberately has none — it only needs Docker).

- [ ] **Step 6: Commit the script**

```bash
git add deploy/bootstrap.sh
git commit -m "deploy: add one-time host bootstrap script"
```

---

### Task 3: Author the compose stack

**Files:**
- Create: `deploy/docker-compose.yml`
- Create: `deploy/Caddyfile`
- Create: `deploy/.env.example`

**Interfaces:**
- Consumes: `TT_CREATE_TOKEN` support from the code-changes plan.
- Produces: the stack copied to `/opt/turn-tracker` in Task 4.

- [ ] **Step 1: Write the compose file**

Create `deploy/docker-compose.yml`:

```yaml
services:
  caddy:
    image: caddy:2-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
    depends_on:
      - app

  app:
    image: ghcr.io/progdroid/turn-tracker:main
    restart: unless-stopped
    # NO ports stanza, deliberately. The rate limiter keys on X-Forwarded-For,
    # which is only unspoofable while Caddy is the sole path to this container.
    env_file:
      - .env
    volumes:
      - tt_data:/data
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://127.0.0.1:8080/health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

volumes:
  # Holds ACME account + certificate state. Losing this re-issues certificates
  # on every deploy and will hit Let's Encrypt rate limits.
  caddy_data:
  caddy_config:
  tt_data:
```

- [ ] **Step 2: Write the Caddyfile**

Create `deploy/Caddyfile`:

```
whosego.app {
	encode zstd gzip
	reverse_proxy app:8080
}

www.whosego.app {
	redir https://whosego.app{uri} permanent
}
```

Caddy sets `X-Forwarded-For` on `reverse_proxy` by default, satisfying the app's trust requirement, handles the HTTP→HTTPS redirect itself, and proxies WebSocket upgrades without extra configuration.

- [ ] **Step 3: Write the env example**

Create `deploy/.env.example`:

```bash
# Copy to /opt/turn-tracker/.env on the box, fill in, chmod 600.
# NEVER commit the filled-in version — this repository is public.

BIND_ADDR=0.0.0.0:8080
STATIC_DIR=/home/app/static
ALLOWED_ORIGINS=https://whosego.app
TT_SNAPSHOT_PATH=/data/rooms.json
TT_SNAPSHOT_INTERVAL_SECS=60

# Closed beta only. Remove this line and recreate the container to open
# room creation to everyone.
TT_CREATE_TOKEN=
```

- [ ] **Step 4: Confirm the real env file can never be committed**

Check that a `.env` in `deploy/` would be ignored:

```bash
cd /path/to/turn-tracker
echo "TT_CREATE_TOKEN=leaked" > deploy/.env
git check-ignore -v deploy/.env; echo "exit=$?"
rm deploy/.env
```

Expected: `exit=0` with a matching rule. If it prints `exit=1`, the file is **not** ignored — add `deploy/.env` to `.gitignore` and re-check before continuing.

- [ ] **Step 5: Commit**

```bash
git add deploy/docker-compose.yml deploy/Caddyfile deploy/.env.example .gitignore
git commit -m "deploy: add compose stack, Caddy config and env template"
```

---

### Task 4: First deploy, by hand

Deliberately manual. Verifying the certificate and the app by hand once is what makes a failure in the automated job in Task 5 easy to interpret.

**Files:** none.

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: a live site at `https://whosego.app`.

- [ ] **Step 1: Point DNS at the box**

Cloudflare → `whosego.app` → *DNS* → add:

| Type | Name | Content | Proxy status |
|---|---|---|---|
| A | `@` | box IPv4 | **DNS only (grey cloud)** |
| AAAA | `@` | box IPv6 | **DNS only (grey cloud)** |
| A | `www` | box IPv4 | **DNS only (grey cloud)** |

Grey cloud is not a default to accept passively — it is required. Cloudflare's proxy would intercept ports 80/443 and break HTTP-01, impose a WebSocket idle timeout, and move the real client IP to `CF-Connecting-IP` where the app does not look for it.

- [ ] **Step 2: Verify DNS before asking Caddy for a certificate**

```bash
dig +short A whosego.app
dig +short AAAA whosego.app
```

Expected: exactly the box's addresses. A premature certificate request against wrong DNS burns Let's Encrypt attempts.

- [ ] **Step 3: Generate the beta token**

```bash
openssl rand -hex 16
```

Keep the output; it goes in `.env` next and is given to hosts in Task 7.

- [ ] **Step 4: Copy the stack to the box**

```bash
scp -i ~/.ssh/turn-tracker-deploy \
  deploy/docker-compose.yml deploy/Caddyfile deploy/.env.example \
  deploy@<BOX_IP>:/opt/turn-tracker/
```

- [ ] **Step 5: Create the real env file**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP>
cd /opt/turn-tracker
cp .env.example .env
nano .env          # paste the token from Step 3 into TT_CREATE_TOKEN
chmod 600 .env
```

- [ ] **Step 6: Make the image public and bring the stack up**

The image does not exist until the CD job in Task 5 has run once. Run Task 5 Steps 1-4 now, let the workflow push an image, then return here.

Once `ghcr.io/progdroid/turn-tracker:main` exists, set the package visibility to public (GitHub → your profile → *Packages* → `turn-tracker` → *Package settings* → *Change visibility* → Public) so the box pulls without authenticating. Then:

```bash
cd /opt/turn-tracker
docker compose pull
docker compose up -d
docker compose ps
```

Expected: both services `running`, `app` reporting `healthy` within about 40 seconds.

- [ ] **Step 7: Verify the certificate and the app**

```bash
curl -sS -o /dev/null -w '%{http_code} %{scheme}\n' https://whosego.app/health
curl -sS https://whosego.app/health
curl -sS -o /dev/null -w '%{http_code} -> %{redirect_url}\n' http://whosego.app/
curl -sS -o /dev/null -w '%{http_code} -> %{redirect_url}\n' https://www.whosego.app/
```

Expected: `200 https`, `{"status":"ok"}`, a 308 from HTTP to `https://whosego.app/`, and a 301 from `www` to the apex.

- [ ] **Step 8: Verify the gate is on and the app port is not exposed**

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -X POST https://whosego.app/api/rooms \
  -H 'content-type: application/json' -d '{"host_name":"Nope"}'
curl -sS -o /dev/null -w '%{http_code}\n' -X POST https://whosego.app/api/rooms \
  -H 'content-type: application/json' -H "X-Create-Token: <TOKEN>" -d '{"host_name":"Host"}'
curl -sS --max-time 5 -o /dev/null -w '%{http_code}\n' http://<BOX_IP>:8080/health || echo "refused (correct)"
```

Expected: `403`, then `200`, then a connection failure on port 8080 — the app must not be reachable except through Caddy.

- [ ] **Step 9: Verify the startup warnings are absent**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP> "cd /opt/turn-tracker && docker compose logs app | head -30"
```

Expected: `restored 0 room(s) from snapshot` and a listening line. There must be **no** `ALLOWED_ORIGINS is unset` warning and **no** `TT_CREATE_TOKEN is unset` line. Either one means `.env` is not being read.

- [ ] **Step 10: Add the nightly snapshot copy**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP>
( crontab -l 2>/dev/null; echo '17 4 * * * docker run --rm -v turn-tracker_tt_data:/data alpine sh -c "cp /data/rooms.json /data/rooms.json.bak" >/dev/null 2>&1' ) | crontab -
crontab -l
```

Covers the one realistic loss case, a corrupt write. Deliberately no offsite backup: the file holds in-progress board-game state, not anything worth protecting. Confirm the volume name with `docker volume ls` first — Compose prefixes it with the project directory name.

---

### Task 5: Automate the deploy

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the box and stack from Tasks 2-4.
- Produces: pushes to `main` deploying automatically.

- [ ] **Step 1: Add the repository secrets**

GitHub → repository → *Settings* → *Secrets and variables* → *Actions* → *New repository secret*:

| Name | Value |
|---|---|
| `DEPLOY_SSH_KEY` | contents of `~/.ssh/turn-tracker-deploy` (the **private** half, whole file including header and footer lines) |
| `DEPLOY_HOST` | box IPv4 |
| `DEPLOY_USER` | `deploy` |

- [ ] **Step 2: Change the docker job to build ARM64 and push**

In `.github/workflows/ci.yml`, replace the `docker` job:

```yaml
  docker:
    runs-on: ubuntu-24.04-arm
    needs: [test, frontend]
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v6
      - uses: docker/setup-buildx-action@v4
      - name: Log in to GHCR
        if: github.ref == 'refs/heads/main' && github.event_name == 'push'
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - name: Build and push
        uses: docker/build-push-action@v7
        with:
          context: .
          platforms: linux/arm64
          push: ${{ github.ref == 'refs/heads/main' && github.event_name == 'push' }}
          tags: |
            ghcr.io/progdroid/turn-tracker:main
            ghcr.io/progdroid/turn-tracker:sha-${{ github.sha }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

`ubuntu-24.04-arm` is free here only because the repository is public; standard arm64 runners do not run on private repositories. On pull requests the job still builds and pushes nothing, preserving the existing validation behaviour.

- [ ] **Step 3: Add the deploy job**

Append to `.github/workflows/ci.yml`:

```yaml
  deploy:
    runs-on: ubuntu-latest
    needs: [docker]
    if: github.ref == 'refs/heads/main' && github.event_name == 'push'
    steps:
      - name: Deploy over SSH
        # No `set -x` anywhere in this job: Actions logs on a public repository
        # are world-readable.
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_SSH_KEY }}" > ~/.ssh/id_ed25519
          chmod 600 ~/.ssh/id_ed25519
          ssh-keyscan -H "${{ secrets.DEPLOY_HOST }}" >> ~/.ssh/known_hosts
          ssh -i ~/.ssh/id_ed25519 "${{ secrets.DEPLOY_USER }}@${{ secrets.DEPLOY_HOST }}" \
            'cd /opt/turn-tracker && docker compose pull && docker compose up -d'
      - name: Verify the deploy is live
        run: |
          for i in $(seq 1 12); do
            code=$(curl -sS -o /dev/null -w '%{http_code}' https://whosego.app/health || true)
            if [ "$code" = "200" ]; then echo "healthy"; exit 0; fi
            sleep 5
          done
          echo "health check never returned 200"
          exit 1
```

`docker compose up -d` recreates the changed service by stopping it before starting the replacement, which is exactly what the app's single-instance lock requires. Do not "improve" this into a rolling update.

- [ ] **Step 4: Push the workflow — this needs YOUR credentials**

**The default `GITHUB_TOKEN` cannot create or update files under `.github/workflows/`**, even with `contents: write`. A push containing this change from an agent session, or from any CI bot, is rejected with `refusing to allow ... without 'workflows' permission`. This has already been hit on this repository.

Push the workflow change from your own machine, or use a PAT with `workflow` scope.

```bash
git add .github/workflows/ci.yml
git commit -m "ci: build arm64 images, push to GHCR and deploy on main"
git push
```

- [ ] **Step 5: Watch the first automated run**

GitHub → *Actions* → the run for that push. Expect `test`, `frontend`, `docker`, then `deploy` to succeed in order. The first `docker` run has a cold cache and will be slow; later ones reuse the GHA cache.

- [ ] **Step 6: Confirm the deployed image is the new one**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP> \
  "docker inspect --format '{{.Image}} {{.State.StartedAt}}' turn-tracker-app-1"
```

Expected: a start time matching the deploy, not the manual `up` from Task 4. Confirm the container name with `docker compose ps` if it differs.

---

### Task 6: Live end-to-end verification

The point of the whole exercise. None of this can be tested any other way, and everything here is a first: first real certificate, first `wss://`, first Linux shutdown path.

**Files:** none.

- [ ] **Step 1: Certificate and redirects, from a phone**

Open `https://whosego.app` on a phone. No certificate warning. `http://` and `www.` both land on the apex over HTTPS.

- [ ] **Step 2: Two phones on mobile data, not wifi**

Turn wifi **off** on both. Host creates a room on one; the other joins by code. Advance turns in both directions and confirm both screens update in real time. Wifi would hide exactly the carrier-NAT and mobile-network behaviour this is meant to expose.

- [ ] **Step 3: QR scan from a third device**

Scan the host's QR. It should land directly in the room via the deep link, with no code typing.

- [ ] **Step 4: Room lock**

Host locks the room. A new device is refused with the "Can't join" screen. A player who was already in, and then force-quit the browser, rejoins successfully by token — the lock must not block a token rejoin.

- [ ] **Step 5: The heartbeat test**

Background one phone for **5 minutes** — lock the screen and leave it. Return to it. The connection should still be live with current state, with no reconnect spinner and no drop to the landing page.

This is the only real test of the 30s ping / 90s timeout; the unit tests cover the decision logic but nothing exercises a live idle socket.

- [ ] **Step 6: Redeploy mid-game**

With a game in progress, push a trivial commit to `main` (or re-run the deploy job). Clients should show a brief reconnect and then resume with state intact — rooms reload from the snapshot and players rejoin by token.

- [ ] **Step 7: The graceful-shutdown snapshot**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP>
cd /opt/turn-tracker
docker compose logs app --tail 5
docker compose down
docker compose logs app --tail 5 2>/dev/null || true
docker compose up -d
sleep 10
docker compose logs app | grep -E "restored|final snapshot"
```

Expected: `final snapshot written on shutdown` before the stop, and `restored N room(s) from snapshot` after the start, with N matching the rooms that existed.

This path has **never been executed** — Windows hard-kills on SIGTERM before Actix drains, so it was verified by code reading only. This is the first real proof.

- [ ] **Step 8: Confirm the gate from a device with no token**

On a device that has never used a `?k=` link, try to create a room. Expect the invite-only message, not a generic error.

- [ ] **Step 9: Record the results**

Note anything surprising against the spec. Failures here feed back into the code-changes plan, not into patching the box by hand.

---

### Task 7: Run the beta, then open up

**Files:** none.

- [ ] **Step 1: Distribute host links**

Send each host `https://whosego.app/?k=<TOKEN>` once, with a note that it only needs opening a single time on each device they intend to host from. Everyone else needs nothing — they join by room link or QR.

- [ ] **Step 2: Open to everyone, when ready**

```bash
ssh -i ~/.ssh/turn-tracker-deploy deploy@<BOX_IP>
cd /opt/turn-tracker
sed -i '/^TT_CREATE_TOKEN=/d' .env
docker compose up -d
docker compose logs app | grep TT_CREATE_TOKEN
```

Expected: `TT_CREATE_TOKEN is unset: room creation is OPEN to anyone`.

The gate code stays in place, inert. Removing it is a one-commit cleanup if you want it gone.

- [ ] **Step 3: Consider an uptime check**

UptimeRobot (free) against `https://whosego.app/health`, 5-minute interval, with push notifications. Worth doing at this point rather than during the beta, when you are watching it live anyway.

- [ ] **Step 4: Revisit the parked decisions**

Spec §13 holds two deferred questions — whether the repository stays public, and whether a non-rolling deploy is the right call. Both are now informed by real operational experience.

---

## Done criteria

- `https://whosego.app` serves the app with a valid certificate; `http://` and `www.` redirect to it.
- A push to `main` results in a deployed change with no manual step.
- All nine checks in Task 6 pass on real phones over mobile data.
- The app is unreachable except through Caddy (port 8080 refuses connections from outside).
- `docker compose logs app` shows neither the `ALLOWED_ORIGINS` warning nor the `TT_CREATE_TOKEN` open notice while the beta is running.
