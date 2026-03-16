use std::path::Path;
use std::sync::Arc;

use crate::db::{RegistrationRepo, RotationRepo, SeasonRepo, SqliteRepo};
use crate::domain::{AccessTier, RegStatus, SeasonId};
use crate::error::AppResult;
use crate::services::rcon::{rcon_command, rcon_command_best_effort, RconConfig};

#[derive(serde::Serialize)]
struct WhitelistEntry {
    name: String,
}

fn lua_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Lua sent via RCON on startup. Creates the Spectators permission group with
/// restricted actions. Does NOT register event handlers (that would cause a
/// "mod event handlers not identical" mismatch with clients).
const SETUP_LUA: &str = "\
do \
  local g = game.permissions.get_group('Spectators') \
  if not g then g = game.permissions.create_group('Spectators') end \
  local restrict = { \
    'build','build_rail','build_terrain','begin_mining','begin_mining_terrain', \
    'craft','cancel_craft','deconstruct','cancel_deconstruct','upgrade','cancel_upgrade', \
    'drop_item','destroy_item','destroy_opened_item', \
    'cursor_split','cursor_transfer','inventory_split','inventory_transfer', \
    'fast_entity_transfer','fast_entity_split','stack_split','stack_transfer', \
    'send_stack_to_trash','send_stacks_to_trash','trash_not_requested_items', \
    'place_equipment','take_equipment', \
    'rotate_entity','flip_entity','paste_entity_settings','copy_entity_settings', \
    'use_item','start_repair','wire_dragging','remove_cables', \
    'select_area','alt_select_area','reverse_select_area','alt_reverse_select_area', \
    'setup_assembling_machine','reset_assembling_machine', \
    'toggle_driving','send_spidertron','launch_rocket', \
    'connect_rolling_stock','disconnect_rolling_stock', \
    'setup_blueprint','change_shooting_state','market_offer' \
  } \
  for _,a in pairs(restrict) do \
    local ia = defines.input_action[a] \
    if ia then g.set_allows_action(ia, false) end \
  end \
end";

/// RCON command to assign a connected player to the Default (full-access) group.
fn rcon_set_player(name: &str) -> String {
    let escaped = lua_escape(name);
    format!(
        "/silent-command do \
         local p = game.get_player('{escaped}') \
         if p and p.connected then \
         p.permission_group = game.permissions.get_group('Default') \
         end end"
    )
}

/// RCON command to assign a connected player to the Spectators (restricted) group.
fn rcon_set_spectator(name: &str) -> String {
    let escaped = lua_escape(name);
    format!(
        "/silent-command do \
         local p = game.get_player('{escaped}') \
         if p and p.connected then \
         p.permission_group = game.permissions.get_group('Spectators') \
         end end"
    )
}

/// Lua that returns connected players, their permission groups, and whether
/// their character is alive as `name:group:alive,name:group:dead,...`.
const POLL_LUA: &str = "/silent-command \
    do local out={} \
    for _,p in pairs(game.connected_players) do \
    local alive = (p.character and p.character.valid) and 'alive' or 'dead' \
    out[#out+1]=p.name..':'..(p.permission_group and p.permission_group.name or 'nil')..':'..alive \
    end rcon.print(table.concat(out,',')) end";

pub async fn write_whitelist_file(
    repo: &SqliteRepo,
    season_id: SeasonId,
    data_dir: &Path,
) -> AppResult<usize> {
    let names = repo.get_confirmed_player_names(season_id).await?;
    let entries: Vec<WhitelistEntry> = names
        .iter()
        .map(|n| WhitelistEntry { name: n.clone() })
        .collect();
    let json = serde_json::to_string_pretty(&entries)?;
    let path = data_dir.join("server-whitelist.json");
    tokio::fs::write(&path, &json).await?;
    let count = entries.len();
    tracing::info!(
        player_count = count,
        season_id = season_id.as_i64(),
        path = %path.display(),
        "whitelist_updated"
    );
    Ok(count)
}

pub async fn write_spectator_file(
    repo: &SqliteRepo,
    season_id: SeasonId,
    data_dir: &Path,
) -> AppResult<usize> {
    let names = repo.get_spectator_names(season_id).await?;
    let json = serde_json::to_string_pretty(&names)?;
    let path = data_dir.join("spectators.json");
    tokio::fs::write(&path, &json).await?;
    let count = names.len();
    tracing::info!(
        spectator_count = count,
        season_id = season_id.as_i64(),
        "spectator_list_updated"
    );
    Ok(count)
}

pub async fn write_empty_spectator_file(data_dir: &Path) -> AppResult<()> {
    let path = data_dir.join("spectators.json");
    tokio::fs::write(&path, "[]").await?;
    Ok(())
}

pub async fn apply_whitelist_for_registration(
    repo: &SqliteRepo,
    rcon_config: &RconConfig,
    data_dir: &Path,
    factorio_name: &str,
    access_tier: AccessTier,
    season_id: SeasonId,
) -> AppResult<()> {
    let season = repo.get_season_by_id(season_id).await?;
    let is_active = season
        .as_ref()
        .map(|s| s.status == crate::domain::SeasonStatus::Active)
        .unwrap_or(false);

    if !is_active {
        tracing::debug!(
            factorio_name,
            season_id = season_id.as_i64(),
            "skipping whitelist apply for non-active season"
        );
        return Ok(());
    }

    rcon_command_best_effort(rcon_config, &format!("/whitelist add {factorio_name}")).await;

    write_spectator_file(repo, season_id, data_dir).await?;

    // Assign permission group if the player is currently online
    match access_tier {
        AccessTier::Standard => {
            rcon_command_best_effort(rcon_config, &rcon_set_spectator(factorio_name)).await;
        }
        AccessTier::InstantPlayer => {
            rcon_command_best_effort(rcon_config, &rcon_set_player(factorio_name)).await;
        }
    }

    tracing::info!(
        factorio_name,
        reason = "registration",
        "whitelist_player_added"
    );
    Ok(())
}

