use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use qorgau_common::Severity;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{AuthUser, OfficerUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::workers::AlertJob;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ueba/anomalies", get(list_anomalies))
        .route("/ueba/anomalies/{id}", patch(update_anomaly_status))
        .route("/ueba/scan", post(trigger_scan))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UebaAnomalyRow {
    pub id: i64,
    pub username: String,
    pub anomaly_type: String,
    pub severity: String,
    pub score_delta: i32,
    pub title: String,
    pub details: Value,
    pub status: String,
    pub detected_date: NaiveDate,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct ListUebaQ {
    status: Option<String>,
    username: Option<String>,
    #[serde(default = "def_limit")]
    limit: i64,
}

fn def_limit() -> i64 {
    100
}

async fn list_anomalies(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListUebaQ>,
) -> AppResult<Json<Vec<UebaAnomalyRow>>> {
    let limit = q.limit.clamp(1, 500);
    let rows: Vec<UebaAnomalyRow> = sqlx::query_as(
        "SELECT id, username, anomaly_type, severity, score_delta, title, details, status, detected_date, created_at \
         FROM ueba_anomalies \
         WHERE ($1::text IS NULL OR status = $1) \
           AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3",
    )
    .bind(q.status.as_deref())
    .bind(q.username.as_deref())
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct PatchUebaReq {
    status: String,
}

async fn update_anomaly_status(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(body): Json<PatchUebaReq>,
) -> AppResult<Json<UebaAnomalyRow>> {
    let st = body.status.trim().to_ascii_lowercase();
    if !matches!(st.as_str(), "open" | "acknowledged" | "resolved" | "false_positive") {
        return Err(AppError::BadRequest("Invalid UEBA status".into()));
    }

    let row: UebaAnomalyRow = sqlx::query_as(
        "UPDATE ueba_anomalies SET status = $1 WHERE id = $2 \
         RETURNING id, username, anomaly_type, severity, score_delta, title, details, status, detected_date, created_at",
    )
    .bind(&st)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("UEBA anomaly not found".into()))?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "ueba_status_updated",
        &format!("ueba:{id}"),
        json!({ "status": st, "username": row.username }),
    )
    .await;

    Ok(Json(row))
}

async fn trigger_scan(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Value>> {
    let detected = run_ueba_scan(&state).await?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "ueba_manual_scan",
        "ueba/scan",
        json!({ "detected": detected }),
    )
    .await;
    Ok(Json(json!({ "detected": detected })))
}

/// Pure spike helper: returns true when `recent_24h >= min_abs` and either baseline is ~0
/// or `recent_24h >= baseline_daily_avg * multiplier`.
pub fn is_behavioral_spike(recent_24h: f64, baseline_daily_avg: f64, min_abs: f64, multiplier: f64) -> bool {
    if recent_24h < min_abs {
        return false;
    }
    if baseline_daily_avg <= 0.1 {
        return true;
    }
    recent_24h >= baseline_daily_avg * multiplier
}

async fn record_anomaly(
    state: &AppState,
    username: &str,
    anomaly_type: &str,
    severity: Severity,
    score_delta: i32,
    title: &str,
    details: Value,
) -> bool {
    let inserted: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO ueba_anomalies (username, anomaly_type, severity, score_delta, title, details) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (username, anomaly_type, detected_date) DO NOTHING \
         RETURNING id",
    )
    .bind(username)
    .bind(anomaly_type)
    .bind(severity.as_str())
    .bind(score_delta)
    .bind(title)
    .bind(&details)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let Some((id,)) = inserted {
        write_audit(
            &state.db,
            "ueba-engine",
            "system",
            "ueba_anomaly_detected",
            &format!("ueba:{id}"),
            json!({
                "username": username,
                "anomaly_type": anomaly_type,
                "severity": severity.as_str(),
                "title": title,
            }),
        )
        .await;

        let _ = state.alert_tx.try_send(AlertJob {
            incident_id: id,
            ts: Utc::now(),
            agent_id: "ueba-engine".to_string(),
            hostname: "server".to_string(),
            username: username.to_string(),
            channel: format!("ueba_{anomaly_type}"),
            severity,
            rules: vec![format!("UEBA: {title}")],
            snippet: format!("{title} ({username}) — {details}"),
        });
        return true;
    }
    false
}

