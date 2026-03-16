use std::path::Path;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub factorio: FactorioConfig,
    pub eth: EthConfig,
    pub schedule: ScheduleConfig,
    pub admin: AdminConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub static_dir: String,
    #[serde(default)]
    pub public_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FactorioConfig {
    pub binary: String,
    pub saves_dir: String,
    pub archive_dir: String,
    pub data_dir: String,
    pub rcon_host: String,
    pub rcon_port: u16,
    pub rcon_pw_file: String,
    pub map_gen_settings: String,
    #[serde(default)]
    pub server_settings: String,
    #[serde(default)]
    pub factorio_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EthConfig {
    pub rpc_url: String,
    pub deposit_address: String,
    #[serde(default = "default_base_fee_wei")]
    pub base_fee_wei: String,
    #[serde(default = "default_payment_expiry_hours")]
    pub payment_expiry_hours: u64,
}

fn default_base_fee_wei() -> String {
    "333333333333333".to_string()
}

fn default_payment_expiry_hours() -> u64 {
    48
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleConfig {
    pub rotation_day: String,
    pub rotation_hour: u8,
    #[serde(default = "default_season_duration_days")]
    pub season_duration_days: u32,
    #[serde(default)]
    pub hardcore: bool,
    #[serde(default = "default_rebuy_multiplier")]
    pub rebuy_multiplier: u32,
}

fn default_season_duration_days() -> u32 {
    30
}

fn default_rebuy_multiplier() -> u32 {
    2
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
        }
    }
}

impl AppConfig {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(toml_str: &str) -> AppResult<Self> {
        toml::from_str(toml_str).map_err(|e| AppError::Config(e.to_string()))
    }

    pub fn from_file(path: &Path) -> AppResult<Self> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            AppError::Config(format!(
                "failed to read config file {}: {e}",
                path.display()
            ))
        })?;
        Self::from_str(&contents)
    }
}
