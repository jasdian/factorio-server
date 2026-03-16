use chrono::Utc;
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

use factorio_seasons::db::{PromoCodeRepo, RegistrationRepo, RotationRepo, SeasonRepo, SqliteRepo};
use factorio_seasons::domain::{
    AccessTier, DiscountPercent, EthAddress, FactorioName, PromoCode, RegStatus, Registration,
    SeasonId, SeasonStatus, Wei,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_repo() -> SqliteRepo {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteRepo::new(pool);
    repo.run_migrations().await.unwrap();
    repo
}

fn make_registration(season_id: SeasonId) -> Registration {
    Registration {
        id: Uuid::new_v4(),
        season_id,
        factorio_name: FactorioName::try_from("TestPlayer".to_string()).unwrap(),
        eth_address: EthAddress::try_from("0x742d35cc6634c0532925a3b844bc9e7595f2bd00".to_string())
            .unwrap(),
        promo_code: None,
        tx_hash: None,
        status: RegStatus::AwaitingPayment,
        access_tier: AccessTier::Standard,
        amount_wei: Wei::from_decimal("50000000000000001".to_string()).unwrap(),
        created_at: Utc::now(),
        confirmed_at: None,
        deaths: 0,
    }
}

fn make_promo() -> PromoCode {
    PromoCode {
        code: "TESTPROMO".to_string(),
        discount_percent: DiscountPercent::try_from(50u8).unwrap(),
        grants_instant_player: false,
        max_uses: Some(10),
        times_used: 0,
        active: true,
        created_at: Utc::now(),
        expires_at: None,
    }
}

// ---------------------------------------------------------------------------
// SeasonRepo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_active_season_from_migration() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    assert_eq!(season.id.as_i64(), 1);
    assert_eq!(season.status, SeasonStatus::Active);
}

#[tokio::test]
async fn get_season_by_id() {
    let repo = setup_repo().await;
    let season = repo
        .get_season_by_id(SeasonId::new(1).unwrap())
        .await
        .unwrap();
    assert!(season.is_some());
    assert_eq!(season.unwrap().id.as_i64(), 1);
}

#[tokio::test]
async fn get_season_by_id_not_found() {
    let repo = setup_repo().await;
    let season = repo
        .get_season_by_id(SeasonId::new(999).unwrap())
        .await
        .unwrap();
    assert!(season.is_none());
}

#[tokio::test]
async fn list_seasons() {
    let repo = setup_repo().await;
    let seasons = repo.list_seasons().await.unwrap();
    assert_eq!(seasons.len(), 1);
}

#[tokio::test]
async fn create_and_archive_season() {
    let repo = setup_repo().await;
    let id = SeasonId::new(2).unwrap();
    let now = Utc::now();
    let ends = now + chrono::Duration::weeks(1);
    repo.archive_season(SeasonId::new(1).unwrap())
        .await
        .unwrap();
    let season = repo.create_season(id, now, ends).await.unwrap();
    assert_eq!(season.id.as_i64(), 2);
    assert_eq!(season.status, SeasonStatus::Active);

    let seasons = repo.list_seasons().await.unwrap();
    assert_eq!(seasons.len(), 2);
}

// ---------------------------------------------------------------------------
// RegistrationRepo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_and_get_registration() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    let reg = make_registration(season.id);
    let reg_id = reg.id;

    repo.create_registration(&reg).await.unwrap();
    let fetched = repo.get_registration_by_id(reg_id).await.unwrap().unwrap();

    assert_eq!(fetched.id, reg_id);
    assert_eq!(fetched.factorio_name.as_str(), "TestPlayer");
    assert_eq!(fetched.status, RegStatus::AwaitingPayment);
}

#[tokio::test]
async fn get_registration_not_found() {
    let repo = setup_repo().await;
    let result = repo.get_registration_by_id(Uuid::new_v4()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_pending_by_amount() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    let reg = make_registration(season.id);
    let amount = reg.amount_wei.clone();

    repo.create_registration(&reg).await.unwrap();

    let found = repo.find_pending_by_amount(&amount).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, reg.id);
}

#[tokio::test]
async fn find_pending_by_amount_no_match() {
    let repo = setup_repo().await;
    let amount = Wei::from_decimal("99999".to_string()).unwrap();
    let found = repo.find_pending_by_amount(&amount).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn confirm_registration() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    let reg = make_registration(season.id);
    let reg_id = reg.id;

    repo.create_registration(&reg).await.unwrap();
    repo.confirm_registration(reg_id, "0xdeadbeef")
        .await
        .unwrap();

    let fetched = repo.get_registration_by_id(reg_id).await.unwrap().unwrap();
    assert_eq!(fetched.status, RegStatus::Confirmed);
    assert_eq!(fetched.tx_hash.as_deref(), Some("0xdeadbeef"));
    assert!(fetched.confirmed_at.is_some());
}

