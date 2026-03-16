# Spectator Control

**Not a Factorio mod** — this Lua logic is sent to the Factorio server via RCON
on backend startup. No client-side mod installation required.

The setup script (embedded in `src/services/whitelist.rs` as `SETUP_LUA`):

1. Creates a "Spectators" permission group with restricted actions (no building,
   mining, crafting, etc.)
2. Initialises `storage.season_players` table in the save
3. Registers an `on_player_joined_game` handler that defaults all players to
   Spectator unless they are marked as `"player"` in the storage table

The backend then manages individual players via `/silent-command` RCON calls
that update `storage.season_players` and reassign permission groups in real time.

## Files

- `control.lua` — reference copy of the Lua logic (the authoritative version
  is the `SETUP_LUA` constant in `src/services/whitelist.rs`)
