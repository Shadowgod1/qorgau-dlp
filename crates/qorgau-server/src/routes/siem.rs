use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::cef::{format_syslog_cef, CefIncident};
use qorgau_common::Severity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/siem", get(get_siem_settings).put(update_siem_settings))
        .route("/settings/siem/test", post(test_siem_delivery))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SiemSettingsRow {
    pub enabled: bool,
    pub webhook_url: String,
    pub webhook_token: String,
    pub syslog_host: String,
    pub syslog_port: i32,
    pub syslog_proto: String,
    pub telegram_enabled: bool,
    pub telegram_token: String,
    pub telegram_chat_id: String,
    pub min_severity: String,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EffectiveSiemConfig {
    pub webhook_url: Option<String>,
    pub webhook_token: Option<String>,
    pub syslog_host: Option<String>,
    pub syslog_port: u16,
    pub syslog_proto: String,
    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub min_severity: Severity,
}

pub fn parse_severity(s: &str) -> Severity {
    match s.to_ascii_lowercase().as_str() {
        "critical" => Severity::Critical,
        "high" => Severity::High,
        "medium" => Severity::Medium,
        _ => Severity::Low,
    }
}

pub fn severity_meets_min(sev: Severity, min: Severity) -> bool {
    sev >= min
}

/// Resolves effective SIEM & Telegram delivery settings:
/// DB settings take precedence when enabled; otherwise `.env` fallback applies.
pub async fn load_effective_siem(state: &AppState) -> EffectiveSiemConfig {
    let row = sqlx::query_as::<_, SiemSettingsRow>(
        "SELECT enabled, webhook_url, webhook_token, syslog_host, syslog_port, syslog_proto, \
                telegram_enabled, telegram_token, telegram_chat_id, min_severity, updated_by, updated_at \
         FROM siem_settings WHERE id = 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let cfg = &state.config;
    match row {
        Some(r) => {
            let webhook_url = if r.enabled && !r.webhook_url.trim().is_empty() {
                Some(r.webhook_url.trim().to_string())
            } else {
                cfg.siem_webhook_url.clone()
            };
            let webhook_token = if r.enabled && !r.webhook_token.trim().is_empty() {
                Some(r.webhook_token.trim().to_string())
            } else {
                cfg.siem_webhook_token.clone()
            };
            let (syslog_host, syslog_port, syslog_proto) =
                if r.enabled && !r.syslog_host.trim().is_empty() {
                    (
                        Some(r.syslog_host.trim().to_string()),
                        u16::try_from(r.syslog_port.clamp(1, 65535)).unwrap_or(514),
                        if r.syslog_proto == "tcp" {
                            "tcp".to_string()
                        } else {
                            "udp".to_string()
                        },
                    )
                } else {
                    (
                        cfg.siem_syslog_host.clone(),
                        cfg.siem_syslog_port,
                        cfg.siem_syslog_proto.clone(),
                    )
                };
            let (telegram_token, telegram_chat_id) =
                if r.telegram_enabled && !r.telegram_token.trim().is_empty() && !r.telegram_chat_id.trim().is_empty() {
                    (
                        Some(r.telegram_token.trim().to_string()),
                        Some(r.telegram_chat_id.trim().to_string()),
                    )
                } else {
                    (cfg.telegram_token.clone(), cfg.telegram_chat_id.clone())
                };

            EffectiveSiemConfig {
                webhook_url,
                webhook_token,
                syslog_host,
                syslog_port,
                syslog_proto,
                telegram_token,
                telegram_chat_id,
                min_severity: parse_severity(&r.min_severity),
            }
        }
        None => EffectiveSiemConfig {
            webhook_url: cfg.siem_webhook_url.clone(),
            webhook_token: cfg.siem_webhook_token.clone(),
            syslog_host: cfg.siem_syslog_host.clone(),
            syslog_port: cfg.siem_syslog_port,
            syslog_proto: cfg.siem_syslog_proto.clone(),
            telegram_token: cfg.telegram_token.clone(),
            telegram_chat_id: cfg.telegram_chat_id.clone(),
            min_severity: Severity::Low,
        },
    }
}

#[derive(Serialize)]
struct SiemSettingsDto {
    enabled: bool,
    webhook_url: String,
    has_webhook_token: bool,
    syslog_host: String,
    syslog_port: i32,
    syslog_proto: String,
    telegram_enabled: bool,
    has_telegram_token: bool,
    telegram_chat_id: String,
    min_severity: String,
    env_fallback_active: bool,
    updated_by: Option<String>,
    updated_at: DateTime<Utc>,
}

fn to_dto(r: SiemSettingsRow, state: &AppState) -> SiemSettingsDto {
    let env_fallback = state.config.siem_webhook_url.is_some()
        || state.config.siem_syslog_host.is_some()
        || state.config.telegram_token.is_some();
    SiemSettingsDto {
        enabled: r.enabled,
        webhook_url: r.webhook_url,
        has_webhook_token: !r.webhook_token.is_empty(),
        syslog_host: r.syslog_host,
        syslog_port: r.syslog_port,
        syslog_proto: r.syslog_proto,
        telegram_enabled: r.telegram_enabled,
        has_telegram_token: !r.telegram_token.is_empty(),
        telegram_chat_id: r.telegram_chat_id,
        min_severity: r.min_severity,
        env_fallback_active: env_fallback,
        updated_by: r.updated_by,
        updated_at: r.updated_at,
    }
}

async fn get_siem_settings(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<SiemSettingsDto>> {
    let row: SiemSettingsRow = sqlx::query_as(
        "SELECT enabled, webhook_url, webhook_token, syslog_host, syslog_port, syslog_proto, \
                telegram_enabled, telegram_token, telegram_chat_id, min_severity, updated_by, updated_at \
         FROM siem_settings WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(to_dto(row, &state)))
}

#[derive(Deserialize)]
struct UpdateSiemReq {
    enabled: bool,
    #[serde(default)]
    webhook_url: String,
    #[serde(default)]
    webhook_token: Option<String>,
    #[serde(default)]
    syslog_host: String,
    #[serde(default = "def_port")]
    syslog_port: i32,
    #[serde(default = "def_proto")]
    syslog_proto: String,
    #[serde(default)]
    telegram_enabled: bool,
    #[serde(default)]
    telegram_token: Option<String>,
    #[serde(default)]
    telegram_chat_id: String,
    #[serde(default = "def_sev")]
    min_severity: String,
}

fn def_port() -> i32 {
    514
}
fn def_proto() -> String {
    "udp".to_string()
}
fn def_sev() -> String {
    "low".to_string()
}

async fn update_siem_settings(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateSiemReq>,
) -> AppResult<Json<SiemSettingsDto>> {
    let proto = body.syslog_proto.trim().to_ascii_lowercase();
    if proto != "udp" && proto != "tcp" {
        return Err(AppError::BadRequest("syslog_proto must be 'udp' or 'tcp'".into()));
    }
    if !(1..=65535).contains(&body.syslog_port) {
        return Err(AppError::BadRequest("syslog_port must be 1..=65535".into()));
    }
    let sev = body.min_severity.trim().to_ascii_lowercase();
    if !matches!(sev.as_str(), "low" | "medium" | "high" | "critical") {
        return Err(AppError::BadRequest("Invalid min_severity".into()));
    }

    let existing: (String, String) = sqlx::query_as(
        "SELECT webhook_token, telegram_token FROM siem_settings WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await?;

    let next_webhook_tok = match body.webhook_token {
        Some(ref s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => existing.0,
    };
    let next_tg_tok = match body.telegram_token {
        Some(ref s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => existing.1,
    };

    let row: SiemSettingsRow = sqlx::query_as(
        "UPDATE siem_settings SET \
            enabled = $1, webhook_url = $2, webhook_token = $3, \
            syslog_host = $4, syslog_port = $5, syslog_proto = $6, \
            telegram_enabled = $7, telegram_token = $8, telegram_chat_id = $9, \
            min_severity = $10, updated_by = $11, updated_at = NOW() \
         WHERE id = 1 \
         RETURNING enabled, webhook_url, webhook_token, syslog_host, syslog_port, syslog_proto, \
                   telegram_enabled, telegram_token, telegram_chat_id, min_severity, updated_by, updated_at",
    )
    .bind(body.enabled)
    .bind(body.webhook_url.trim())
    .bind(&next_webhook_tok)
    .bind(body.syslog_host.trim())
    .bind(body.syslog_port)
    .bind(&proto)
    .bind(body.telegram_enabled)
    .bind(&next_tg_tok)
    .bind(body.telegram_chat_id.trim())
    .bind(&sev)
    .bind(&admin.username)
    .fetch_one(&state.db)
    .await?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "siem_settings_updated",
        "settings/siem",
        json!({
            "enabled": row.enabled,
            "syslog_host": row.syslog_host,
            "syslog_port": row.syslog_port,
            "syslog_proto": row.syslog_proto,
            "telegram_enabled": row.telegram_enabled,
            "min_severity": row.min_severity,
        }),
    )
    .await;

    Ok(Json(to_dto(row, &state)))
}

async fn test_siem_delivery(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let eff = load_effective_siem(&state).await;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default();

    let now = Utc::now();
    let rules = vec!["QORGAU-SIEM-TEST".to_string()];

    // 1. Webhook test
    let webhook_res = if let Some(url) = eff.webhook_url.as_deref() {
        let mut req = http.post(url).json(&json!({
            "id": 0,
            "event": "siem_test",
            "timestamp": now.to_rfc3339(),
            "agent_id": "qorgau-server",
            "hostname": "qorgau-dlp",
            "username": admin.username,
            "channel": "system",
            "severity": "low",
            "matched_rules": rules,
            "snippet": "Qorgau DLP SIEM connectivity test event",
        }));
        if let Some(tok) = eff.webhook_token.as_deref() {
            req = req.bearer_auth(tok);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => format!("ok ({})", resp.status()),
            Ok(resp) => format!("error (HTTP {})", resp.status()),
            Err(e) => format!("error ({e})"),
        }
    } else {
        "not_configured".to_string()
    };

    // 2. Syslog CEF test
    let syslog_res = if let Some(host) = eff.syslog_host.as_deref() {
        let msg = format_syslog_cef(
            &CefIncident {
                id: 0,
                ts: now,
                hostname: "qorgau-dlp",
                username: &admin.username,
                agent_id: "qorgau-server",
                channel: "system",
                severity: Severity::Low,
                rules: &rules,
                snippet: "Qorgau DLP Syslog CEF test event",
            },
            if eff.syslog_proto == "tcp" { 4096 } else { 1350 },
        );
        let addr = format!("{}:{}", host, eff.syslog_port);
        if eff.syslog_proto == "tcp" {
            let framed = format!("{} {}", msg.len(), msg);
            match TcpStream::connect(&addr).await {
                Ok(mut s) => match s.write_all(framed.as_bytes()).await {
                    Ok(_) => format!("ok (tcp://{addr})"),
                    Err(e) => format!("error ({e})"),
                },
                Err(e) => format!("error ({e})"),
            }
        } else {
            match UdpSocket::bind("0.0.0.0:0").await {
                Ok(sock) => match sock.send_to(msg.as_bytes(), &addr).await {
                    Ok(_) => format!("ok (udp://{addr})"),
                    Err(e) => format!("error ({e})"),
                },
                Err(e) => format!("error ({e})"),
            }
        }
    } else {
        "not_configured".to_string()
    };

    // 3. Telegram test
    let telegram_res = if let (Some(tok), Some(chat)) = (
        eff.telegram_token.as_deref(),
        eff.telegram_chat_id.as_deref(),
    ) {
        let url = format!("https://api.telegram.org/bot{tok}/sendMessage");
        match http
            .post(&url)
            .json(&json!({
                "chat_id": chat,
                "text": "<b>[QORGAU DLP | TEST]</b>\nПроверка связи с Telegram-каналом оповещений.",
                "parse_mode": "HTML"
            }))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => "ok".to_string(),
            Ok(resp) => format!("error (HTTP {})", resp.status()),
            Err(e) => format!("error ({e})"),
        }
    } else {
        "not_configured".to_string()
    };

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "siem_test_triggered",
        "settings/siem/test",
        json!({
            "webhook": webhook_res,
            "syslog": syslog_res,
            "telegram": telegram_res,
        }),
    )
    .await;

    Ok(Json(json!({
        "webhook": webhook_res,
        "syslog": syslog_res,
        "telegram": telegram_res,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_threshold_filtering_works() {
        assert!(severity_meets_min(Severity::Critical, Severity::Low));
        assert!(severity_meets_min(Severity::High, Severity::High));
        assert!(!severity_meets_min(Severity::Medium, Severity::High));
        assert!(!severity_meets_min(Severity::Low, Severity::Critical));
    }
}
