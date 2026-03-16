use std::future::Future;

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::sqlite::SqlitePool;
use sqlx::FromRow;
use uuid::Uuid;

use crate::domain::{
    AccessTier, DiscountPercent, EthAddress, FactorioName, PromoCode, RegStatus, Registration,
    Season, SeasonId, SeasonStatus, Wei,
};
use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Row types (raw DB representation)
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
pub struct SeasonRow {
    pub id: i64,
    pub status: String,
    pub started_at: String,
    pub ends_at: String,
    pub map_seed: Option<String>,
    pub save_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
pub struct RegistrationRow {
    pub id: String,
    pub season_id: i64,
    pub factorio_name: String,
    pub eth_address: String,
    pub promo_code: Option<String>,
    pub tx_hash: Option<String>,
    pub status: String,
    pub access_tier: String,
    pub amount_wei: String,
    pub created_at: String,
    pub confirmed_at: Option<String>,
    pub deaths: i32,
}

#[derive(Debug, FromRow)]
pub struct PromoCodeRow {
    pub code: String,
    pub discount_percent: i32,
    pub grants_instant_player: i32,
    pub max_uses: Option<i32>,
    pub times_used: i32,
    pub active: i32,
    pub created_at: String,
    pub expires_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Datetime parsing helper
// ---------------------------------------------------------------------------

fn parse_datetime(s: &str) -> AppResult<DateTime<Utc>> {
    // Try RFC3339 first (what we write), then SQLite's default format
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|ndt| ndt.and_utc())
        .map_err(|e| AppError::BadRequest(format!("invalid datetime '{s}': {e}")))
}

// ---------------------------------------------------------------------------
// TryFrom<Row> for domain types
// ---------------------------------------------------------------------------

impl TryFrom<SeasonRow> for Season {
    type Error = AppError;

    fn try_from(row: SeasonRow) -> Result<Self, Self::Error> {
        Ok(Season {
            id: SeasonId::new(row.id)?,
            status: SeasonStatus::try_from(row.status.as_str())?,
            started_at: parse_datetime(&row.started_at)?,
            ends_at: parse_datetime(&row.ends_at)?,
            map_seed: row.map_seed,
            save_path: row.save_path,
            created_at: parse_datetime(&row.created_at)?,
        })
    }
}

impl TryFrom<RegistrationRow> for Registration {
    type Error = AppError;

    fn try_from(row: RegistrationRow) -> Result<Self, Self::Error> {
        Ok(Registration {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AppError::BadRequest(format!("invalid uuid: {e}")))?,
            season_id: SeasonId::new(row.season_id)?,
            factorio_name: FactorioName::try_from(row.factorio_name)?,
            eth_address: EthAddress::try_from(row.eth_address)?,
            promo_code: row.promo_code,
            tx_hash: row.tx_hash,
            status: RegStatus::try_from(row.status.as_str())?,
            access_tier: AccessTier::try_from(row.access_tier.as_str())?,
            amount_wei: Wei::from_decimal(row.amount_wei)?,
            created_at: parse_datetime(&row.created_at)?,
            confirmed_at: row.confirmed_at.map(|s| parse_datetime(&s)).transpose()?,
            deaths: row.deaths as u32,
        })
    }
}

impl TryFrom<PromoCodeRow> for PromoCode {
    type Error = AppError;

    fn try_from(row: PromoCodeRow) -> Result<Self, Self::Error> {
        Ok(PromoCode {
            code: row.code,
            discount_percent: DiscountPercent::try_from(row.discount_percent as u8)?,
            grants_instant_player: row.grants_instant_player != 0,
            max_uses: row.max_uses.map(|v| v as u32),
            times_used: row.times_used as u32,
            active: row.active != 0,
            created_at: parse_datetime(&row.created_at)?,
            expires_at: row.expires_at.map(|s| parse_datetime(&s)).transpose()?,
        })
    }
}

// ---------------------------------------------------------------------------
// Repo traits (RPITIT — Rust 1.75+)
// ---------------------------------------------------------------------------

