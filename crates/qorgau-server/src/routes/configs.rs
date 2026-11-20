use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::{AgentConfigDto, AssignedAgentRef, WsAgentCommand};
use serde::Deserialize;
use serde_json::json;

use crate::audit::write_audit;
use crate::auth::{AdminUser, AuthUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/configs", get(list_configs).post(create_config))
        .route(
            "/configs/{config_id}",
            get(get_config).put(update_config).delete(delete_config),
        )
        .route(
            "/configs/{config_id}/assign",
            post(assign_config).delete(unassign_config),
        )
        .route("/configs/agent/{agent_id}", get(get_agent_config_admin))
}

#[derive(sqlx::FromRow)]
pub struct AgentConfigDbRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub mod_sessions: bool,
    pub mod_apps: bool,
    pub mod_web: bool,
    pub mod_messenger: bool,
    pub mod_clipboard: bool,
    pub mod_network: bool,
    pub mod_usb: bool,
    pub mod_printer: bool,
    pub mod_screenshots: bool,
    pub mod_microphone: bool,
    pub mod_webcam: bool,
    pub screenshot_interval: i32,
    pub screenshot_quality: i32,
    pub screenshot_max_w: i32,
    pub microphone_duration: i32,
    pub microphone_sample_rate: i32,
    pub webcam_interval: i32,
    pub webcam_quality: i32,
    pub webcam_max_w: i32,
    pub net_alert_sensitive_ports: bool,
    pub net_alert_unknown_ext: bool,
    pub net_block_tor: bool,
    pub web_alert_social: bool,
    pub web_alert_adult: bool,
    pub web_alert_gambling: bool,
    pub web_alert_streaming: bool,
    pub web_alert_crypto: bool,
    pub usb_alert_on_connect: bool,
    pub usb_analyze_content: bool,
    pub usb_alert_executables: bool,
    pub created_by: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl AgentConfigDbRow {
    pub fn into_dto(self, agents: Option<Vec<AssignedAgentRef>>) -> AgentConfigDto {
        let count = agents.as_ref().map(|a| a.len() as i64);
        AgentConfigDto {
            id: Some(self.id),
            name: self.name,
            description: self.description,
            mod_sessions: self.mod_sessions,
            mod_apps: self.mod_apps,
            mod_web: self.mod_web,
            mod_messenger: self.mod_messenger,
            mod_clipboard: self.mod_clipboard,
            mod_network: self.mod_network,
            mod_usb: self.mod_usb,
            mod_printer: self.mod_printer,
            mod_screenshots: self.mod_screenshots,
            mod_microphone: self.mod_microphone,
            mod_webcam: self.mod_webcam,
            screenshot_interval: self.screenshot_interval,
            screenshot_quality: self.screenshot_quality,
            screenshot_max_w: self.screenshot_max_w,
            microphone_duration: self.microphone_duration,
            microphone_sample_rate: self.microphone_sample_rate,
            webcam_interval: self.webcam_interval,
            webcam_quality: self.webcam_quality,
            webcam_max_w: self.webcam_max_w,
            net_alert_sensitive_ports: self.net_alert_sensitive_ports,
            net_alert_unknown_ext: self.net_alert_unknown_ext,
            net_block_tor: self.net_block_tor,
            web_alert_social: self.web_alert_social,
            web_alert_adult: self.web_alert_adult,
            web_alert_gambling: self.web_alert_gambling,
            web_alert_streaming: self.web_alert_streaming,
            web_alert_crypto: self.web_alert_crypto,
            usb_alert_on_connect: self.usb_alert_on_connect,
            usb_analyze_content: self.usb_analyze_content,
            usb_alert_executables: self.usb_alert_executables,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            agent_count: count,
            agents,
        }
    }
}

