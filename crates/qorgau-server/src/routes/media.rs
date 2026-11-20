use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use qorgau_common::WsAgentCommand;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::io::ReaderStream;

use crate::audit::write_audit;
use crate::auth::OfficerUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::verify_path_within;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Screenshots
        .route("/screenshots", get(list_screenshots))
        .route("/screenshots/latest", get(latest_screenshots))
        .route("/screenshots/trigger", post(trigger_screenshot))
        .route("/screenshots/{id}/image", get(get_screenshot_image))
        .route("/screenshots/{id}", delete(delete_screenshot))
        // Shadow copies
        .route("/shadow", get(list_shadow))
        .route("/shadow/{id}/download", get(download_shadow))
        // Audio recordings
        .route("/audio", get(list_audio))
        .route("/audio/{id}/download", get(download_audio))
        .route("/audio/{id}", delete(delete_audio))
        // Webcam photos & remote commands
        .route("/webcam", get(list_webcam))
        .route("/webcam/{id}/image", get(get_webcam_image))
        .route("/webcam/{id}", delete(delete_webcam))
        .route("/webcam/command", post(send_agent_command))
}

#[derive(Deserialize)]
struct MediaQuery {
    agent_id: Option<String>,
    username: Option<String>,
    #[serde(default = "def_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn def_limit() -> i64 {
    50
}

async fn stream_file(path: &std::path::Path, content_type: &str, download_name: Option<&str>) -> AppResult<Response> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| AppError::NotFound("File not found on disk".to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, no-store");
    if let Some(name) = download_name {
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", name.replace('"', "")),
        );
    }
    builder
        .body(body)
        .map_err(|e| AppError::Internal(e.to_string()))
}

// ── Screenshots ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct ScreenshotRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    file_size: i64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ScreenshotDto {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    file_size: i64,
    created_at: DateTime<Utc>,
    image_url: String,
}

impl From<ScreenshotRow> for ScreenshotDto {
    fn from(r: ScreenshotRow) -> Self {
        let image_url = format!("/api/v1/screenshots/{}/image", r.id);
        Self {
            id: r.id,
            agent_id: r.agent_id,
            hostname: r.hostname,
            username: r.username,
            file_size: r.file_size,
            created_at: r.created_at,
            image_url,
        }
    }
}

#[derive(Serialize)]
struct PagedScreenshots {
    total: i64,
    items: Vec<ScreenshotDto>,
}

async fn list_screenshots(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<MediaQuery>,
) -> AppResult<Json<PagedScreenshots>> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM screenshots \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2)",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .fetch_one(&state.db)
    .await?;

    let rows: Vec<ScreenshotRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, file_size, created_at FROM screenshots \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 200))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(PagedScreenshots {
        total: total.0,
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Deserialize)]
struct LatestQuery {
    agent_id: Option<String>,
}

async fn latest_screenshots(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<LatestQuery>,
) -> AppResult<Json<Vec<ScreenshotDto>>> {
    let rows: Vec<ScreenshotRow> = sqlx::query_as(
        "SELECT DISTINCT ON (agent_id) id, agent_id, hostname, username, file_size, created_at \
         FROM screenshots \
         WHERE ($1::text IS NULL OR agent_id = $1) \
         ORDER BY agent_id, created_at DESC",
    )
    .bind(&q.agent_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
struct TriggerQuery {
    agent_id: String,
}

async fn trigger_screenshot(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TriggerQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let delivered = state
        .ws
        .send_to_agent(&q.agent_id, WsAgentCommand::TakeScreenshot);
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "command_sent",
        &format!("agent:{}", q.agent_id),
        json!({ "command": "take_screenshot", "delivered": delivered }),
    )
    .await;
    Ok(Json(json!({ "status": "ok", "delivered": delivered })))
}

async fn get_screenshot_image(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT file_path, agent_id FROM screenshots WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Screenshot not found".to_string()))?;

    let safe = verify_path_within(&state.config.screenshots_dir, &file_path).await?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "view_screenshot",
        &format!("screenshot:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    stream_file(&safe, "image/jpeg", None).await
}

async fn delete_screenshot(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let row: Option<(String, String)> =
        sqlx::query_as("DELETE FROM screenshots WHERE id = $1 RETURNING file_path, agent_id")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Screenshot not found".to_string()))?;
    if let Ok(safe) = verify_path_within(&state.config.screenshots_dir, &file_path).await {
        let _ = tokio::fs::remove_file(safe).await;
    }
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "screenshot_deleted",
        &format!("screenshot:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

// ── Shadow Copies ────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct ShadowRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    channel: String,
    original_path: String,
    file_name: String,
    sha256: String,
    size_bytes: i64,
    incident_id: Option<i64>,
    created_at: DateTime<Utc>,
}

async fn list_shadow(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<MediaQuery>,
) -> AppResult<Json<Vec<ShadowRow>>> {
    let rows: Vec<ShadowRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, channel, original_path, file_name, sha256, size_bytes, incident_id, created_at \
         FROM shadow_copies \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 200))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

async fn download_shadow(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT stored_path, file_name, agent_id FROM shadow_copies WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (stored_path, file_name, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Shadow copy not found".to_string()))?;
    let safe = verify_path_within(&state.config.shadow_dir, &stored_path).await?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "download_shadow_copy",
        &format!("shadow:{id}"),
        json!({ "agent_id": agent_id, "file_name": &file_name }),
    )
    .await;
    stream_file(&safe, "application/octet-stream", Some(&file_name)).await
}

// ── Audio Recordings ─────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AudioRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    file_size: i64,
    duration_sec: i32,
    sample_rate: i32,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct PagedAudio {
    total: i64,
    items: Vec<AudioRow>,
}

async fn list_audio(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<MediaQuery>,
) -> AppResult<Json<PagedAudio>> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audio_recordings \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2)",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<AudioRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, file_size, duration_sec, sample_rate, created_at \
         FROM audio_recordings \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 200))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(PagedAudio {
        total: total.0,
        items,
    }))
}