pub trait SeasonRepo {
    fn get_active_season(&self) -> impl Future<Output = AppResult<Season>> + Send;
    fn get_season_by_id(
        &self,
        id: SeasonId,
    ) -> impl Future<Output = AppResult<Option<Season>>> + Send;
    fn list_seasons(&self) -> impl Future<Output = AppResult<Vec<Season>>> + Send;
    fn create_season(
        &self,
        id: SeasonId,
        started_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> impl Future<Output = AppResult<Season>> + Send;
    fn archive_season(&self, id: SeasonId) -> impl Future<Output = AppResult<()>> + Send;
}

pub trait RegistrationRepo {
    fn create_registration(&self, reg: &Registration)
        -> impl Future<Output = AppResult<()>> + Send;
    fn get_registration_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = AppResult<Option<Registration>>> + Send;
    fn find_pending_by_amount(
        &self,
        amount: &Wei,
    ) -> impl Future<Output = AppResult<Option<Registration>>> + Send;
    fn confirm_registration(
        &self,
        id: Uuid,
        tx_hash: &str,
    ) -> impl Future<Output = AppResult<()>> + Send;
    #[allow(dead_code)]
    fn get_confirmed_for_season(
        &self,
        season_id: SeasonId,
    ) -> impl Future<Output = AppResult<Vec<Registration>>> + Send;
    fn expire_stale_registrations(
        &self,
        cutoff: DateTime<Utc>,
    ) -> impl Future<Output = AppResult<u64>> + Send;
    fn find_registration_by_name_and_season(
        &self,
        name: &FactorioName,
        season_id: SeasonId,
    ) -> impl Future<Output = AppResult<Option<Registration>>> + Send;
    fn demote_to_spectator(
        &self,
        name: &FactorioName,
        season_id: SeasonId,
    ) -> impl Future<Output = AppResult<bool>> + Send;
    fn expire_registration(
        &self,
        id: Uuid,
    ) -> impl Future<Output = AppResult<()>> + Send;
}

pub trait RotationRepo {
    fn carry_forward_players(
        &self,
        prev: SeasonId,
        new: SeasonId,
    ) -> impl Future<Output = AppResult<u64>> + Send;
    fn get_confirmed_player_names(
        &self,
        season_id: SeasonId,
    ) -> impl Future<Output = AppResult<Vec<String>>> + Send;
    fn get_spectator_names(
        &self,
        season_id: SeasonId,
    ) -> impl Future<Output = AppResult<Vec<String>>> + Send;
    fn get_archived_seasons_for_purge(
        &self,
        keep: usize,
    ) -> impl Future<Output = AppResult<Vec<Season>>> + Send;
    fn update_season_save_path(
        &self,
        id: SeasonId,
        path: &str,
    ) -> impl Future<Output = AppResult<()>> + Send;
}

pub trait PromoCodeRepo {
    fn get_promo_code(
        &self,
        code: &str,
    ) -> impl Future<Output = AppResult<Option<PromoCode>>> + Send;
    fn increment_promo_usage(&self, code: &str) -> impl Future<Output = AppResult<()>> + Send;
    fn create_promo_code(&self, promo: &PromoCode) -> impl Future<Output = AppResult<()>> + Send;
    fn list_promo_codes(&self) -> impl Future<Output = AppResult<Vec<PromoCode>>> + Send;
    fn deactivate_promo_code(&self, code: &str) -> impl Future<Output = AppResult<bool>> + Send;
}

// ---------------------------------------------------------------------------
// SqliteRepo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SqliteRepo {
    pool: SqlitePool,
}

impl SqliteRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn run_migrations(&self) -> AppResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| AppError::Config(format!("migration failed: {e}")))?;
        Ok(())
    }

    pub async fn count_season_registrations(&self, season_id: SeasonId) -> AppResult<u64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM registrations WHERE season_id = ? AND status != 'expired'",
        )
        .bind(season_id.as_i64())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 as u64)
    }

    pub async fn get_player_count(&self, season_id: SeasonId) -> AppResult<u64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT factorio_name) FROM registrations \
             WHERE season_id = ? AND status = 'confirmed' AND access_tier = 'instant_player'",
        )
        .bind(season_id.as_i64())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 as u64)
    }

    pub async fn get_spectator_count(&self, season_id: SeasonId) -> AppResult<u64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT factorio_name) FROM registrations \
             WHERE season_id = ? AND status = 'confirmed' AND access_tier = 'standard'",
        )
        .bind(season_id.as_i64())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 as u64)
    }

    pub async fn list_registrations_for_season(
        &self,
        season_id: SeasonId,
    ) -> AppResult<Vec<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations WHERE season_id = ? ORDER BY created_at DESC",
        )
        .bind(season_id.as_i64())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Registration::try_from)
        .collect()
    }

    pub async fn list_all_registrations(&self) -> AppResult<Vec<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Registration::try_from)
        .collect()
    }
}

// ---------------------------------------------------------------------------
// SeasonRepo impl
// ---------------------------------------------------------------------------

