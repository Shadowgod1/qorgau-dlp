use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::{mask_text, sanitize_spreadsheet_cell};
use qorgau_common::Role;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::{verify_chain, write_audit};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::routes::activity::query_worktime;
use crate::routes::incidents::{IncidentRow, INCIDENT_COLS};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/reports/incidents", get(incident_report))
        .route("/reports/user/{username}", get(user_report))
        .route("/reports/compliance", get(compliance_report))
        .route("/reports/agents", get(agents_report))
        .route("/reports/export", get(export_report))
        .route("/reports/cdr", get(export_cdr))
        .route("/reports/activity/export", get(export_activity))
}

#[derive(Deserialize)]
struct DaysQuery {
    #[serde(default = "def_days")]
    days: i32,
}
fn def_days() -> i32 {
    30
}

#[derive(Deserialize)]
struct ExportQuery {
    #[serde(default = "def_csv")]
    format: String,
    #[serde(default = "def_days")]
    days: i32,
}
fn def_csv() -> String {
    "csv".to_string()
}

async fn incident_report(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Value>> {
    let days = q.days.clamp(1, 365);

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM incidents WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL",
    )
    .bind(days)
    .fetch_one(&state.db)
    .await?;

    let sev_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT severity, COUNT(*) FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL GROUP BY severity",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let ch_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT channel, COUNT(*) FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL GROUP BY channel",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let st_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL GROUP BY status",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let top_users: Vec<(String, i64)> = sqlx::query_as(
        "SELECT username, COUNT(*) AS cnt FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         GROUP BY username ORDER BY cnt DESC LIMIT 10",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    let by_severity: HashMap<String, i64> = sev_rows.into_iter().collect();
    let by_channel: HashMap<String, i64> = ch_rows.into_iter().collect();
    let by_status: HashMap<String, i64> = st_rows.into_iter().collect();
    let top_violators: Vec<Value> = top_users
        .into_iter()
        .map(|(username, count)| json!({ "username": username, "count": count }))
        .collect();

    Ok(Json(json!({
        "period_days": days,
        "total": total.0,
        "by_severity": by_severity,
        "by_channel": by_channel,
        "by_status": by_status,
        "top_users": top_violators,
    })))
}

