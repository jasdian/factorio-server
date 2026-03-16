use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use sqlx::sqlite::SqlitePool;
use tower::ServiceExt;

use factorio_seasons::api::build_api_router;
use factorio_seasons::config::AppConfig;
use factorio_seasons::db::{PromoCodeRepo, SqliteRepo};
use factorio_seasons::domain::{DiscountPercent, PromoCode};
use factorio_seasons::services::rcon::RconConfig;
use factorio_seasons::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_config_toml() -> &'static str {
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
deposit_address = "0x742d35cc6634c0532925a3b844bc9e7595f2bd00"
base_fee_wei = "50000000000000000"

[schedule]
rotation_day = "Monday"
rotation_hour = 12

[admin]
token = "test-admin-token"

[database]
url = "sqlite::memory:"
"#
}

async fn setup_app() -> (axum::Router, SqliteRepo) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let repo = SqliteRepo::new(pool);
    repo.run_migrations().await.unwrap();

    let config = AppConfig::from_str(test_config_toml()).unwrap();

    let data_dir = std::env::temp_dir().join(format!("fseasons_test_{}", std::process::id()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let state = AppState {
        repo: repo.clone(),
        archive_dir: PathBuf::from("/tmp/archive"),
        factorio_data_dir: data_dir,
        deposit_address: config.eth.deposit_address.clone(),
        base_fee_wei: config.eth.base_fee_wei.clone(),
        admin_token: Arc::from(config.admin.token.as_str()),
        static_dir: PathBuf::from("./static"),
        config: Arc::new(config),
        rcon_config: Arc::new(RconConfig {
            url: "127.0.0.1:27015".to_string(),
            password: "test".to_string(),
        }),
    };

    let app = build_api_router()
        .route("/health", axum::routing::get(|| async { StatusCode::OK }))
        .with_state(state);

    (app, repo)
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_endpoint() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Season endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_current_season() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/season")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], 1);
    assert_eq!(json["status"], "active");
    assert!(json["player_count"].is_number());
    assert!(json["spectator_count"].is_number());
}

#[tokio::test]
async fn list_seasons() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/seasons")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(!json.as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_success() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "factorio_name": "TestPlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["registration_id"].is_string());
    assert_eq!(json["status"], "awaiting_payment");
    assert_eq!(json["access_tier"], "standard");
    assert!(json["deposit_address"].is_string());
}

#[tokio::test]
async fn register_with_100_percent_promo() {
    let (app, repo) = setup_app().await;

    let promo = PromoCode {
        code: "FREEWEEK".to_string(),
        discount_percent: DiscountPercent::try_from(100u8).unwrap(),
        grants_instant_player: false,
        max_uses: None,
        times_used: 0,
        active: true,
        created_at: chrono::Utc::now(),
        expires_at: None,
    };
    repo.create_promo_code(&promo).await.unwrap();

    let body = serde_json::json!({
        "factorio_name": "FreePlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00",
        "promo_code": "FREEWEEK"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "confirmed");
    assert_eq!(json["amount_wei"], "0");
    assert!(json["deposit_address"].is_null());
}

#[tokio::test]
async fn register_with_instant_player_promo() {
    let (app, repo) = setup_app().await;

    let promo = PromoCode {
        code: "VIP".to_string(),
        discount_percent: DiscountPercent::try_from(100u8).unwrap(),
        grants_instant_player: true,
        max_uses: None,
        times_used: 0,
        active: true,
        created_at: chrono::Utc::now(),
        expires_at: None,
    };
    repo.create_promo_code(&promo).await.unwrap();

    let body = serde_json::json!({
        "factorio_name": "VipPlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00",
        "promo_code": "VIP"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "confirmed");
    assert_eq!(json["access_tier"], "instant_player");
}

#[tokio::test]
async fn register_duplicate_rejected() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "factorio_name": "DupePlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00"
    });
    let json_str = serde_json::to_string(&body).unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(json_str.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(json_str))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn register_invalid_eth_address() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "factorio_name": "Player",
        "eth_address": "not-an-address"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_invalid_promo_rejected() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "factorio_name": "Player",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00",
        "promo_code": "NONEXISTENT"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Registration status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registration_status_ok() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "factorio_name": "StatusPlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00"
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let reg_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let reg_json: serde_json::Value = serde_json::from_slice(&reg_body).unwrap();
    let reg_id = reg_json["registration_id"].as_str().unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/register/{reg_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["factorio_name"], "StatusPlayer");
    assert_eq!(json["status"], "awaiting_payment");
}

#[tokio::test]
async fn registration_status_not_found() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/register/00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Admin endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_no_auth_rejected() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/promo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_wrong_token_rejected() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/promo")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_create_and_list_promo() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "code": "HALFOFF",
        "discount_percent": 50,
        "grants_instant_player": false,
        "max_uses": 100
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/admin/promo")
                .header("authorization", "Bearer test-admin-token")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["code"], "HALFOFF");
    assert_eq!(json["discount_percent"], 50);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/promo")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn admin_revoke_promo() {
    let (app, _) = setup_app().await;
    let body = serde_json::json!({
        "code": "REVOKEME",
        "discount_percent": 25,
        "grants_instant_player": false
    });

    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/admin/promo")
                .header("authorization", "Bearer test-admin-token")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri("/api/admin/promo/REVOKEME")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .oneshot(
            Request::builder()
                .method(http::Method::DELETE)
                .uri("/api/admin/promo/NOPE")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_list_registrations() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/registrations")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
}

#[tokio::test]
async fn admin_list_registrations_filter_by_season() {
    let (app, _) = setup_app().await;

    let body = serde_json::json!({
        "factorio_name": "FilterPlayer",
        "eth_address": "0x742d35cc6634c0532925a3b844bc9e7595f2bd00"
    });
    app.clone()
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin/registrations?season_id=1")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/admin/registrations?season_id=999")
                .header("authorization", "Bearer test-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Map download
// ---------------------------------------------------------------------------

#[tokio::test]
async fn map_download_active_season_rejected() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/maps/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn map_download_nonexistent_season() {
    let (app, _) = setup_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/maps/999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
