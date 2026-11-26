use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::{mask_json_value, mask_text};
use qorgau_common::{IncidentStatus, Role, Severity, WsDashboardEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{AuthUser, OfficerUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/incidents/stats", get(incident_stats))
        .route("/incidents", get(list_incidents).post(create_manual_incident))
        .route(
            "/incidents/{incident_id}",
            get(get_incident).patch(update_incident),
        )
        .route(
            "/incidents/{incident_id}/comments",
            get(list_comments).post(add_comment),
        )
        .route("/incidents/{incident_id}/evidence", get(get_evidence))
}

pub const INCIDENT_COLS: &str = "\
    id, agent_id, hostname, username, channel, severity, status, \
    matched_rules, snippet, meta, created_at, updated_at, \
    legal_hold, assigned_to, group_tag, resolution_note";

#[derive(Serialize, sqlx::FromRow)]
pub struct IncidentRow {
    pub id: i64,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub channel: String,
    pub severity: String,
    pub status: String,
    pub matched_rules: Value,
    pub snippet: String,
    pub meta: Value,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub legal_hold: bool,
    pub assigned_to: Option<String>,
    pub group_tag: Option<String>,
    pub resolution_note: Option<String>,
}

impl IncidentRow {
    pub fn apply_rbac(mut self, role: Role) -> Self {
        if role == Role::Viewer {
            self.snippet = mask_text(&self.snippet);
            self.meta = mask_json_value(&self.meta);
        }
        self
    }
}

#[derive(Serialize)]
struct StatsResp {
    by_severity: HashMap<String, i64>,
    by_status: HashMap<String, i64>,
    by_channel: HashMap<String, i64>,
}

async fn incident_stats(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<StatsResp>> {
    let sev_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT severity, COUNT(*) FROM incidents GROUP BY severity")
            .fetch_all(&state.db)
            .await?;
    let st_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, COUNT(*) FROM incidents GROUP BY status")
            .fetch_all(&state.db)
            .await?;
    let ch_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT channel, COUNT(*) FROM incidents GROUP BY channel")
            .fetch_all(&state.db)
            .await?;

    Ok(Json(StatsResp {
        by_severity: sev_rows.into_iter().collect(),
        by_status: st_rows.into_iter().collect(),
        by_channel: ch_rows.into_iter().collect(),
    }))
}

#[derive(Deserialize)]
struct ListQuery {
    agent_id: Option<String>,
    severity: Option<String>,
    status: Option<String>,
    username: Option<String>,
    channel: Option<String>,
    assigned_to: Option<String>,
    legal_hold: Option<bool>,
    search: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Serialize)]
struct ListResp {
    total: i64,
    items: Vec<IncidentRow>,
}

async fn list_incidents(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<ListResp>> {
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);
    let sev = q.severity.map(|s| s.to_ascii_lowercase());
    let status = q.status.map(|s| s.to_ascii_lowercase());
    let search_pattern = q
        .search
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("%{}%", s.trim()));

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM incidents \
         WHERE ($1::text IS NULL OR agent_id = $1) \
           AND ($2::text IS NULL OR severity = $2) \
           AND ($3::text IS NULL OR status = $3) \
           AND ($4::text IS NULL OR username = $4) \
           AND ($5::text IS NULL OR channel = $5) \
           AND ($6::text IS NULL OR assigned_to = $6) \
           AND ($7::bool IS NULL OR legal_hold = $7) \
           AND ($8::text IS NULL OR snippet ILIKE $8 OR username ILIKE $8 OR hostname ILIKE $8 OR COALESCE(group_tag,'') ILIKE $8)",
    )
    .bind(&q.agent_id)
    .bind(&sev)
    .bind(&status)
    .bind(&q.username)
    .bind(&q.channel)
    .bind(&q.assigned_to)
    .bind(q.legal_hold)
    .bind(&search_pattern)
    .fetch_one(&state.db)
    .await?;

    let sql = format!(
        "SELECT {INCIDENT_COLS} FROM incidents \
         WHERE ($1::text IS NULL OR agent_id = $1) \
           AND ($2::text IS NULL OR severity = $2) \
           AND ($3::text IS NULL OR status = $3) \
           AND ($4::text IS NULL OR username = $4) \
           AND ($5::text IS NULL OR channel = $5) \
           AND ($6::text IS NULL OR assigned_to = $6) \
           AND ($7::bool IS NULL OR legal_hold = $7) \
           AND ($8::text IS NULL OR snippet ILIKE $8 OR username ILIKE $8 OR hostname ILIKE $8 OR COALESCE(group_tag,'') ILIKE $8) \
         ORDER BY created_at DESC LIMIT $9 OFFSET $10"
    );

    let rows: Vec<IncidentRow> = sqlx::query_as(&sql)
        .bind(&q.agent_id)
        .bind(&sev)
        .bind(&status)
        .bind(&q.username)
        .bind(&q.channel)
        .bind(&q.assigned_to)
        .bind(q.legal_hold)
        .bind(&search_pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

    let items = rows.into_iter().map(|r| r.apply_rbac(user.role)).collect();
    Ok(Json(ListResp {
        total: total.0,
        items,
    }))
}

