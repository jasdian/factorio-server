use std::path::Path;
use std::time::Instant;

use chrono::{TimeDelta, Utc};

use crate::config::AppConfig;
use crate::db::{RotationRepo, SeasonRepo, SqliteRepo};
use crate::domain::SeasonId;
use crate::error::{AppError, AppResult};
use crate::services::rcon::{rcon_command_best_effort, RconConfig};
use crate::services::whitelist;

pub async fn rotate_season(
    repo: &SqliteRepo,
    config: &AppConfig,
    rcon_config: &RconConfig,
    force: bool,
) -> AppResult<()> {
    let started = Instant::now();
    let current = repo.get_active_season().await?;
    let new_id = current.id.next();

    // Step 0: Guard
    if !force {
        let now = Utc::now();
        if now < current.ends_at {
            tracing::info!(
                season_id = current.id.as_i64(),
                ends_at = %current.ends_at,
                "rotation_skipped_not_ended"
            );
            return Ok(());
        }
    }

    tracing::info!(
        current_season = current.id.as_i64(),
        new_season = new_id.as_i64(),
        "rotation_started"
    );

    // Step 1: RCON /save
    tracing::info!(step = "save_game", "rotation_step");
    rcon_command_best_effort(rcon_config, "/save").await;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Step 2: RCON /quit
    tracing::info!(step = "quit_server", "rotation_step");
    rcon_command_best_effort(rcon_config, "/quit").await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Step 3: systemctl stop
    tracing::info!(step = "systemctl_stop", "rotation_step");
    run_systemctl("stop").await?;

    // Step 4: Archive save
    tracing::info!(step = "archive_save", "rotation_step");
    let archive_dir = Path::new(&config.factorio.archive_dir);
    let saves_dir = Path::new(&config.factorio.saves_dir);
    let archive_path = archive_dir.join(format!("season_{}.zip", current.id.as_i64()));

    tokio::fs::create_dir_all(archive_dir)
        .await
        .map_err(|e| AppError::Rotation(format!("failed to create archive dir: {e}")))?;

    let save_source = saves_dir.join("current.zip");
    if save_source.exists() {
        tokio::fs::copy(&save_source, &archive_path)
            .await
            .map_err(|e| {
                AppError::Rotation(format!(
                    "failed to archive save from {}: {e}",
                    save_source.display()
                ))
            })?;
    } else {
        return Err(AppError::Rotation(format!(
            "save file not found: {}",
            save_source.display()
        )));
    }

    // Step 5: DB archive season
    tracing::info!(step = "archive_season_db", "rotation_step");
    repo.archive_season(current.id).await?;
    repo.update_season_save_path(current.id, &archive_path.to_string_lossy())
        .await?;

    // Step 6: DB create new season
    tracing::info!(step = "create_season_db", "rotation_step");
    let now = Utc::now();
    let season_duration_days = config.schedule.season_duration_days;
    let ends_at = now + TimeDelta::days(i64::from(season_duration_days));
    repo.create_season(new_id, now, ends_at).await?;

    // Step 7: Carry forward
    tracing::info!(step = "carry_forward", "rotation_step");
    let carried = repo.carry_forward_players(current.id, new_id).await?;
    tracing::info!(
        from_season = current.id.as_i64(),
        to_season = new_id.as_i64(),
        players_carried = carried,
        "carry_forward_complete"
    );

    // Step 8: Generate map
    tracing::info!(step = "generate_map", "rotation_step");
    generate_map(config, new_id).await?;

    // Step 9: Update symlink
    tracing::info!(step = "update_symlink", "rotation_step");
    let symlink_path = saves_dir.join("current.zip");
    let new_save = saves_dir.join(format!("season_{}.zip", new_id.as_i64()));
    // Remove old symlink (or file)
    let _ = tokio::fs::remove_file(&symlink_path).await;
    tokio::fs::symlink(&new_save, &symlink_path)
        .await
        .map_err(|e| {
            AppError::Rotation(format!(
                "failed to create symlink {} -> {}: {e}",
                symlink_path.display(),
                new_save.display()
            ))
        })?;

    // Step 10: Build whitelist
    tracing::info!(step = "build_whitelist", "rotation_step");
    let data_dir = Path::new(&config.factorio.data_dir);
    whitelist::write_whitelist_file(repo, new_id, data_dir).await?;
    whitelist::write_empty_spectator_file(data_dir).await?;

    // Step 11: Update server name
    tracing::info!(step = "update_server_name", "rotation_step");
    update_server_name(config, new_id).await?;

    // Step 12: systemctl start
    tracing::info!(step = "systemctl_start", "rotation_step");
    run_systemctl("start").await?;

    // Step 13: Purge old archives
    tracing::info!(step = "purge_archives", "rotation_step");
    if let Err(e) = purge_old_archives(repo, archive_dir, 3).await {
        tracing::warn!(error = %e, "archive_purge_failed");
    }

    let duration_ms = started.elapsed().as_millis();
    tracing::info!(
        new_season = new_id.as_i64(),
        duration_ms = duration_ms as u64,
        "rotation_complete"
    );

    Ok(())
}

