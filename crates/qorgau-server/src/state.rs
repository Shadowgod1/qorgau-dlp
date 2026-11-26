use chrono::{DateTime, Utc};
use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::auth::LoginRateLimiter;
use crate::config::AppConfig;
use crate::policy_engine::PolicyEngine;
use crate::workers::{AlertJob, OcrJob};
use crate::ws::WsHub;

pub struct AppState {
    pub config: AppConfig,
    pub db: PgPool,
    pub policy: PolicyEngine,
    pub ws: WsHub,
    pub login_limiter: LoginRateLimiter,
    pub alert_tx: mpsc::Sender<AlertJob>,
    pub ocr_tx: mpsc::Sender<OcrJob>,
    /// In-memory latest live frame per agent for Quadrator (0 disk/DB bloat!)
    pub live_frames: DashMap<String, (DateTime<Utc>, Vec<u8>)>,
}