async fn user_report(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Value>> {
    let days = q.days.clamp(1, 365);

    let sql = format!(
        "SELECT {INCIDENT_COLS} FROM incidents \
         WHERE username = $1 AND created_at >= NOW() - ($2 || ' days')::INTERVAL \
         ORDER BY created_at DESC LIMIT 200"
    );
    let rows: Vec<IncidentRow> = sqlx::query_as(&sql)
        .bind(&username)
        .bind(days)
        .fetch_all(&state.db)
        .await?;

    let items: Vec<IncidentRow> = rows.into_iter().map(|r| r.apply_rbac(user.role)).collect();
    let total = items.len();
    Ok(Json(json!({
        "username": username,
        "period_days": days,
        "total_incidents": total,
        "incidents": items,
    })))
}

#[derive(Serialize)]
struct ComplianceItem {
    id: &'static str,
    name: &'static str,
    law_ref: &'static str,
    description: &'static str,
    mandatory: bool,
    status: &'static str,
    details: String,
}

/// Real dynamic KZ Law compliance evaluation (ТК РК, Закон РК «О ПДн», Приказ №410, ЕТ ИБ).
async fn compliance_report(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Value>> {
    let audit_check = verify_chain(&state.db).await.ok();
    let audit_ok = audit_check.as_ref().map(|r| r.valid).unwrap_or(false);
    let audit_count = audit_check.as_ref().map(|r| r.total_entries).unwrap_or(0);

    let policies_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM policies WHERE enabled = TRUE")
            .fetch_one(&state.db)
            .await?;

    let secret_strong = state.config.secret_key.len() >= 32
        && !state.config.secret_key.contains("change-me");
    let retention_ok = state.config.data_retention_days <= 1095
        && state.config.shadow_retention_days <= 1095;
    let siem_configured = state.config.siem_syslog_host.is_some()
        || state.config.siem_webhook_url.is_some();
    let no_foreign_tg = state.config.telegram_token.is_none();

    let requirements = vec![
        ComplianceItem {
            id: "kz_pdn_localization",
            name: "Локализация ПДн на территории РК",
            law_ref: "Закон РК «О ПДн» ст. 12",
            description: "Хранение базы ПДн локально без передачи открытых ПДн в зарубежные сервисы",
            mandatory: true,
            status: if no_foreign_tg { "compliant" } else { "warning" },
            details: if no_foreign_tg {
                "Внешние мессенджер-алерты отключены; данные хранятся локально в PostgreSQL".to_string()
            } else {
                "Включены Telegram-алерты (ПДн маскируются, но для строгой локализации рекомендуется Syslog CEF)".to_string()
            },
        },
        ComplianceItem {
            id: "kz_audit_chain",
            name: "Журналирование доступа к ПДн (Hash-Chain)",
            law_ref: "Приказ МЦРИАП РК №410",
            description: "Неизменяемый криптографический журнал всех обращений к ПДн и действий операторов",
            mandatory: true,
            status: if audit_ok { "compliant" } else { "non_compliant" },
            details: format!("Проверено записей: {audit_count}, целостность: {audit_ok}"),
        },
        ComplianceItem {
            id: "kz_retention",
            name: "Ограничение сроков хранения данных (<= 3 лет)",
            law_ref: "Закон РК «О ПДн» ст. 12, 18",
            description: "Автоматическая очистка событий, скриншотов, аудио, видео и теневых копий",
            mandatory: true,
            status: if retention_ok { "compliant" } else { "non_compliant" },
            details: format!(
                "События: {} дн., Shadow: {} дн., Скриншоты/Аудио/Камера: {}/{}/{} дн.",
                state.config.data_retention_days,
                state.config.shadow_retention_days,
                state.config.screenshots_retention_days,
                state.config.audio_retention_days,
                state.config.webcam_retention_days
            ),
        },
        ComplianceItem {
            id: "kz_tk_notification",
            name: "Уведомление работников и fail-closed контроль",
            law_ref: "Трудовой кодекс РК ст. 23, 28",
            description: "Модули микрофона и веб-камеры отключены по умолчанию (fail-closed) до явного включения",
            mandatory: true,
            status: "compliant",
            details: "Fail-closed реализован в протоколе агента и схеме БД".to_string(),
        },
        ComplianceItem {
            id: "kz_rbac_masking",
            name: "Разграничение доступа (RBAC) и маскирование ПДн",
            law_ref: "Постановление Правительства РК №832 (ЕТ ИБ)",
            description: "3 роли (admin/security_officer/viewer), маскирование ПДн и meta для viewer, Argon2id",
            mandatory: true,
            status: if secret_strong { "compliant" } else { "warning" },
            details: if secret_strong {
                "RBAC активен, JWT ключ соответствует требованиям длины".to_string()
            } else {
                "Замените дефолтный SECRET_KEY в .env на случайный ключ >= 32 символов".to_string()
            },
        },
        ComplianceItem {
            id: "kz_ocib_siem",
            name: "Интеграция с ОЦИБ / SIEM (Syslog CEF)",
            law_ref: "Единые требования в области ИКТ и ИБ",
            description: "Передача инцидентов в формате RFC 5424 + ArcSight CEF:0",
            mandatory: false,
            status: if siem_configured { "compliant" } else { "warning" },
            details: if siem_configured {
                "SIEM / Syslog CEF настроен".to_string()
            } else {
                format!("SIEM не настроен; активных пользовательских DLP-политик: {}", policies_count.0)
            },
        },
    ];

    let compliant_count = requirements.iter().filter(|r| r.status == "compliant").count();
    let score = (compliant_count * 100) / requirements.len();

    Ok(Json(json!({
        "standard": "Законодательство РК (Закон о ПДн, ТК РК, ЕТ ИБ)",
        "score_pct": score,
        "checked_at": Utc::now(),
        "requirements": requirements,
    })))
}

async fn agents_report(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Value>> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&state.db)
        .await?;
    let online: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM agents WHERE last_seen >= NOW() - INTERVAL '3 minutes'",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({
        "total": total.0,
        "online": online.0,
        "offline": (total.0 - online.0).max(0),
        "ws_connected": state.ws.connected_agent_count(),
    })))
}

