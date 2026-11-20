use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::{verify_chain, AuditVerifyResult};
use crate::auth::OfficerUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/audit", get(list_audit))
        .route("/audit/verify", get(verify_audit))
}

#[derive(Deserialize)]
struct AuditQuery {
    actor: Option<String>,
    action: Option<String>,
    #[serde(default = "def_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn def_limit() -> i64 {
    100
}

#[derive(Serialize, sqlx::FromRow)]
struct AuditRow {
    id: i64,
    ts: DateTime<Utc>,
    actor: String,
    actor_role: String,
    action: String,
    resource: String,
    details: Value,
    prev_hash: String,
    entry_hash: String,
}

#[derive(Serialize)]
struct AuditListResp {
    total: i64,
    items: Vec<AuditRow>,
}

async fn list_audit(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<AuditListResp>> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log \
         WHERE ($1::text IS NULL OR actor = $1) AND ($2::text IS NULL OR action = $2)",
    )
    .bind(&q.actor)
    .bind(&q.action)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<AuditRow> = sqlx::query_as(
        "SELECT id, ts, actor, actor_role, action, resource, details, prev_hash, entry_hash \
         FROM audit_log \
         WHERE ($1::text IS NULL OR actor = $1) AND ($2::text IS NULL OR action = $2) \
         ORDER BY id DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.actor)
    .bind(&q.action)
    .bind(q.limit.clamp(1, 500))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(AuditListResp {
        total: total.0,
        items,
    }))
}

async fn verify_audit(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<AuditVerifyResult>> {
    Ok(Json(verify_chain(&state.db).await?))
}
