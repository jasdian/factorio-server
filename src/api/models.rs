use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AccessTier, PromoCode, RegStatus, Registration, SeasonId, SeasonStatus, Wei};

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub factorio_name: String,
    pub eth_address: Option<String>,
    pub promo_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePromoRequest {
    pub code: String,
    pub discount_percent: u8,
    pub grants_instant_player: bool,
    pub max_uses: Option<u32>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ListRegistrationsQuery {
    pub season_id: Option<i64>,
    pub status: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub registration_id: Uuid,
    pub status: RegStatus,
    pub access_tier: AccessTier,
    pub amount_wei: Wei,
    pub deposit_address: Option<String>,
    pub deaths: u32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct SeasonInfoResponse {
    pub id: SeasonId,
    pub status: SeasonStatus,
    pub started_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub player_count: u64,
    pub spectator_count: u64,
    pub factorio_version: String,
    pub base_fee_wei: String,
}

#[derive(Debug, Serialize)]
pub struct SeasonSummary {
    pub id: SeasonId,
    pub status: SeasonStatus,
    pub started_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub has_map_download: bool,
}

#[derive(Debug, Serialize)]
pub struct RegistrationStatusResponse {
    pub id: Uuid,
    pub season_id: SeasonId,
    pub factorio_name: String,
    pub status: RegStatus,
    pub access_tier: AccessTier,
    pub amount_wei: Wei,
    pub tx_hash: Option<String>,
    pub deaths: u32,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PromoCodeResponse {
    pub code: String,
    pub discount_percent: u8,
    pub grants_instant_player: bool,
    pub max_uses: Option<u32>,
    pub times_used: u32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RegistrationAdminResponse {
    pub id: Uuid,
    pub season_id: SeasonId,
    pub factorio_name: String,
    pub eth_address: String,
    pub promo_code: Option<String>,
    pub tx_hash: Option<String>,
    pub status: RegStatus,
    pub access_tier: AccessTier,
    pub amount_wei: Wei,
    pub deaths: u32,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct RotateResponse {
    pub status: &'static str,
}

// ---------------------------------------------------------------------------
// Conversion impls
// ---------------------------------------------------------------------------

impl From<&Registration> for RegistrationStatusResponse {
    fn from(r: &Registration) -> Self {
        Self {
            id: r.id,
            season_id: r.season_id,
            factorio_name: r.factorio_name.as_str().to_string(),
            status: r.status,
            access_tier: r.access_tier,
            amount_wei: r.amount_wei.clone(),
            tx_hash: r.tx_hash.clone(),
            deaths: r.deaths,
            created_at: r.created_at,
            confirmed_at: r.confirmed_at,
        }
    }
}

impl From<&Registration> for RegistrationAdminResponse {
    fn from(r: &Registration) -> Self {
        Self {
            id: r.id,
            season_id: r.season_id,
            factorio_name: r.factorio_name.as_str().to_string(),
            eth_address: r.eth_address.as_str().to_string(),
            promo_code: r.promo_code.clone(),
            tx_hash: r.tx_hash.clone(),
            status: r.status,
            access_tier: r.access_tier,
            amount_wei: r.amount_wei.clone(),
            deaths: r.deaths,
            created_at: r.created_at,
            confirmed_at: r.confirmed_at,
        }
    }
}

impl From<&PromoCode> for PromoCodeResponse {
    fn from(p: &PromoCode) -> Self {
        Self {
            code: p.code.clone(),
            discount_percent: p.discount_percent.as_u8(),
            grants_instant_player: p.grants_instant_player,
            max_uses: p.max_uses,
            times_used: p.times_used,
            active: p.active,
            created_at: p.created_at,
            expires_at: p.expires_at,
        }
    }
}
