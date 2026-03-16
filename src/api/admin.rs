use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRef, FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;

use crate::api::models::{
    CreatePromoRequest, ListRegistrationsQuery, PromoCodeResponse, RegistrationAdminResponse,
    RotateResponse,
};
use crate::db::{PromoCodeRepo, RegistrationRepo};
use crate::domain::{DiscountPercent, PromoCode, RegStatus, SeasonId};
use crate::error::{AppError, AppResult};
use crate::services::{rcon::rcon_command_best_effort, rotation, whitelist};
use crate::AppState;

// ---------------------------------------------------------------------------
// AdminAuth extractor
// ---------------------------------------------------------------------------

pub struct AdminAuth;

impl<S> FromRequestParts<S> for AdminAuth
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        if token != &*app_state.admin_token {
            return Err(AppError::Unauthorized);
        }

        Ok(AdminAuth)
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/promo", post(create_promo).get(list_promos))
        .route("/api/admin/promo/{code}", delete(revoke_promo))
        .route("/api/admin/rotate", post(force_rotate))
        .route("/api/admin/registrations", get(list_registrations))
        .route(
            "/api/admin/registrations/{id}",
            delete(revoke_registration),
        )
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn create_promo(
    _auth: AdminAuth,
    State(state): State<AppState>,
    Json(req): Json<CreatePromoRequest>,
) -> AppResult<Json<PromoCodeResponse>> {
    // Validate code: 1-32 chars, uppercase alphanumeric + underscore
    let code = req.code.to_uppercase();
    if code.is_empty() || code.len() > 32 {
        return Err(AppError::BadRequest(
            "promo code must be 1-32 characters".into(),
        ));
    }
    if !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::BadRequest(
            "promo code must be alphanumeric or underscore".into(),
        ));
    }

    // Validate discount
    let discount = DiscountPercent::try_from(req.discount_percent)?;

    // Check if code already exists
    if state.repo.get_promo_code(&code).await?.is_some() {
        return Err(AppError::BadRequest("promo code already exists".into()));
    }

    let promo = PromoCode {
        code: code.clone(),
        discount_percent: discount,
        grants_instant_player: req.grants_instant_player,
        max_uses: req.max_uses,
        times_used: 0,
        active: true,
        created_at: Utc::now(),
        expires_at: req.expires_at,
    };

    state.repo.create_promo_code(&promo).await?;

    tracing::info!(
        promo_code = &code,
        discount_percent = discount.as_u8(),
        grants_instant_player = req.grants_instant_player,
        "admin_promo_created"
    );

    Ok(Json(PromoCodeResponse::from(&promo)))
}

async fn list_promos(
    _auth: AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<PromoCodeResponse>>> {
    let promos = state.repo.list_promo_codes().await?;
    let responses: Vec<PromoCodeResponse> = promos.iter().map(PromoCodeResponse::from).collect();
    Ok(Json(responses))
}

async fn revoke_promo(
    _auth: AdminAuth,
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<impl IntoResponse> {
    let deactivated = state.repo.deactivate_promo_code(&code).await?;
    if !deactivated {
        return Err(AppError::NotFound);
    }

    tracing::info!(promo_code = &code, "admin_promo_revoked");

    Ok(StatusCode::NO_CONTENT)
}

async fn force_rotate(
    _auth: AdminAuth,
    State(state): State<AppState>,
) -> AppResult<Json<RotateResponse>> {
    tracing::warn!("admin_force_rotation");

    rotation::rotate_season(&state.repo, &state.config, &state.rcon_config, true).await?;

    Ok(Json(RotateResponse { status: "rotated" }))
}

async fn list_registrations(
    _auth: AdminAuth,
    State(state): State<AppState>,
    query: Result<Query<ListRegistrationsQuery>, QueryRejection>,
) -> AppResult<Json<Vec<RegistrationAdminResponse>>> {
    let query = query.map(|Query(q)| q).unwrap_or(ListRegistrationsQuery {
        season_id: None,
        status: None,
    });

    let registrations = if let Some(sid) = query.season_id {
        let season_id = SeasonId::new(sid)?;
        state.repo.list_registrations_for_season(season_id).await?
    } else {
        state.repo.list_all_registrations().await?
    };

    let mut responses: Vec<RegistrationAdminResponse> = registrations
        .iter()
        .map(RegistrationAdminResponse::from)
        .collect();

    // Filter by status if provided
    if let Some(ref status_filter) = query.status {
        responses.retain(|r| r.status.as_db_str() == status_filter);
    }

    Ok(Json(responses))
}

async fn revoke_registration(
    _auth: AdminAuth,
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    // Look up the registration to get player name + season before expiring
    let reg = state
        .repo
        .get_registration_by_id(id)
        .await?
        .ok_or(AppError::NotFound)?;

    if reg.status == RegStatus::Expired {
        return Err(AppError::BadRequest("registration already expired".into()));
    }

    let name = reg.factorio_name.as_str().to_string();
    let season_id = reg.season_id;

    state.repo.expire_registration(id).await?;

    // Resync whitelist files and remove player via RCON
    let data_dir = std::path::Path::new(&state.config.factorio.data_dir);
    let _ = whitelist::write_whitelist_file(&state.repo, season_id, data_dir).await;
    let _ = whitelist::write_spectator_file(&state.repo, season_id, data_dir).await;
    rcon_command_best_effort(&state.rcon_config, &format!("/whitelist remove {name}")).await;

    tracing::info!(
        registration_id = %id,
        factorio_name = name,
        season_id = season_id.as_i64(),
        "admin_registration_revoked"
    );

    Ok(StatusCode::NO_CONTENT)
}
