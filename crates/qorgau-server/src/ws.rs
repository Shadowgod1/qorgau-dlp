use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use qorgau_common::dlp::mask_text;
use qorgau_common::{validate_agent_id, Role, WsAgentCommand, WsDashboardEvent};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};

use crate::auth::{authenticate_token, extract_bearer_token, verify_agent_token};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Targeted WebSocket hub:
/// - Dashboard clients subscribe to a broadcast channel (with role-aware PII masking on send).
/// - Agents register by `agent_id` in `DashMap` and ONLY receive commands addressed to them.
pub struct WsHub {
    agents: DashMap<String, mpsc::Sender<WsAgentCommand>>,
    dashboard_tx: broadcast::Sender<WsDashboardEvent>,
}

impl WsHub {
    pub fn new() -> Self {
        let (dashboard_tx, _) = broadcast::channel(512);
        Self {
            agents: DashMap::new(),
            dashboard_tx,
        }
    }

    pub fn broadcast_dashboard(&self, event: WsDashboardEvent) {
        let _ = self.dashboard_tx.send(event);
    }

    pub fn send_to_agent(&self, agent_id: &str, cmd: WsAgentCommand) -> bool {
        if let Some(tx) = self.agents.get(agent_id) {
            return tx.try_send(cmd).is_ok();
        }
        false
    }

    pub fn connected_agent_count(&self) -> usize {
        self.agents.len()
    }
}

#[derive(Debug, Deserialize)]
pub struct AgentWsQuery {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DashboardWsQuery {
    /// Optional fallback for browsers that pass JWT via first auth message instead of header
    pub token: Option<String>,
}

/// Authenticated agent WebSocket endpoint (`GET /ws/agent?agent_id=...` + `X-Agent-Token`).
pub async fn ws_agent_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<AgentWsQuery>,
) -> AppResult<impl IntoResponse> {
    verify_agent_token(&state.config.agent_token, &headers)?;
    validate_agent_id(&q.agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let agent_id = q.agent_id;

    Ok(ws.on_upgrade(move |socket| handle_agent_socket(socket, state, agent_id)))
}

async fn handle_agent_socket(socket: WebSocket, state: Arc<AppState>, agent_id: String) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<WsAgentCommand>(64);

    state.ws.agents.insert(agent_id.clone(), tx);
    tracing::info!(%agent_id, "Agent WS connected");

    let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    Some(c) => {
                        if let Ok(json) = serde_json::to_string(&c) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            }
        }
    }

    state.ws.agents.remove(&agent_id);
    tracing::info!(%agent_id, "Agent WS disconnected");
}

/// Authenticated dashboard WebSocket endpoint (`GET /ws/dashboard`).
/// Supports `Authorization: Bearer` header or first JSON auth frame `{"type":"auth","token":"..."}`
/// so browser WebSockets never have to put JWTs in URLs.
pub async fn ws_dashboard_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<DashboardWsQuery>,
) -> AppResult<impl IntoResponse> {
    let initial_token = extract_bearer_token(&headers)
        .map(|s| s.to_string())
        .or(q.token);

    let pre_authed = if let Some(tok) = initial_token {
        Some(authenticate_token(&state, &tok).await?.role)
    } else {
        None
    };

    Ok(ws.on_upgrade(move |socket| handle_dashboard_socket(socket, state, pre_authed)))
}

async fn handle_dashboard_socket(
    socket: WebSocket,
    state: Arc<AppState>,
    pre_authed: Option<Role>,
) {
    let (mut sender, mut receiver) = socket.split();

    // If not authenticated via header, require an explicit auth message within 5 seconds
    let role = match pre_authed {
        Some(r) => r,
        None => {
            let auth_res = tokio::time::timeout(Duration::from_secs(5), receiver.next()).await;
            let Ok(Some(Ok(Message::Text(txt)))) = auth_res else {
                let _ = sender.send(Message::Close(None)).await;
                return;
            };
            let parsed: serde_json::Value = serde_json::from_str(&txt).unwrap_or_default();
            let Some(tok) = parsed.get("token").and_then(|v| v.as_str()) else {
                let _ = sender.send(Message::Close(None)).await;
                return;
            };
            match authenticate_token(&state, tok).await {
                Ok(u) => u.role,
                Err(_) => {
                    let _ = sender.send(Message::Close(None)).await;
                    return;
                }
            }
        }
    };

    let mut rx = state.ws.dashboard_tx.subscribe();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(25));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            ev = rx.recv() => {
                match ev {
                    Ok(mut event) => {
                        // Role-aware filtering & masking
                        if role == Role::Viewer {
                            match &mut event {
                                WsDashboardEvent::Incident { snippet, .. } => {
                                    *snippet = mask_text(snippet);
                                }
                                WsDashboardEvent::Screenshot { .. }
                                | WsDashboardEvent::Audio { .. }
                                | WsDashboardEvent::Webcam { .. } => continue,
                            }
                        }
                        if let Ok(json) = serde_json::to_string(&event) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
        }
    }
}
