use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agents", get(list_agents))
        .route("/agents/{agent_id}", get(get_agent).delete(delete_agent))
        .route("/agents/{agent_id}/policies", get(get_agent_policies))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: i64,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub ip_address: String,
    pub os_version: String,
    pub agent_version: String,
    pub is_online: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub registered_at: Option<DateTime<Utc>>,
    pub config_id: Option<i64>,
}

async fn list_agents(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<AgentRow>>> {
    let rows: Vec<AgentRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, ip_address, os_version, agent_version, \
                (last_seen >= NOW() - INTERVAL '3 minutes') AS is_online, \
                last_seen, registered_at, config_id \
         FROM agents ORDER BY last_seen DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

async fn get_agent(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> AppResult<Json<AgentRow>> {
    let row: Option<AgentRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, ip_address, os_version, agent_version, \
                (last_seen >= NOW() - INTERVAL '3 minutes') AS is_online, \
                last_seen, registered_at, config_id \
         FROM agents WHERE agent_id = $1",
    )
    .bind(&agent_id)
    .fetch_optional(&state.db)
    .await?;

    row.map(Json)
        .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))
}

async fn delete_agent(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let res = sqlx::query("DELETE FROM agents WHERE agent_id = $1")
        .bind(&agent_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Agent not found".to_string()));
    }
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "agent_deleted",
        &format!("agent:{agent_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted", "agent_id": agent_id })))
}

#[derive(Serialize, sqlx::FromRow)]
struct PolicyBrief {
    id: i64,
    name: String,
    channel: String,
    action: String,
    severity: String,
    pattern: String,
}

async fn get_agent_policies(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(_agent_id): Path<String>,
) -> AppResult<Json<Vec<PolicyBrief>>> {
    let rows: Vec<PolicyBrief> = sqlx::query_as(
        "SELECT id, name, channel, action, severity, pattern FROM policies WHERE enabled = TRUE ORDER BY id",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}
