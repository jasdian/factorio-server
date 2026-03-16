#!/usr/bin/env bash
set -euo pipefail

# One-time server setup: firewall rules + directories
# Usage: ssh -p 55555 root@xxx.xxx.xxx.xxx 'bash -s' < scripts/setup-server.sh

echo "==> Creating directories..."
mkdir -p /root/factorio-seasons
mkdir -p /opt/factorio/saves
mkdir -p /opt/factorio/data

echo "==> Configuring UFW firewall..."
apt-get update && apt-get install -y ufw

ufw default deny incoming
ufw default allow outgoing
ufw allow 55555/tcp comment "SSH"
ufw allow 80/tcp comment "HTTP"
ufw allow 443/tcp comment "HTTPS"
ufw allow 34197/udp comment "Factorio"
# RCON from Docker containers only (not external)
ufw allow from 172.16.0.0/12 to any port 27015 proto tcp comment "RCON from Docker"

echo "==> Enabling UFW..."
ufw --force enable
ufw status verbose

echo "==> Server setup complete."
