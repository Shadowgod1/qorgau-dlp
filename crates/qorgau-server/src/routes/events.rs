use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{AuthUser, OfficerUser};
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events/usb", get(list_usb))
        .route("/events/app", get(list_app))
        .route("/events/messenger", get(list_messenger))
        .route("/events/clipboard", get(list_clipboard))
        .route("/events/network", get(list_network))
        .route("/events/print", get(list_print))
        .route("/sessions", get(list_sessions))
        .route("/web-activity", get(list_web))
        .route("/web-activity/stats", get(web_stats))
}

#[derive(Deserialize)]
struct FilterQuery {
    agent_id: Option<String>,
    username: Option<String>,
    #[serde(default = "def_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn def_limit() -> i64 {
    100
}

#[derive(Serialize, sqlx::FromRow)]
struct UsbRow {
    id: i64,
    agent_id: String,
    username: String,
    device_id: String,
    device_name: String,
    action: String,
    drive_letter: Option<String>,
    file_path: Option<String>,
    size_gb: Option<f64>,
    created_at: DateTime<Utc>,
}

async fn list_usb(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<UsbRow>>> {
    let rows: Vec<UsbRow> = sqlx::query_as(
        "SELECT id, agent_id, username, device_id, device_name, action, drive_letter, file_path, size_gb, created_at \
         FROM usb_events \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct AppRow {
    id: i64,
    agent_id: String,
    username: String,
    app_name: String,
    window_title: String,
    exe_path: Option<String>,
    duration_sec: i32,
    active_sec: i32,
    idle_sec: i32,
    productivity: String,
    created_at: DateTime<Utc>,
}

async fn list_app(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<AppRow>>> {
    let rows: Vec<AppRow> = sqlx::query_as(
        "SELECT id, agent_id, username, app_name, window_title, exe_path, duration_sec, active_sec, idle_sec, productivity, created_at \
         FROM app_activity \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct MessengerRow {
    id: i64,
    agent_id: String,
    username: String,
    messenger: String,
    direction: String,
    contact: Option<String>,
    content: Option<String>,
    has_file: bool,
    file_name: Option<String>,
    flagged: bool,
    matched_rules: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_messenger(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<MessengerRow>>> {
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "view_messenger_events",
        "messenger",
        json!({ "agent_id": &q.agent_id, "username": &q.username }),
    )
    .await;

    let rows: Vec<MessengerRow> = sqlx::query_as(
        "SELECT id, agent_id, username, messenger, direction, contact, content, has_file, file_name, flagged, matched_rules, created_at \
         FROM messenger_events \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct ClipboardRow {
    id: i64,
    agent_id: String,
    username: String,
    content: String,
    source_app: String,
    flagged: bool,
    matched_rules: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_clipboard(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<ClipboardRow>>> {
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "view_clipboard_events",
        "clipboard",
        json!({ "agent_id": &q.agent_id, "username": &q.username }),
    )
    .await;

    let rows: Vec<ClipboardRow> = sqlx::query_as(
        "SELECT id, agent_id, username, content, source_app, flagged, matched_rules, created_at \
         FROM clipboard_events \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct NetworkRow {
    id: i64,
    agent_id: String,
    username: String,
    local_ip: String,
    local_port: i32,
    remote_ip: String,
    remote_port: i32,
    protocol: String,
    status: String,
    process_name: String,
    exe_path: Option<String>,
    bytes_sent: i64,
    bytes_recv: i64,
    created_at: DateTime<Utc>,
}

async fn list_network(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<NetworkRow>>> {
    let rows: Vec<NetworkRow> = sqlx::query_as(
        "SELECT id, agent_id, username, local_ip, local_port, remote_ip, remote_port, \
                protocol, status, process_name, exe_path, bytes_sent, bytes_recv, created_at \
         FROM network_connections \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct PrintRow {
    id: i64,
    agent_id: String,
    username: String,
    printer_name: String,
    document_name: String,
    pages: i32,
    size_bytes: i64,
    status: String,
    created_at: DateTime<Utc>,
}

async fn list_print(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<PrintRow>>> {
    let rows: Vec<PrintRow> = sqlx::query_as(
        "SELECT id, agent_id, username, printer_name, document_name, pages, size_bytes, status, created_at \
         FROM print_jobs \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct SessionRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    ip_address: String,
    event_type: String,
    logon_type: String,
    meta: Value,
    created_at: DateTime<Utc>,
}

async fn list_sessions(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<SessionRow>>> {
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, ip_address, event_type, logon_type, meta, created_at \
         FROM user_sessions \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct WebRow {
    id: i64,
    agent_id: String,
    username: String,
    url: String,
    domain: String,
    category: String,
    blocked: bool,
    browser: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_web(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQuery>,
) -> AppResult<Json<Vec<WebRow>>> {
    let rows: Vec<WebRow> = sqlx::query_as(
        "SELECT id, agent_id, username, url, domain, category, blocked, browser, created_at \
         FROM web_activity \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct WebStatRow {
    domain: String,
    category: String,
    visits: i64,
    blocked_count: i64,
}

async fn web_stats(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<WebStatRow>>> {
    let rows: Vec<WebStatRow> = sqlx::query_as(
        "SELECT domain, MAX(category) AS category, COUNT(*) AS visits, \
                COUNT(*) FILTER (WHERE blocked) AS blocked_count \
         FROM web_activity GROUP BY domain ORDER BY visits DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}