async fn export_report(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    let days = q.days.clamp(1, 365);
    let sql = format!(
        "SELECT {INCIDENT_COLS} FROM incidents \
         WHERE created_at >= NOW() - ($1 || ' days')::INTERVAL \
         ORDER BY created_at DESC LIMIT 10000"
    );
    let rows: Vec<IncidentRow> = sqlx::query_as(&sql)
        .bind(days)
        .fetch_all(&state.db)
        .await?;

    let items: Vec<IncidentRow> = rows.into_iter().map(|r| r.apply_rbac(user.role)).collect();

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "export_incidents",
        "reports/export",
        json!({ "format": &q.format, "days": days, "rows": items.len() }),
    )
    .await;

    match q.format.to_ascii_lowercase().as_str() {
        "json" => {
            let bytes = serde_json::to_vec_pretty(&items)
                .map_err(|e| AppError::Internal(e.to_string()))?;
            download_response(bytes, "application/json", &format!("incidents_{days}d.json"))
        }
        "xlsx" => {
            let mut wb = Workbook::new();
            let ws = wb.add_worksheet();
            let headers = [
                "ID",
                "Timestamp",
                "Hostname",
                "Username",
                "Channel",
                "Severity",
                "Status",
                "Matched Rules",
                "Snippet",
            ];
            for (c, h) in headers.iter().enumerate() {
                let _ = ws.write_string(0, c as u16, *h);
            }
            for (idx, it) in items.iter().enumerate() {
                let r = (idx + 1) as u32;
                let _ = ws.write_number(r, 0, it.id as f64);
                let _ = ws.write_string(
                    r,
                    1,
                    it.created_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                );
                let _ = ws.write_string(r, 2, sanitize_spreadsheet_cell(&it.hostname));
                let _ = ws.write_string(r, 3, sanitize_spreadsheet_cell(&it.username));
                let _ = ws.write_string(r, 4, sanitize_spreadsheet_cell(&it.channel));
                let _ = ws.write_string(r, 5, &it.severity);
                let _ = ws.write_string(r, 6, &it.status);
                let _ = ws.write_string(r, 7, sanitize_spreadsheet_cell(&it.matched_rules.to_string()));
                let _ = ws.write_string(r, 8, sanitize_spreadsheet_cell(&it.snippet));
            }
            let buf = wb
                .save_to_buffer()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            download_response(
                buf,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                &format!("incidents_{days}d.xlsx"),
            )
        }
        _ => {
            let mut wtr = csv::Writer::from_writer(Vec::new());
            let _ = wtr.write_record([
                "id",
                "created_at",
                "hostname",
                "username",
                "channel",
                "severity",
                "status",
                "matched_rules",
                "snippet",
            ]);
            for it in &items {
                let _ = wtr.write_record([
                    it.id.to_string(),
                    it.created_at.map(|t| t.to_rfc3339()).unwrap_or_default(),
                    sanitize_spreadsheet_cell(&it.hostname),
                    sanitize_spreadsheet_cell(&it.username),
                    sanitize_spreadsheet_cell(&it.channel),
                    it.severity.clone(),
                    it.status.clone(),
                    sanitize_spreadsheet_cell(&it.matched_rules.to_string()),
                    sanitize_spreadsheet_cell(&it.snippet),
                ]);
            }
            let bytes = wtr
                .into_inner()
                .map_err(|e| AppError::Internal(e.to_string()))?;
            download_response(bytes, "text/csv; charset=utf-8", &format!("incidents_{days}d.csv"))
        }
    }
}

#[derive(sqlx::FromRow)]
struct CdrRow {
    ts: DateTime<Utc>,
    agent_id: String,
    username: String,
    channel: String,
    action: String,
    target: String,
    flagged: bool,
}

