use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use tokio::io::BufReader;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::api::models::{
    RegisterRequest, RegisterResponse, RegistrationStatusResponse, SeasonInfoResponse,
    SeasonSummary,
};
use crate::db::{PromoCodeRepo, RegistrationRepo, SeasonRepo};
use crate::domain::{
    compute_effective_price, compute_rebuy_price, determine_access_tier, determine_initial_status,
    validate_promo_usable, AccessTier, DiscountPercent, EthAddress, FactorioName, RegStatus,
    Registration, SeasonId, SeasonStatus, Wei,
};
use crate::error::{AppError, AppResult};
use crate::services::whitelist;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/season", get(get_current_season))
        .route("/api/seasons", get(list_seasons))
        .route("/api/register", post(register))
        .route("/api/register/{id}", get(registration_status))
        .route("/api/maps/{season_id}", get(download_map))
}

async fn get_current_season(State(state): State<AppState>) -> AppResult<Json<SeasonInfoResponse>> {
    let season = state.repo.get_active_season().await?;
    let player_count = state.repo.get_player_count(season.id).await?;
    let spectator_count = state.repo.get_spectator_count(season.id).await?;

    Ok(Json(SeasonInfoResponse {
        id: season.id,
        status: season.status,
        started_at: season.started_at,
        ends_at: season.ends_at,
        player_count,
        spectator_count,
        factorio_version: state.config.factorio.factorio_version.clone(),
        base_fee_wei: state.base_fee_wei.clone(),
    }))
}

