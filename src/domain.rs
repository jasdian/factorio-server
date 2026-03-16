use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Newtypes with validated constructors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeasonId(i64);

impl SeasonId {
    pub fn new(id: i64) -> AppResult<Self> {
        if id > 0 {
            Ok(Self(id))
        } else {
            Err(AppError::BadRequest(format!(
                "season id must be positive, got {id}"
            )))
        }
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for SeasonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FactorioName(String);

impl FactorioName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FactorioName {
    type Error = AppError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() || s.len() > 50 {
            return Err(AppError::BadRequest(
                "factorio name must be 1-50 characters".into(),
            ));
        }
        if !s.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
            return Err(AppError::BadRequest(
                "factorio name must contain only printable ASCII".into(),
            ));
        }
        Ok(Self(s))
    }
}

impl std::fmt::Display for FactorioName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EthAddress(String);

impl EthAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EthAddress {
    type Error = AppError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let s = s.to_lowercase();
        if s.len() != 42 {
            return Err(AppError::BadRequest(
                "eth address must be 42 characters (0x + 40 hex)".into(),
            ));
        }
        if !s.starts_with("0x") {
            return Err(AppError::BadRequest(
                "eth address must start with 0x".into(),
            ));
        }
        if !s[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::BadRequest(
                "eth address must contain only hex digits after 0x".into(),
            ));
        }
        Ok(Self(s))
    }
}

impl std::fmt::Display for EthAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Wei(String);

impl Wei {
    pub fn from_decimal(s: String) -> AppResult<Self> {
        if s.is_empty() {
            return Err(AppError::BadRequest("wei amount cannot be empty".into()));
        }
        if !s.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::BadRequest(
                "wei amount must be non-negative decimal digits".into(),
            ));
        }
        Ok(Self(s))
    }

    pub fn zero() -> Self {
        Self("0".to_string())
    }

    pub fn is_zero(&self) -> bool {
        self.0.chars().all(|c| c == '0')
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Wei {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscountPercent(u8);

impl DiscountPercent {
    pub fn as_u8(self) -> u8 {
        self.0
    }

    #[allow(dead_code)]
    pub fn is_full_discount(self) -> bool {
        self.0 == 100
    }
}

impl TryFrom<u8> for DiscountPercent {
    type Error = AppError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        if v > 100 {
            Err(AppError::BadRequest(format!(
                "discount percent must be 0..=100, got {v}"
            )))
        } else {
            Ok(Self(v))
        }
    }
}

impl std::fmt::Display for DiscountPercent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}%", self.0)
    }
}

// ---------------------------------------------------------------------------
// Enums with exhaustive DB string conversion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTier {
    Standard,
    InstantPlayer,
}

impl AccessTier {
    pub fn as_db_str(self) -> &'static str {
        match self {
            AccessTier::Standard => "standard",
            AccessTier::InstantPlayer => "instant_player",
        }
    }
}

impl TryFrom<&str> for AccessTier {
    type Error = AppError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "standard" => Ok(AccessTier::Standard),
            "instant_player" => Ok(AccessTier::InstantPlayer),
            other => Err(AppError::BadRequest(format!(
                "unknown access tier: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for AccessTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_db_str())
    }
}

// ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonStatus {
    Pending,
    Active,
    Archived,
}

impl SeasonStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            SeasonStatus::Pending => "pending",
            SeasonStatus::Active => "active",
            SeasonStatus::Archived => "archived",
        }
    }
}

impl TryFrom<&str> for SeasonStatus {
    type Error = AppError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "pending" => Ok(SeasonStatus::Pending),
            "active" => Ok(SeasonStatus::Active),
            "archived" => Ok(SeasonStatus::Archived),
            other => Err(AppError::BadRequest(format!(
                "unknown season status: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for SeasonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_db_str())
    }
}

// ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegStatus {
    AwaitingPayment,
    Confirmed,
    Expired,
}

impl RegStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            RegStatus::AwaitingPayment => "awaiting_payment",
            RegStatus::Confirmed => "confirmed",
            RegStatus::Expired => "expired",
        }
    }
}

impl TryFrom<&str> for RegStatus {
    type Error = AppError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "awaiting_payment" => Ok(RegStatus::AwaitingPayment),
            "confirmed" => Ok(RegStatus::Confirmed),
            "expired" => Ok(RegStatus::Expired),
            other => Err(AppError::BadRequest(format!(
                "unknown registration status: {other}"
            ))),
        }
    }
}

impl std::fmt::Display for RegStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_db_str())
    }
}

// ---------------------------------------------------------------------------
// Domain structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Season {
    pub id: SeasonId,
    pub status: SeasonStatus,
    pub started_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub map_seed: Option<String>,
    pub save_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Registration {
    pub id: Uuid,
    pub season_id: SeasonId,
    pub factorio_name: FactorioName,
    pub eth_address: EthAddress,
    pub promo_code: Option<String>,
    pub tx_hash: Option<String>,
    pub status: RegStatus,
    pub access_tier: AccessTier,
    pub amount_wei: Wei,
    pub created_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub deaths: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromoCode {
    pub code: String,
    pub discount_percent: DiscountPercent,
    pub grants_instant_player: bool,
    pub max_uses: Option<u32>,
    pub times_used: u32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Pure domain functions
// ---------------------------------------------------------------------------

pub fn determine_access_tier(_promo: Option<&PromoCode>) -> AccessTier {
    // All registrations get full player access for the current season
    // and are carried forward to the next season automatically.
    AccessTier::InstantPlayer
}

pub fn determine_initial_status(amount_is_zero: bool) -> RegStatus {
    if amount_is_zero {
        RegStatus::Confirmed
    } else {
        RegStatus::AwaitingPayment
    }
}

pub fn validate_promo_usable(promo: &PromoCode, now: DateTime<Utc>) -> Result<(), AppError> {
    if !promo.active {
        return Err(AppError::InvalidPromoCode);
    }
    if let Some(expires) = promo.expires_at {
        if now > expires {
            return Err(AppError::PromoExpired);
        }
    }
    if let Some(max) = promo.max_uses {
        if promo.times_used >= max {
            return Err(AppError::PromoExhausted);
        }
    }
    Ok(())
}

pub fn compute_effective_price(base_fee: u128, discount: DiscountPercent, offset: u64) -> u128 {
    let discount_amount = base_fee * u128::from(discount.as_u8()) / 100;
    let effective = base_fee - discount_amount;
    if effective == 0 {
        0
    } else {
        effective + u128::from(offset)
    }
}

pub fn compute_rebuy_price(base_fee: u128, multiplier: u32, deaths: u32, offset: u64) -> u128 {
    let price = base_fee * u128::from(multiplier).pow(deaths);
    price + u128::from(offset)
}
