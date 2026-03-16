use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use sqlx::sqlite::SqlitePoolOptions;
use tracing_subscriber::EnvFilter;

use factorio_seasons::config::{AppConfig, LoggingConfig};
use factorio_seasons::db::{SeasonRepo, SqliteRepo};
use factorio_seasons::error;
use factorio_seasons::services::rcon::RconConfig;
use factorio_seasons::AppState;

fn init_tracing(config: &LoggingConfig) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = tracing_subscriber::fmt().with_env_filter(env_filter);

    match config.format.as_str() {
        "json" => subscriber.json().init(),
        _ => subscriber.init(),
    }
}

async fn health() -> StatusCode {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> error::AppResult<()> {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config = AppConfig::from_file(config_path.as_ref())?;

    init_tracing(&config.logging);

    tracing::info!(config_path = %config_path, "loaded configuration");

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database.url)
        .await
        .map_err(|e| error::AppError::Config(format!("database connection failed: {e}")))?;

    // WAL mode must be set outside a transaction (can't go in migrations)
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await?;

    let repo = SqliteRepo::new(pool);
    repo.run_migrations().await?;
    tracing::info!("database migrations complete");

    let rcon_config = RconConfig::from_factorio_config(&config.factorio)?;

    let season = repo.get_active_season().await?;
    tracing::info!(
        season_id = season.id.as_i64(),
        ends_at = %season.ends_at,
        "active season verified"
    );

    let state = AppState {
        repo,
        archive_dir: PathBuf::from(&config.factorio.archive_dir),
        factorio_data_dir: PathBuf::from(&config.factorio.data_dir),
        deposit_address: config.eth.deposit_address.clone(),
        base_fee_wei: config.eth.base_fee_wei.clone(),
        admin_token: Arc::from(config.admin.token.as_str()),
        static_dir: PathBuf::from(&config.server.static_dir),
        config: Arc::new(config.clone()),
        rcon_config: Arc::new(rcon_config),
    };

    factorio_seasons::services::scheduler::spawn_scheduler(
        state.repo.clone(),
        state.config.clone(),
        state.rcon_config.clone(),
    );

    factorio_seasons::services::payment::spawn_payment_poller(
        state.repo.clone(),
        state.rcon_config.clone(),
        state.factorio_data_dir.clone(),
        &config.eth.rpc_url,
        &state.deposit_address,
    );

    factorio_seasons::services::payment::spawn_expiry_cleanup(
        state.repo.clone(),
        config.eth.payment_expiry_hours,
    );

    // Update server name/description for current season
    if let Err(e) =
        factorio_seasons::services::rotation::update_server_name(&config, season.id).await
    {
        tracing::warn!(error = %e, "server_name_update_failed_on_startup");
    }

    // Enable whitelist and sync player roles to the Factorio server
    if let Err(e) = factorio_seasons::services::whitelist::sync_whitelist_to_server(
        &state.repo,
        &state.rcon_config,
        &state.factorio_data_dir,
    )
    .await
    {
        tracing::warn!(error = %e, "whitelist_sync_failed_on_startup");
    }

    // Poll connected players and enforce permission groups
    factorio_seasons::services::whitelist::spawn_permission_poller(
        state.repo.clone(),
        state.rcon_config.clone(),
        config.schedule.hardcore,
    );

    let app = factorio_seasons::api::build_api_router()
        .route("/health", axum::routing::get(health))
        .fallback_service(
            tower_http::services::ServeDir::new(&state.static_dir).not_found_service(
                tower_http::services::ServeFile::new(state.static_dir.join("index.html")),
            ),
        )
        .with_state(state)
        .layer(tower_governor::GovernorLayer::new(
            tower_governor::governor::GovernorConfigBuilder::default()
                .key_extractor(tower_governor::key_extractor::SmartIpKeyExtractor)
                .per_second(50)
                .burst_size(100)
                .finish()
                .expect("valid governor config"),
        ));

    let listener = tokio::net::TcpListener::bind(&config.server.bind).await?;
    tracing::info!(bind = %config.server.bind, "server listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