/// Runs UEBA behavioral anomaly detectors across all active employees.
pub async fn run_ueba_scan(state: &AppState) -> Result<usize, sqlx::Error> {
    let mut detected = 0usize;

    // 1. Off-hours activity (22:00 - 06:00) in last 24h vs 14-day baseline
    let off_hours: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT username, \
                COALESCE(SUM(duration_sec) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours' \
                    AND (EXTRACT(HOUR FROM created_at) >= 22 OR EXTRACT(HOUR FROM created_at) < 6)), 0)::bigint AS night_sec_24h, \
                COALESCE(SUM(duration_sec) FILTER (WHERE created_at >= NOW() - INTERVAL '14 days' AND created_at < NOW() - INTERVAL '24 hours' \
                    AND (EXTRACT(HOUR FROM created_at) >= 22 OR EXTRACT(HOUR FROM created_at) < 6)), 0)::bigint AS night_sec_base \
         FROM app_activity \
         WHERE username <> '' AND created_at >= NOW() - INTERVAL '14 days' \
         GROUP BY username \
         HAVING COALESCE(SUM(duration_sec) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours' \
                    AND (EXTRACT(HOUR FROM created_at) >= 22 OR EXTRACT(HOUR FROM created_at) < 6)), 0) >= 600",
    )
    .fetch_all(&state.db)
    .await?;

    for (username, night_24h, night_base) in off_hours {
        let base_daily = (night_base as f64) / 13.0;
        if is_behavioral_spike(night_24h as f64, base_daily, 600.0, 2.5) {
            let mins = night_24h / 60;
            if record_anomaly(
                state,
                &username,
                "off_hours_activity",
                Severity::Medium,
                15,
                &format!("Нетипичная ночная активность ({mins} мин)"),
                json!({ "night_sec_24h": night_24h, "baseline_daily_sec": base_daily.round() }),
            )
            .await
            {
                detected += 1;
            }
        }
    }

    // 2. Mass USB / Shadow copy spike in last 24h vs 14-day baseline
    let exfil_rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "WITH combined AS ( \
            SELECT username, created_at FROM shadow_copies WHERE username <> '' AND created_at >= NOW() - INTERVAL '14 days' \
            UNION ALL \
            SELECT username, created_at FROM usb_events WHERE username <> '' AND created_at >= NOW() - INTERVAL '14 days' \
         ) \
         SELECT username, \
                COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours') AS cnt_24h, \
                COUNT(*) FILTER (WHERE created_at < NOW() - INTERVAL '24 hours') AS cnt_base \
         FROM combined GROUP BY username \
         HAVING COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours') >= 5",
    )
    .fetch_all(&state.db)
    .await?;

    for (username, cnt_24h, cnt_base) in exfil_rows {
        let base_daily = (cnt_base as f64) / 13.0;
        if is_behavioral_spike(cnt_24h as f64, base_daily, 5.0, 3.0) {
            if record_anomaly(
                state,
                &username,
                "mass_exfiltration",
                Severity::High,
                25,
                &format!("Аномальный всплеск USB / теневых копий ({cnt_24h} за 24ч)"),
                json!({ "count_24h": cnt_24h, "baseline_daily_avg": (base_daily * 10.0).round() / 10.0 }),
            )
            .await
            {
                detected += 1;
            }
        }
    }

    // 3. Incident spike in last 24h vs 14-day baseline
    let inc_rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT username, \
                COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours') AS cnt_24h, \
                COUNT(*) FILTER (WHERE created_at < NOW() - INTERVAL '24 hours') AS cnt_base \
         FROM incidents \
         WHERE username <> '' AND status != 'false_positive' AND created_at >= NOW() - INTERVAL '14 days' \
         GROUP BY username \
         HAVING COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours') >= 4",
    )
    .fetch_all(&state.db)
    .await?;

    for (username, cnt_24h, cnt_base) in inc_rows {
        let base_daily = (cnt_base as f64) / 13.0;
        if is_behavioral_spike(cnt_24h as f64, base_daily, 4.0, 3.0) {
            if record_anomaly(
                state,
                &username,
                "incident_spike",
                Severity::High,
                20,
                &format!("Резкий рост DLP-инцидентов ({cnt_24h} за 24ч)"),
                json!({ "incidents_24h": cnt_24h, "baseline_daily_avg": (base_daily * 10.0).round() / 10.0 }),
            )
            .await
            {
                detected += 1;
            }
        }
    }

    Ok(detected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavioral_spike_detection_works() {
        assert!(!is_behavioral_spike(3.0, 0.5, 5.0, 3.0));
        assert!(is_behavioral_spike(5.0, 0.0, 5.0, 3.0));
        assert!(!is_behavioral_spike(6.0, 3.0, 5.0, 3.0));
        assert!(is_behavioral_spike(9.0, 3.0, 5.0, 3.0));
    }
}