pub const CONFIG_SELECT_COLS: &str = "\
    id, name, description, \
    mod_sessions, mod_apps, mod_web, mod_messenger, mod_clipboard, \
    mod_network, mod_usb, mod_printer, mod_screenshots, \
    mod_microphone, mod_webcam, \
    screenshot_interval, screenshot_quality, screenshot_max_w, \
    microphone_duration, microphone_sample_rate, \
    webcam_interval, webcam_quality, webcam_max_w, \
    net_alert_sensitive_ports, net_alert_unknown_ext, net_block_tor, \
    web_alert_social, web_alert_adult, web_alert_gambling, web_alert_streaming, web_alert_crypto, \
    usb_alert_on_connect, usb_analyze_content, usb_alert_executables, \
    created_by, created_at, updated_at";

pub async fn fetch_effective_config_for_agent(
    db: &sqlx::PgPool,
    agent_id: &str,
) -> AppResult<AgentConfigDto> {
    let sql = format!(
        "SELECT c.{cols} FROM agents a \
         JOIN agent_configs c ON c.id = a.config_id \
         WHERE a.agent_id = $1",
        cols = CONFIG_SELECT_COLS.replace(", ", ", c.")
    );
    let row: Option<AgentConfigDbRow> = sqlx::query_as(&sql)
        .bind(agent_id)
        .fetch_optional(db)
        .await?;

    Ok(match row {
        Some(r) => r.into_dto(None),
        None => AgentConfigDto::default(),
    })
}

async fn list_configs(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<AgentConfigDto>>> {
    let sql = format!("SELECT {CONFIG_SELECT_COLS} FROM agent_configs ORDER BY id ASC");
    let rows: Vec<AgentConfigDbRow> = sqlx::query_as(&sql).fetch_all(&state.db).await?;

    // Fetch all assigned agents in one query (eliminates N+1)
    let assigned: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT config_id, agent_id, hostname FROM agents WHERE config_id IS NOT NULL",
    )
    .fetch_all(&state.db)
    .await?;

    let mut by_cfg: std::collections::HashMap<i64, Vec<AssignedAgentRef>> =
        std::collections::HashMap::new();
    for (cid, aid, host) in assigned {
        by_cfg.entry(cid).or_default().push(AssignedAgentRef {
            agent_id: aid,
            hostname: host,
        });
    }

    let out = rows
        .into_iter()
        .map(|r| {
            let list = by_cfg.remove(&r.id).unwrap_or_default();
            r.into_dto(Some(list))
        })
        .collect();
    Ok(Json(out))
}

async fn get_config(
    AdminUser(_): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(config_id): Path<i64>,
) -> AppResult<Json<AgentConfigDto>> {
    let sql = format!("SELECT {CONFIG_SELECT_COLS} FROM agent_configs WHERE id = $1");
    let row: Option<AgentConfigDbRow> = sqlx::query_as(&sql)
        .bind(config_id)
        .fetch_optional(&state.db)
        .await?;
    let r = row.ok_or_else(|| AppError::NotFound("Config not found".to_string()))?;

    let agents: Vec<(String, String)> =
        sqlx::query_as("SELECT agent_id, hostname FROM agents WHERE config_id = $1")
            .bind(config_id)
            .fetch_all(&state.db)
            .await?;
    let refs = agents
        .into_iter()
        .map(|(agent_id, hostname)| AssignedAgentRef { agent_id, hostname })
        .collect();

    Ok(Json(r.into_dto(Some(refs))))
}