/// Set up spectator permission group, enable whitelist, and sync all confirmed
/// registrations for the active season via RCON. Call once at backend startup.
pub async fn sync_whitelist_to_server(
    repo: &SqliteRepo,
    rcon_config: &RconConfig,
    data_dir: &Path,
) -> AppResult<()> {
    let season = repo.get_active_season().await?;

    // 1. Enable Factorio's built-in whitelist
    rcon_command_best_effort(rcon_config, "/whitelist enable").await;
    tracing::info!("whitelist_enabled");

    // 2. Create the Spectators permission group (needs "repeat to confirm" on
    //    fresh saves because of the achievements-disabled prompt).
    let setup_cmd = format!("/silent-command {SETUP_LUA}");
    rcon_command_best_effort(rcon_config, &setup_cmd).await;
    rcon_command_best_effort(rcon_config, &setup_cmd).await;
    tracing::info!("spectator_permissions_setup_complete");

    // 3. Write JSON files
    write_whitelist_file(repo, season.id, data_dir).await?;
    write_spectator_file(repo, season.id, data_dir).await?;

    // 4. Sync each confirmed registration via RCON whitelist
    let regs = repo.list_registrations_for_season(season.id).await?;
    let mut players = 0u32;
    let mut spectators = 0u32;
    for reg in &regs {
        if reg.status != RegStatus::Confirmed {
            continue;
        }
        let name = reg.factorio_name.as_str();
        rcon_command_best_effort(rcon_config, &format!("/whitelist add {name}")).await;

        match reg.access_tier {
            AccessTier::Standard => {
                spectators += 1;
            }
            AccessTier::InstantPlayer => {
                players += 1;
            }
        }
    }

    tracing::info!(
        season_id = season.id.as_i64(),
        players_synced = players,
        spectators_synced = spectators,
        "whitelist_sync_complete"
    );
    Ok(())
}

/// Background poller that checks connected players every few seconds and
/// assigns the correct permission group based on their registration status.
/// In hardcore mode, also detects deaths and demotes players to spectator.
pub fn spawn_permission_poller(
    repo: SqliteRepo,
    rcon_config: Arc<RconConfig>,
    hardcore: bool,
) {
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(5);
        tracing::info!(hardcore, "permission_poller_started");
        loop {
            tokio::time::sleep(interval).await;
            if let Err(e) =
                poll_and_assign_permissions(&repo, &rcon_config, hardcore).await
            {
                tracing::warn!(error = %e, "permission_poll_failed");
            }
        }
    });
}

/// RCON command to whisper a message to a specific player.
fn rcon_whisper(name: &str, msg: &str) -> String {
    let escaped_name = lua_escape(name);
    let escaped_msg = lua_escape(msg);
    format!(
        "/silent-command do \
         local p = game.get_player('{escaped_name}') \
         if p then p.print('{escaped_msg}') end end"
    )
}

async fn poll_and_assign_permissions(
    repo: &SqliteRepo,
    rcon_config: &RconConfig,
    hardcore: bool,
) -> AppResult<()> {
    // 1. Get connected players from Factorio
    let response = rcon_command(rcon_config, POLL_LUA).await?;
    let response = response.trim();
    if response.is_empty() {
        return Ok(()); // No one online
    }

    // 2. Parse "name:group:alive,name:group:alive,..."
    let season = repo.get_active_season().await?;

    for entry in response.split(',') {
        let parts: Vec<&str> = entry.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let name = parts[0];
        let current_group = parts[1];
        let alive = parts[2];

        // 3. Look up the player's registration for the active season
        let factorio_name = match crate::domain::FactorioName::try_from(name.to_string()) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let reg = repo
            .find_registration_by_name_and_season(&factorio_name, season.id)
            .await?;

        // 4. Hardcore: if a player died, demote them to spectator
        if hardcore && alive == "dead" {
            if let Some(ref r) = reg {
                if r.status == RegStatus::Confirmed
                    && r.access_tier == AccessTier::InstantPlayer
                {
                    let demoted = repo
                        .demote_to_spectator(&factorio_name, season.id)
                        .await?;
                    if demoted {
                        tracing::info!(
                            player = name,
                            season_id = season.id.as_i64(),
                            "hardcore_death_demoted"
                        );
                        rcon_command_best_effort(
                            rcon_config,
                            &rcon_set_spectator(name),
                        )
                        .await;
                        rcon_command_best_effort(
                            rcon_config,
                            &rcon_whisper(
                                name,
                                "[HARDCORE] You died. You are now a spectator for the rest of this season. See you next week.",
                            ),
                        )
                        .await;
                        continue;
                    }
                }
            }
        }

        // 5. Determine desired permission group
        let desired_group = match reg {
            Some(ref r) if r.status == RegStatus::Confirmed => match r.access_tier {
                AccessTier::InstantPlayer => "Default",
                AccessTier::Standard => "Spectators",
            },
            _ => "Spectators",
        };

        // 6. Fix if wrong
        if current_group != desired_group {
            tracing::info!(
                player = name,
                from = current_group,
                to = desired_group,
                "permission_group_corrected"
            );
            match desired_group {
                "Default" => {
                    rcon_command_best_effort(rcon_config, &rcon_set_player(name)).await;
                }
                _ => {
                    rcon_command_best_effort(rcon_config, &rcon_set_spectator(name))
                        .await;
                }
            }
        }
    }

    Ok(())
}
