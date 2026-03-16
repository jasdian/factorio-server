#!/usr/bin/env bash
set -euo pipefail

# Sync local factorio/ directory to remote server
# Usage: ./scripts/sync-factorio.sh
# Run once initially and whenever Factorio is updated locally.

SERVER="root@xxx.xxx.xxx.xxx"
SSH_PORT="55555"
REMOTE_DIR="/opt/factorio/"
LOCAL_DIR="factorio/"

if [ ! -d "$LOCAL_DIR" ]; then
    echo "ERROR: Local factorio/ directory not found"
    exit 1
fi

echo "==> Syncing factorio/ to ${SERVER}:${REMOTE_DIR}..."
rsync -avz --delete -e "ssh -p ${SSH_PORT}" "$LOCAL_DIR" "${SERVER}:${REMOTE_DIR}"

echo "==> Sync complete."