async fn list_seasons(State(state): State<AppState>) -> AppResult<Json<Vec<SeasonSummary>>> {
    let seasons = state.repo.list_seasons().await?;
    let summaries = seasons
        .iter()
        .map(|s| SeasonSummary {
            id: s.id,
            status: s.status,
            started_at: s.started_at,
            ends_at: s.ends_at,
            has_map_download: s.save_path.as_ref().map(|p| !p.is_empty()).unwrap_or(false),
        })
        .collect();
    Ok(Json(summaries))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<RegisterResponse>> {
    // 1. Validate factorio name
    let factorio_name = FactorioName::try_from(req.factorio_name)?;

    // 2. Get active season
    let season = state.repo.get_active_season().await?;

    // 3. Check duplicate — allow rebuy if player was demoted (died in hardcore)
    let existing = state
        .repo
        .find_registration_by_name_and_season(&factorio_name, season.id)
        .await?;

    let rebuy_deaths = if let Some(ref existing_reg) = existing {
        // Demoted player: confirmed + standard tier + has deaths → eligible for rebuy
        if existing_reg.status == RegStatus::Confirmed
            && existing_reg.access_tier == AccessTier::Standard
            && existing_reg.deaths > 0
        {
            // Expire the old registration so the new one replaces it
            state.repo.expire_registration(existing_reg.id).await?;
            tracing::info!(
                factorio_name = factorio_name.as_str(),
                old_reg_id = %existing_reg.id,
                deaths = existing_reg.deaths,
                "rebuy_replacing_demoted_registration"
            );
            existing_reg.deaths
        } else {
            tracing::warn!(
                factorio_name = factorio_name.as_str(),
                season_id = season.id.as_i64(),
                "duplicate_registration_attempt"
            );
            return Err(AppError::DuplicateRegistration);
        }
    } else {
        0
    };

    let is_rebuy = rebuy_deaths > 0;

    // 4. Handle promo code (not allowed on rebuy — you pay full multiplied price)
    let promo = if is_rebuy {
        if req.promo_code.is_some() {
            return Err(AppError::BadRequest(
                "promo codes cannot be used on rebuy".into(),
            ));
        }
        None
    } else if let Some(ref code) = req.promo_code {
        let p = state.repo.get_promo_code(code).await?.ok_or_else(|| {
            tracing::warn!(promo_code = code, "promo_not_found");
            AppError::InvalidPromoCode
        })?;

        if let Err(e) = validate_promo_usable(&p, Utc::now()) {
            match &e {
                AppError::PromoExpired => {
                    tracing::warn!(promo_code = code, "promo_expired");
                }
                AppError::PromoExhausted => {
                    tracing::warn!(promo_code = code, "promo_exhausted");
                }
                _ => {}
            }
            return Err(e);
        }

        state.repo.increment_promo_usage(code).await?;
        Some(p)
    } else {
        None
    };

    // 5. Determine access tier (rebuy always gets instant_player back)
    let access_tier = if is_rebuy {
        AccessTier::InstantPlayer
    } else {
        determine_access_tier(promo.as_ref())
    };

    // 6. Count registrations for unique offset
    let offset = state.repo.count_season_registrations(season.id).await?;

    // 7. Compute effective price
    let base_fee: u128 = state
        .base_fee_wei
        .parse()
        .map_err(|e| AppError::Config(format!("invalid base_fee_wei: {e}")))?;

    let effective_price = if is_rebuy {
        let multiplier = state.config.schedule.rebuy_multiplier;
        compute_rebuy_price(base_fee, multiplier, rebuy_deaths, offset)
    } else {
        let discount = promo
            .as_ref()
            .map(|p| p.discount_percent)
            .unwrap_or(DiscountPercent::try_from(0u8).expect("0 is valid"));
        compute_effective_price(base_fee, discount, offset)
    };

    // 8. Determine initial status
    let amount_is_zero = effective_price == 0;
    let initial_status = determine_initial_status(amount_is_zero);
    let amount_wei = Wei::from_decimal(effective_price.to_string())?;

    // 9. Validate ETH address — required when payment is needed, optional when free
    let eth_address = if amount_is_zero {
        match req.eth_address {
            Some(addr) if !addr.is_empty() => EthAddress::try_from(addr)?,
            _ => EthAddress::try_from("0x0000000000000000000000000000000000000000".to_string())?,
        }
    } else {
        let addr = req.eth_address.ok_or_else(|| {
            AppError::BadRequest("eth_address is required when payment is needed".into())
        })?;
        EthAddress::try_from(addr)?
    };

    // 10. Build and create registration
    let now = Utc::now();
    let reg = Registration {
        id: Uuid::new_v4(),
        season_id: season.id,
        factorio_name: factorio_name.clone(),
        eth_address,
        promo_code: req.promo_code,
        tx_hash: None,
        status: initial_status,
        access_tier,
        amount_wei: amount_wei.clone(),
        created_at: now,
        confirmed_at: if amount_is_zero { Some(now) } else { None },
        deaths: rebuy_deaths,
    };
    state.repo.create_registration(&reg).await?;

    tracing::info!(
        registration_id = %reg.id,
        factorio_name = reg.factorio_name.as_str(),
        season_id = season.id.as_i64(),
        access_tier = %reg.access_tier,
        amount_wei = reg.amount_wei.as_str(),
        is_rebuy = is_rebuy,
        deaths = reg.deaths,
        "registration_created"
    );

    // 11. If free (100% discount), apply whitelist immediately
    if amount_is_zero {
        whitelist::apply_whitelist_for_registration(
            &state.repo,
            &state.rcon_config,
            &state.factorio_data_dir,
            reg.factorio_name.as_str(),
            reg.access_tier,
            season.id,
        )
        .await?;
    }

    // 12. Build response
    let deposit_address = if amount_is_zero {
        None
    } else {
        Some(state.deposit_address.clone())
    };
    let message = if is_rebuy {
        format!(
            "Rebuy #{}: Send exactly {} wei to {} to get back in the game.",
            rebuy_deaths, reg.amount_wei, state.deposit_address
        )
    } else if amount_is_zero {
        "Registration confirmed (no payment required).".to_string()
    } else {
        format!(
            "Send exactly {} wei to {} to complete registration.",
            reg.amount_wei, state.deposit_address
        )
    };

    Ok(Json(RegisterResponse {
        registration_id: reg.id,
        status: reg.status,
        access_tier: reg.access_tier,
        amount_wei: reg.amount_wei,
        deposit_address,
        deaths: reg.deaths,
        message,
    }))
}

async fn registration_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<RegistrationStatusResponse>> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|e| AppError::BadRequest(format!("invalid registration id: {e}")))?;

    let reg = state
        .repo
        .get_registration_by_id(uuid)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(RegistrationStatusResponse::from(&reg)))
}

async fn download_map(
    State(state): State<AppState>,
    Path(season_id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let season_id = SeasonId::new(season_id)?;
    let season = state
        .repo
        .get_season_by_id(season_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if season.status != SeasonStatus::Archived {
        return Err(AppError::BadRequest(
            "map download only available for archived seasons".into(),
        ));
    }

    let save_path = season
        .save_path
        .as_ref()
        .filter(|p| !p.is_empty())
        .ok_or(AppError::NotFound)?;

    let path = std::path::Path::new(save_path);
    if !path.exists() {
        return Err(AppError::NotFound);
    }

    let file = tokio::fs::File::open(path).await?;
    let reader = BufReader::new(file);
    let stream = ReaderStream::new(reader);
    let body = axum::body::Body::from_stream(stream);

    let filename = format!("season_{}.zip", season_id.as_i64());
    let headers = [
        (header::CONTENT_TYPE, "application/zip".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        ),
    ];

    Ok((headers, body))
}
