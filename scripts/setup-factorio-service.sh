#!/usr/bin/env bash
set -euo pipefail

# Update factorio.service to work with the seasons system
# Usage: ssh -p 55555 root@xxx.xxx.xxx.xxx 'bash -s' < scripts/setup-factorio-service.sh
#
# Requires /root/factorio-seasons/rcon.pw to exist with the RCON password.

UNIT_FILE="/etc/systemd/system/factorio.service"
RCON_PW_FILE="/root/factorio-seasons/rcon.pw"

if [ ! -f "$RCON_PW_FILE" ]; then
    echo "ERROR: $RCON_PW_FILE not found. Create it first."
    exit 1
fi

RCON_PW=$(cat "$RCON_PW_FILE")

echo "==> Backing up current unit file..."
cp "$UNIT_FILE" "${UNIT_FILE}.bak"

echo "==> Writing updated factorio.service..."
cat > "$UNIT_FILE" <<EOF
[Unit]
Description=Factorio Dedicated Server
After=network.target

[Service]
Type=simple
ExecStart=/opt/factorio/bin/x64/factorio \\
    --start-server /opt/factorio/saves/current.zip \\
    --server-settings /opt/factorio/data/server-settings.json \\
    --map-gen-settings /opt/factorio/data/map-gen-settings.json \\
    --rcon-bind 0.0.0.0:27015 \\
    --rcon-password "${RCON_PW}"
WorkingDirectory=/opt/factorio
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

echo "==> Reloading systemd..."
systemctl daemon-reload

echo "==> Restarting factorio.service..."
systemctl restart factorio.service
sleep 2
systemctl status factorio.service --no-pager

echo "==> Done. RCON enabled on 0.0.0.0:27015"
