use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("config error: {0}")]
    Config(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("no active season")]
    NoActiveSeason,
    #[error("invalid promo code")]
    InvalidPromoCode,
    #[error("promo code expired")]
    PromoExpired,
    #[error("promo code exhausted")]
    PromoExhausted,
    #[error("duplicate registration")]
    DuplicateRegistration,
    #[error("rcon error: {0}")]
    Rcon(String),
    #[error("rotation error: {0}")]
    Rotation(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            AppError::Config(e) => {
                tracing::error!(error = %e, "config error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            AppError::Serialization(e) => {
                tracing::error!(error = %e, "serialization error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            AppError::Io(e) => {
                tracing::error!(error = %e, "io error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::NoActiveSeason => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::InvalidPromoCode => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::PromoExpired => (StatusCode::GONE, self.to_string()),
            AppError::PromoExhausted => (StatusCode::CONFLICT, self.to_string()),
            AppError::DuplicateRegistration => (StatusCode::CONFLICT, self.to_string()),
            AppError::Rcon(e) => {
                tracing::warn!(error = %e, "rcon error");
                (
                    StatusCode::BAD_GATEWAY,
                    "upstream service error".to_string(),
                )
            }
            AppError::Rotation(e) => {
                tracing::error!(error = %e, "rotation error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}
