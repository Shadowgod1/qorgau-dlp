use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{authenticate_token, extract_bearer_token, verify_agent_token, AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/directory/settings", get(get_ad_settings).put(update_ad_settings))
        .route("/directory/users", get(list_ad_users))
        .route("/directory/import", post(import_ad_users))
}

#[derive(Serialize, sqlx::FromRow)]
struct AdSettingsDto {
    enabled: bool,
    server_url: String,
    base_dn: String,
    bind_dn: String,
    corporate_domains: String,
    sync_interval_min: i32,
    last_sync_at: Option<DateTime<Utc>>,
    last_sync_status: String,
    synced_count: i32,
    updated_by: Option<String>,
    updated_at: DateTime<Utc>,
}

async fn get_ad_settings(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<AdSettingsDto>> {
    let row: AdSettingsDto = sqlx::query_as(
        "SELECT enabled, server_url, base_dn, bind_dn, corporate_domains, sync_interval_min, \
                last_sync_at, last_sync_status, synced_count, updated_by, updated_at \
         FROM ad_settings WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct UpdateAdReq {
    enabled: bool,
    #[serde(default)]
    server_url: String,
    #[serde(default)]
    base_dn: String,
    #[serde(default)]
    bind_dn: String,
    #[serde(default)]
    corporate_domains: String,
    #[serde(default = "def_interval")]
    sync_interval_min: i32,
}

fn def_interval() -> i32 {
    60
}

async fn update_ad_settings(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateAdReq>,
) -> AppResult<Json<AdSettingsDto>> {
    let interval = body.sync_interval_min.clamp(5, 1440);
    let row: AdSettingsDto = sqlx::query_as(
        "UPDATE ad_settings SET \
            enabled = $1, server_url = $2, base_dn = $3, bind_dn = $4, \
            corporate_domains = $5, sync_interval_min = $6, updated_by = $7, updated_at = NOW() \
         WHERE id = 1 \
         RETURNING enabled, server_url, base_dn, bind_dn, corporate_domains, sync_interval_min, \
                   last_sync_at, last_sync_status, synced_count, updated_by, updated_at",
    )
    .bind(body.enabled)
    .bind(body.server_url.trim())
    .bind(body.base_dn.trim())
    .bind(body.bind_dn.trim())
    .bind(body.corporate_domains.trim())
    .bind(interval)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "ad_settings_updated",
        "directory/settings",
        json!({ "enabled": row.enabled, "server_url": row.server_url, "base_dn": row.base_dn }),
    )
    .await;

    Ok(Json(row))
}

#[derive(Deserialize)]
struct ListAdQ {
    search: Option<String>,
    department: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct AdUserRow {
    username: String,
    full_name: String,
    email: String,
    department: String,
    title: String,
    manager: String,
    is_enabled: bool,
    distinguished_name: String,
    synced_at: DateTime<Utc>,
}

async fn list_ad_users(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListAdQ>,
) -> AppResult<Json<Vec<AdUserRow>>> {
    let search_pat = q.search.as_deref().map(|s| format!("%{}%", s.trim()));
    let rows: Vec<AdUserRow> = sqlx::query_as(
        "SELECT username, full_name, email, department, title, manager, is_enabled, distinguished_name, synced_at \
         FROM ad_users \
         WHERE ($1::text IS NULL OR department = $1) \
           AND ($2::text IS NULL OR username ILIKE $2 OR full_name ILIKE $2 OR email ILIKE $2) \
         ORDER BY department, full_name, username LIMIT 2000",
    )
    .bind(q.department.as_deref())
    .bind(search_pat.as_deref())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct AdImportUser {
    pub username: String,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub department: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub manager: String,
    #[serde(default = "def_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub distinguished_name: String,
}

fn def_true() -> bool {
    true
}

#[derive(Deserialize)]
struct ImportAdReq {
    users: Vec<AdImportUser>,
}

/// Imports / syncs Active Directory users (callable by Admin JWT or AD sync script with X-Agent-Token).
async fn import_ad_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ImportAdReq>,
) -> AppResult<Json<Value>> {
    if verify_agent_token(&state.config.agent_token, &headers).is_err() {
        let tok = extract_bearer_token(&headers)
            .ok_or_else(|| AppError::Unauthorized("Missing X-Agent-Token or Bearer JWT".into()))?;
        let user = authenticate_token(&state, tok).await?;
        if !user.role.is_admin() {
            return Err(AppError::Forbidden("Requires admin role".into()));
        }
    }
    let mut synced = 0i32;
    for u in body.users {
        let uname = u.username.trim().to_ascii_lowercase();
        if uname.is_empty() {
            continue;
        }
        sqlx::query(
            "INSERT INTO ad_users (username, full_name, email, department, title, manager, is_enabled, distinguished_name, synced_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW()) \
             ON CONFLICT (username) DO UPDATE SET \
                full_name = EXCLUDED.full_name, \
                email = EXCLUDED.email, \
                department = EXCLUDED.department, \
                title = EXCLUDED.title, \
                manager = EXCLUDED.manager, \
                is_enabled = EXCLUDED.is_enabled, \
                distinguished_name = EXCLUDED.distinguished_name, \
                synced_at = NOW()",
        )
        .bind(&uname)
        .bind(u.full_name.trim())
        .bind(u.email.trim())
        .bind(u.department.trim())
        .bind(u.title.trim())
        .bind(u.manager.trim())
        .bind(u.is_enabled)
        .bind(u.distinguished_name.trim())
        .execute(&state.db)
        .await?;
        synced += 1;
    }

    let _ = sqlx::query(
        "UPDATE ad_settings SET last_sync_at = NOW(), last_sync_status = 'ok', synced_count = $1 WHERE id = 1",
    )
    .bind(synced)
    .execute(&state.db)
    .await;

    write_audit(
        &state.db,
        "ad-sync",
        "system",
        "ad_directory_synced",
        "directory/import",
        json!({ "synced": synced }),
    )
    .await;

    Ok(Json(json!({ "synced": synced })))
}
