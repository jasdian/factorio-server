#!/usr/bin/env bash
set -euo pipefail

# Backup factorio-seasons database and config
# Usage: ./scripts/backup.sh [backup_dir]
#
# Intended to run as a daily cron job:
#   0 3 * * * /opt/factorio-seasons/scripts/backup.sh /opt/factorio-seasons/backups

BACKUP_DIR="${1:-/opt/factorio-seasons/backups}"
DB_PATH="/var/lib/fseasons/seasons.db"
CONFIG_PATH="/opt/factorio-seasons/config.toml"
KEEP_DAYS=14
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

# SQLite online backup (safe while the database is in use)
BACKUP_FILE="${BACKUP_DIR}/seasons_${TIMESTAMP}.db"
if [ -f "$DB_PATH" ]; then
    sqlite3 "$DB_PATH" ".backup '${BACKUP_FILE}'"
    gzip "$BACKUP_FILE"
    echo "Database backed up to ${BACKUP_FILE}.gz"
else
    echo "WARNING: Database not found at $DB_PATH"
fi

# Config backup
if [ -f "$CONFIG_PATH" ]; then
    cp "$CONFIG_PATH" "${BACKUP_DIR}/config_${TIMESTAMP}.toml"
    echo "Config backed up"
fi

# Prune old backups
find "$BACKUP_DIR" -name "seasons_*.db.gz" -mtime "+${KEEP_DAYS}" -delete
find "$BACKUP_DIR" -name "config_*.toml" -mtime "+${KEEP_DAYS}" -delete
echo "Pruned backups older than ${KEEP_DAYS} days"

echo "Backup complete."