impl SeasonRepo for SqliteRepo {
    async fn get_active_season(&self) -> AppResult<Season> {
        let row = sqlx::query_as::<_, SeasonRow>(
            "SELECT id, status, started_at, ends_at, map_seed, save_path, created_at \
             FROM seasons WHERE status = 'active' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AppError::NoActiveSeason)?;

        Season::try_from(row)
    }

    async fn get_season_by_id(&self, id: SeasonId) -> AppResult<Option<Season>> {
        sqlx::query_as::<_, SeasonRow>(
            "SELECT id, status, started_at, ends_at, map_seed, save_path, created_at \
             FROM seasons WHERE id = ?",
        )
        .bind(id.as_i64())
        .fetch_optional(&self.pool)
        .await?
        .map(Season::try_from)
        .transpose()
    }

    async fn list_seasons(&self) -> AppResult<Vec<Season>> {
        sqlx::query_as::<_, SeasonRow>(
            "SELECT id, status, started_at, ends_at, map_seed, save_path, created_at \
             FROM seasons ORDER BY id DESC",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Season::try_from)
        .collect()
    }

    async fn create_season(
        &self,
        id: SeasonId,
        started_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> AppResult<Season> {
        let started_str = started_at.to_rfc3339();
        let ends_str = ends_at.to_rfc3339();
        let created_str = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO seasons (id, status, started_at, ends_at, created_at) \
             VALUES (?, 'active', ?, ?, ?)",
        )
        .bind(id.as_i64())
        .bind(&started_str)
        .bind(&ends_str)
        .bind(&created_str)
        .execute(&self.pool)
        .await?;

        self.get_season_by_id(id).await?.ok_or(AppError::NotFound)
    }

    async fn archive_season(&self, id: SeasonId) -> AppResult<()> {
        sqlx::query("UPDATE seasons SET status = 'archived' WHERE id = ?")
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RegistrationRepo impl
// ---------------------------------------------------------------------------

impl RegistrationRepo for SqliteRepo {
    async fn create_registration(&self, reg: &Registration) -> AppResult<()> {
        let confirmed_str = reg.confirmed_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO registrations \
             (id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
              status, access_tier, amount_wei, created_at, confirmed_at, deaths) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(reg.id.to_string())
        .bind(reg.season_id.as_i64())
        .bind(reg.factorio_name.as_str())
        .bind(reg.eth_address.as_str())
        .bind(&reg.promo_code)
        .bind(&reg.tx_hash)
        .bind(reg.status.as_db_str())
        .bind(reg.access_tier.as_db_str())
        .bind(reg.amount_wei.as_str())
        .bind(reg.created_at.to_rfc3339())
        .bind(&confirmed_str)
        .bind(reg.deaths as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_registration_by_id(&self, id: Uuid) -> AppResult<Option<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(Registration::try_from)
        .transpose()
    }

    async fn find_pending_by_amount(&self, amount: &Wei) -> AppResult<Option<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations \
             WHERE status = 'awaiting_payment' AND amount_wei = ? \
             ORDER BY created_at ASC LIMIT 1",
        )
        .bind(amount.as_str())
        .fetch_optional(&self.pool)
        .await?
        .map(Registration::try_from)
        .transpose()
    }

    async fn confirm_registration(&self, id: Uuid, tx_hash: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE registrations SET status = 'confirmed', tx_hash = ?, confirmed_at = ? \
             WHERE id = ?",
        )
        .bind(tx_hash)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_confirmed_for_season(&self, season_id: SeasonId) -> AppResult<Vec<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations \
             WHERE season_id = ? AND status = 'confirmed'",
        )
        .bind(season_id.as_i64())
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(Registration::try_from)
        .collect()
    }