#[tokio::test]
async fn expire_stale_registrations() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(season.id);
    reg.created_at = Utc::now() - chrono::Duration::hours(72);

    repo.create_registration(&reg).await.unwrap();

    let cutoff = Utc::now() - chrono::Duration::hours(48);
    let expired = repo.expire_stale_registrations(cutoff).await.unwrap();
    assert_eq!(expired, 1);

    let fetched = repo.get_registration_by_id(reg.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, RegStatus::Expired);
}

#[tokio::test]
async fn expire_does_not_touch_recent() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    let reg = make_registration(season.id);

    repo.create_registration(&reg).await.unwrap();

    let cutoff = Utc::now() - chrono::Duration::hours(48);
    let expired = repo.expire_stale_registrations(cutoff).await.unwrap();
    assert_eq!(expired, 0);
}

#[tokio::test]
async fn expire_does_not_touch_confirmed() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(season.id);
    reg.status = RegStatus::Confirmed;
    reg.confirmed_at = Some(Utc::now() - chrono::Duration::hours(72));
    reg.created_at = Utc::now() - chrono::Duration::hours(72);

    repo.create_registration(&reg).await.unwrap();

    let cutoff = Utc::now() - chrono::Duration::hours(48);
    let expired = repo.expire_stale_registrations(cutoff).await.unwrap();
    assert_eq!(expired, 0);
}

#[tokio::test]
async fn find_by_name_and_season() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();
    let reg = make_registration(season.id);

    repo.create_registration(&reg).await.unwrap();

    let found = repo
        .find_registration_by_name_and_season(&reg.factorio_name, season.id)
        .await
        .unwrap();
    assert!(found.is_some());

    let other_name = FactorioName::try_from("OtherPlayer".to_string()).unwrap();
    let not_found = repo
        .find_registration_by_name_and_season(&other_name, season.id)
        .await
        .unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn find_by_name_excludes_expired() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(season.id);
    reg.status = RegStatus::Expired;

    repo.create_registration(&reg).await.unwrap();

    let found = repo
        .find_registration_by_name_and_season(&reg.factorio_name, season.id)
        .await
        .unwrap();
    assert!(found.is_none());
}

// ---------------------------------------------------------------------------
// PromoCodeRepo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_and_get_promo() {
    let repo = setup_repo().await;
    let promo = make_promo();
    repo.create_promo_code(&promo).await.unwrap();

    let fetched = repo.get_promo_code("TESTPROMO").await.unwrap().unwrap();
    assert_eq!(fetched.code, "TESTPROMO");
    assert_eq!(fetched.discount_percent.as_u8(), 50);
    assert_eq!(fetched.times_used, 0);
}

#[tokio::test]
async fn get_promo_not_found() {
    let repo = setup_repo().await;
    let result = repo.get_promo_code("NONEXISTENT").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn increment_promo_usage() {
    let repo = setup_repo().await;
    let promo = make_promo();
    repo.create_promo_code(&promo).await.unwrap();

    repo.increment_promo_usage("TESTPROMO").await.unwrap();
    repo.increment_promo_usage("TESTPROMO").await.unwrap();

    let fetched = repo.get_promo_code("TESTPROMO").await.unwrap().unwrap();
    assert_eq!(fetched.times_used, 2);
}

#[tokio::test]
async fn list_promo_codes() {
    let repo = setup_repo().await;
    let promo1 = make_promo();
    let mut promo2 = make_promo();
    promo2.code = "ANOTHER".to_string();

    repo.create_promo_code(&promo1).await.unwrap();
    repo.create_promo_code(&promo2).await.unwrap();

    let list = repo.list_promo_codes().await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn deactivate_promo() {
    let repo = setup_repo().await;
    let promo = make_promo();
    repo.create_promo_code(&promo).await.unwrap();

    let deactivated = repo.deactivate_promo_code("TESTPROMO").await.unwrap();
    assert!(deactivated);

    let fetched = repo.get_promo_code("TESTPROMO").await.unwrap().unwrap();
    assert!(!fetched.active);

    let again = repo.deactivate_promo_code("TESTPROMO").await.unwrap();
    assert!(!again);
}

#[tokio::test]
async fn deactivate_nonexistent_promo() {
    let repo = setup_repo().await;
    let result = repo.deactivate_promo_code("NOPE").await.unwrap();
    assert!(!result);
}

// ---------------------------------------------------------------------------
// RotationRepo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn carry_forward_players() {
    let repo = setup_repo().await;
    let s1 = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(s1.id);
    reg.status = RegStatus::Confirmed;
    reg.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg).await.unwrap();

    repo.archive_season(s1.id).await.unwrap();
    let now = Utc::now();
    let s2 = repo
        .create_season(
            SeasonId::new(2).unwrap(),
            now,
            now + chrono::Duration::weeks(1),
        )
        .await
        .unwrap();

    let carried = repo.carry_forward_players(s1.id, s2.id).await.unwrap();
    assert_eq!(carried, 1);

    let names = repo.get_confirmed_player_names(s2.id).await.unwrap();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "TestPlayer");
}

