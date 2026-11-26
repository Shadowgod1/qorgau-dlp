use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::mask_text;
use qorgau_common::{Role, WsAgentCommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser, OfficerUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // USB Device Manager (Менеджер ВНИ)
        .route("/usb-devices", get(list_usb_devices).post(upsert_usb_device))
        .route(
            "/usb-devices/{id}",
            patch(update_usb_device).delete(delete_usb_device),
        )
        // Blocked Applications
        .route("/rules/apps", get(list_blocked_apps).post(add_blocked_app))
        .route("/rules/apps/{id}", delete(remove_blocked_app))
        // File Operations (FileModule)
        .route("/events/files", get(list_file_events))
        // Data Discovery (Crawler)
        .route("/discovery", get(list_discovery))
        .route("/discovery/{id}", patch(update_discovery_status))
        .route("/discovery/trigger", post(trigger_discovery))
        // Hardware & Software Inventory
        .route("/inventory", get(list_inventory))
        .route("/inventory/{agent_id}", get(get_agent_inventory))
        .route("/inventory/trigger", post(trigger_inventory))
        // Agent binary download from web console (iBatyr parity)
        .route("/agent-download/windows", get(download_windows_agent))
}

// ── USB Device Manager ───────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct UsbDeviceRow {
    id: i64,
    serial_number: String,
    vendor_id: String,
    product_id: String,
    label: String,
    status: String,
    owner: Option<String>,
    comment: Option<String>,
    created_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn list_usb_devices(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<UsbDeviceRow>>> {
    let rows: Vec<UsbDeviceRow> = sqlx::query_as(
        "SELECT id, serial_number, vendor_id, product_id, label, status, owner, comment, created_by, created_at, updated_at \
         FROM usb_devices ORDER BY updated_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct UsbDeviceReq {
    serial_number: String,
    #[serde(default)]
    vendor_id: String,
    #[serde(default)]
    product_id: String,
    #[serde(default)]
    label: String,
    #[serde(default = "def_allowed")]
    status: String,
    owner: Option<String>,
    comment: Option<String>,
}
fn def_allowed() -> String {
    "allowed".to_string()
}

async fn upsert_usb_device(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UsbDeviceReq>,
) -> AppResult<Json<UsbDeviceRow>> {
    let serial = body.serial_number.trim();
    if serial.is_empty() {
        return Err(AppError::BadRequest("serial_number is required".to_string()));
    }
    let st = body.status.to_ascii_lowercase();
    if !matches!(st.as_str(), "allowed" | "blocked" | "readonly") {
        return Err(AppError::BadRequest("Invalid USB device status".to_string()));
    }

    let row: UsbDeviceRow = sqlx::query_as(
        "INSERT INTO usb_devices (serial_number, vendor_id, product_id, label, status, owner, comment, created_by) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT (serial_number) DO UPDATE SET \
            vendor_id = EXCLUDED.vendor_id, \
            product_id = EXCLUDED.product_id, \
            label = EXCLUDED.label, \
            status = EXCLUDED.status, \
            owner = EXCLUDED.owner, \
            comment = EXCLUDED.comment, \
            updated_at = NOW() \
         RETURNING id, serial_number, vendor_id, product_id, label, status, owner, comment, created_by, created_at, updated_at",
    )
    .bind(serial)
    .bind(body.vendor_id.trim())
    .bind(body.product_id.trim())
    .bind(body.label.trim())
    .bind(&st)
    .bind(&body.owner)
    .bind(&body.comment)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await?;

    state
        .ws
        .broadcast_to_all_agents(WsAgentCommand::ReloadPreventionRules);
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "usb_device_saved",
        &format!("usb:{}", row.serial_number),
        json!({ "status": &row.status, "label": &row.label }),
    )
    .await;

    Ok(Json(row))
}

#[derive(Deserialize)]
struct UsbDevicePatch {
    status: Option<String>,
    label: Option<String>,
    owner: Option<String>,
    comment: Option<String>,
}

async fn update_usb_device(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<UsbDevicePatch>,
) -> AppResult<Json<UsbDeviceRow>> {
    if let Some(ref s) = body.status {
        if !matches!(s.to_ascii_lowercase().as_str(), "allowed" | "blocked" | "readonly") {
            return Err(AppError::BadRequest("Invalid USB status".to_string()));
        }
    }
    let row: Option<UsbDeviceRow> = sqlx::query_as(
        "UPDATE usb_devices SET \
            status = COALESCE($1, status), \
            label = COALESCE($2, label), \
            owner = COALESCE($3, owner), \
            comment = COALESCE($4, comment), \
            updated_at = NOW() \
         WHERE id = $5 \
         RETURNING id, serial_number, vendor_id, product_id, label, status, owner, comment, created_by, created_at, updated_at",
    )
    .bind(body.status.map(|s| s.to_ascii_lowercase()))
    .bind(body.label)
    .bind(body.owner)
    .bind(body.comment)
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let r = row.ok_or_else(|| AppError::NotFound("USB device not found".to_string()))?;
    state
        .ws
        .broadcast_to_all_agents(WsAgentCommand::ReloadPreventionRules);
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "usb_device_updated",
        &format!("usb:{id}"),
        json!({ "serial": &r.serial_number, "status": &r.status }),
    )
    .await;
    Ok(Json(r))
}

