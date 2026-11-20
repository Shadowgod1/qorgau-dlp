use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/activity/worktime", get(worktime))
        .route("/activity/summary", get(summary))
        .route(
            "/activity/categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/activity/categories/{cat_id}",
            patch(update_category).delete(delete_category),
        )
}

#[derive(Deserialize)]
pub struct ActivityQuery {
    pub username: Option<String>,
    #[serde(default = "def_days")]
    pub days: i32,
}
fn def_days() -> i32 {
    30
}

#[derive(Serialize, sqlx::FromRow)]
pub struct WorktimeRow {
    pub username: String,
    pub day: String,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    pub total_sec: i64,
    pub active_sec: i64,
    pub idle_sec: i64,
    pub productive_sec: i64,
    pub unproductive_sec: i64,
    pub neutral_sec: i64,
    pub productivity_pct: f64,
}

pub async fn query_worktime(
    db: &sqlx::PgPool,
    username: Option<&str>,
    days: i32,
) -> AppResult<Vec<WorktimeRow>> {
    let d = days.clamp(1, 365);
    let rows: Vec<WorktimeRow> = sqlx::query_as(
        "SELECT username, \
                TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, \
                MIN(created_at) AS first_seen, \
                MAX(created_at) AS last_seen, \
                COALESCE(SUM(duration_sec), 0)::bigint AS total_sec, \
                COALESCE(SUM(active_sec), 0)::bigint AS active_sec, \
                COALESCE(SUM(idle_sec), 0)::bigint AS idle_sec, \
                COALESCE(SUM(CASE WHEN productivity = 'productive' THEN duration_sec ELSE 0 END), 0)::bigint AS productive_sec, \
                COALESCE(SUM(CASE WHEN productivity = 'unproductive' THEN duration_sec ELSE 0 END), 0)::bigint AS unproductive_sec, \
                COALESCE(SUM(CASE WHEN productivity = 'neutral' THEN duration_sec ELSE 0 END), 0)::bigint AS neutral_sec, \
                CASE WHEN COALESCE(SUM(duration_sec), 0) > 0 \
                     THEN ROUND((SUM(CASE WHEN productivity = 'productive' THEN duration_sec ELSE 0 END)::numeric * 100.0) / SUM(duration_sec)::numeric, 1)::double precision \
                     ELSE 0.0 END AS productivity_pct \
         FROM app_activity \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
           AND ($2::text IS NULL OR username = $2) \
         GROUP BY username, day ORDER BY day DESC, username ASC",
    )
    .bind(d)
    .bind(username)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn worktime(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ActivityQuery>,
) -> AppResult<Json<Vec<WorktimeRow>>> {
    Ok(Json(query_worktime(&state.db, q.username.as_deref(), q.days).await?))
}

#[derive(Serialize, sqlx::FromRow)]
struct SummaryRow {
    username: String,
    work_days: i64,
    active_sec: i64,
    idle_sec: i64,
    productive_sec: i64,
    unproductive_sec: i64,
    productivity_pct: f64,
}

async fn summary(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ActivityQuery>,
) -> AppResult<Json<Vec<SummaryRow>>> {
    let d = q.days.clamp(1, 365);
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT username, \
                COUNT(DISTINCT TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')) AS work_days, \
                COALESCE(SUM(active_sec), 0)::bigint AS active_sec, \
                COALESCE(SUM(idle_sec), 0)::bigint AS idle_sec, \
                COALESCE(SUM(CASE WHEN productivity = 'productive' THEN duration_sec ELSE 0 END), 0)::bigint AS productive_sec, \
                COALESCE(SUM(CASE WHEN productivity = 'unproductive' THEN duration_sec ELSE 0 END), 0)::bigint AS unproductive_sec, \
                CASE WHEN COALESCE(SUM(duration_sec), 0) > 0 \
                     THEN ROUND((SUM(CASE WHEN productivity = 'productive' THEN duration_sec ELSE 0 END)::numeric * 100.0) / SUM(duration_sec)::numeric, 1)::double precision \
                     ELSE 0.0 END AS productivity_pct \
         FROM app_activity \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY username ORDER BY productivity_pct DESC",
    )
    .bind(d)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct CategoryRow {
    id: i64,
    pattern: String,
    productivity: String,
    label: Option<String>,
}

async fn list_categories(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<CategoryRow>>> {
    let rows: Vec<CategoryRow> = sqlx::query_as(
        "SELECT id, pattern, productivity, label FROM activity_categories ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CategoryReq {
    pattern: String,
    productivity: String,
    label: Option<String>,
}

async fn create_category(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CategoryReq>,
) -> AppResult<Json<CategoryRow>> {
    let prod = body.productivity.to_ascii_lowercase();
    if !matches!(prod.as_str(), "productive" | "unproductive" | "neutral") {
        return Err(AppError::BadRequest("Invalid productivity value".to_string()));
    }
    let pat = body.pattern.trim().to_ascii_lowercase();
    if pat.is_empty() {
        return Err(AppError::BadRequest("Pattern is required".to_string()));
    }

    let row: CategoryRow = sqlx::query_as(
        "INSERT INTO activity_categories (pattern, productivity, label) VALUES ($1, $2, $3) \
         RETURNING id, pattern, productivity, label",
    )
    .bind(&pat)
    .bind(&prod)
    .bind(&body.label)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Category pattern already exists"))?;

    state.policy.reload_activity_categories(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "activity_category_created",
        &format!("category:{}", row.id),
        json!({ "pattern": &row.pattern, "productivity": &row.productivity }),
    )
    .await;

    Ok(Json(row))
}

async fn update_category(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(cat_id): Path<i64>,
    Json(body): Json<CategoryReq>,
) -> AppResult<Json<CategoryRow>> {
    let prod = body.productivity.to_ascii_lowercase();
    if !matches!(prod.as_str(), "productive" | "unproductive" | "neutral") {
        return Err(AppError::BadRequest("Invalid productivity value".to_string()));
    }
    let pat = body.pattern.trim().to_ascii_lowercase();
    let row: Option<CategoryRow> = sqlx::query_as(
        "UPDATE activity_categories SET pattern=$1, productivity=$2, label=$3 WHERE id=$4 \
         RETURNING id, pattern, productivity, label",
    )
    .bind(&pat)
    .bind(&prod)
    .bind(&body.label)
    .bind(cat_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::from_db_unique(e, "Category pattern already exists"))?;

    let r = row.ok_or_else(|| AppError::NotFound("Category not found".to_string()))?;
    state.policy.reload_activity_categories(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "activity_category_updated",
        &format!("category:{cat_id}"),
        json!({ "pattern": &r.pattern, "productivity": &r.productivity }),
    )
    .await;
    Ok(Json(r))
}

async fn delete_category(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(cat_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let res = sqlx::query("DELETE FROM activity_categories WHERE id = $1")
        .bind(cat_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Category not found".to_string()));
    }
    state.policy.reload_activity_categories(&state.db).await?;
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "activity_category_deleted",
        &format!("category:{cat_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}
