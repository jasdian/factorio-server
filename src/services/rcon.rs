use crate::config::FactorioConfig;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct RconConfig {
    pub url: String,
    pub password: String,
}

impl RconConfig {
    pub fn from_factorio_config(cfg: &FactorioConfig) -> AppResult<Self> {
        let password = std::fs::read_to_string(&cfg.rcon_pw_file)
            .map_err(|e| {
                AppError::Config(format!(
                    "failed to read rcon password file {}: {e}",
                    cfg.rcon_pw_file
                ))
            })?
            .trim()
            .to_string();

        let url = format!("{}:{}", cfg.rcon_host, cfg.rcon_port);
        tracing::info!(url = %url, "rcon config loaded");
        Ok(Self { url, password })
    }
}

pub async fn rcon_command(config: &RconConfig, command: &str) -> AppResult<String> {
    let url = config.url.clone();
    let password = config.password.clone();
    let command = command.to_string();

    let result = tokio::task::spawn_blocking(move || {
        use rcon_client::{AuthRequest, RCONClient, RCONConfig, RCONRequest};

        let config = RCONConfig {
            url: url.clone(),
            read_timeout: Some(10),
            write_timeout: Some(10),
        };

        let mut client = RCONClient::new(config).map_err(|e| {
            tracing::error!(error = %e, "rcon_connection_failed");
            AppError::Rcon(e.to_string())
        })?;

        let auth = AuthRequest::new(password);
        client.auth(auth).map_err(|e| {
            tracing::error!(error = %e, "rcon_auth_failed");
            AppError::Rcon(e.to_string())
        })?;

        tracing::debug!("rcon_connected");

        let request = RCONRequest::new(command.clone());
        let response = client.execute(request).map_err(|e| {
            tracing::error!(error = %e, command = %command, "rcon_execute_failed");
            AppError::Rcon(e.to_string())
        })?;

        tracing::debug!(command = %command, "rcon_sent");
        Ok::<String, AppError>(response.body)
    })
    .await
    .map_err(|e| AppError::Rcon(format!("spawn_blocking join error: {e}")))?;

    result
}

pub async fn rcon_command_best_effort(config: &RconConfig, command: &str) {
    match rcon_command(config, command).await {
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, command = %command, "rcon_command_failed");
        }
    }
}
