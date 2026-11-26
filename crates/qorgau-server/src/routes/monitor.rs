use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Multipart, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use qorgau_common::dlp::DlpMatch;
use qorgau_common::{
    validate_agent_id, AgentConfigDto, EventBatchRequest, EventBatchResponse, PreventionRulesDto,
    RegisterRequest, Severity, WsDashboardEvent,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::audit::write_audit;
use crate::auth::verify_agent_token;
use crate::error::{AppError, AppResult};
use crate::routes::configs::fetch_effective_config_for_agent;
use crate::state::AppState;
use crate::storage::prepare_safe_storage_path;
use crate::workers::{AlertJob, OcrJob};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/monitor/register", post(register_agent))
        .route("/monitor/events", post(receive_events))
        .route("/monitor/heartbeat", post(heartbeat))
        .route("/monitor/config", get(get_agent_config))
        .route("/monitor/prevention-rules", get(get_prevention_rules))
        .route("/monitor/policy-sync", get(policy_sync))
        .route("/monitor/discovery", post(receive_discovery))
        .route("/monitor/inventory", post(receive_inventory))
        .route("/monitor/screenshot", post(upload_screenshot))
        .route("/monitor/live-frame", post(upload_live_frame))
        .route("/monitor/shadow", post(upload_shadow))
        .route("/monitor/audio", post(upload_audio))
        .route("/monitor/webcam", post(upload_webcam))
}

#[derive(Deserialize)]
struct AgentIdQuery {
    agent_id: String,
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    if let Some(ip) = headers
        .get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = ip.split(',').next() {
            return first.trim().to_string();
        }
    }
    peer.ip().to_string()
}

async fn register_agent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<RegisterRequest>,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&body.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let ip = client_ip(&headers, peer);
    sqlx::query(
        "INSERT INTO agents (agent_id, hostname, username, ip_address, os_version, agent_version, is_online, last_seen) \
         VALUES ($1, $2, $3, $4, $5, $6, TRUE, NOW()) \
         ON CONFLICT (agent_id) DO UPDATE SET \
            hostname = EXCLUDED.hostname, \
            username = EXCLUDED.username, \
            ip_address = EXCLUDED.ip_address, \
            os_version = EXCLUDED.os_version, \
            agent_version = EXCLUDED.agent_version, \
            is_online = TRUE, \
            last_seen = NOW()",
    )
    .bind(&body.agent_id)
    .bind(&body.hostname)
    .bind(&body.username)
    .bind(&ip)
    .bind(&body.os_version)
    .bind(&body.agent_version)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "status": "registered", "agent_id": body.agent_id })))
}

async fn heartbeat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<AgentIdQuery>,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&q.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    sqlx::query("UPDATE agents SET is_online = TRUE, last_seen = NOW() WHERE agent_id = $1")
        .bind(&q.agent_id)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({ "status": "ok" })))
}

async fn get_agent_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<AgentIdQuery>,
) -> AppResult<Json<AgentConfigDto>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&q.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let cfg = fetch_effective_config_for_agent(&state.db, &q.agent_id).await?;
    Ok(Json(cfg))
}

/// Returns active prevention lists (USB whitelist/blocklist, blocked apps, blocked domains) for an agent.
async fn get_prevention_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<AgentIdQuery>,
) -> AppResult<Json<PreventionRulesDto>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&q.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let cfg = fetch_effective_config_for_agent(&state.db, &q.agent_id).await?;

    let usb_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT serial_number, status FROM usb_devices")
            .fetch_all(&state.db)
            .await?;
    let mut allowed_usb_serials = Vec::new();
    let mut blocked_usb_serials = Vec::new();
    for (serial, st) in usb_rows {
        if st == "allowed" {
            allowed_usb_serials.push(serial);
        } else if st == "blocked" {
            blocked_usb_serials.push(serial);
        }
    }

    let app_rows: Vec<(String,)> =
        sqlx::query_as("SELECT pattern FROM blocked_apps WHERE enabled = TRUE AND action = 'block'")
            .fetch_all(&state.db)
            .await?;
    let domain_rows: Vec<(String,)> =
        sqlx::query_as("SELECT domain FROM blocked_domains WHERE enabled = TRUE")
            .fetch_all(&state.db)
            .await?;

    Ok(Json(PreventionRulesDto {
        usb_whitelist_mode: cfg.usb_whitelist_mode,
        allowed_usb_serials,
        blocked_usb_serials,
        blocked_apps: app_rows.into_iter().map(|r| r.0).collect(),
        blocked_domains: domain_rows.into_iter().map(|r| r.0).collect(),
    }))
}