async fn download_audio(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT file_path, agent_id FROM audio_recordings WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Audio recording not found".to_string()))?;
    let safe = verify_path_within(&state.config.audio_dir, &file_path).await?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "download_audio",
        &format!("audio:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    stream_file(&safe, "audio/wav", Some(&format!("recording_{id}.wav"))).await
}

async fn delete_audio(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let row: Option<(String, String)> =
        sqlx::query_as("DELETE FROM audio_recordings WHERE id = $1 RETURNING file_path, agent_id")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Audio recording not found".to_string()))?;
    if let Ok(safe) = verify_path_within(&state.config.audio_dir, &file_path).await {
        let _ = tokio::fs::remove_file(safe).await;
    }
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "audio_deleted",
        &format!("audio:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

// ── Webcam Photos & Remote Commands ──────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct WebcamRow {
    id: i64,
    agent_id: String,
    hostname: String,
    username: String,
    file_size: i64,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct PagedWebcam {
    total: i64,
    items: Vec<WebcamRow>,
}

async fn list_webcam(
    OfficerUser(_): OfficerUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<MediaQuery>,
) -> AppResult<Json<PagedWebcam>> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM webcam_photos \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2)",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<WebcamRow> = sqlx::query_as(
        "SELECT id, agent_id, hostname, username, file_size, created_at \
         FROM webcam_photos \
         WHERE ($1::text IS NULL OR agent_id = $1) AND ($2::text IS NULL OR username = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&q.agent_id)
    .bind(&q.username)
    .bind(q.limit.clamp(1, 200))
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(PagedWebcam {
        total: total.0,
        items,
    }))
}

async fn get_webcam_image(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT file_path, agent_id FROM webcam_photos WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Webcam photo not found".to_string()))?;
    let safe = verify_path_within(&state.config.webcam_dir, &file_path).await?;
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "view_webcam_photo",
        &format!("webcam:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    stream_file(&safe, "image/jpeg", None).await
}

async fn delete_webcam(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let row: Option<(String, String)> =
        sqlx::query_as("DELETE FROM webcam_photos WHERE id = $1 RETURNING file_path, agent_id")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (file_path, agent_id) =
        row.ok_or_else(|| AppError::NotFound("Webcam photo not found".to_string()))?;
    if let Ok(safe) = verify_path_within(&state.config.webcam_dir, &file_path).await {
        let _ = tokio::fs::remove_file(safe).await;
    }
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "webcam_deleted",
        &format!("webcam:{id}"),
        json!({ "agent_id": agent_id }),
    )
    .await;
    Ok(Json(json!({ "status": "deleted" })))
}

#[derive(Deserialize)]
struct CommandReq {
    agent_id: String,
    command: String,
}

async fn send_agent_command(
    OfficerUser(user): OfficerUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CommandReq>,
) -> AppResult<Json<serde_json::Value>> {
    let cmd = match body.command.as_str() {
        "take_screenshot" => WsAgentCommand::TakeScreenshot,
        "take_webcam" => WsAgentCommand::TakeWebcam,
        "start_webcam_stream" => WsAgentCommand::StartWebcamStream,
        "stop_webcam_stream" => WsAgentCommand::StopWebcamStream,
        "take_recording" => WsAgentCommand::TakeRecording,
        "start_recording" => WsAgentCommand::StartRecording,
        "stop_recording" => WsAgentCommand::StopRecording,
        "reload_config" => WsAgentCommand::ReloadConfig,
        other => {
            return Err(AppError::BadRequest(format!("Unknown command: {other}")));
        }
    };

    let delivered = state.ws.send_to_agent(&body.agent_id, cmd);
    write_audit(
        &state.db,
        &user.username,
        user.role.as_str(),
        "agent_command",
        &format!("agent:{}", body.agent_id),
        json!({ "command": &body.command, "delivered": delivered }),
    )
    .await;

    Ok(Json(json!({
        "status": "ok",
        "agent_id": body.agent_id,
        "command": body.command,
        "delivered": delivered
    })))
}