async fn create_config(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentConfigDto>,
) -> AppResult<Json<AgentConfigDto>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Config name is required".to_string()));
    }
    let sql = format!(
        "INSERT INTO agent_configs ( \
            name, description, \
            mod_sessions, mod_apps, mod_web, mod_messenger, mod_clipboard, \
            mod_network, mod_usb, mod_printer, mod_screenshots, \
            mod_microphone, mod_webcam, \
            screenshot_interval, screenshot_quality, screenshot_max_w, \
            microphone_duration, microphone_sample_rate, \
            webcam_interval, webcam_quality, webcam_max_w, \
            net_alert_sensitive_ports, net_alert_unknown_ext, net_block_tor, \
            web_alert_social, web_alert_adult, web_alert_gambling, web_alert_streaming, web_alert_crypto, \
            usb_alert_on_connect, usb_analyze_content, usb_alert_executables, created_by \
        ) VALUES ( \
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33 \
        ) RETURNING {CONFIG_SELECT_COLS}"
    );

    let row: AgentConfigDbRow = sqlx::query_as(&sql)
        .bind(name)
        .bind(&body.description)
        .bind(body.mod_sessions)
        .bind(body.mod_apps)
        .bind(body.mod_web)
        .bind(body.mod_messenger)
        .bind(body.mod_clipboard)
        .bind(body.mod_network)
        .bind(body.mod_usb)
        .bind(body.mod_printer)
        .bind(body.mod_screenshots)
        .bind(body.mod_microphone)
        .bind(body.mod_webcam)
        .bind(body.screenshot_interval.clamp(10, 3600))
        .bind(body.screenshot_quality.clamp(10, 95))
        .bind(body.screenshot_max_w.clamp(640, 3840))
        .bind(body.microphone_duration.clamp(5, 300))
        .bind(body.microphone_sample_rate.clamp(8000, 48000))
        .bind(body.webcam_interval.clamp(10, 3600))
        .bind(body.webcam_quality.clamp(10, 95))
        .bind(body.webcam_max_w.clamp(320, 1920))
        .bind(body.net_alert_sensitive_ports)
        .bind(body.net_alert_unknown_ext)
        .bind(body.net_block_tor)
        .bind(body.web_alert_social)
        .bind(body.web_alert_adult)
        .bind(body.web_alert_gambling)
        .bind(body.web_alert_streaming)
        .bind(body.web_alert_crypto)
        .bind(body.usb_alert_on_connect)
        .bind(body.usb_analyze_content)
        .bind(body.usb_alert_executables)
        .bind(&admin.username)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::from_db_unique(e, "Config with this name already exists"))?;

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "config_created",
        &format!("config:{}", row.id),
        json!({ "name": &row.name, "mod_microphone": row.mod_microphone, "mod_webcam": row.mod_webcam }),
    )
    .await;

    Ok(Json(row.into_dto(Some(Vec::new()))))
}

async fn update_config(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(config_id): Path<i64>,
    Json(body): Json<AgentConfigDto>,
) -> AppResult<Json<AgentConfigDto>> {
    let sql = format!(
        "UPDATE agent_configs SET \
            name=$1, description=$2, \
            mod_sessions=$3, mod_apps=$4, mod_web=$5, mod_messenger=$6, mod_clipboard=$7, \
            mod_network=$8, mod_usb=$9, mod_printer=$10, mod_screenshots=$11, \
            mod_microphone=$12, mod_webcam=$13, \
            screenshot_interval=$14, screenshot_quality=$15, screenshot_max_w=$16, \
            microphone_duration=$17, microphone_sample_rate=$18, \
            webcam_interval=$19, webcam_quality=$20, webcam_max_w=$21, \
            net_alert_sensitive_ports=$22, net_alert_unknown_ext=$23, net_block_tor=$24, \
            web_alert_social=$25, web_alert_adult=$26, web_alert_gambling=$27, \
            web_alert_streaming=$28, web_alert_crypto=$29, \
            usb_alert_on_connect=$30, usb_analyze_content=$31, usb_alert_executables=$32, \
            updated_at=NOW() \
         WHERE id=$33 RETURNING {CONFIG_SELECT_COLS}"
    );

    let row: Option<AgentConfigDbRow> = sqlx::query_as(&sql)
        .bind(body.name.trim())
        .bind(&body.description)
        .bind(body.mod_sessions)
        .bind(body.mod_apps)
        .bind(body.mod_web)
        .bind(body.mod_messenger)
        .bind(body.mod_clipboard)
        .bind(body.mod_network)
        .bind(body.mod_usb)
        .bind(body.mod_printer)
        .bind(body.mod_screenshots)
        .bind(body.mod_microphone)
        .bind(body.mod_webcam)
        .bind(body.screenshot_interval.clamp(10, 3600))
        .bind(body.screenshot_quality.clamp(10, 95))
        .bind(body.screenshot_max_w.clamp(640, 3840))
        .bind(body.microphone_duration.clamp(5, 300))
        .bind(body.microphone_sample_rate.clamp(8000, 48000))
        .bind(body.webcam_interval.clamp(10, 3600))
        .bind(body.webcam_quality.clamp(10, 95))
        .bind(body.webcam_max_w.clamp(320, 1920))
        .bind(body.net_alert_sensitive_ports)
        .bind(body.net_alert_unknown_ext)
        .bind(body.net_block_tor)
        .bind(body.web_alert_social)
        .bind(body.web_alert_adult)
        .bind(body.web_alert_gambling)
        .bind(body.web_alert_streaming)
        .bind(body.web_alert_crypto)
        .bind(body.usb_alert_on_connect)
        .bind(body.usb_analyze_content)
        .bind(body.usb_alert_executables)
        .bind(config_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::from_db_unique(e, "Config with this name already exists"))?;

    let r = row.ok_or_else(|| AppError::NotFound("Config not found".to_string()))?;

    // Notify all agents using this config to hot-reload immediately over WS
    let assigned: Vec<(String,)> =
        sqlx::query_as("SELECT agent_id FROM agents WHERE config_id = $1")
            .bind(config_id)
            .fetch_all(&state.db)
            .await?;
    for (aid,) in assigned {
        state.ws.send_to_agent(&aid, WsAgentCommand::ReloadConfig);
    }

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "config_updated",
        &format!("config:{config_id}"),
        json!({ "name": &r.name, "mod_microphone": r.mod_microphone, "mod_webcam": r.mod_webcam }),
    )
    .await;

    Ok(Json(r.into_dto(None)))
}

