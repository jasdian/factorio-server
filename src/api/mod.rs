pub mod admin;
pub mod models;
pub mod public;

use axum::Router;

use crate::AppState;

pub fn build_api_router() -> Router<AppState> {
    Router::new().merge(public::router()).merge(admin::router())
}
