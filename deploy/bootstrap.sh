#!/usr/bin/env bash
# One-time host setup for the turn-tracker box.
#
# Run as root on a fresh Ubuntu 24.04 (ARM64) server:
#   bash bootstrap.sh "$(cat ~/.ssh/turn-tracker-deploy.pub)"
#
# Idempotent: safe to re-run.
set -euo pipefail

DEPLOY_KEY="${1:?usage: bootstrap.sh <deploy-ssh-public-key>}"

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get upgrade -y
apt-get install -y ca-certificates curl ufw unattended-upgrades

# --- deploy user -----------------------------------------------------------
# Owns /opt/turn-tracker and the docker socket. Deliberately has NO sudo: it
# only ever needs to run docker compose.
id -u deploy >/dev/null 2>&1 || useradd -m -s /bin/bash deploy
install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
printf '%s\n' "$DEPLOY_KEY" > /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 600 /home/deploy/.ssh/authorized_keys

# --- docker ----------------------------------------------------------------
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
cat > /etc/apt/sources.list.d/docker.list <<SOURCES
deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable
SOURCES
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
usermod -aG docker deploy

# --- firewall --------------------------------------------------------------
# The app's port 8080 is deliberately absent: it must be reachable only through
# Caddy on the internal compose network, or X-Forwarded-For becomes forgeable.
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# --- ssh hardening ---------------------------------------------------------
sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl reload ssh

# --- automatic security updates --------------------------------------------
dpkg-reconfigure -f noninteractive unattended-upgrades

install -d -o deploy -g deploy /opt/turn-tracker

echo
echo "bootstrap complete."
echo "Next: scp docker-compose.yml, Caddyfile and .env.example to"
echo "      deploy@<box>:/opt/turn-tracker/ and create .env from the example."