    async fn expire_stale_registrations(&self, cutoff: DateTime<Utc>) -> AppResult<u64> {
        let cutoff_str = cutoff.to_rfc3339();
        let result = sqlx::query(
            "UPDATE registrations SET status = 'expired' \
             WHERE status = 'awaiting_payment' AND created_at < ?",
        )
        .bind(&cutoff_str)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn find_registration_by_name_and_season(
        &self,
        name: &FactorioName,
        season_id: SeasonId,
    ) -> AppResult<Option<Registration>> {
        sqlx::query_as::<_, RegistrationRow>(
            "SELECT id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
             status, access_tier, amount_wei, created_at, confirmed_at, deaths \
             FROM registrations \
             WHERE factorio_name = ? AND season_id = ? AND status != 'expired' \
             LIMIT 1",
        )
        .bind(name.as_str())
        .bind(season_id.as_i64())
        .fetch_optional(&self.pool)
        .await?
        .map(Registration::try_from)
        .transpose()
    }

    async fn demote_to_spectator(
        &self,
        name: &FactorioName,
        season_id: SeasonId,
    ) -> AppResult<bool> {
        let result = sqlx::query(
            "UPDATE registrations SET access_tier = 'standard', deaths = deaths + 1 \
             WHERE factorio_name = ? AND season_id = ? AND status = 'confirmed' \
             AND access_tier = 'instant_player'",
        )
        .bind(name.as_str())
        .bind(season_id.as_i64())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn expire_registration(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE registrations SET status = 'expired' WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RotationRepo impl
// ---------------------------------------------------------------------------

impl RotationRepo for SqliteRepo {
    async fn carry_forward_players(&self, prev: SeasonId, new: SeasonId) -> AppResult<u64> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
            "INSERT INTO registrations \
             (id, season_id, factorio_name, eth_address, promo_code, tx_hash, \
              status, access_tier, amount_wei, created_at, confirmed_at, deaths) \
             SELECT hex(randomblob(16)), ?, factorio_name, eth_address, NULL, NULL, \
                    'confirmed', 'instant_player', '0', ?, ?, 0 \
             FROM registrations \
             WHERE season_id = ? AND status = 'confirmed' \
             AND factorio_name NOT IN ( \
                 SELECT factorio_name FROM registrations WHERE season_id = ? \
             )",
        )
        .bind(new.as_i64())
        .bind(&now)
        .bind(&now)
        .bind(prev.as_i64())
        .bind(new.as_i64())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn get_confirmed_player_names(&self, season_id: SeasonId) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT factorio_name FROM registrations \
             WHERE season_id = ? AND status = 'confirmed'",
        )
        .bind(season_id.as_i64())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    async fn get_spectator_names(&self, season_id: SeasonId) -> AppResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT factorio_name FROM registrations \
             WHERE season_id = ? AND status = 'confirmed' AND access_tier = 'standard'",
        )
        .bind(season_id.as_i64())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    async fn get_archived_seasons_for_purge(&self, keep: usize) -> AppResult<Vec<Season>> {
        let rows = sqlx::query_as::<_, SeasonRow>(
            "SELECT id, status, started_at, ends_at, map_seed, save_path, created_at \
             FROM seasons WHERE status = 'archived' ORDER BY id DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().skip(keep).map(Season::try_from).collect()
    }

    async fn update_season_save_path(&self, id: SeasonId, path: &str) -> AppResult<()> {
        sqlx::query("UPDATE seasons SET save_path = ? WHERE id = ?")
            .bind(path)
            .bind(id.as_i64())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PromoCodeRepo impl
// ---------------------------------------------------------------------------

impl PromoCodeRepo for SqliteRepo {
    async fn get_promo_code(&self, code: &str) -> AppResult<Option<PromoCode>> {
        sqlx::query_as::<_, PromoCodeRow>(
            "SELECT code, discount_percent, grants_instant_player, max_uses, \
             times_used, active, created_at, expires_at \
             FROM promo_codes WHERE code = ?",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?
        .map(PromoCode::try_from)
        .transpose()
    }

    async fn increment_promo_usage(&self, code: &str) -> AppResult<()> {
        sqlx::query("UPDATE promo_codes SET times_used = times_used + 1 WHERE code = ?")
            .bind(code)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_promo_code(&self, promo: &PromoCode) -> AppResult<()> {
        let expires_str = promo.expires_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            "INSERT INTO promo_codes \
             (code, discount_percent, grants_instant_player, max_uses, \
              times_used, active, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&promo.code)
        .bind(promo.discount_percent.as_u8() as i32)
        .bind(i32::from(promo.grants_instant_player))
        .bind(promo.max_uses.map(|v| v as i32))
        .bind(promo.times_used as i32)
        .bind(i32::from(promo.active))
        .bind(promo.created_at.to_rfc3339())
        .bind(&expires_str)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_promo_codes(&self) -> AppResult<Vec<PromoCode>> {
        sqlx::query_as::<_, PromoCodeRow>(
            "SELECT code, discount_percent, grants_instant_player, max_uses, \
             times_used, active, created_at, expires_at \
             FROM promo_codes ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(PromoCode::try_from)
        .collect()
    }

    async fn deactivate_promo_code(&self, code: &str) -> AppResult<bool> {
        let result = sqlx::query("UPDATE promo_codes SET active = 0 WHERE code = ? AND active = 1")
            .bind(code)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
