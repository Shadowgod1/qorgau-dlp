use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use qorgau_common::cef::{format_syslog_cef, CefIncident};
use qorgau_common::dlp::mask_text;
use qorgau_common::{Severity, WsDashboardEvent};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{mpsc, Semaphore};

use crate::audit::write_audit;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AlertJob {
    pub incident_id: i64,
    pub ts: DateTime<Utc>,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub channel: String,
    pub severity: Severity,
    pub rules: Vec<String>,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct OcrJob {
    pub shadow_id: i64,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub file_name: String,
    pub stored_path: PathBuf,
}

pub fn spawn_background_workers(
    state: Arc<AppState>,
    mut alert_rx: mpsc::Receiver<AlertJob>,
    mut ocr_rx: mpsc::Receiver<OcrJob>,
) {
    // 1. Alert dispatcher worker (Telegram + SIEM Webhook + Syslog CEF)
    let st_alerts = state.clone();
    tokio::spawn(async move {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        while let Some(job) = alert_rx.recv().await {
            let masked = mask_text(&job.snippet);

            // Telegram alert (PII-masked, plain HTML-escaped so Windows paths never fail)
            if let (Some(tok), Some(chat)) = (
                st_alerts.config.telegram_token.as_deref(),
                st_alerts.config.telegram_chat_id.as_deref(),
            ) {
                let text = format!(
                    "<b>[QORGAU DLP | {}]</b>\nПользователь: <code>{}</code> ({})\nКанал: <code>{}</code>\nПравила: {}\nФрагмент: <code>{}</code>",
                    escape_html(&job.severity.as_str().to_uppercase()),
                    escape_html(&job.username),
                    escape_html(&job.hostname),
                    escape_html(&job.channel),
                    escape_html(&job.rules.join(", ")),
                    escape_html(&masked.chars().take(160).collect::<String>()),
                );
                let url = format!("https://api.telegram.org/bot{tok}/sendMessage");
                if let Err(e) = http
                    .post(&url)
                    .json(&json!({ "chat_id": chat, "text": text, "parse_mode": "HTML" }))
                    .send()
                    .await
                {
                    tracing::warn!(error = %e, "Telegram alert failed");
                }
            }

            // SIEM JSON Webhook
            if let Some(url) = st_alerts.config.siem_webhook_url.as_deref() {
                let mut req = http.post(url).json(&json!({
                    "id": job.incident_id,
                    "timestamp": job.ts.to_rfc3339(),
                    "agent_id": job.agent_id,
                    "hostname": job.hostname,
                    "username": job.username,
                    "channel": job.channel,
                    "severity": job.severity.as_str(),
                    "matched_rules": job.rules,
                    "snippet": masked,
                }));
                if let Some(tok) = st_alerts.config.siem_webhook_token.as_deref() {
                    req = req.bearer_auth(tok);
                }
                if let Err(e) = req.send().await {
                    tracing::warn!(error = %e, "SIEM webhook failed");
                }
            }

            // Syslog + ArcSight CEF (ОЦИБ)
            if let Some(host) = st_alerts.config.siem_syslog_host.as_deref() {
                let msg = format_syslog_cef(
                    &CefIncident {
                        id: job.incident_id,
                        ts: job.ts,
                        hostname: &job.hostname,
                        username: &job.username,
                        agent_id: &job.agent_id,
                        channel: &job.channel,
                        severity: job.severity,
                        rules: &job.rules,
                        snippet: &masked,
                    },
                    if st_alerts.config.siem_syslog_proto == "tcp" {
                        4096
                    } else {
                        1350
                    },
                );
                let addr = format!("{}:{}", host, st_alerts.config.siem_syslog_port);
                if st_alerts.config.siem_syslog_proto == "tcp" {
                    let framed = format!("{} {}", msg.len(), msg);
                    match TcpStream::connect(&addr).await {
                        Ok(mut stream) => {
                            let _ = stream.write_all(framed.as_bytes()).await;
                        }
                        Err(e) => tracing::warn!(error = %e, %addr, "Syslog TCP failed"),
                    }
                } else if let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await {
                    if let Err(e) = sock.send_to(msg.as_bytes(), &addr).await {
                        tracing::warn!(error = %e, %addr, "Syslog UDP failed");
                    }
                }
            }
        }
    });

    // 2. Bounded async OCR worker (runs `tesseract` subprocess without blocking HTTP handlers)
    let st_ocr = state.clone();
    let sem = Arc::new(Semaphore::new(state.config.ocr_concurrency));
    tokio::spawn(async move {
        while let Some(job) = ocr_rx.recv().await {
            let permit = match sem.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let st = st_ocr.clone();
            tokio::spawn(async move {
                let _permit = permit;
                run_ocr_job(&st, job).await;
            });
        }
    });

    // 3. Periodic agent online status sweep (every 60s, unified 3-min threshold)
    let st_online = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let _ = sqlx::query(
                "UPDATE agents SET is_online = (last_seen >= NOW() - INTERVAL '3 minutes')",
            )
            .execute(&st_online.db)
            .await;
        }
    });

    // 4. Daily retention cleanup (DB rows + disk files, respecting Legal Hold!)
    let st_clean = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600 * 6));
        interval.tick().await; // skip immediate run on boot
        loop {
            interval.tick().await;
            run_retention_cleanup(&st_clean).await;
        }
    });

    // 5. Risk-score threshold monitor (iBatyr 5.6 parity: alerts when employee risk crosses threshold or grows)
    let st_risk = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(180));
        interval.tick().await;
        loop {
            interval.tick().await;
            check_risk_thresholds(&st_risk).await;
        }
    });
}