async fn run_systemctl(action: &str) -> AppResult<()> {
    let output = tokio::process::Command::new("systemctl")
        .args([action, "factorio.service"])
        .output()
        .await
        .map_err(|e| AppError::Rotation(format!("failed to run systemctl {action}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Rotation(format!(
            "systemctl {action} failed: {stderr}"
        )));
    }
    Ok(())
}

async fn generate_map(config: &AppConfig, season_id: SeasonId) -> AppResult<()> {
    let saves_dir = Path::new(&config.factorio.saves_dir);
    let output_path = saves_dir.join(format!("season_{}.zip", season_id.as_i64()));

    let output = tokio::process::Command::new(&config.factorio.binary)
        .arg("--create")
        .arg(&output_path)
        .arg("--map-gen-settings")
        .arg(&config.factorio.map_gen_settings)
        .output()
        .await
        .map_err(|e| AppError::Rotation(format!("failed to run factorio --create: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Rotation(format!(
            "factorio --create failed: {stderr}"
        )));
    }

    tracing::info!(
        path = %output_path.display(),
        season_id = season_id.as_i64(),
        "map_generated"
    );
    Ok(())
}

pub async fn update_server_name(config: &AppConfig, season_id: SeasonId) -> AppResult<()> {
    let path = Path::new(&config.factorio.server_settings);
    let contents = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AppError::Rotation(format!("failed to read server-settings.json: {e}")))?;

    let mut settings: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| AppError::Rotation(format!("failed to parse server-settings.json: {e}")))?;

    let mode = if config.schedule.hardcore {
        "HARDCORE"
    } else {
        "SEASONS"
    };
    let name = format!("[{mode}] Season {:02}", season_id.as_i64());
    settings["name"] = serde_json::Value::String(name.clone());

    let url = &config.server.public_url;
    let desc = if config.schedule.hardcore {
        if url.is_empty() {
            format!(
                "Die = spectator. Fresh map every month. Season {} LIVE.",
                season_id.as_i64()
            )
        } else {
            format!(
                "Die = spectator. Fresh map every month. Register: {url}",
            )
        }
    } else if url.is_empty() {
        format!(
            "Seasonal Factorio. Fresh map every month. Season {} LIVE.",
            season_id.as_i64()
        )
    } else {
        format!(
            "Seasonal Factorio. Fresh map every month. Register: {url}",
        )
    };
    settings["description"] = serde_json::Value::String(desc);

    let output = serde_json::to_string_pretty(&settings)
        .map_err(|e| AppError::Rotation(format!("failed to serialize server-settings.json: {e}")))?;

    tokio::fs::write(path, output)
        .await
        .map_err(|e| AppError::Rotation(format!("failed to write server-settings.json: {e}")))?;

    tracing::info!(server_name = %name, "server_name_updated");
    Ok(())
}

async fn purge_old_archives(repo: &SqliteRepo, archive_dir: &Path, keep: usize) -> AppResult<()> {
    let to_purge = repo.get_archived_seasons_for_purge(keep).await?;
    if to_purge.is_empty() {
        return Ok(());
    }

    let mut purged = 0u32;
    for season in &to_purge {
        if let Some(ref save_path) = season.save_path {
            let path = Path::new(save_path);
            if path.exists() {
                if let Err(e) = tokio::fs::remove_file(path).await {
                    tracing::warn!(
                        season_id = season.id.as_i64(),
                        path = %path.display(),
                        error = %e,
                        "archive_delete_failed"
                    );
                    continue;
                }
            }
            repo.update_season_save_path(season.id, "").await?;
            purged += 1;
        }
    }

    // Also clean up any archive files not tracked in DB
    let _ = archive_dir; // archive_dir available if needed for future cleanup

    tracing::info!(purged_seasons = purged, kept = keep, "archives_purged");
    Ok(())
}
