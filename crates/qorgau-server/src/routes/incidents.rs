use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::{mask_json_value, mask_text};
use qorgau_common::{IncidentStatus, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{AuthUser, OfficerUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/incidents/stats", get(incident_stats))
        .route("/incidents", get(list_incidents))
        .route(
            "/incidents/{incident_id}",
            get(get_incident).patch(update_incident_status),
        )
}

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
    let search_pattern = q.search.filter(|s| !s.trim().is_empty()).map(|s| format!("%{}%", s.trim()));

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM incidents \
         WHERE ($1::text IS NULL OR agent_id = $1) \
           AND ($2::text IS NULL OR severity = $2) \
           AND ($3::text IS NULL OR status = $3) \
           AND ($4::text IS NULL OR username = $4) \
           AND ($5::text IS NULL OR channel = $5) \
           AND ($6::text IS NULL OR snippet ILIKE $6 OR username ILIKE $6 OR hostname ILIKE $6)",
    )
    .bind(&q.agent_id)
    .bind(&sev)
    .bind(&status)
    .bind(&q.username)
    .bind(&q.channel)
    .bind(&search_pattern)
    .fetch_one(&state.db)
    .await?;

    let rows: Vec<IncidentRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, channel, severity, status, \
                matched_rules, snippet, meta, created_at, updated_at \
         FROM incidents \
         WHERE ($1::text IS NULL OR agent_id = $1) \
           AND ($2::text IS NULL OR severity = $2) \
           AND ($3::text IS NULL OR status = $3) \
           AND ($4::text IS NULL OR username = $4) \
           AND ($5::text IS NULL OR channel = $5) \
           AND ($6::text IS NULL OR snippet ILIKE $6 OR username ILIKE $6 OR hostname ILIKE $6) \
         ORDER BY created_at DESC LIMIT $7 OFFSET $8",
    )
    .bind(&q.agent_id)
    .bind(&sev)
    .bind(&status)
    .bind(&q.username)
    .bind(&q.channel)
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

async fn get_incident(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
) -> AppResult<Json<IncidentRow>> {
    let row: Option<IncidentRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, channel, severity, status, \
                matched_rules, snippet, meta, created_at, updated_at \
         FROM incidents WHERE id = $1",
    )
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
    status: String,
}

async fn update_incident_status(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(incident_id): Path<i64>,
    Json(body): Json<PatchIncidentReq>,
) -> AppResult<Json<IncidentRow>> {
    let st = IncidentStatus::parse_strict(&body.status)
        .ok_or_else(|| AppError::BadRequest("Invalid incident status".to_string()))?;

    let row: Option<IncidentRow> = sqlx::query_as(
        "UPDATE incidents SET status = $1, updated_at = NOW() WHERE id = $2 \
         RETURNING id, agent_id, hostname, username, channel, severity, status, \
                   matched_rules, snippet, meta, created_at, updated_at",
    )
    .bind(st.as_str())
    .bind(incident_id)
    .fetch_optional(&state.db)
    .await?;

    let inc = row.ok_or_else(|| AppError::NotFound("Incident not found".to_string()))?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "incident_status_updated",
        &format!("incident:{incident_id}"),
        json!({ "status": st.as_str() }),
    )
    .await;

    Ok(Json(inc))
}
