#!/usr/bin/env bash
set -euo pipefail

# Deploy factorio-seasons to production server (Docker-based)
# Usage: nix-shell --run ./scripts/deploy.sh

SERVER="root@xxx.xxx.xxx.xxx"
SSH_PORT="55555"
REMOTE_DIR="/root/factorio-seasons"
BINARY_NAME="factorio-seasons"

SSH_CMD="ssh -p ${SSH_PORT} ${SERVER}"
SCP_CMD="scp -P ${SSH_PORT}"

echo "==> Building release binary (musl static)..."
cargo build --release --target x86_64-unknown-linux-musl

BINARY="target/x86_64-unknown-linux-musl/release/${BINARY_NAME}"
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

echo "==> Binary size: $(du -h "$BINARY" | cut -f1)"

echo "==> Creating remote directories..."
$SSH_CMD "mkdir -p ${REMOTE_DIR}/{static,migrations}"

echo "==> Copying files to server..."
$SCP_CMD "$BINARY" "${SERVER}:${REMOTE_DIR}/${BINARY_NAME}"
$SCP_CMD Dockerfile "${SERVER}:${REMOTE_DIR}/Dockerfile"
$SCP_CMD compose.yml "${SERVER}:${REMOTE_DIR}/compose.yml"
$SCP_CMD config.prod.toml "${SERVER}:${REMOTE_DIR}/config.prod.toml"
rsync -avz -e "ssh -p ${SSH_PORT}" static/ "${SERVER}:${REMOTE_DIR}/static/"
rsync -avz -e "ssh -p ${SSH_PORT}" migrations/ "${SERVER}:${REMOTE_DIR}/migrations/"

echo "==> Building and restarting container..."
$SSH_CMD "cd ${REMOTE_DIR} && docker compose up -d --build"

echo "==> Container logs:"
$SSH_CMD "cd ${REMOTE_DIR} && docker compose logs --tail 20"

echo "==> Deploy complete."
