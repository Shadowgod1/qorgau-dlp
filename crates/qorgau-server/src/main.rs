mod audit;
mod auth;
mod config;
mod error;
mod policy_engine;
mod routes;
mod state;
mod storage;
mod workers;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::mpsc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::auth::{seed_default_admin, LoginRateLimiter};
use crate::config::AppConfig;
use crate::policy_engine::{seed_default_categories, PolicyEngine};
use crate::state::AppState;
use crate::workers::spawn_background_workers;
use crate::ws::{ws_agent_handler, ws_dashboard_handler, WsHub};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let cfg = AppConfig::from_env();
    tracing::info!(bind = %cfg.bind_addr, "Starting Qorgau DLP Server v2.0 (Rust)");

    let db = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&cfg.database_url)
        .await?;

    // Run embedded SQLx migrations automatically
    sqlx::migrate!("./migrations").run(&db).await?;

    seed_default_admin(&db, &cfg.admin_username, &cfg.admin_password).await?;
    seed_default_categories(&db).await?;

    let policy = PolicyEngine::new();
    policy.reload_all(&db).await?;

    let (alert_tx, alert_rx) = mpsc::channel(4096);
    let (ocr_tx, ocr_rx) = mpsc::channel(256);

    let max_body = cfg.shadow_max_bytes + (5 * 1024 * 1024);
    let cors_origins: Vec<HeaderValue> = cfg
        .cors_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();

    let state = Arc::new(AppState {
        config: cfg.clone(),
        db,
        policy,
        ws: WsHub::new(),
        login_limiter: LoginRateLimiter::default(),
        alert_tx,
        ocr_tx,
    });

    spawn_background_workers(state.clone(), alert_rx, ocr_rx);

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderName::from_static("x-agent-token"),
        ])
        .allow_credentials(true);

    let app = Router::new()
        .route("/health", get(health))
        .route("/ws/agent", get(ws_agent_handler))
        .route("/ws/dashboard", get(ws_dashboard_handler))
        .nest("/api/v1", routes::api_v1_router())
        .layer(DefaultBodyLimit::max(max_body))
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = cfg.bind_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "Qorgau DLP Server listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "Qorgau DLP",
        "engine": "rust-axum",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Graceful shutdown initiated");
}