#[derive(Deserialize)]
struct ManualIncidentReq {
    agent_id: Option<String>,
    hostname: Option<String>,
    username: String,
    channel: String,
    severity: String,
    snippet: String,
    group_tag: Option<String>,
    assigned_to: Option<String>,
    #[serde(default)]
    legal_hold: bool,
}

async fn create_manual_incident(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ManualIncidentReq>,
) -> AppResult<Json<IncidentRow>> {
    let uname = body.username.trim();
    if uname.is_empty() || body.snippet.trim().is_empty() {
        return Err(AppError::BadRequest(
            "username and snippet are required".to_string(),
        ));
    }
    let sev = Severity::from_str_loose(&body.severity);
    let agent_id = body.agent_id.unwrap_or_else(|| "manual".to_string());
    let hostname = body.hostname.unwrap_or_else(|| "manual".to_string());

    let sql = format!(
        "INSERT INTO incidents (agent_id, hostname, username, channel, severity, status, matched_rules, snippet, meta, legal_hold, assigned_to, group_tag) \
         VALUES ($1,$2,$3,$4,$5,'open',$6,$7,$8,$9,$10,$11) RETURNING {INCIDENT_COLS}"
    );

    let row: IncidentRow = sqlx::query_as(&sql)
        .bind(&agent_id)
        .bind(&hostname)
        .bind(uname)
        .bind(body.channel.trim().to_ascii_lowercase())
        .bind(sev.as_str())
        .bind(json!(["Ручной инцидент"]))
        .bind(body.snippet.trim())
        .bind(json!({ "created_manually_by": &user.username }))
        .bind(body.legal_hold)
        .bind(&body.assigned_to)
        .bind(&body.group_tag)
        .fetch_one(&state.db)
        .await?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "incident_created_manual",
        &format!("incident:{}", row.id),
        json!({ "username": uname, "severity": sev.as_str(), "legal_hold": row.legal_hold }),
    )
    .await;

    state.ws.broadcast_dashboard(WsDashboardEvent::Incident {
        id: row.id,
        agent_id: row.agent_id.clone(),
        hostname: row.hostname.clone(),
        username: row.username.clone(),
        channel: row.channel.clone(),
        severity: sev,
        snippet: row.snippet.chars().take(120).collect(),
    });

    Ok(Json(row))
}

async fn get_incident(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
) -> AppResult<Json<IncidentRow>> {
    let sql = format!("SELECT {INCIDENT_COLS} FROM incidents WHERE id = $1");
    let row: Option<IncidentRow> = sqlx::query_as(&sql)
        .bind(incident_id)
        .fetch_optional(&state.db)
        .await?;

    let inc = row.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "view_incident",
        &format!("incident:{incident_id}"),
        json!({ "channel": &inc.channel, "target_user": &inc.username }),
    )
    .await;

    Ok(Json(inc.apply_rbac(user.role)))
}

#[derive(Deserialize)]
struct PatchIncidentReq {
    status: Option<String>,
    legal_hold: Option<bool>,
    assigned_to: Option<String>,
    group_tag: Option<String>,
    resolution_note: Option<String>,
}