async fn delete_config(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(config_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let res = sqlx::query("DELETE FROM agent_configs WHERE id = $1")
        .bind(config_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Config not found".to_string()));
    }
    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "config_deleted",
        &format!("config:{config_id}"),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

#[derive(Deserialize)]
struct AssignReq {
    agent_ids: Vec<String>,
}

async fn assign_config(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(config_id): Path<i64>,
    Json(body): Json<AssignReq>,
) -> AppResult<Json<serde_json::Value>> {
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM agent_configs WHERE id = $1")
        .bind(config_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Config not found".to_string()));
    }

    sqlx::query("UPDATE agents SET config_id = $1 WHERE agent_id = ANY($2)")
        .bind(config_id)
        .bind(&body.agent_ids)
        .execute(&state.db)
        .await?;

    for aid in &body.agent_ids {
        state.ws.send_to_agent(aid, WsAgentCommand::ReloadConfig);
    }

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "config_assigned",
        &format!("config:{config_id}"),
        json!({ "agent_ids": &body.agent_ids }),
    )
    .await;

    Ok(Json(json!({ "status": "assigned", "count": body.agent_ids.len() })))
}

async fn unassign_config(
    AdminUser(admin): AdminUser,
    State(state): State<Arc<AppState>>,
    Path(config_id): Path<i64>,
    Json(body): Json<AssignReq>,
) -> AppResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE agents SET config_id = NULL WHERE config_id = $1 AND agent_id = ANY($2)")
        .bind(config_id)
        .bind(&body.agent_ids)
        .execute(&state.db)
        .await?;

    for aid in &body.agent_ids {
        state.ws.send_to_agent(aid, WsAgentCommand::ReloadConfig);
    }

    write_audit(
        &state.db,
        &admin.username,
        admin.role.as_str(),
        "config_unassigned",
        &format!("config:{config_id}"),
        json!({ "agent_ids": &body.agent_ids }),
    )
    .await;

    Ok(Json(json!({ "status": "unassigned" })))
}

async fn get_agent_config_admin(
    AuthUser(_): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> AppResult<Json<AgentConfigDto>> {
    Ok(Json(fetch_effective_config_for_agent(&state.db, &agent_id).await?))
}