async fn run_ocr_job(state: &AppState, job: OcrJob) {
    let output = tokio::process::Command::new("tesseract")
        .arg(&job.stored_path)
        .arg("stdout")
        .arg("-l")
        .arg("rus+kaz+eng")
        .output()
        .await;

    let Ok(out) = output else {
        tracing::debug!("tesseract binary not available; skipping OCR");
        return;
    };
    if !out.status.success() {
        return;
    }

    let text: String = String::from_utf8_lossy(&out.stdout)
        .chars()
        .take(4000)
        .collect();
    if text.trim().is_empty() {
        return;
    }

    let matches = state.policy.evaluate("usb", &text);
    if matches.is_empty() {
        return;
    }

    let max_sev = matches
        .iter()
        .map(|m| m.severity)
        .max()
        .unwrap_or(Severity::Medium);
    let rule_names: Vec<String> = matches.iter().map(|m| m.rule_name.clone()).collect();
    let snippet: String = text.chars().take(500).collect();

    let inc_res: Result<(i64, DateTime<Utc>), _> = sqlx::query_as(
        "INSERT INTO incidents (agent_id, hostname, username, channel, severity, status, matched_rules, snippet, meta) \
         VALUES ($1, $2, $3, 'usb', $4, 'open', $5, $6, $7) RETURNING id, created_at",
    )
    .bind(&job.agent_id)
    .bind(&job.hostname)
    .bind(&job.username)
    .bind(max_sev.as_str())
    .bind(json!(&rule_names))
    .bind(&snippet)
    .bind(json!({ "source": "ocr", "file_name": job.file_name, "shadow_id": job.shadow_id }))
    .fetch_one(&state.db)
    .await;

    if let Ok((inc_id, ts)) = inc_res {
        // Safely link shadow copy directly by shadow_id (no global-latest race condition!)
        let _ = sqlx::query("UPDATE shadow_copies SET incident_id = $1 WHERE id = $2")
            .bind(inc_id)
            .bind(job.shadow_id)
            .execute(&state.db)
            .await;

        write_audit(
            &state.db,
            "system",
            "system",
            "incident_created_ocr",
            &format!("incident:{inc_id}"),
            json!({ "shadow_id": job.shadow_id, "rules": &rule_names }),
        )
        .await;

        state.ws.broadcast_dashboard(WsDashboardEvent::Incident {
            id: inc_id,
            agent_id: job.agent_id.clone(),
            hostname: job.hostname.clone(),
            username: job.username.clone(),
            channel: "usb".to_string(),
            severity: max_sev,
            snippet: snippet.chars().take(120).collect(),
        });

        let _ = state.alert_tx.try_send(AlertJob {
            incident_id: inc_id,
            ts,
            agent_id: job.agent_id,
            hostname: job.hostname,
            username: job.username,
            channel: "usb".to_string(),
            severity: max_sev,
            rules: rule_names,
            snippet,
        });
    }
}

async fn run_retention_cleanup(state: &AppState) {
    let cfg = &state.config;
    purge_media_respecting_legal_hold(&state.db, "screenshots", "file_path", cfg.screenshots_retention_days).await;
    purge_shadow_respecting_legal_hold(&state.db, cfg.shadow_retention_days).await;
    purge_media_respecting_legal_hold(&state.db, "audio_recordings", "file_path", cfg.audio_retention_days).await;
    purge_media_respecting_legal_hold(&state.db, "webcam_photos", "file_path", cfg.webcam_retention_days).await;

    let tables = [
        "user_sessions",
        "usb_events",
        "web_activity",
        "app_activity",
        "messenger_events",
        "clipboard_events",
        "network_connections",
        "print_jobs",
    ];
    for tbl in tables {
        // Protect rows within ±15 min of any Legal Hold incident for the same agent
        let q = format!(
            "DELETE FROM {tbl} t WHERE t.created_at < NOW() - ($1 || ' days')::INTERVAL \
             AND NOT EXISTS ( \
                SELECT 1 FROM incidents i \
                WHERE i.legal_hold = TRUE AND i.agent_id = t.agent_id \
                  AND t.created_at BETWEEN i.created_at - INTERVAL '15 minutes' AND i.created_at + INTERVAL '15 minutes' \
             )"
        );
        let _ = sqlx::query(&q)
            .bind(cfg.data_retention_days)
            .execute(&state.db)
            .await;
    }
    tracing::info!("Completed scheduled retention cleanup (Legal Hold protected)");
}