async fn update_incident(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
    Json(body): Json<PatchIncidentReq>,
) -> AppResult<Json<IncidentRow>> {
    let cur_sql = format!("SELECT {INCIDENT_COLS} FROM incidents WHERE id = $1");
    let cur: IncidentRow = sqlx::query_as(&cur_sql)
        .bind(incident_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    let new_status = match body.status {
        Some(ref s) => IncidentStatus::parse_strict(s)
            .ok_or_else(|| AppError::BadRequest("Invalid incident status".to_string()))?
            .as_str()
            .to_string(),
        None => cur.status,
    };
    let new_hold = body.legal_hold.unwrap_or(cur.legal_hold);
    let new_assigned = match body.assigned_to {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s.trim().to_string()),
        None => cur.assigned_to,
    };
    let new_group = match body.group_tag {
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(s.trim().to_string()),
        None => cur.group_tag,
    };
    let new_note = body.resolution_note.or(cur.resolution_note);

    let upd_sql = format!(
        "UPDATE incidents SET status=$1, legal_hold=$2, assigned_to=$3, group_tag=$4, resolution_note=$5, updated_at=NOW() \
         WHERE id=$6 RETURNING {INCIDENT_COLS}"
    );
    let inc: IncidentRow = sqlx::query_as(&upd_sql)
        .bind(&new_status)
        .bind(new_hold)
        .bind(&new_assigned)
        .bind(&new_group)
        .bind(&new_note)
        .bind(incident_id)
        .fetch_one(&state.db)
        .await?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "incident_updated",
        &format!("incident:{incident_id}"),
        json!({
            "status": &new_status,
            "legal_hold": new_hold,
            "assigned_to": &new_assigned,
            "group_tag": &new_group
        }),
    )
    .await;

    Ok(Json(inc))
}

#[derive(Serialize, sqlx::FromRow)]
struct CommentRow {
    id: i64,
    incident_id: i64,
    author: String,
    author_role: String,
    body: String,
    created_at: DateTime<Utc>,
}

async fn list_comments(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
) -> AppResult<Json<Vec<CommentRow>>> {
    let rows: Vec<CommentRow> = sqlx::query_as(
        "SELECT id, incident_id, author, author_role, body, created_at \
         FROM incident_comments WHERE incident_id = $1 ORDER BY created_at ASC",
    )
    .bind(incident_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CommentCreateReq {
    body: String,
}

async fn add_comment(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
    Json(req): Json<CommentCreateReq>,
) -> AppResult<Json<CommentRow>> {
    let text = req.body.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("Comment body cannot be empty".to_string()));
    }
    let row: CommentRow = sqlx::query_as(
        "INSERT INTO incident_comments (incident_id, author, author_role, body) \
         VALUES ($1, $2, $3, $4) RETURNING id, incident_id, author, author_role, body, created_at",
    )
    .bind(incident_id)
    .bind(&user.username)
    .bind(user.role.as_str())
    .bind(text)
    .fetch_one(&state.db)
    .await?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "incident_comment_added",
        &format!("incident:{incident_id}"),
        json!({ "comment_id": row.id }),
    )
    .await;

    Ok(Json(row))
}

#[derive(Serialize, sqlx::FromRow)]
struct EvidenceShadow {
    id: i64,
    file_name: String,
    original_path: String,
    sha256: String,
    size_bytes: i64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct EvidenceScreenshot {
    id: i64,
    hostname: String,
    file_size: i64,
    created_at: DateTime<Utc>,
}

/// Returns linked shadow copies and nearby screenshots (±5 min) as forensic evidence for an incident.
async fn get_evidence(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
) -> AppResult<Json<Value>> {
    let row: Option<(String, DateTime<Utc>)> =
        sqlx::query_as("SELECT agent_id, created_at FROM incidents WHERE id = $1")
            .bind(incident_id)
            .fetch_optional(&state.db)
            .await?;

    let (agent_id, ts) =
        row.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    let shadows: Vec<EvidenceShadow> = sqlx::query_as(
        "SELECT id, file_name, original_path, sha256, size_bytes, created_at \
         FROM shadow_copies WHERE incident_id = $1 ORDER BY created_at DESC",
    )
    .bind(incident_id)
    .fetch_all(&state.db)
    .await?;

    let screenshots: Vec<EvidenceScreenshot> = sqlx::query_as(
        "SELECT id, hostname, file_size, created_at FROM screenshots \
         WHERE agent_id = $1 AND created_at BETWEEN $2 - INTERVAL '5 minutes' AND $2 + INTERVAL '5 minutes' \
         ORDER BY created_at DESC LIMIT 6",
    )
    .bind(&agent_id)
    .bind(ts)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "incident_id": incident_id,
        "shadow_copies": shadows,
        "screenshots": screenshots,
    })))
}
