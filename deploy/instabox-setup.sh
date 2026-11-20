#!/usr/bin/env bash
# One-shot InstaBox/VPS setup — run FROM the Mac:
#   SSH_HOST='ibssh_TOKEN@ssh.instabox.sh' SSH_PORT=2222 \
#     ./deploy/instabox-setup.sh [DOMAIN] [IG_SESSIONID]
# Installs docker, syncs the repo (no secrets, no build artifacts), builds
# both images on the VPS, writes /opt/ig/.env, and brings the stack up.
set -euo pipefail

SSH_HOST=${SSH_HOST:?set SSH_HOST=user@host}
SSH_PORT=${SSH_PORT:-22}
DOMAIN=${1:-}
IG_SESSIONID=${2:-}
SSH="ssh -p $SSH_PORT -o StrictHostKeyChecking=accept-new $SSH_HOST"
TAG=ghcr.io/syahrularifuddin/ig-grabber

echo "==> 1/5 docker"
$SSH "curl -fsSL https://get.docker.com | sudo sh >/dev/null && sudo usermod -aG docker \$(whoami)"

echo "==> 2/5 sync source (no secrets, no build artifacts)"
rsync -az --delete -e "$SSH" \
  --exclude .git --exclude node_modules --exclude .next --exclude backend/target \
  --exclude .env --exclude .env.* --exclude backend/.env \
  "$(dirname "$0")/.." "$SSH_HOST:/opt/ig/"

echo "==> 3/5 build images on the VPS (BoringSSL takes a few minutes)"
$SSH "cd /opt/ig && docker build -t $TAG/backend:latest ./backend && docker build -t $TAG/frontend:latest ./frontend"

echo "==> 4/5 write .env + up"
ENVFILE=$(mktemp)
printf 'DOMAIN=%s\nIG_SESSIONID=%s\n' "$DOMAIN" "$IG_SESSIONID" > "$ENVFILE"
rsync -az -e "$SSH" "$ENVFILE" "$SSH_HOST:/opt/ig/.env" && rm -f "$ENVFILE"
$SSH "cd /opt/ig && sudo chown 65534:65534 .env && docker compose up -d"

echo "==> 5/5 status"
$SSH "cd /opt/ig && docker compose ps"
echo "Reminder: save a machine snapshot now — if InstaBox resets the box, rerun this script."