async fn delete_usb_device(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM usb_devices WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("USB device not found".to_string()));
    }
    state
        .ws
        .broadcast_to_all_agents(WsAgentCommand::ReloadPreventionRules);
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "usb_device_deleted",
        &format!("usb:{id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

// ── Blocked Applications ─────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct BlockedAppRow {
    id: i64,
    pattern: String,
    action: String,
    reason: Option<String>,
    enabled: bool,
    created_by: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_blocked_apps(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<BlockedAppRow>>> {
    let rows: Vec<BlockedAppRow> = sqlx::query_as(
        "SELECT id, pattern, action, reason, enabled, created_by, created_at FROM blocked_apps ORDER BY id DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct BlockedAppReq {
    pattern: String,
    #[serde(default = "def_block")]
    action: String,
    reason: Option<String>,
}
fn def_block() -> String {
    "block".to_string()
}

async fn add_blocked_app(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<BlockedAppReq>,
) -> AppResult<Json<BlockedAppRow>> {
    let pat = body.pattern.trim().to_ascii_lowercase();
    if pat.is_empty() {
        return Err(AppError::BadRequest("App pattern is required".to_string()));
    }
    let act = body.action.to_ascii_lowercase();
    if !matches!(act.as_str(), "block" | "alert") {
        return Err(AppError::BadRequest("Invalid action".to_string()));
    }

    let row: BlockedAppRow = sqlx::query_as(
        "INSERT INTO blocked_apps (pattern, action, reason, enabled, created_by) \
         VALUES ($1,$2,$3,TRUE,$4) \
         RETURNING id, pattern, action, reason, enabled, created_by, created_at",
    )
    .bind(&pat)
    .bind(&act)
    .bind(&body.reason)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "App pattern already exists"))?;

    state
        .ws
        .broadcast_to_all_agents(WsAgentCommand::ReloadPreventionRules);
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "app_blocked",
        &format!("app:{pat}"),
        json!({ "action": &act }),
    )
    .await;
    Ok(Json(row))
}

async fn remove_blocked_app(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let res = sqlx::query("DELETE FROM blocked_apps WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Blocked app not found".to_string()));
    }
    state
        .ws
        .broadcast_to_all_agents(WsAgentCommand::ReloadPreventionRules);
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "app_unblocked",
        &format!("app:{id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

// ── File Events ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FilterQ {
    agent_id: Option<String>,
    username: Option<String>,
    status: Option<String>,
    #[serde(default = "def_limit")]
    limit: i64,
}
fn def_limit() -> i64 {
    200
}

