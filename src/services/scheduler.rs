use std::sync::Arc;

use chrono::{Datelike, Timelike, Weekday};

use crate::config::AppConfig;
use crate::db::SqliteRepo;
use crate::services::rcon::RconConfig;
use crate::services::rotation;

fn parse_weekday(s: &str) -> Weekday {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" => Weekday::Tue,
        "wednesday" | "wed" => Weekday::Wed,
        "thursday" | "thu" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        other => {
            tracing::warn!(value = other, "unknown rotation_day, defaulting to Monday");
            Weekday::Mon
        }
    }
}

pub fn spawn_scheduler(
    repo: SqliteRepo,
    config: Arc<AppConfig>,
    rcon_config: Arc<RconConfig>,
) -> tokio::task::JoinHandle<()> {
    let target_day = parse_weekday(&config.schedule.rotation_day);
    let target_hour = config.schedule.rotation_hour;

    tracing::info!(
        rotation_day = %target_day,
        rotation_hour = target_hour,
        "scheduler_started"
    );

    tokio::spawn(async move {
        let mut last_rotation_date: Option<chrono::NaiveDate> = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;

            let now = chrono::Utc::now();
            let today = now.date_naive();

            if now.weekday() == target_day
                && now.hour() == u32::from(target_hour)
                && last_rotation_date != Some(today)
            {
                tracing::info!("scheduler_triggering_rotation");
                last_rotation_date = Some(today);

                match rotation::rotate_season(&repo, &config, &rcon_config, false).await {
                    Ok(()) => {
                        tracing::info!("scheduler_rotation_success");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "scheduler_rotation_failed");
                    }
                }
            }
        }
    })
}
