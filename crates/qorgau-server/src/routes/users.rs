use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser};
use crate::error::AppResult;
use crate::routes::incidents::{IncidentRow, INCIDENT_COLS};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", get(list_employees))
        .route("/users/{username}/profile", get(employee_profile))
        .route(
            "/settings/risk-threshold",
            get(get_risk_threshold).put(update_risk_threshold),
        )
}

#[derive(Deserialize)]
struct DaysQ {
    #[serde(default = "def_days")]
    days: i32,
}
fn def_days() -> i32 {
    30
}

#[derive(Serialize, sqlx::FromRow)]
struct EmployeeSummary {
    username: String,
    hostnames: Vec<String>,
    is_online: bool,
    last_seen: Option<DateTime<Utc>>,
    total_incidents: i64,
    critical_incidents: i64,
    open_incidents: i64,
    risk_score: i64,
    active_sec: i64,
    productivity_pct: f64,
}

/// Aggregated Employee Directory (combines agents, incidents, and productivity over `days`).
async fn list_employees(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQ>,
) -> AppResult<Json<Vec<EmployeeSummary>>> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<EmployeeSummary> = sqlx::query_as(
        "WITH all_users AS ( \
            SELECT DISTINCT username FROM agents WHERE username <> '' \
            UNION SELECT DISTINCT username FROM incidents WHERE username <> '' \
            UNION SELECT DISTINCT username FROM app_activity WHERE username <> '' \
         ), \
         ag AS ( \
            SELECT username, \
                   ARRAY_AGG(DISTINCT hostname) AS hostnames, \
                   BOOL_OR(last_seen >= NOW() - INTERVAL '3 minutes') AS is_online, \
                   MAX(last_seen) AS last_seen \
            FROM agents GROUP BY username \
         ), \
         inc AS ( \
            SELECT username, \
                   COUNT(*) AS total_incidents, \
                   COUNT(*) FILTER (WHERE severity = 'critical') AS critical_incidents, \
                   COUNT(*) FILTER (WHERE status = 'open') AS open_incidents, \
                   LEAST(100, \
                       COUNT(*) FILTER (WHERE severity = 'critical') * 25 + \
                       COUNT(*) FILTER (WHERE severity = 'high') * 12 + \
                       COUNT(*) FILTER (WHERE severity = 'medium') * 5 + \
                       COUNT(*) FILTER (WHERE severity = 'low') * 1 \
                   ) AS risk_score \
            FROM incidents \
            WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL AND status != 'false_positive' \
            GROUP BY username \
         ), \
         act AS ( \
            SELECT username, \
                   COALESCE(SUM(active_sec), 0)::bigint AS active_sec, \
                   CASE WHEN COALESCE(SUM(duration_sec), 0) > 0 \
                        THEN ROUND((SUM(CASE WHEN productivity = 'productive' THEN duration_sec ELSE 0 END)::numeric * 100.0) / SUM(duration_sec)::numeric, 1)::double precision \
                        ELSE 0.0 END AS productivity_pct \
            FROM app_activity \
            WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
            GROUP BY username \
         ) \
         SELECT u.username, \
                COALESCE(ag.hostnames, ARRAY[]::text[]) AS hostnames, \
                COALESCE(ag.is_online, FALSE) AS is_online, \
                ag.last_seen, \
                COALESCE(inc.total_incidents, 0) AS total_incidents, \
                COALESCE(inc.critical_incidents, 0) AS critical_incidents, \
                COALESCE(inc.open_incidents, 0) AS open_incidents, \
                COALESCE(inc.risk_score, 0) AS risk_score, \
                COALESCE(act.active_sec, 0) AS active_sec, \
                COALESCE(act.productivity_pct, 0.0) AS productivity_pct \
         FROM all_users u \
         LEFT JOIN ag ON ag.username = u.username \
         LEFT JOIN inc ON inc.username = u.username \
         LEFT JOIN act ON act.username = u.username \
         ORDER BY risk_score DESC, total_incidents DESC, u.username ASC",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct TopApp {
    app_name: String,
    total_sec: i64,
    productivity: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct TopSite {
    domain: String,
    category: String,
    visits: i64,
    blocked: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct UserUsb {
    id: i64,
    device_name: String,
    action: String,
    file_path: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize, sqlx::FromRow)]
struct UserAgentBrief {
    agent_id: String,
    hostname: String,
    ip_address: String,
    is_online: bool,
    last_seen: Option<DateTime<Utc>>,
}