async fn purge_media_respecting_legal_hold(
    db: &sqlx::PgPool,
    table: &str,
    path_col: &str,
    days: i64,
) {
    let select_sql = format!(
        "SELECT t.id, t.{path_col} FROM {table} t \
         WHERE t.created_at < NOW() - ($1 || ' days')::INTERVAL \
           AND NOT EXISTS ( \
              SELECT 1 FROM incidents i \
              WHERE i.legal_hold = TRUE AND i.agent_id = t.agent_id \
                AND t.created_at BETWEEN i.created_at - INTERVAL '15 minutes' AND i.created_at + INTERVAL '15 minutes' \
           ) \
         LIMIT 2000"
    );
    let Ok(rows): Result<Vec<(i64, String)>, _> =
        sqlx::query_as(&select_sql).bind(days).fetch_all(db).await
    else {
        return;
    };
    for (id, path) in rows {
        let _ = tokio::fs::remove_file(&path).await;
        let del_sql = format!("DELETE FROM {table} WHERE id = $1");
        let _ = sqlx::query(&del_sql).bind(id).execute(db).await;
    }
}

async fn purge_shadow_respecting_legal_hold(db: &sqlx::PgPool, days: i64) {
    let Ok(rows): Result<Vec<(i64, String)>, _> = sqlx::query_as(
        "SELECT s.id, s.stored_path FROM shadow_copies s \
         LEFT JOIN incidents i ON i.id = s.incident_id \
         WHERE s.created_at < NOW() - ($1 || ' days')::INTERVAL \
           AND COALESCE(i.legal_hold, FALSE) = FALSE \
         LIMIT 2000",
    )
    .bind(days)
    .fetch_all(db)
    .await
    else {
        return;
    };
    for (id, path) in rows {
        let _ = tokio::fs::remove_file(&path).await;
        let _ = sqlx::query("DELETE FROM shadow_copies WHERE id = $1")
            .bind(id)
            .execute(db)
            .await;
    }
}

async fn check_risk_thresholds(state: &AppState) {
    let Ok((threshold, notify_growth)): Result<(i32, bool), _> = sqlx::query_as(
        "SELECT risk_threshold, notify_on_risk_growth FROM alert_settings WHERE id = 1",
    )
    .fetch_one(&state.db)
    .await
    else {
        return;
    };

    let Ok(users): Result<Vec<(String, i64)>, _> = sqlx::query_as(
        "SELECT username, \
                LEAST(100, \
                    COUNT(*) FILTER (WHERE severity = 'critical') * 25 + \
                    COUNT(*) FILTER (WHERE severity = 'high') * 12 + \
                    COUNT(*) FILTER (WHERE severity = 'medium') * 5 + \
                    COUNT(*) FILTER (WHERE severity = 'low') * 1 \
                ) AS risk_score \
         FROM incidents \
         WHERE created_at >= NOW() - INTERVAL '30 days' AND status != 'false_positive' \
         GROUP BY username \
         HAVING LEAST(100, \
                    COUNT(*) FILTER (WHERE severity = 'critical') * 25 + \
                    COUNT(*) FILTER (WHERE severity = 'high') * 12 + \
                    COUNT(*) FILTER (WHERE severity = 'medium') * 5 + \
                    COUNT(*) FILTER (WHERE severity = 'low') * 1 \
                ) >= $1",
    )
    .bind(threshold as i64)
    .fetch_all(&state.db)
    .await
    else {
        return;
    };

    for (username, score) in users {
        let prev: Option<(i32,)> =
            sqlx::query_as("SELECT last_notified_score FROM user_risk_alerts WHERE username = $1")
                .bind(&username)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

        let should_notify = match prev {
            None => true,
            Some((last,)) => notify_growth && (score as i32) >= last + 10,
        };

        if should_notify {
            let _ = sqlx::query(
                "INSERT INTO user_risk_alerts (username, last_notified_score, last_notified_at) \
                 VALUES ($1, $2, NOW()) \
                 ON CONFLICT (username) DO UPDATE SET last_notified_score = $2, last_notified_at = NOW()",
            )
            .bind(&username)
            .bind(score as i32)
            .execute(&state.db)
            .await;

            state.ws.broadcast_dashboard(WsDashboardEvent::RiskAlert {
                username: username.clone(),
                risk_score: score,
                threshold,
            });

            let _ = state.alert_tx.try_send(AlertJob {
                incident_id: 0,
                ts: Utc::now(),
                agent_id: "risk-engine".to_string(),
                hostname: "server".to_string(),
                username: username.clone(),
                channel: "risk_threshold".to_string(),
                severity: if score >= 80 { Severity::Critical } else { Severity::High },
                rules: vec![format!("Превышен порог риска ({score}/{threshold})")],
                snippet: format!("Риск сотрудника {username} достиг {score} из 100 (порог: {threshold})"),
            });
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
