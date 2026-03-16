use chrono::Utc;
use factorio_seasons::domain::{
    compute_effective_price, determine_access_tier, determine_initial_status,
    validate_promo_usable, AccessTier, DiscountPercent, EthAddress, FactorioName, PromoCode,
    RegStatus, SeasonId, SeasonStatus, Wei,
};
use factorio_seasons::error::AppError;

// ---------------------------------------------------------------------------
// SeasonId
// ---------------------------------------------------------------------------

#[test]
fn season_id_valid() {
    let id = SeasonId::new(1).unwrap();
    assert_eq!(id.as_i64(), 1);
}

#[test]
fn season_id_zero_rejected() {
    assert!(SeasonId::new(0).is_err());
}

#[test]
fn season_id_negative_rejected() {
    assert!(SeasonId::new(-1).is_err());
}

#[test]
fn season_id_next() {
    let id = SeasonId::new(5).unwrap();
    assert_eq!(id.next().as_i64(), 6);
}

// ---------------------------------------------------------------------------
// FactorioName
// ---------------------------------------------------------------------------

#[test]
fn factorio_name_valid() {
    let name = FactorioName::try_from("Player1".to_string()).unwrap();
    assert_eq!(name.as_str(), "Player1");
}

#[test]
fn factorio_name_empty_rejected() {
    assert!(FactorioName::try_from("".to_string()).is_err());
}

#[test]
fn factorio_name_too_long_rejected() {
    let long = "a".repeat(51);
    assert!(FactorioName::try_from(long).is_err());
}

#[test]
fn factorio_name_50_chars_ok() {
    let name = "a".repeat(50);
    assert!(FactorioName::try_from(name).is_ok());
}

#[test]
fn factorio_name_non_ascii_rejected() {
    assert!(FactorioName::try_from("Player\u{00e9}".to_string()).is_err());
}

#[test]
fn factorio_name_with_space_ok() {
    assert!(FactorioName::try_from("Player One".to_string()).is_ok());
}

// ---------------------------------------------------------------------------
// EthAddress
// ---------------------------------------------------------------------------

#[test]
fn eth_address_valid() {
    let addr =
        EthAddress::try_from("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD00".to_string()).unwrap();
    assert!(addr.as_str().starts_with("0x"));
    assert_eq!(addr.as_str().len(), 42);
}

#[test]
fn eth_address_normalized_lowercase() {
    let addr =
        EthAddress::try_from("0xABCDEF1234567890ABCDEF1234567890ABCDEF12".to_string()).unwrap();
    assert_eq!(addr.as_str(), "0xabcdef1234567890abcdef1234567890abcdef12");
}

#[test]
fn eth_address_too_short() {
    assert!(EthAddress::try_from("0x1234".to_string()).is_err());
}

#[test]
fn eth_address_missing_prefix() {
    assert!(EthAddress::try_from("742d35Cc6634C0532925a3b844Bc9e7595f2bD00".to_string()).is_err());
}

#[test]
fn eth_address_non_hex() {
    assert!(
        EthAddress::try_from("0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ".to_string()).is_err()
    );
}

// ---------------------------------------------------------------------------
// Wei
// ---------------------------------------------------------------------------

#[test]
fn wei_valid() {
    let w = Wei::from_decimal("1000000000000000000".to_string()).unwrap();
    assert_eq!(w.as_str(), "1000000000000000000");
}

#[test]
fn wei_zero() {
    let w = Wei::zero();
    assert!(w.is_zero());
    assert_eq!(w.as_str(), "0");
}

#[test]
fn wei_empty_rejected() {
    assert!(Wei::from_decimal("".to_string()).is_err());
}

#[test]
fn wei_non_digit_rejected() {
    assert!(Wei::from_decimal("123abc".to_string()).is_err());
}

#[test]
fn wei_negative_rejected() {
    assert!(Wei::from_decimal("-100".to_string()).is_err());
}

// ---------------------------------------------------------------------------
// DiscountPercent
// ---------------------------------------------------------------------------

#[test]
fn discount_valid_range() {
    assert!(DiscountPercent::try_from(0u8).is_ok());
    assert!(DiscountPercent::try_from(50u8).is_ok());
    assert!(DiscountPercent::try_from(100u8).is_ok());
}

#[test]
fn discount_over_100_rejected() {
    assert!(DiscountPercent::try_from(101u8).is_err());
}

