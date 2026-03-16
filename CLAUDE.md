# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust backend for a seasonal Factorio server with Ethereum payment integration. Manages player registrations, promo codes, access tiers (Spectator vs Player), automated weekly season rotation, and ETH payment verification. The full specification lives in `factorio-seasons-plan-v3.md` with logging details in `factorio-seasons-logging-addendum.md`.

**Status:** Architectural planning complete, implementation not yet started. No Cargo.toml or source code exists yet.

## Tech Stack

- **Language:** Rust (latest stable)
- **Web framework:** Axum (async HTTP on Tokio)
- **Database:** SQLite via sqlx (async, compile-time checked queries)
- **Blockchain:** Alloy (Ethereum RPC client)
- **Server control:** RCON protocol to Factorio dedicated server
- **Logging:** `tracing` + `tracing-subscriber` with JSON output to journald
- **Dev environment:** Nix (`nix-shell` to enter)

## Build Commands

```bash
# Enter dev environment
nix-shell

# Development build
cargo build
cargo run

# Production static binary (MUSL)
cargo build --release --no-default-features --target x86_64-unknown-linux-musl

# Tests, lint, format
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## Architecture

### System Layout

The backend (`factorio-seasons.service`) runs alongside the Factorio game server (`factorio.service`) on the same Linux host. Caddy/Nginx terminates TLS and proxies to `localhost:3000`.

### Key Domain Concepts

- **Access Tiers:** Standard registration → Spectator this season, Player next season. Instant Player promo → Player immediately + next season.
- **Carry-Forward:** All Confirmed registrations auto-become Players in the next season.
- **Payment Matching:** Single deposit address + unique wei offset per registration (no per-user addresses).
- **Spectator Enforcement:** Lua mod + `game.permissions` restricts spectators from building/crafting. Backend writes `spectators.json` and `whitelist.json` that the mod reads.
- **Season Rotation:** Stop server → archive save → create new season in DB → carry forward players → generate fresh map → update symlink (`/opt/factorio/saves/current.zip`) → restart server.

### Background Services

1. **ETH payment poller** (~15s interval): scans new blocks for matching transfers
2. **Season rotation scheduler** (weekly): full rotation cycle with carry-forward
3. **Registration expiry cleanup**: marks stale AwaitingPayment registrations as Expired

### API Structure

- **Public:** `POST /api/register`, `GET /api/season`, `GET /api/seasons`, `GET /api/register/{id}`, `GET /api/maps/{season_id}`
- **Admin** (bearer token): `POST/GET/DELETE /api/admin/promo`, `POST /api/admin/rotate`, `GET /api/admin/registrations`

### Database Tables

Three tables: `seasons` (id, status, dates, map_seed, save_path), `registrations` (UUID id, season_id, factorio_name, eth_address, tx_hash, promo_code, status, access_tier, amount_wei, timestamps), `promo_codes` (code, discount_percent, grants_instant_player, max_uses, times_used, active, timestamps).

### Key Invariants

- Exactly one Active season at any time
- Promo code usage incremented atomically via sqlx transaction (prevents race on max_uses)
- 100% discount promos skip ETH payment entirely (instant confirmation)
- Rotation carry-forward deduplicates (skips players who already registered for new season)
- Only last 3 archived seasons retain downloadable saves

## Deployment

- Backend runs as `factorio-seasons` user with sudoers permission to stop/start/restart `factorio.service`
- Config expected in TOML format covering server port, admin token, RCON credentials, ETH RPC URL, deposit address, base fee, and database path
- Logs queryable via `journalctl -u factorio-seasons -o json | jq`
