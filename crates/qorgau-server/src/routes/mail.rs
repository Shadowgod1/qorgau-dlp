use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::dlp::mask_text;
use qorgau_common::{Severity, WsDashboardEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::audit::write_audit;
use crate::auth::{verify_agent_token, AuthUser};
use crate::error::AppResult;
use crate::state::AppState;
use crate::workers::AlertJob;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mail/events", get(list_mail_events).post(ingest_mail_event))
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MailAttachmentDto {
    pub filename: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Deserialize)]
pub struct IngestMailReq {
    #[serde(default = "def_agent")]
    pub agent_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "def_dir")]
    pub direction: String,
    #[serde(default = "def_proto")]
    pub protocol: String,
    pub sender: String,
    #[serde(default)]
    pub recipients: Vec<String>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body_preview: String,
    #[serde(default)]
    pub attachments: Vec<MailAttachmentDto>,
}

fn def_agent() -> String {
    "mail-relay".to_string()
}
fn def_dir() -> String {
    "outbound".to_string()
}
fn def_proto() -> String {
    "smtp".to_string()
}

pub fn extract_mail_username(sender: &str) -> String {
    sender
        .split('@')
        .next()
        .unwrap_or(sender)
        .trim()
        .to_ascii_lowercase()
}

pub fn has_external_recipient(recipients: &[String], corp_domains: &[String]) -> bool {
    if corp_domains.is_empty() {
        return false;
    }
    recipients.iter().any(|r| {
        let domain = r.split('@').nth(1).unwrap_or("").trim().to_ascii_lowercase();
        !domain.is_empty() && !corp_domains.iter().any(|cd| cd == &domain)
    })
}

async fn ingest_mail_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<IngestMailReq>,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let username = if body.username.trim().is_empty() {
        extract_mail_username(&body.sender)
    } else {
        body.username.trim().to_string()
    };

    let direction = match body.direction.trim().to_ascii_lowercase().as_str() {
        "inbound" => "inbound",
        "internal" => "internal",
        _ => "outbound",
    };
    let protocol = match body.protocol.trim().to_ascii_lowercase().as_str() {
        "imap" => "imap",
        "outlook" => "outlook",
        "exchange" => "exchange",
        _ => "smtp",
    };

    let att_names: Vec<String> = body.attachments.iter().map(|a| a.filename.clone()).collect();
    let combined = format!(
        "From: {}\nTo: {}\nSubject: {}\nAttachments: {}\n{}",
        body.sender,
        body.recipients.join(", "),
        body.subject,
        att_names.join(", "),
        body.body_preview
    );

    let matches = state.policy.evaluate("mail", &combined);
    let mut blocked = false;
    let mut matched_rules: Vec<String> = Vec::new();
    let mut max_sev = Severity::Low;

    if !matches.is_empty() {
        blocked = matches.iter().any(|m| m.action == "block");
        matched_rules = matches.iter().map(|m| m.rule_name.clone()).collect();
        max_sev = matches.iter().map(|m| m.severity).max().unwrap_or(Severity::Medium);
    }

    let preview_masked = mask_text(&body.body_preview);
    let preview_short: String = preview_masked.chars().take(500).collect();

    let (mail_id,): (i64,) = sqlx::query_as(
        "INSERT INTO mail_events \
         (agent_id, username, direction, protocol, sender, recipients, subject, body_preview, attachments, blocked) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(body.agent_id.trim())
    .bind(&username)
    .bind(direction)
    .bind(protocol)
    .bind(body.sender.trim())
    .bind(json!(&body.recipients))
    .bind(body.subject.trim())
    .bind(&preview_short)
    .bind(json!(&body.attachments))
    .bind(blocked)
    .fetch_one(&state.db)
    .await?;

    let mut incident_id: Option<i64> = None;
    if !matched_rules.is_empty() {
        let snippet = format!(
            "[{}] {} -> {} | Тема: {} | Вложения: {}",
            protocol.to_uppercase(),
            body.sender,
            body.recipients.join(", "),
            body.subject,
            if att_names.is_empty() { "нет".to_string() } else { att_names.join(", ") }
        );
        let inc_res: Result<(i64, DateTime<Utc>), _> = sqlx::query_as(
            "INSERT INTO incidents (agent_id, hostname, username, channel, severity, status, matched_rules, snippet, meta) \
             VALUES ($1, $2, $3, 'mail', $4, 'open', $5, $6, $7) RETURNING id, created_at",
        )
        .bind(body.agent_id.trim())
        .bind(protocol)
        .bind(&username)
        .bind(max_sev.as_str())
        .bind(json!(&matched_rules))
        .bind(&snippet)
        .bind(json!({
            "mail_id": mail_id,
            "direction": direction,
            "protocol": protocol,
            "blocked": blocked,
            "attachments": body.attachments,
        }))
        .fetch_one(&state.db)
        .await;

        if let Ok((inc_id, ts)) = inc_res {
            incident_id = Some(inc_id);
            let _ = sqlx::query("UPDATE mail_events SET incident_id = $1 WHERE id = $2")
                .bind(inc_id)
                .bind(mail_id)
                .execute(&state.db)
                .await;

            write_audit(
                &state.db,
                &username,
                "agent",
                "mail_incident_created",
                &format!("incident:{inc_id}"),
                json!({ "mail_id": mail_id, "blocked": blocked, "rules": &matched_rules }),
            )
            .await;

            state.ws.broadcast_dashboard(WsDashboardEvent::Incident {
                id: inc_id,
                agent_id: body.agent_id.clone(),
                hostname: protocol.to_string(),
                username: username.clone(),
                channel: "mail".to_string(),
                severity: max_sev,
                snippet: snippet.chars().take(120).collect(),
            });

            let _ = state.alert_tx.try_send(AlertJob {
                incident_id: inc_id,
                ts,
                agent_id: body.agent_id,
                hostname: protocol.to_string(),
                username,
                channel: "mail".to_string(),
                severity: max_sev,
                rules: matched_rules.clone(),
                snippet,
            });
        }
    }

    Ok(Json(json!({
        "id": mail_id,
        "incident_id": incident_id,
        "action": if blocked { "block" } else if !matched_rules.is_empty() { "alert" } else { "allow" },
        "matched_rules": matched_rules,
    })))
}