#[test]
fn discount_full_discount() {
    let d = DiscountPercent::try_from(100u8).unwrap();
    assert!(d.is_full_discount());
    let d = DiscountPercent::try_from(50u8).unwrap();
    assert!(!d.is_full_discount());
}

// ---------------------------------------------------------------------------
// Enum roundtrips
// ---------------------------------------------------------------------------

#[test]
fn access_tier_roundtrip() {
    assert_eq!(
        AccessTier::try_from(AccessTier::Standard.as_db_str()).unwrap(),
        AccessTier::Standard
    );
    assert_eq!(
        AccessTier::try_from(AccessTier::InstantPlayer.as_db_str()).unwrap(),
        AccessTier::InstantPlayer
    );
}

#[test]
fn season_status_roundtrip() {
    for s in [
        SeasonStatus::Pending,
        SeasonStatus::Active,
        SeasonStatus::Archived,
    ] {
        assert_eq!(SeasonStatus::try_from(s.as_db_str()).unwrap(), s);
    }
}

#[test]
fn reg_status_roundtrip() {
    for s in [
        RegStatus::AwaitingPayment,
        RegStatus::Confirmed,
        RegStatus::Expired,
    ] {
        assert_eq!(RegStatus::try_from(s.as_db_str()).unwrap(), s);
    }
}

#[test]
fn unknown_access_tier_rejected() {
    assert!(AccessTier::try_from("unknown").is_err());
}

#[test]
fn unknown_season_status_rejected() {
    assert!(SeasonStatus::try_from("deleted").is_err());
}

#[test]
fn unknown_reg_status_rejected() {
    assert!(RegStatus::try_from("cancelled").is_err());
}

// ---------------------------------------------------------------------------
// compute_effective_price
// ---------------------------------------------------------------------------

#[test]
fn compute_price_no_discount_no_offset() {
    let d = DiscountPercent::try_from(0u8).unwrap();
    assert_eq!(compute_effective_price(1000, d, 0), 1000);
}

#[test]
fn compute_price_50_percent_discount() {
    let d = DiscountPercent::try_from(50u8).unwrap();
    assert_eq!(compute_effective_price(1000, d, 0), 500);
}

#[test]
fn compute_price_with_offset() {
    let d = DiscountPercent::try_from(0u8).unwrap();
    assert_eq!(compute_effective_price(1000, d, 5), 1005);
}

#[test]
fn compute_price_100_percent_returns_zero() {
    let d = DiscountPercent::try_from(100u8).unwrap();
    assert_eq!(compute_effective_price(1000, d, 42), 0);
}

#[test]
fn compute_price_100_discount_no_offset() {
    let d = DiscountPercent::try_from(100u8).unwrap();
    assert_eq!(compute_effective_price(50000000000000000, d, 0), 0);
}

// ---------------------------------------------------------------------------
// determine_access_tier
// ---------------------------------------------------------------------------

#[test]
fn determine_access_tier_no_promo() {
    assert_eq!(determine_access_tier(None), AccessTier::Standard);
}

#[test]
fn determine_access_tier_standard_promo() {
    let promo = make_promo(50, false);
    assert_eq!(determine_access_tier(Some(&promo)), AccessTier::Standard);
}

#[test]
fn determine_access_tier_instant_promo() {
    let promo = make_promo(100, true);
    assert_eq!(
        determine_access_tier(Some(&promo)),
        AccessTier::InstantPlayer
    );
}

// ---------------------------------------------------------------------------
// determine_initial_status
// ---------------------------------------------------------------------------

#[test]
fn determine_initial_status_free() {
    assert_eq!(determine_initial_status(true), RegStatus::Confirmed);
}

#[test]
fn determine_initial_status_paid() {
    assert_eq!(determine_initial_status(false), RegStatus::AwaitingPayment);
}

// ---------------------------------------------------------------------------
// validate_promo_usable
// ---------------------------------------------------------------------------

#[test]
fn promo_usable_active_no_limits() {
    let promo = make_promo(50, false);
    assert!(validate_promo_usable(&promo, Utc::now()).is_ok());
}

#[test]
fn promo_inactive_rejected() {
    let mut promo = make_promo(50, false);
    promo.active = false;
    assert!(matches!(
        validate_promo_usable(&promo, Utc::now()),
        Err(AppError::InvalidPromoCode)
    ));
}