#[tokio::test]
async fn carry_forward_deduplicates() {
    let repo = setup_repo().await;
    let s1 = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(s1.id);
    reg.status = RegStatus::Confirmed;
    reg.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg).await.unwrap();

    repo.archive_season(s1.id).await.unwrap();
    let now = Utc::now();
    let s2 = repo
        .create_season(
            SeasonId::new(2).unwrap(),
            now,
            now + chrono::Duration::weeks(1),
        )
        .await
        .unwrap();

    let mut reg2 = make_registration(s2.id);
    reg2.status = RegStatus::Confirmed;
    reg2.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg2).await.unwrap();

    let carried = repo.carry_forward_players(s1.id, s2.id).await.unwrap();
    assert_eq!(carried, 0);
}

#[tokio::test]
async fn spectator_names() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(season.id);
    reg.status = RegStatus::Confirmed;
    reg.access_tier = AccessTier::Standard;
    reg.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg).await.unwrap();

    let spectators = repo.get_spectator_names(season.id).await.unwrap();
    assert_eq!(spectators.len(), 1);

    let players = repo.get_confirmed_player_names(season.id).await.unwrap();
    assert_eq!(players.len(), 1);
}

#[tokio::test]
async fn season_save_path_update() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    repo.update_season_save_path(season.id, "/archive/s1.zip")
        .await
        .unwrap();

    let updated = repo.get_season_by_id(season.id).await.unwrap().unwrap();
    assert_eq!(updated.save_path.as_deref(), Some("/archive/s1.zip"));
}

#[tokio::test]
async fn count_season_registrations() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    assert_eq!(repo.count_season_registrations(season.id).await.unwrap(), 0);

    let reg = make_registration(season.id);
    repo.create_registration(&reg).await.unwrap();

    assert_eq!(repo.count_season_registrations(season.id).await.unwrap(), 1);
}

#[tokio::test]
async fn count_excludes_expired() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg = make_registration(season.id);
    reg.status = RegStatus::Expired;
    repo.create_registration(&reg).await.unwrap();

    assert_eq!(repo.count_season_registrations(season.id).await.unwrap(), 0);
}

#[tokio::test]
async fn player_and_spectator_counts() {
    let repo = setup_repo().await;
    let season = repo.get_active_season().await.unwrap();

    let mut reg1 = make_registration(season.id);
    reg1.status = RegStatus::Confirmed;
    reg1.access_tier = AccessTier::Standard;
    reg1.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg1).await.unwrap();

    let mut reg2 = make_registration(season.id);
    reg2.id = Uuid::new_v4();
    reg2.factorio_name = FactorioName::try_from("Player2".to_string()).unwrap();
    reg2.status = RegStatus::Confirmed;
    reg2.access_tier = AccessTier::InstantPlayer;
    reg2.confirmed_at = Some(Utc::now());
    repo.create_registration(&reg2).await.unwrap();

    assert_eq!(repo.get_player_count(season.id).await.unwrap(), 1);
    assert_eq!(repo.get_spectator_count(season.id).await.unwrap(), 1);
}

#[tokio::test]
async fn archived_seasons_for_purge() {
    let repo = setup_repo().await;

    repo.archive_season(SeasonId::new(1).unwrap())
        .await
        .unwrap();
    for i in 2..=5 {
        let id = SeasonId::new(i).unwrap();
        let now = Utc::now();
        repo.create_season(id, now, now + chrono::Duration::weeks(1))
            .await
            .unwrap();
        if i < 5 {
            repo.archive_season(id).await.unwrap();
        }
    }

    let to_purge = repo.get_archived_seasons_for_purge(3).await.unwrap();
    assert_eq!(to_purge.len(), 1);
    assert_eq!(to_purge[0].id.as_i64(), 1);
}