#[derive(Serialize, sqlx::FromRow)]
struct FileEventRow {
    id: i64,
    agent_id: String,
    username: String,
    action: String,
    file_path: String,
    new_path: Option<String>,
    size_bytes: i64,
    sha256: String,
    flagged: bool,
    matched_rules: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_file_events(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQ>,
) -> AppResult<Json<Vec<FileEventRow>>> {
    let rows: Vec<FileEventRow> = sqlx::query_as(
        "SELECT id, agent_id, username, action, file_path, new_path, size_bytes, sha256, flagged, matched_rules, created_at \
         FROM file_events \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 500))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ── Data Discovery / Crawler ─────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct DiscoveryRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    file_path: String,
    file_size: i64,
    sha256: String,
    matched_rules: Value,
    severity: String,
    snippet: String,
    status: String,
    scanned_at: DateTime<Utc>,
}

async fn list_discovery(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<FilterQ>,
) -> AppResult<Json<Vec<DiscoveryRow>>> {
    let mut rows: Vec<DiscoveryRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, file_path, file_size, sha256, matched_rules, severity, snippet, status, scanned_at \
         FROM discovery_findings \
         WHERE ($1::text IS NULL OR agent_id = $1) \
           AND ($2::text IS NULL OR username = $2) \
           AND ($3::text IS NULL OR status = $3) \
         ORDER BY scanned_at DESC LIMIT $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(&q.status)
    .bind(q.limit.clamp(1, 500))
    .fetch_all(&state.db)
    .await?;

    if user.role == Role::Viewer {
        for r in &mut rows {
            r.snippet = mask_text(&r.snippet);
        }
    }
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct DiscoveryPatch {
    status: String,
}

async fn update_discovery_status(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<DiscoveryPatch>,
) -> AppResult<Json<DiscoveryRow>> {
    let st = body.status.to_ascii_lowercase();
    if !matches!(st.as_str(), "open" | "resolved" | "ignored") {
        return Err(AppError::BadRequest("Invalid discovery status".to_string()));
    }
    let row: Option<DiscoveryRow> = sqlx::query_as(
        "UPDATE discovery_findings SET status = $1 WHERE id = $2 \
         RETURNING id, agent_id, hostname, username, file_path, file_size, sha256, matched_rules, severity, snippet, status, scanned_at",
    )
    .bind(&st)
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let r = row.ok_or_else(|| AppError::NotFound("Finding not found".to_string()))?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "discovery_status_updated",
        &format!("discovery:{id}"),
        json!({ "status": &st }),
    )
    .await;
    Ok(Json(r))
}

#[derive(Deserialize)]
struct AgentTriggerReq {
    agent_id: Option<String>,
}

async fn trigger_discovery(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentTriggerReq>,
) -> AppResult<Json<Value>> {
    if let Some(aid) = body.agent_id {
        let delivered = state.ws.send_to_agent(&aid, WsAgentCommand::RunDiscoveryScan);
        write_audit(
            &state.db,
            &user.username,
            user.role.as_str(),
            "discovery_triggered",
            &format!("agent:{aid}"),
            json!({ "delivered": delivered }),
        )
        .await;
        Ok(Json(json!({ "status": "ok", "delivered": delivered })))
    } else {
        state
            .ws
            .broadcast_to_all_agents(WsAgentCommand::RunDiscoveryScan);
        Ok(Json(json!({ "status": "ok", "broadcast": true })))
    }
}

// ── Hardware & Software Inventory ────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct InventoryRow {
    agent_id: String,
    hostname: String,
    username: String,
    cpu_model: String,
    cpu_cores: i32,
    ram_total_mb: i64,
    disk_total_gb: f64,
    disk_free_gb: f64,
    os_build: String,
    updated_at: DateTime<Utc>,
    software_count: i64,
}

async fn list_inventory(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<InventoryRow>>> {
    let rows: Vec<InventoryRow> = sqlx::query_as(
        "SELECT i.agent_id, i.hostname, i.username, i.cpu_model, i.cpu_cores, \
                i.ram_total_mb, i.disk_total_gb, i.disk_free_gb, i.os_build, i.updated_at, \
                COALESCE(s.c, 0) AS software_count \
         FROM agent_inventory i \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM installed_software WHERE status = 'installed' GROUP BY agent_id) s \
           ON s.agent_id = i.agent_id \
         ORDER BY i.updated_at DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct SoftwareRow {
    id: i64,
    name: String,
    version: String,
    publisher: String,
    install_date: String,
    status: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

async fn get_agent_inventory(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> AppResult<Json<Value>> {
    let hw: Option<InventoryRow> = sqlx::query_as(
        "SELECT i.agent_id, i.hostname, i.username, i.cpu_model, i.cpu_cores, \
                i.ram_total_mb, i.disk_total_gb, i.disk_free_gb, i.os_build, i.updated_at, \
                (SELECT COUNT(*) FROM installed_software WHERE agent_id = i.agent_id AND status = 'installed') AS software_count \
         FROM agent_inventory i WHERE i.agent_id = $1",
    )
    .bind(&agent_id)
    .fetch_optional(&state.db)
    .await?;

    let software: Vec<SoftwareRow> = sqlx::query_as(
        "SELECT id, name, version, publisher, install_date, status, first_seen, last_seen \
         FROM installed_software WHERE agent_id = $1 ORDER BY status ASC, name ASC",
    )
    .bind(&agent_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "hardware": hw,
        "software": software,
    })))
}

async fn trigger_inventory(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentTriggerReq>,
) -> AppResult<Json<Value>> {
    if let Some(aid) = body.agent_id {
        let delivered = state.ws.send_to_agent(&aid, WsAgentCommand::RunInventoryScan);
        Ok(Json(json!({ "status": "ok", "delivered": delivered })))
    } else {
        state
            .ws
            .broadcast_to_all_agents(WsAgentCommand::RunInventoryScan);
        Ok(Json(json!({ "status": "ok", "broadcast": true })))
    }
}

// ── Agent Installer Download ─────────────────────────────────────────────────

async fn download_windows_agent(AuthUser(_): AuthUser) -> AppResult<Response> {
    let candidates = [
        std::env::var("AGENT_BIN_PATH").ok().map(PathBuf::from),
        Some(PathBuf::from("/app/downloads/qorgau-agent.exe")),
        Some(PathBuf::from("./target/release/qorgau-agent.exe")),
    ];
    let found = candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists());

    let Some(path) = found else {
        return Err(AppError::NotFound(
            "qorgau-agent.exe is not packaged on this server yet".to_string(),
        ));
    };

    let file = tokio::fs::File::open(&path).await?;
    let stream = ReaderStream::new(file);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.microsoft.portable-executable")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"qorgau-agent.exe\"",
        )
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::Internal(e.to_string()))
}