async fn export_cdr(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    let days = q.days.clamp(1, 365);
    let rows: Vec<CdrRow> = sqlx::query_as(
        "SELECT * FROM ( \
            SELECT created_at AS ts, agent_id, username, 'usb'::text AS channel, action, COALESCE(file_path, device_name) AS target, FALSE AS flagged FROM usb_events \
            UNION ALL SELECT created_at, agent_id, username, 'web', category, url, blocked FROM web_activity \
            UNION ALL SELECT created_at, agent_id, username, 'messenger', messenger, COALESCE(contact, direction), flagged FROM messenger_events \
            UNION ALL SELECT created_at, agent_id, username, 'network', protocol, remote_ip || ':' || remote_port, FALSE FROM network_connections \
            UNION ALL SELECT created_at, agent_id, username, 'print', status, document_name, FALSE FROM print_jobs \
         ) c WHERE c.ts >= NOW() - ($1 || ' days')::INTERVAL \
         ORDER BY c.ts DESC LIMIT 10000",
    )
    .bind(days)
    .fetch_all(&state.db)
    .await?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "export_cdr",
        "reports/cdr",
        json!({ "format": &q.format, "days": days, "rows": rows.len() }),
    )
    .await;

    let mask_target = |t: &str| {
        if user.role == Role::Viewer {
            sanitize_spreadsheet_cell(&mask_text(t))
        } else {
            sanitize_spreadsheet_cell(t)
        }
    };

    if q.format.eq_ignore_ascii_case("xlsx") {
        let mut wb = Workbook::new();
        let ws = wb.add_worksheet();
        for (c, h) in ["Timestamp", "Agent", "User", "Channel", "Action", "Target", "Flagged"]
            .iter()
            .enumerate()
        {
            let _ = ws.write_string(0, c as u16, *h);
        }
        for (i, row) in rows.iter().enumerate() {
            let r = (i + 1) as u32;
            let _ = ws.write_string(r, 0, row.ts.to_rfc3339());
            let _ = ws.write_string(r, 1, &row.agent_id);
            let _ = ws.write_string(r, 2, sanitize_spreadsheet_cell(&row.username));
            let _ = ws.write_string(r, 3, &row.channel);
            let _ = ws.write_string(r, 4, sanitize_spreadsheet_cell(&row.action));
            let _ = ws.write_string(r, 5, mask_target(&row.target));
            let _ = ws.write_boolean(r, 6, row.flagged);
        }
        let buf = wb
            .save_to_buffer()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        download_response(
            buf,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            &format!("cdr_{days}d.xlsx"),
        )
    } else {
        let mut wtr = csv::Writer::from_writer(Vec::new());
        let _ = wtr.write_record(["ts", "agent_id", "username", "channel", "action", "target", "flagged"]);
        for row in &rows {
            let _ = wtr.write_record([
                row.ts.to_rfc3339(),
                row.agent_id.clone(),
                sanitize_spreadsheet_cell(&row.username),
                row.channel.clone(),
                sanitize_spreadsheet_cell(&row.action),
                mask_target(&row.target),
                row.flagged.to_string(),
            ]);
        }
        let bytes = wtr
            .into_inner()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        download_response(bytes, "text/csv; charset=utf-8", &format!("cdr_{days}d.csv"))
    }
}

async fn export_activity(
    AuthUser(user): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Response> {
    let days = q.days.clamp(1, 365);
    let rows = query_worktime(&state.db, None, days).await?;

    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "export_worktime",
        "reports/activity/export",
        json!({ "days": days, "rows": rows.len() }),
    )
    .await;

    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (c, h) in [
        "Username",
        "Day",
        "Total (s)",
        "Active (s)",
        "Idle (s)",
        "Productive (s)",
        "Unproductive (s)",
        "Neutral (s)",
        "Productivity (%)",
    ]
    .iter()
    .enumerate()
    {
        let _ = ws.write_string(0, c as u16, *h);
    }
    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        let _ = ws.write_string(r, 0, sanitize_spreadsheet_cell(&row.username));
        let _ = ws.write_string(r, 1, &row.day);
        let _ = ws.write_number(r, 2, row.total_sec as f64);
        let _ = ws.write_number(r, 3, row.active_sec as f64);
        let _ = ws.write_number(r, 4, row.idle_sec as f64);
        let _ = ws.write_number(r, 5, row.productive_sec as f64);
        let _ = ws.write_number(r, 6, row.unproductive_sec as f64);
        let _ = ws.write_number(r, 7, row.neutral_sec as f64);
        let _ = ws.write_number(r, 8, row.productivity_pct);
    }
    let buf = wb
        .save_to_buffer()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    download_response(
        buf,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &format!("worktime_{days}d.xlsx"),
    )
}

fn download_response(bytes: Vec<u8>, content_type: &str, filename: &str) -> AppResult<Response> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|e| AppError::Internal(e.to_string()))
}
