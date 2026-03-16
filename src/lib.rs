pub mod api;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod services;

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::db::SqliteRepo;
use crate::services::rcon::RconConfig;

#[derive(Clone)]
pub struct AppState {
    pub repo: SqliteRepo,
    pub archive_dir: PathBuf,
    pub factorio_data_dir: PathBuf,
    pub deposit_address: String,
    pub base_fee_wei: String,
    pub admin_token: Arc<str>,
    pub static_dir: PathBuf,
    pub config: Arc<AppConfig>,
    pub rcon_config: Arc<RconConfig>,
}