#[test]
fn promo_expired_rejected() {
    let mut promo = make_promo(50, false);
    promo.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
    assert!(matches!(
        validate_promo_usable(&promo, Utc::now()),
        Err(AppError::PromoExpired)
    ));
}

#[test]
fn promo_exhausted_rejected() {
    let mut promo = make_promo(50, false);
    promo.max_uses = Some(5);
    promo.times_used = 5;
    assert!(matches!(
        validate_promo_usable(&promo, Utc::now()),
        Err(AppError::PromoExhausted)
    ));
}

#[test]
fn promo_not_yet_exhausted_ok() {
    let mut promo = make_promo(50, false);
    promo.max_uses = Some(5);
    promo.times_used = 4;
    assert!(validate_promo_usable(&promo, Utc::now()).is_ok());
}

#[test]
fn promo_future_expiry_ok() {
    let mut promo = make_promo(50, false);
    promo.expires_at = Some(Utc::now() + chrono::Duration::hours(24));
    assert!(validate_promo_usable(&promo, Utc::now()).is_ok());
}

// ---------------------------------------------------------------------------
// Config parsing (moved from config.rs inline tests)
// ---------------------------------------------------------------------------

use factorio_seasons::config::AppConfig;
use std::path::Path;

fn minimal_toml() -> &'static str {
    r#"
[server]
bind = "127.0.0.1:3000"
static_dir = "./static"

[factorio]
binary = "/opt/factorio/bin/x64/factorio"
saves_dir = "/opt/factorio/saves"
archive_dir = "/opt/factorio/archive"
data_dir = "/opt/factorio/data"
rcon_host = "127.0.0.1"
rcon_port = 27015
rcon_pw_file = "/opt/factorio/rcon.pw"
map_gen_settings = "/opt/factorio/data/map-gen-settings.json"

[eth]
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/test"
deposit_address = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD00"
base_fee_wei = "50000000000000000"

[schedule]
rotation_day = "Monday"
rotation_hour = 12

[admin]
token = "secret-admin-token"

[database]
url = "sqlite::memory:"
"#
}

#[test]
fn parse_minimal_config() {
    let config = AppConfig::from_str(minimal_toml()).unwrap();
    assert_eq!(config.server.bind, "127.0.0.1:3000");
    assert_eq!(config.eth.base_fee_wei, "50000000000000000");
    assert_eq!(config.schedule.rotation_hour, 12);
    assert_eq!(config.admin.token, "secret-admin-token");
    assert_eq!(config.database.url, "sqlite::memory:");
}

#[test]
fn payment_expiry_default_48h() {
    let config = AppConfig::from_str(minimal_toml()).unwrap();
    assert_eq!(config.eth.payment_expiry_hours, 48);
}

#[test]
fn payment_expiry_custom() {
    let toml = minimal_toml().replace(
        "base_fee_wei = \"50000000000000000\"",
        "base_fee_wei = \"50000000000000000\"\npayment_expiry_hours = 24",
    );
    let config = AppConfig::from_str(&toml).unwrap();
    assert_eq!(config.eth.payment_expiry_hours, 24);
}

#[test]
fn logging_defaults() {
    let config = AppConfig::from_str(minimal_toml()).unwrap();
    assert_eq!(config.logging.level, "info");
    assert_eq!(config.logging.format, "json");
}

#[test]
fn logging_custom() {
    let toml = format!(
        "{}\n[logging]\nlevel = \"debug\"\nformat = \"pretty\"",
        minimal_toml()
    );
    let config = AppConfig::from_str(&toml).unwrap();
    assert_eq!(config.logging.level, "debug");
    assert_eq!(config.logging.format, "pretty");
}

#[test]
fn missing_section_rejected() {
    let bad = "[server]\nbind = \"0.0.0.0:3000\"\nstatic_dir = \".\"";
    assert!(AppConfig::from_str(bad).is_err());
}

#[test]
fn nonexistent_file_rejected() {
    assert!(AppConfig::from_file(Path::new("/nonexistent/path.toml")).is_err());
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_promo(discount: u8, instant: bool) -> PromoCode {
    PromoCode {
        code: "TEST".to_string(),
        discount_percent: DiscountPercent::try_from(discount).unwrap(),
        grants_instant_player: instant,
        max_uses: None,
        times_used: 0,
        active: true,
        created_at: Utc::now(),
        expires_at: None,
    }
}
