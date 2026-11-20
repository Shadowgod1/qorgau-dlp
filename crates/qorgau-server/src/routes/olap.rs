use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/olap/facts", get(get_facts))
        .route("/olap/dim/agents", get(dim_agents))
        .route("/olap/dim/users", get(dim_users))
        .route("/olap/dim/apps", get(dim_apps))
        .route("/olap/dim/sites", get(dim_sites))
        .route("/olap/cube/activity", get(cube_activity))
        .route("/olap/cube/risk", get(cube_risk))
}

#[derive(Deserialize)]
struct DaysQuery {
    #[serde(default = "def_days")]
    days: i32,
    #[serde(default = "def_limit")]
    limit: i64,
}
fn def_days() -> i32 {
    30
}
fn def_limit() -> i64 {
    50
}

#[derive(Serialize, sqlx::FromRow)]
struct FactRow {
    id: i64,
    event_type: String,
    agent_id: String,
    username: String,
    subtype: String,
    severity: Option<String>,
    ts: DateTime<Utc>,
}

async fn get_facts(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<FactRow>>> {
    let days = q.days.clamp(1, 365);
    let limit = q.limit.clamp(1, 500);
    let rows: Vec<FactRow> = sqlx::query_as(
        "SELECT * FROM ( \
            SELECT id, 'incident'::text AS event_type, agent_id, username, channel AS subtype, severity::text, created_at AS ts FROM incidents \
            UNION ALL SELECT id, 'usb'::text, agent_id, username, action, NULL::text, created_at FROM usb_events \
            UNION ALL SELECT id, 'app'::text, agent_id, username, app_name, NULL::text, created_at FROM app_activity \
            UNION ALL SELECT id, 'web'::text, agent_id, username, domain, CASE WHEN blocked THEN 'medium' ELSE NULL END, created_at FROM web_activity \
            UNION ALL SELECT id, 'network'::text, agent_id, username, protocol, NULL::text, created_at FROM network_connections \
            UNION ALL SELECT id, 'print'::text, agent_id, username, printer_name, NULL::text, created_at FROM print_jobs \
         ) f WHERE f.ts >= NOW() - ($1 || ' days')::INTERVAL \
         ORDER BY f.ts DESC LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct DimAgentRow {
    agent_id: String,
    hostname: String,
    username: String,
    is_online: bool,
    incidents: i64,
    screenshots: i64,
    usb_events: i64,
}

/// Fixed: uses pre-aggregated subqueries instead of 3-way LEFT JOIN Cartesian product.
async fn dim_agents(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<DimAgentRow>>> {
    let rows: Vec<DimAgentRow> = sqlx::query_as(
        "SELECT a.agent_id, a.hostname, a.username, \
                (a.last_seen >= NOW() - INTERVAL '3 minutes') AS is_online, \
                COALESCE(i.c, 0) AS incidents, \
                COALESCE(s.c, 0) AS screenshots, \
                COALESCE(u.c, 0) AS usb_events \
         FROM agents a \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM incidents GROUP BY agent_id) i ON i.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM screenshots GROUP BY agent_id) s ON s.agent_id = a.agent_id \
         LEFT JOIN (SELECT agent_id, COUNT(*) AS c FROM usb_events GROUP BY agent_id) u ON u.agent_id = a.agent_id \
         ORDER BY incidents DESC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct DimUserRow {
    username: String,
    total_incidents: i64,
    critical: i64,
    high: i64,
    medium: i64,
    low: i64,
    open_incidents: i64,
    risk_score: i64,
}

async fn dim_users(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<DimUserRow>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<DimUserRow> = sqlx::query_as(
        "SELECT username, \
                COUNT(*) AS total_incidents, \
                COUNT(*) FILTER (WHERE severity = 'critical') AS critical, \
                COUNT(*) FILTER (WHERE severity = 'high') AS high, \
                COUNT(*) FILTER (WHERE severity = 'medium') AS medium, \
                COUNT(*) FILTER (WHERE severity = 'low') AS low, \
                COUNT(*) FILTER (WHERE status = 'open') AS open_incidents, \
                LEAST(100, \
                    COUNT(*) FILTER (WHERE severity = 'critical') * 25 + \
                    COUNT(*) FILTER (WHERE severity = 'high') * 12 + \
                    COUNT(*) FILTER (WHERE severity = 'medium') * 5 + \
                    COUNT(*) FILTER (WHERE severity = 'low') * 1 \
                ) AS risk_score \
         FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY username ORDER BY risk_score DESC, total_incidents DESC LIMIT $2",
    )
    .bind(days)
    .bind(q.limit.clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct DimAppRow {
    app_name: String,
    total_sec: i64,
    active_sec: i64,
    sessions: i64,
    users: i64,
    productivity: String,
}

async fn dim_apps(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<DimAppRow>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<DimAppRow> = sqlx::query_as(
        "SELECT app_name, \
                COALESCE(SUM(duration_sec), 0)::bigint AS total_sec, \
                COALESCE(SUM(active_sec), 0)::bigint AS active_sec, \
                COUNT(*) AS sessions, \
                COUNT(DISTINCT username) AS users, \
                MAX(productivity) AS productivity \
         FROM app_activity \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY app_name ORDER BY total_sec DESC LIMIT $2",
    )
    .bind(days)
    .bind(q.limit.clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct DimSiteRow {
    domain: String,
    category: String,
    visits: i64,
    blocked: i64,
    users: i64,
}

async fn dim_sites(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<DimSiteRow>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<DimSiteRow> = sqlx::query_as(
        "SELECT domain, MAX(category) AS category, COUNT(*) AS visits, \
                COUNT(*) FILTER (WHERE blocked) AS blocked, \
                COUNT(DISTINCT username) AS users \
         FROM web_activity \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY domain ORDER BY visits DESC LIMIT $2",
    )
    .bind(days)
    .bind(q.limit.clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct CubeActivityRow {
    day: String,
    event_type: String,
    count: i64,
}

async fn cube_activity(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<CubeActivityRow>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<CubeActivityRow> = sqlx::query_as(
        "SELECT TO_CHAR(ts AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, event_type, COUNT(*) AS count \
         FROM ( \
            SELECT 'incident'::text AS event_type, created_at AS ts FROM incidents \
            UNION ALL SELECT 'usb', created_at FROM usb_events \
            UNION ALL SELECT 'app', created_at FROM app_activity \
            UNION ALL SELECT 'web', created_at FROM web_activity \
            UNION ALL SELECT 'messenger', created_at FROM messenger_events \
            UNION ALL SELECT 'clipboard', created_at FROM clipboard_events \
            UNION ALL SELECT 'network', created_at FROM network_connections \
            UNION ALL SELECT 'print', created_at FROM print_jobs \
         ) t WHERE ts >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY day, event_type ORDER BY day ASC",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct CubeRiskRow {
    day: String,
    critical: i64,
    high: i64,
    medium: i64,
    low: i64,
    risk_score: i64,
}

async fn cube_risk(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<CubeRiskRow>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<CubeRiskRow> = sqlx::query_as(
        "SELECT TO_CHAR(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, \
                COUNT(*) FILTER (WHERE severity = 'critical') AS critical, \
                COUNT(*) FILTER (WHERE severity = 'high') AS high, \
                COUNT(*) FILTER (WHERE severity = 'medium') AS medium, \
                COUNT(*) FILTER (WHERE severity = 'low') AS low, \
                (COUNT(*) FILTER (WHERE severity = 'critical') * 10 + \
                 COUNT(*) FILTER (WHERE severity = 'high') * 5 + \
                 COUNT(*) FILTER (WHERE severity = 'medium') * 2 + \
                 COUNT(*) FILTER (WHERE severity = 'low')) AS risk_score \
         FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY day ORDER BY day ASC",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}
