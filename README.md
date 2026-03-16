# factorio-seasons

Rust backend for a seasonal Factorio server with Ethereum payment integration. Manages player registrations, promo codes, access tiers (Spectator vs Player), automated weekly season rotation, and ETH payment verification.

The Factorio game server runs natively on the host. This backend runs in Docker behind nginx-proxy for TLS termination.

## Requirements

- [Nix](https://nixos.org/download/) (provides Rust toolchain, MUSL cross-compiler, and all dependencies)
- Docker + Docker Compose on the remote server
- nginx-proxy (jwilder) on the `nging-proxy-external` Docker network

## Development

```bash
nix-shell                # enter dev environment
cargo build              # development build
cargo test               # run tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## Configuration

Copy `test-config.toml` as a starting point. Production config uses `config.prod.toml` (gitignored).

Key config sections: `[server]`, `[factorio]`, `[eth]`, `[schedule]`, `[admin]`, `[database]`, `[logging]`.

See `config.prod.toml` for Docker-specific paths and `test-config.toml` for local development.

## Deployment

### First-time server setup

```bash
# 1. Configure firewall and create directories
ssh -p 55555 root@xxx.xxx.xxx.xxx 'bash -s' < scripts/setup-server.sh

# 2. Sync Factorio installation to server
./scripts/sync-factorio.sh

# 3. Set up Factorio systemd service (RCON, current.zip symlink)
ssh -p 55555 root@xxx.xxx.xxx.xxx 'bash -s' < scripts/setup-factorio-service.sh

# 4. Create RCON password file on server
ssh -p 55555 root@xxx.xxx.xxx.xxx 'echo "your-rcon-password" > /root/factorio-seasons/rcon.pw'

# 5. Fill in secrets in config.prod.toml (CHANGEME_ placeholders)
```

### Deploy (repeatable)

```bash
nix-shell --run ./scripts/deploy.sh
```

Builds a static MUSL binary locally in nix-shell, ships it to the server with the Dockerfile and compose config, then runs `docker compose up -d --build`.

### Verify

```bash
ssh -p 55555 root@xxx.xxx.xxx.xxx 'docker compose -f /root/factorio-seasons/compose.yml logs --tail 30'
curl https://factorio.princeofcrypto.com/health
curl https://factorio.princeofcrypto.com/api/season
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Remote Host (xxx.xxx.xxx.xxx)                        │
│                                                     │
│  ┌──────────────┐    ┌───────────────────────────┐  │
│  │ nginx-proxy  │───▸│ factorio-seasons (Docker)  │  │
│  │ :443 TLS     │    │ :3000 (Axum)              │  │
│  └──────────────┘    │                           │  │
│                      │  RCON ──▸ host.docker.internal:27015  │
│                      │  D-Bus ──▸ systemctl      │  │
│                      │  /opt/factorio (bind mount)│  │
│                      └───────────────────────────┘  │
│                                                     │
│  ┌───────────────────────────┐                      │
│  │ Factorio server (native)  │                      │
│  │ factorio.service :34197   │                      │
│  └───────────────────────────┘                      │
└─────────────────────────────────────────────────────┘
```

### API

**Public:**
- `GET /health` — health check
- `GET /api/season` — current season info
- `GET /api/seasons` — all seasons
- `POST /api/register` — player registration
- `GET /api/register/{id}` — registration status
- `GET /api/maps/{season_id}` — download archived map

**Admin** (bearer token):
- `GET /api/admin/registrations` — list registrations
- `POST /api/admin/promo` — create promo code
- `GET /api/admin/promo` — list promo codes
- `DELETE /api/admin/promo/{code}` — revoke promo code
- `POST /api/admin/rotate` — force season rotation

### Background services

- **ETH payment poller** (~15s) — scans blocks for matching transfers
- **Season rotation scheduler** — weekly rotation with carry-forward
- **Registration expiry cleanup** — marks stale payments as expired
- **Permission poller** — enforces spectator/player groups on connected players

## Scripts

| Script | Purpose |
|---|---|
| `scripts/deploy.sh` | Build + ship + restart container |
| `scripts/sync-factorio.sh` | Sync local `factorio/` to remote `/opt/factorio/` |
| `scripts/setup-server.sh` | One-time firewall + directory setup |
| `scripts/setup-factorio-service.sh` | Set up Factorio systemd unit with RCON |
| `scripts/backup.sh` | Database + config backup (cron) |
