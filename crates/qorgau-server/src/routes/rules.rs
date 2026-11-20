use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::write_audit;
use crate::auth::AdminUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{rule_id}", put(update_rule).delete(delete_rule))
        .route("/rules/domains", get(list_domains).post(add_domain))
        .route("/rules/domains/{domain_id}", delete(remove_domain))
}

#[derive(Serialize, sqlx::FromRow)]
struct PolicyRow {
    id: i64,
    name: String,
    channel: String,
    action: String,
    severity: String,
    pattern: String,
    enabled: bool,
    created_by: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct PolicyReq {
    name: String,
    channel: String,
    #[serde(default = "default_alert")]
    action: String,
    #[serde(default = "default_medium")]
    severity: String,
    pattern: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_alert() -> String {
    "alert".to_string()
}
fn default_medium() -> String {
    "medium".to_string()
}
fn default_true() -> bool {
    true
}

fn validate_policy_req(body: &PolicyReq) -> AppResult<()> {
    if body.name.trim().is_empty() || body.pattern.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Policy name and pattern are required".to_string(),
        ));
    }
    let action = body.action.to_ascii_lowercase();
    if !matches!(action.as_str(), "block" | "alert" | "log") {
        return Err(AppError::BadRequest("Invalid action".to_string()));
    }
    let sev = body.severity.to_ascii_lowercase();
    if !matches!(sev.as_str(), "low" | "medium" | "high" | "critical") {
        return Err(AppError::BadRequest("Invalid severity".to_string()));
    }
    regex::RegexBuilder::new(&body.pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| AppError::BadRequest(format!("Invalid regex pattern: {e}")))?;
    Ok(())
}

async fn list_rules(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<PolicyRow>>> {
    let rows: Vec<PolicyRow> = sqlx::query_as(
        "SELECT id, name, channel, action, severity, pattern, enabled, created_by, created_at \
         FROM policies ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

async fn create_rule(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PolicyReq>,
) -> AppResult<Json<PolicyRow>> {
    validate_policy_req(&body)?;

    let row: PolicyRow = sqlx::query_as(
        "INSERT INTO policies (name, channel, action, severity, pattern, enabled, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, name, channel, action, severity, pattern, enabled, created_by, created_at",
    )
    .bind(body.name.trim())
    .bind(body.channel.trim().to_ascii_lowercase())
    .bind(body.action.to_ascii_lowercase())
    .bind(body.severity.to_ascii_lowercase())
    .bind(body.pattern.trim())
    .bind(body.enabled)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Policy with this name already exists"))?;

    state.policy.reload_policies(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "policy_created",
        &format!("policy:{}", row.id),
        json!({ "name": &row.name, "channel": &row.channel }),
    )
    .await;

    Ok(Json(row))
}

async fn update_rule(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(rule_id): Path<i64>,
    Json(body): Json<PolicyReq>,
) -> AppResult<Json<PolicyRow>> {
    validate_policy_req(&body)?;

    let row: Option<PolicyRow> = sqlx::query_as(
        "UPDATE policies SET name=$1, channel=$2, action=$3, severity=$4, pattern=$5, enabled=$6 \
         WHERE id=$7 \
         RETURNING id, name, channel, action, severity, pattern, enabled, created_by, created_at",
    )
    .bind(body.name.trim())
    .bind(body.channel.trim().to_ascii_lowercase())
    .bind(body.action.to_ascii_lowercase())
    .bind(body.severity.to_ascii_lowercase())
    .bind(body.pattern.trim())
    .bind(body.enabled)
    .bind(rule_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Policy with this name already exists"))?;

    let r = row.ok_or_else(|| AppError::NotFound("Policy not found".to_string()))?;

    state.policy.reload_policies(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "policy_updated",
        &format!("policy:{rule_id}"),
        json!({ "name": &r.name, "enabled": r.enabled }),
    )
    .await;

    Ok(Json(r))
}

async fn delete_rule(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(rule_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let res = sqlx::query("DELETE FROM policies WHERE id = $1")
        .bind(rule_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Policy not found".to_string()));
    }
    state.policy.reload_policies(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "policy_deleted",
        &format!("policy:{rule_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

#[derive(Serialize, sqlx::FromRow)]
struct DomainRow {
    id: i64,
    domain: String,
    category: String,
    reason: Option<String>,
    enabled: bool,
    created_by: Option<String>,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct DomainReq {
    domain: String,
    #[serde(default = "default_custom")]
    category: String,
    reason: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_custom() -> String {
    "custom".to_string()
}

async fn list_domains(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<DomainRow>>> {
    let rows: Vec<DomainRow> = sqlx::query_as(
        "SELECT id, domain, category, reason, enabled, created_by, created_at \
         FROM blocked_domains ORDER BY id DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

async fn add_domain(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<DomainReq>,
) -> AppResult<Json<DomainRow>> {
    let dom = body
        .domain
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("www.")
        .trim_end_matches('/')
        .to_string();
    if dom.is_empty() || dom.len() > 255 {
        return Err(AppError::BadRequest("Invalid domain".to_string()));
    }

    let row: DomainRow = sqlx::query_as(
        "INSERT INTO blocked_domains (domain, category, reason, enabled, created_by) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, domain, category, reason, enabled, created_by, created_at",
    )
    .bind(&dom)
    .bind(body.category.trim().to_ascii_lowercase())
    .bind(&body.reason)
    .bind(body.enabled)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Domain is already in blocklist"))?;

    state.policy.reload_blocked_domains(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "domain_blocked",
        &format!("domain:{dom}"),
        json!({ "category": &row.category }),
    )
    .await;

    Ok(Json(row))
}

async fn remove_domain(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(domain_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let res = sqlx::query("DELETE FROM blocked_domains WHERE id = $1")
        .bind(domain_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Domain not found".to_string()));
    }
    state.policy.reload_blocked_domains(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "domain_unblocked",
        &format!("domain:{domain_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}