#[derive(Deserialize)]
struct DiscoveryUploadItem {
    file_path: String,
    file_size: i64,
    sha256: String,
    matched_rules: Vec<String>,
    severity: String,
    snippet: String,
}

#[derive(Deserialize)]
struct DiscoveryUploadReq {
    agent_id: String,
    hostname: String,
    username: String,
    findings: Vec<DiscoveryUploadItem>,
}

async fn receive_discovery(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DiscoveryUploadReq>,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&body.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let mut saved = 0usize;
    for item in body.findings.into_iter().take(500) {
        let sev = Severity::from_str_loose(&item.severity);
        let res = sqlx::query(
            "INSERT INTO discovery_findings (agent_id, hostname, username, file_path, file_size, sha256, matched_rules, severity, snippet, status, scanned_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'open',NOW()) \
             ON CONFLICT (agent_id, file_path) DO UPDATE SET \
                file_size = EXCLUDED.file_size, \
                sha256 = EXCLUDED.sha256, \
                matched_rules = EXCLUDED.matched_rules, \
                severity = EXCLUDED.severity, \
                snippet = EXCLUDED.snippet, \
                scanned_at = NOW()",
        )
        .bind(&body.agent_id)
        .bind(&body.hostname)
        .bind(&body.username)
        .bind(&item.file_path)
        .bind(item.file_size)
        .bind(&item.sha256)
        .bind(json!(&item.matched_rules))
        .bind(sev.as_str())
        .bind(item.snippet.chars().take(400).collect::<String>())
        .execute(&state.db)
        .await;
        if res.is_ok() {
            saved += 1;
        }
    }
    Ok(Json(json!({ "status": "ok", "saved": saved })))
}

#[derive(Deserialize)]
struct SoftwareUploadItem {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    publisher: String,
    #[serde(default)]
    install_date: String,
}

#[derive(Deserialize)]
struct InventoryUploadReq {
    agent_id: String,
    hostname: String,
    username: String,
    cpu_model: String,
    cpu_cores: i32,
    ram_total_mb: i64,
    disk_total_gb: f64,
    disk_free_gb: f64,
    os_build: String,
    software: Vec<SoftwareUploadItem>,
}

async fn receive_inventory(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<InventoryUploadReq>,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&body.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    sqlx::query(
        "INSERT INTO agent_inventory (agent_id, hostname, username, cpu_model, cpu_cores, ram_total_mb, disk_total_gb, disk_free_gb, os_build, updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW()) \
         ON CONFLICT (agent_id) DO UPDATE SET \
            hostname=EXCLUDED.hostname, username=EXCLUDED.username, cpu_model=EXCLUDED.cpu_model, \
            cpu_cores=EXCLUDED.cpu_cores, ram_total_mb=EXCLUDED.ram_total_mb, \
            disk_total_gb=EXCLUDED.disk_total_gb, disk_free_gb=EXCLUDED.disk_free_gb, \
            os_build=EXCLUDED.os_build, updated_at=NOW()",
    )
    .bind(&body.agent_id)
    .bind(&body.hostname)
    .bind(&body.username)
    .bind(&body.cpu_model)
    .bind(body.cpu_cores)
    .bind(body.ram_total_mb)
    .bind(body.disk_total_gb)
    .bind(body.disk_free_gb)
    .bind(&body.os_build)
    .execute(&state.db)
    .await?;

    let mut current_names: Vec<String> = Vec::with_capacity(body.software.len());
    for sw in body.software.into_iter().take(1000) {
        let name = sw.name.trim();
        if name.is_empty() {
            continue;
        }
        current_names.push(name.to_string());
        let _ = sqlx::query(
            "INSERT INTO installed_software (agent_id, name, version, publisher, install_date, status, first_seen, last_seen) \
             VALUES ($1,$2,$3,$4,$5,'installed',NOW(),NOW()) \
             ON CONFLICT (agent_id, name) DO UPDATE SET \
                version=EXCLUDED.version, publisher=EXCLUDED.publisher, \
                install_date=EXCLUDED.install_date, status='installed', last_seen=NOW()",
        )
        .bind(&body.agent_id)
        .bind(name)
        .bind(sw.version.trim())
        .bind(sw.publisher.trim())
        .bind(sw.install_date.trim())
        .execute(&state.db)
        .await;
    }

    if !current_names.is_empty() {
        let _ = sqlx::query(
            "UPDATE installed_software SET status = 'removed' \
             WHERE agent_id = $1 AND status = 'installed' AND NOT (name = ANY($2))",
        )
        .bind(&body.agent_id)
        .bind(&current_names)
        .execute(&state.db)
        .await;
    }

    Ok(Json(json!({ "status": "ok", "software_count": current_names.len() })))
}