/// Full Employee Card (iBatyr «Карточка сотрудника» parity).
async fn employee_profile(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
    Query(q): Query<DaysQ>,
) -> AppResult<Json<Value>> {
    let days = q.days.clamp(1, 365);

    let agents: Vec<UserAgentBrief> = sqlx::query_as(
        "SELECT agent_id, hostname, ip_address, (last_seen >= NOW() - INTERVAL '3 minutes') AS is_online, last_seen \
         FROM agents WHERE username = $1 ORDER BY last_seen DESC",
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await?;

    let inc_sql = format!(
        "SELECT {INCIDENT_COLS} FROM incidents \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL \
         ORDER BY created_at DESC LIMIT 50"
    );
    let raw_incs: Vec<IncidentRow> = sqlx::query_as(&inc_sql)
        .bind(&username)
        .bind(days)
        .fetch_all(&state.db)
        .await?;
    let incidents: Vec<IncidentRow> = raw_incs
        .into_iter()
        .map(|r| r.apply_rbac(user.role))
        .collect();

    let (risk_score, crit_c, high_c, med_c, low_c): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            LEAST(100, \
                COUNT(*) FILTER (WHERE severity = 'critical') * 25 + \
                COUNT(*) FILTER (WHERE severity = 'high') * 12 + \
                COUNT(*) FILTER (WHERE severity = 'medium') * 5 + \
                COUNT(*) FILTER (WHERE severity = 'low') * 1 \
            ), \
            COUNT(*) FILTER (WHERE severity = 'critical'), \
            COUNT(*) FILTER (WHERE severity = 'high'), \
            COUNT(*) FILTER (WHERE severity = 'medium'), \
            COUNT(*) FILTER (WHERE severity = 'low') \
         FROM incidents \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL AND status != 'false_positive'",
    )
    .bind(&username)
    .bind(days)
    .fetch_one(&state.db)
    .await?;

    let top_apps: Vec<TopApp> = sqlx::query_as(
        "SELECT app_name, COALESCE(SUM(duration_sec), 0)::bigint AS total_sec, MAX(productivity) AS productivity \
         FROM app_activity \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL \
         GROUP BY app_name ORDER BY total_sec DESC LIMIT 10",
    )
    .bind(&username)
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let top_sites: Vec<TopSite> = sqlx::query_as(
        "SELECT domain, MAX(category) AS category, COUNT(*) AS visits, COUNT(*) FILTER (WHERE blocked) AS blocked \
         FROM web_activity \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL \
         GROUP BY domain ORDER BY visits DESC LIMIT 10",
    )
    .bind(&username)
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let recent_usb: Vec<UserUsb> = sqlx::query_as(
        "SELECT id, device_name, action, file_path, created_at FROM usb_events \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL \
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(&username)
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let worktime = crate::routes::activity::query_worktime(&state.db, Some(&username), days).await?;

    Ok(Json(json!({
        "username": username,
        "period_days": days,
        "risk_score": risk_score,
        "risk_breakdown": {
            "critical": crit_c,
            "high": high_c,
            "medium": med_c,
            "low": low_c,
        },
        "agents": agents,
        "incidents": incidents,
        "top_apps": top_apps,
        "top_sites": top_sites,
        "recent_usb": recent_usb,
        "worktime": worktime,
    })))
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct RiskSettingsDto {
    risk_threshold: i32,
    notify_on_risk_growth: bool,
    updated_by: Option<String>,
    updated_at: Option<DateTime<Utc>>,
}

async fn get_risk_threshold(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<RiskSettingsDto>> {
    let row: RiskSettingsDto = sqlx::query_as(
        "SELECT risk_threshold, notify_on_risk_growth, updated_by, updated_at \
         FROM alert_settings WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Deserialize)]
struct UpdateRiskReq {
    risk_threshold: i32,
    notify_on_risk_growth: bool,
}

async fn update_risk_threshold(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateRiskReq>,
) -> AppResult<Json<RiskSettingsDto>> {
    let thr = body.risk_threshold.clamp(10, 100);
    let row: RiskSettingsDto = sqlx::query_as(
        "UPDATE alert_settings SET risk_threshold = $1, notify_on_risk_growth = $2, updated_by = $3, updated_at = NOW() \
         WHERE id = 1 RETURNING risk_threshold, notify_on_risk_growth, updated_by, updated_at",
    )
    .bind(thr)
    .bind(body.notify_on_risk_growth)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "risk_threshold_updated",
        "settings/risk-threshold",
        json!({ "risk_threshold": thr, "notify_on_risk_growth": body.notify_on_risk_growth }),
    )
    .await;

    Ok(Json(row))
}