#[derive(Deserialize)]
struct ListMailQ {
    username: Option<String>,
    direction: Option<String>,
    blocked: Option<bool>,
    #[serde(default = "def_limit")]
    limit: i64,
}
fn def_limit() -> i64 {
    100
}

#[derive(Serialize, sqlx::FromRow)]
struct MailEventRow {
    id: i64,
    agent_id: String,
    username: String,
    direction: String,
    protocol: String,
    sender: String,
    recipients: Value,
    subject: String,
    body_preview: String,
    attachments: Value,
    blocked: bool,
    incident_id: Option<i64>,
    created_at: DateTime<Utc>,
}

async fn list_mail_events(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListMailQ>,
) -> AppResult<Json<Vec<MailEventRow>>> {
    let rows: Vec<MailEventRow> = sqlx::query_as(
        "SELECT id, agent_id, username, direction, protocol, sender, recipients, subject, body_preview, attachments, blocked, incident_id, created_at \
         FROM mail_events \
         WHERE ($1::text IS NULL OR username = $1) \
           AND ($2::text IS NULL OR direction = $2) \
           AND ($3::boolean IS NULL OR blocked = $3) \
         ORDER BY created_at DESC LIMIT $4",
    )
    .bind(q.username.as_deref())
    .bind(q.direction.as_deref())
    .bind(q.blocked)
    .bind(q.limit.clamp(1, 500))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_helpers_work() {
        assert_eq!(extract_mail_username("Ivanov.A@company.kz"), "ivanov.a");
        let domains = vec!["company.kz".to_string()];
        assert!(!has_external_recipient(&["b@company.kz".into()], &domains));
        assert!(has_external_recipient(&["a@gmail.com".into()], &domains));
    }
}