async fn policy_sync(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, channel, action, severity, pattern FROM policies WHERE enabled = TRUE ORDER BY id",
    )
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, channel, action, severity, pattern)| {
            json!({
                "id": id,
                "name": name,
                "channel": channel,
                "action": action,
                "severity": severity,
                "pattern": pattern
            })
        })
        .collect();
    Ok(Json(json!({ "policies": items })))
}

async fn receive_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(batch): Json<EventBatchRequest>,
) -> AppResult<Json<EventBatchResponse>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&batch.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    if batch.events.len() > 2000 {
        return Err(AppError::BadRequest("Batch size exceeds 2000 events".to_string()));
    }

    let _ = sqlx::query("UPDATE agents SET is_online = TRUE, last_seen = NOW() WHERE agent_id = $1")
        .bind(&batch.agent_id)
        .execute(&state.db)
        .await;

    let agent_cfg = fetch_effective_config_for_agent(&state.db, &batch.agent_id)
        .await
        .unwrap_or_default();

    let mut processed = 0usize;
    let mut incidents_created = 0usize;
    let mut blocked_channels: Vec<String> = Vec::new();

    for ev in &batch.events {
        processed += 1;
        let d = &ev.data;
        let ts = ev.timestamp;

        match ev.event_type.as_str() {
            "session" => {
                let _ = sqlx::query(
                    "INSERT INTO user_sessions (agent_id, hostname, username, ip_address, event_type, logon_type, meta, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.hostname)
                .bind(&batch.username)
                .bind(jstr(d, "ip_address"))
                .bind(jstr(d, "event_type"))
                .bind(jstr(d, "logon_type"))
                .bind(d)
                .bind(ts)
                .execute(&state.db)
                .await;
            }
            "usb" => {
                let action = jstr(d, "action");
                let device_name = jstr(d, "device_name");
                let file_path = jopt_str(d, "file_path");
                let content_snippet = jstr(d, "content_snippet");
                let sha256 = jopt_str(d, "sha256");

                let _ = sqlx::query(
                    "INSERT INTO usb_events (agent_id, username, device_id, device_name, action, drive_letter, file_path, size_gb, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(jstr(d, "device_id"))
                .bind(&device_name)
                .bind(&action)
                .bind(jopt_str(d, "drive_letter"))
                .bind(&file_path)
                .bind(d.get("size_gb").and_then(|v| v.as_f64()))
                .bind(ts)
                .execute(&state.db)
                .await;

                // Evaluate BOTH file_path AND extracted content_snippet (fixes legacy bug!)
                let mut combined = String::new();
                if let Some(fp) = &file_path {
                    combined.push_str(fp);
                    combined.push('\n');
                }
                if agent_cfg.usb_analyze_content && !content_snippet.is_empty() {
                    combined.push_str(&content_snippet);
                }

                let mut matches = state.policy.evaluate("usb", &combined);

                // Executable copied to USB check
                if agent_cfg.usb_alert_executables {
                    if let Some(fp) = &file_path {
                        let lower = fp.to_ascii_lowercase();
                        if [".exe", ".bat", ".ps1", ".cmd", ".vbs", ".msi", ".dll"]
                            .iter()
                            .any(|ext| lower.ends_with(ext))
                        {
                            matches.push(DlpMatch {
                                rule_name: "Копирование исполняемого файла на USB".to_string(),
                                severity: Severity::High,
                                action: "alert".to_string(),
                                matched_sample: fp.clone(),
                            });
                        }
                    }
                }

                if agent_cfg.usb_alert_on_connect && action == "connected" {
                    matches.push(DlpMatch {
                        rule_name: "Подключение USB-носителя".to_string(),
                        severity: Severity::Low,
                        action: "log".to_string(),
                        matched_sample: device_name.clone(),
                    });
                }

                if !matches.is_empty() {
                    let snippet = if !content_snippet.is_empty() {
                        content_snippet.chars().take(400).collect()
                    } else {
                        file_path.clone().unwrap_or(device_name)
                    };
                    if let Some((inc_id, blocked)) = create_incident(
                        &state,
                        &batch,
                        "usb",
                        &matches,
                        &snippet,
                        d.clone(),
                    )
                    .await
                    {
                        incidents_created += 1;
                        if blocked {
                            blocked_channels.push("usb".to_string());
                        }
                        // Back-link any shadow copy that arrived slightly earlier with same sha256
                        if let Some(hash) = sha256 {
                            let _ = sqlx::query(
                                "UPDATE shadow_copies SET incident_id = $1 \
                                 WHERE agent_id = $2 AND sha256 = $3 AND incident_id IS NULL",
                            )
                            .bind(inc_id)
                            .bind(&batch.agent_id)
                            .bind(&hash)
                            .execute(&state.db)
                            .await;
                        }
                    }
                }
            }
            "app" => {
                let app_name = jstr(d, "app_name");
                let window_title = jstr(d, "window_title");
                let prod = state.policy.classify_productivity(&app_name, &window_title);

                let _ = sqlx::query(
                    "INSERT INTO app_activity (agent_id, username, app_name, window_title, exe_path, duration_sec, active_sec, idle_sec, productivity, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(&app_name)
                .bind(&window_title)
                .bind(jopt_str(d, "exe_path"))
                .bind(jint(d, "duration_sec"))
                .bind(jint(d, "active_sec"))
                .bind(jint(d, "idle_sec"))
                .bind(&prod)
                .bind(ts)
                .execute(&state.db)
                .await;
            }
            "web" => {
                let url = jstr(d, "url");
                let domain = jstr(d, "domain").to_ascii_lowercase();
                let blocked_entry = state.policy.check_domain_blocked(&domain);
                let is_blocked = blocked_entry.is_some();
                let category = blocked_entry
                    .as_ref()
                    .map(|e| e.category.as_str())
                    .unwrap_or_else(|| state.policy.classify_web_category(&domain));

                let _ = sqlx::query(
                    "INSERT INTO web_activity (agent_id, username, url, domain, category, blocked, browser, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(&url)
                .bind(&domain)
                .bind(category)
                .bind(is_blocked)
                .bind(jopt_str(d, "browser"))
                .bind(ts)
                .execute(&state.db)
                .await;

                let mut matches = state.policy.evaluate("web", &url);
                if is_blocked {
                    matches.push(DlpMatch {
                        rule_name: format!("Запрещённый домен ({category})"),
                        severity: Severity::Medium,
                        action: "block".to_string(),
                        matched_sample: domain.clone(),
                    });
                } else {
                    let cat_alert = match category {
                        "social" => agent_cfg.web_alert_social,
                        "adult" => agent_cfg.web_alert_adult,
                        "gambling" => agent_cfg.web_alert_gambling,
                        "streaming" => agent_cfg.web_alert_streaming,
                        "crypto" => agent_cfg.web_alert_crypto,
                        _ => false,
                    };
                    if cat_alert {
                        matches.push(DlpMatch {
                            rule_name: format!("Нежелательная категория веб ({category})"),
                            severity: Severity::Low,
                            action: "alert".to_string(),
                            matched_sample: domain.clone(),
                        });
                    }
                }

                if !matches.is_empty() {
                    if let Some((_, blocked)) =
                        create_incident(&state, &batch, "web", &matches, &url, d.clone()).await
                    {
                        incidents_created += 1;
                        if blocked {
                            blocked_channels.push("web".to_string());
                        }
                    }
                }
            }
            "messenger" => {
                let content = jstr(d, "content");
                let matches = state.policy.evaluate("messenger", &content);
                let flagged = !matches.is_empty();
                let rules_str = if flagged {
                    Some(
                        matches
                            .iter()
                            .map(|m| m.rule_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                } else {
                    None
                };

                let _ = sqlx::query(
                    "INSERT INTO messenger_events (agent_id, username, messenger, direction, contact, content, has_file, file_name, flagged, matched_rules, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(jstr(d, "messenger"))
                .bind(jstr(d, "direction"))
                .bind(jopt_str(d, "contact"))
                .bind(&content)
                .bind(d.get("has_file").and_then(|v| v.as_bool()).unwrap_or(false))
                .bind(jopt_str(d, "file_name"))
                .bind(flagged)
                .bind(&rules_str)
                .bind(ts)
                .execute(&state.db)
                .await;

                if flagged {
                    if let Some((_, blocked)) =
                        create_incident(&state, &batch, "messenger", &matches, &content, d.clone())
                            .await
                    {
                        incidents_created += 1;
                        if blocked {
                            blocked_channels.push("messenger".to_string());
                        }
                    }
                }
            }
            "clipboard" => {
                let content = jstr(d, "content");
                let matches = state.policy.evaluate("clipboard", &content);
                let flagged = !matches.is_empty();
                let rules_str = if flagged {
                    Some(
                        matches
                            .iter()
                            .map(|m| m.rule_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                } else {
                    None
                };

                let _ = sqlx::query(
                    "INSERT INTO clipboard_events (agent_id, username, content, source_app, flagged, matched_rules, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(&content)
                .bind(jstr(d, "source_app"))
                .bind(flagged)
                .bind(&rules_str)
                .bind(ts)
                .execute(&state.db)
                .await;

                if flagged {
                    if let Some((_, blocked)) =
                        create_incident(&state, &batch, "clipboard", &matches, &content, d.clone())
                            .await
                    {
                        incidents_created += 1;
                        if blocked {
                            blocked_channels.push("clipboard".to_string());
                        }
                    }
                }
            }
            "network" => {
                let remote_ip = jstr(d, "remote_ip");
                let remote_port = jint(d, "remote_port");
                let process_name = jstr(d, "process_name");

                let _ = sqlx::query(
                    "INSERT INTO network_connections (agent_id, username, local_ip, local_port, remote_ip, remote_port, protocol, status, process_name, exe_path, bytes_sent, bytes_recv, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(jstr(d, "local_ip"))
                .bind(jint(d, "local_port"))
                .bind(&remote_ip)
                .bind(remote_port)
                .bind(jstr(d, "protocol"))
                .bind(jstr(d, "status"))
                .bind(&process_name)
                .bind(jopt_str(d, "exe_path"))
                .bind(d.get("bytes_sent").and_then(|v| v.as_i64()).unwrap_or(0))
                .bind(d.get("bytes_recv").and_then(|v| v.as_i64()).unwrap_or(0))
                .bind(ts)
                .execute(&state.db)
                .await;

                // Apply AgentConfig network alert rules (previously dead config!)
                let mut matches = Vec::new();
                if agent_cfg.net_block_tor
                    && (matches!(remote_port, 9001 | 9030 | 9050 | 9150)
                        || process_name.to_ascii_lowercase().contains("tor"))
                {
                    matches.push(DlpMatch {
                        rule_name: "Подозрение на трафик Tor".to_string(),
                        severity: Severity::High,
                        action: "alert".to_string(),
                        matched_sample: format!("{process_name} -> {remote_ip}:{remote_port}"),
                    });
                } else if agent_cfg.net_alert_sensitive_ports
                    && matches!(remote_port, 21 | 22 | 23 | 445 | 3389 | 5900)
                {
                    matches.push(DlpMatch {
                        rule_name: format!("Соединение на чувствительный порт {remote_port}"),
                        severity: Severity::Medium,
                        action: "alert".to_string(),
                        matched_sample: format!("{process_name} -> {remote_ip}:{remote_port}"),
                    });
                }

                if !matches.is_empty() {
                    let snip = format!("{process_name} -> {remote_ip}:{remote_port}");
                    if create_incident(&state, &batch, "network", &matches, &snip, d.clone())
                        .await
                        .is_some()
                    {
                        incidents_created += 1;
                    }
                }
            }
            "print" => {
                let doc = jstr(d, "document_name");
                let _ = sqlx::query(
                    "INSERT INTO print_jobs (agent_id, username, printer_name, document_name, pages, size_bytes, status, created_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(jstr(d, "printer_name"))
                .bind(&doc)
                .bind(jint(d, "pages"))
                .bind(d.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0))
                .bind(jstr(d, "status"))
                .bind(ts)
                .execute(&state.db)
                .await;

                let matches = state.policy.evaluate("print", &doc);
                if !matches.is_empty()
                    && create_incident(&state, &batch, "print", &matches, &doc, d.clone())
                        .await
                        .is_some()
                {
                    incidents_created += 1;
                }
            }
            "file" => {
                let fpath = jstr(d, "file_path");
                let content_snippet = jstr(d, "content_snippet");
                let combined = format!("{fpath}\n{content_snippet}");
                let matches = state.policy.evaluate("file", &combined);
                let flagged = !matches.is_empty();
                let rules_str = if flagged {
                    Some(
                        matches
                            .iter()
                            .map(|m| m.rule_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                } else {
                    None
                };

                let _ = sqlx::query(
                    "INSERT INTO file_events (agent_id, username, action, file_path, new_path, size_bytes, sha256, flagged, matched_rules, created_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                )
                .bind(&batch.agent_id)
                .bind(&batch.username)
                .bind(jstr(d, "action"))
                .bind(&fpath)
                .bind(jopt_str(d, "new_path"))
                .bind(d.get("size_bytes").and_then(|v| v.as_i64()).unwrap_or(0))
                .bind(jstr(d, "sha256"))
                .bind(flagged)
                .bind(&rules_str)
                .bind(ts)
                .execute(&state.db)
                .await;

                if flagged {
                    let snip = if !content_snippet.is_empty() {
                        content_snippet.chars().take(400).collect()
                    } else {
                        fpath
                    };
                    if create_incident(&state, &batch, "file", &matches, &snip, d.clone())
                        .await
                        .is_some()
                    {
                        incidents_created += 1;
                    }
                }
            }
            "app_blocked" | "usb_blocked" => {
                let reason = jstr(d, "reason");
                let channel = if ev.event_type == "app_blocked" { "app" } else { "usb" };
                let m = vec![DlpMatch {
                    rule_name: if ev.event_type == "app_blocked" {
                        "Блокировка запрещённого приложения".to_string()
                    } else {
                        "Блокировка неавторизованного USB (ВНИ)".to_string()
                    },
                    severity: Severity::High,
                    action: "block".to_string(),
                    matched_sample: reason.clone(),
                }];
                if create_incident(&state, &batch, channel, &m, &reason, d.clone())
                    .await
                    .is_some()
                {
                    incidents_created += 1;
                }
            }
            other => {
                tracing::debug!(event_type = other, "Ignoring unknown event type");
            }
        }
    }

    Ok(Json(EventBatchResponse {
        status: "ok".to_string(),
        processed,
        incidents: incidents_created,
        blocked: blocked_channels,
    }))
}

async fn create_incident(
    state: &AppState,
    batch: &EventBatchRequest,
    channel: &str,
    matches: &[DlpMatch],
    snippet: &str,
    meta: Value,
) -> Option<(i64, bool)> {
    let max_sev = matches
        .iter()
        .map(|m| m.severity)
        .max()
        .unwrap_or(Severity::Low);
    let requires_block = matches.iter().any(|m| m.action == "block");
    let rules: Vec<String> = matches.iter().map(|m| m.rule_name.clone()).collect();
    let short_snip: String = snippet.chars().take(500).collect();

    let res: Result<(i64, chrono::DateTime<Utc>), _> = sqlx::query_as(
        "INSERT INTO incidents (agent_id, hostname, username, channel, severity, status, matched_rules, snippet, meta) \
         VALUES ($1, $2, $3, $4, $5, 'open', $6, $7, $8) RETURNING id, created_at",
    )
    .bind(&batch.agent_id)
    .bind(&batch.hostname)
    .bind(&batch.username)
    .bind(channel)
    .bind(max_sev.as_str())
    .bind(json!(&rules))
    .bind(&short_snip)
    .bind(&meta)
    .fetch_one(&state.db)
    .await;

    let Ok((inc_id, ts)) = res else {
        return None;
    };

    write_audit(
        &state.db,
        "system",
        "system",
        "incident_created",
        &format!("incident:{inc_id}"),
        json!({ "agent_id": &batch.agent_id, "channel": channel, "severity": max_sev.as_str(), "rules": &rules }),
    )
    .await;

    state.ws.broadcast_dashboard(WsDashboardEvent::Incident {
        id: inc_id,
        agent_id: batch.agent_id.clone(),
        hostname: batch.hostname.clone(),
        username: batch.username.clone(),
        channel: channel.to_string(),
        severity: max_sev,
        snippet: short_snip.chars().take(120).collect(),
    });

    if matches!(max_sev, Severity::Medium | Severity::High | Severity::Critical) {
        let _ = state.alert_tx.try_send(AlertJob {
            incident_id: inc_id,
            ts,
            agent_id: batch.agent_id.clone(),
            hostname: batch.hostname.clone(),
            username: batch.username.clone(),
            channel: channel.to_string(),
            severity: max_sev,
            rules,
            snippet: short_snip,
        });
    }

    Some((inc_id, requires_block))
}

// ── Streaming Multipart Upload Helpers ───────────────────────────────────────

struct ParsedUpload {
    agent_id: String,
    hostname: String,
    username: String,
    channel: String,
    original_path: String,
    duration_sec: i32,
    sample_rate: i32,
    file_name: String,
    bytes: Vec<u8>,
}

async fn read_multipart_limited(mut mp: Multipart, max_bytes: usize) -> AppResult<ParsedUpload> {
    let mut out = ParsedUpload {
        agent_id: String::new(),
        hostname: String::new(),
        username: String::new(),
        channel: "usb".to_string(),
        original_path: String::new(),
        duration_sec: 0,
        sample_rate: 16000,
        file_name: "upload.bin".to_string(),
        bytes: Vec::new(),
    };

    while let Some(mut field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid multipart payload: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            if let Some(fname) = field.file_name() {
                out.file_name = fname.to_string();
            }
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| AppError::BadRequest(format!("Upload read failed: {e}")))?
            {
                if out.bytes.len() + chunk.len() > max_bytes {
                    return Err(AppError::PayloadTooLarge(format!(
                        "File exceeds maximum allowed size ({} MB)",
                        max_bytes / (1024 * 1024)
                    )));
                }
                out.bytes.extend_from_slice(&chunk);
            }
        } else {
            let txt = field.text().await.unwrap_or_default();
            match name.as_str() {
                "agent_id" => out.agent_id = txt,
                "hostname" => out.hostname = txt,
                "username" => out.username = txt,
                "channel" => out.channel = txt,
                "original_path" => out.original_path = txt,
                "duration_sec" => out.duration_sec = txt.parse().unwrap_or(0),
                "sample_rate" => out.sample_rate = txt.parse().unwrap_or(16000),
                _ => {}
            }
        }
    }

    validate_agent_id(&out.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if out.bytes.is_empty() {
        return Err(AppError::BadRequest("Empty file upload".to_string()));
    }
    Ok(out)
}

async fn write_bytes_file(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    let mut f = tokio::fs::File::create(path).await?;
    f.write_all(bytes).await?;
    f.flush().await?;
    Ok(())
}

async fn upload_screenshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mp: Multipart,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let up = read_multipart_limited(mp, state.config.media_max_bytes).await?;
    let fname = format!("{}.jpg", Utc::now().format("%Y%m%d_%H%M%S_%3f"));
    let dest = prepare_safe_storage_path(&state.config.screenshots_dir, &up.agent_id, &fname).await?;
    write_bytes_file(&dest, &up.bytes).await?;

    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO screenshots (agent_id, hostname, username, file_path, file_size) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&up.agent_id)
    .bind(&up.hostname)
    .bind(&up.username)
    .bind(dest.to_string_lossy().to_string())
    .bind(up.bytes.len() as i64)
    .fetch_one(&state.db)
    .await?;

    state.ws.broadcast_dashboard(WsDashboardEvent::Screenshot {
        agent_id: up.agent_id,
        screenshot_id: id,
    });

    Ok(Json(json!({ "status": "ok", "id": id })))
}

/// Receives a lightweight JPEG frame for the Quadrator live wall and stores it strictly in RAM
/// (zero DB inserts, zero disk churn!).
async fn upload_live_frame(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mp: Multipart,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let up = read_multipart_limited(mp, 2 * 1024 * 1024).await?;
    let now = Utc::now();
    state
        .live_frames
        .insert(up.agent_id.clone(), (now, up.bytes));
    state.ws.broadcast_dashboard(WsDashboardEvent::LiveFrame {
        agent_id: up.agent_id,
        ts: now,
    });
    Ok(Json(json!({ "status": "ok" })))
}

async fn upload_shadow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mp: Multipart,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let up = read_multipart_limited(mp, state.config.shadow_max_bytes).await?;

    let mut hasher = Sha256::new();
    hasher.update(&up.bytes);
    let sha256 = hex::encode(hasher.finalize());

    let fname = format!("{}_{}", &sha256[..12], up.file_name);
    let dest = prepare_safe_storage_path(&state.config.shadow_dir, &up.agent_id, &fname).await?;
    write_bytes_file(&dest, &up.bytes).await?;

    // Link to an existing recent incident for THIS agent matching the sha256 (if already ingested)
    let linked_inc: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM incidents \
         WHERE agent_id = $1 AND meta->>'sha256' = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&up.agent_id)
    .bind(&sha256)
    .fetch_optional(&state.db)
    .await?;

    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO shadow_copies (agent_id, hostname, username, channel, original_path, file_name, sha256, size_bytes, stored_path, incident_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(&up.agent_id)
    .bind(&up.hostname)
    .bind(&up.username)
    .bind(&up.channel)
    .bind(&up.original_path)
    .bind(&up.file_name)
    .bind(&sha256)
    .bind(up.bytes.len() as i64)
    .bind(dest.to_string_lossy().to_string())
    .bind(linked_inc.map(|r| r.0))
    .fetch_one(&state.db)
    .await?;

    // Dispatch async non-blocking OCR if image file
    let lower = up.file_name.to_ascii_lowercase();
    if [".png", ".jpg", ".jpeg", ".bmp", ".tif", ".tiff", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        let _ = state.ocr_tx.try_send(OcrJob {
            shadow_id: id,
            agent_id: up.agent_id,
            hostname: up.hostname,
            username: up.username,
            file_name: up.file_name,
            stored_path: dest,
        });
    }

    Ok(Json(json!({ "status": "ok", "id": id, "sha256": sha256 })))
}

async fn upload_audio(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mp: Multipart,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let up = read_multipart_limited(mp, state.config.audio_max_bytes).await?;
    let fname = format!("{}.wav", Utc::now().format("%Y%m%d_%H%M%S_%3f"));
    let dest = prepare_safe_storage_path(&state.config.audio_dir, &up.agent_id, &fname).await?;
    write_bytes_file(&dest, &up.bytes).await?;

    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO audio_recordings (agent_id, hostname, username, file_path, file_size, duration_sec, sample_rate) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&up.agent_id)
    .bind(&up.hostname)
    .bind(&up.username)
    .bind(dest.to_string_lossy().to_string())
    .bind(up.bytes.len() as i64)
    .bind(up.duration_sec)
    .bind(up.sample_rate)
    .fetch_one(&state.db)
    .await?;

    state.ws.broadcast_dashboard(WsDashboardEvent::Audio {
        agent_id: up.agent_id,
        recording_id: id,
    });

    Ok(Json(json!({ "status": "ok", "id": id })))
}

async fn upload_webcam(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mp: Multipart,
) -> AppResult<Json<Value>> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    let up = read_multipart_limited(mp, state.config.media_max_bytes).await?;
    let fname = format!("{}.jpg", Utc::now().format("%Y%m%d_%H%M%S_%3f"));
    let dest = prepare_safe_storage_path(&state.config.webcam_dir, &up.agent_id, &fname).await?;
    write_bytes_file(&dest, &up.bytes).await?;

    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO webcam_photos (agent_id, hostname, username, file_path, file_size) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&up.agent_id)
    .bind(&up.hostname)
    .bind(&up.username)
    .bind(dest.to_string_lossy().to_string())
    .bind(up.bytes.len() as i64)
    .fetch_one(&state.db)
    .await?;

    state.ws.broadcast_dashboard(WsDashboardEvent::Webcam {
        agent_id: up.agent_id,
        photo_id: id,
    });

    Ok(Json(json!({ "status": "ok", "id": id })))
}

fn jstr(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn jopt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn jint(v: &Value, key: &str) -> i32 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0) as i32
}
