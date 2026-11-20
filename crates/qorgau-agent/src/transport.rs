use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use qorgau_common::{
    AgentConfigDto, EventBatchRequest, EventBatchResponse, EventItem, RegisterRequest,
    WsAgentCommand,
};
use reqwest::multipart::{Form, Part};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::storage::DiskQueue;

pub struct AgentTransport {
    client: reqwest::Client,
    pub server_url: String,
    pub token: String,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub queue: Arc<DiskQueue>,
}

impl AgentTransport {
    pub fn new(
        server_url: String,
        token: String,
        agent_id: String,
        hostname: String,
        username: String,
        queue: Arc<DiskQueue>,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(4)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .gzip(true)
            .build()?;

        Ok(Self {
            client,
            server_url: server_url.trim_end_matches('/').to_string(),
            token,
            agent_id,
            hostname,
            username,
            queue,
        })
    }

    pub fn enqueue(&self, event_type: &str, data: serde_json::Value) {
        self.queue.push(&EventItem {
            event_type: event_type.to_string(),
            data,
            timestamp: chrono::Utc::now(),
        });
    }

    pub async fn register(&self, os_version: &str, agent_version: &str) -> bool {
        let url = format!("{}/api/v1/monitor/register", self.server_url);
        let body = RegisterRequest {
            agent_id: self.agent_id.clone(),
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            os_version: os_version.to_string(),
            agent_version: agent_version.to_string(),
        };
        match self
            .client
            .post(&url)
            .header("X-Agent-Token", &self.token)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!("Registered agent with server");
                true
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "Agent registration rejected");
                false
            }
            Err(e) => {
                tracing::warn!(error = %e, "Agent registration failed (server offline)");
                false
            }
        }
    }

    pub async fn fetch_config(&self) -> Option<AgentConfigDto> {
        let url = format!(
            "{}/api/v1/monitor/config?agent_id={}",
            self.server_url, self.agent_id
        );
        match self
            .client
            .get(&url)
            .header("X-Agent-Token", &self.token)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp.json::<AgentConfigDto>().await.ok(),
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "Failed to fetch agent config");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "Config fetch failed; using fail-closed defaults");
                None
            }
        }
    }

    pub async fn send_heartbeat(&self) {
        let url = format!(
            "{}/api/v1/monitor/heartbeat?agent_id={}",
            self.server_url, self.agent_id
        );
        let _ = self
            .client
            .post(&url)
            .header("X-Agent-Token", &self.token)
            .send()
            .await;
    }

    pub async fn flush_batch(&self) -> usize {
        let batch = self.queue.peek_batch(200);
        if batch.is_empty() {
            return 0;
        }
        let count = batch.len();
        let url = format!("{}/api/v1/monitor/events", self.server_url);
        let req = EventBatchRequest {
            agent_id: self.agent_id.clone(),
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            events: batch,
        };

        match self
            .client
            .post(&url)
            .header("X-Agent-Token", &self.token)
            .json(&req)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                self.queue.ack_batch(count);
                if let Ok(parsed) = resp.json::<EventBatchResponse>().await {
                    tracing::debug!(
                        processed = parsed.processed,
                        incidents = parsed.incidents,
                        "Flushed event batch"
                    );
                }
                count
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "Event batch upload failed; keeping in disk queue");
                0
            }
            Err(e) => {
                tracing::debug!(error = %e, "Server unreachable; events preserved in disk queue");
                0
            }
        }
    }

    pub async fn upload_screenshot(&self, jpeg_bytes: Vec<u8>) {
        let form = Form::new()
            .text("agent_id", self.agent_id.clone())
            .text("hostname", self.hostname.clone())
            .text("username", self.username.clone())
            .part(
                "file",
                Part::bytes(jpeg_bytes)
                    .file_name("screenshot.jpg")
                    .mime_str("image/jpeg")
                    .unwrap(),
            );
        self.post_multipart("/api/v1/monitor/screenshot", form).await;
    }

    pub async fn upload_webcam(&self, jpeg_bytes: Vec<u8>) {
        let form = Form::new()
            .text("agent_id", self.agent_id.clone())
            .text("hostname", self.hostname.clone())
            .text("username", self.username.clone())
            .part(
                "file",
                Part::bytes(jpeg_bytes)
                    .file_name("webcam.jpg")
                    .mime_str("image/jpeg")
                    .unwrap(),
            );
        self.post_multipart("/api/v1/monitor/webcam", form).await;
    }

    pub async fn upload_audio(&self, wav_bytes: Vec<u8>, duration_sec: i32, sample_rate: i32) {
        let form = Form::new()
            .text("agent_id", self.agent_id.clone())
            .text("hostname", self.hostname.clone())
            .text("username", self.username.clone())
            .text("duration_sec", duration_sec.to_string())
            .text("sample_rate", sample_rate.to_string())
            .part(
                "file",
                Part::bytes(wav_bytes)
                    .file_name("recording.wav")
                    .mime_str("audio/wav")
                    .unwrap(),
            );
        self.post_multipart("/api/v1/monitor/audio", form).await;
    }

    pub async fn upload_shadow(
        &self,
        channel: &str,
        original_path: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) {
        let form = Form::new()
            .text("agent_id", self.agent_id.clone())
            .text("hostname", self.hostname.clone())
            .text("username", self.username.clone())
            .text("channel", channel.to_string())
            .text("original_path", original_path.to_string())
            .part(
                "file",
                Part::bytes(bytes)
                    .file_name(file_name.to_string())
                    .mime_str("application/octet-stream")
                    .unwrap(),
            );
        self.post_multipart("/api/v1/monitor/shadow", form).await;
    }

    async fn post_multipart(&self, path: &str, form: Form) {
        let url = format!("{}{}", self.server_url, path);
        match self
            .client
            .post(&url)
            .header("X-Agent-Token", &self.token)
            .timeout(Duration::from_secs(60))
            .multipart(form)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => tracing::warn!(path, status = %r.status(), "Multipart upload failed"),
            Err(e) => tracing::debug!(path, error = %e, "Multipart upload network error"),
        }
    }

    /// Single persistent, authenticated WebSocket client to `/ws/agent` with exponential backoff
    /// and automatic Ping/Pong handling (eliminates the legacy 30s disconnect & socket leak bug).
    pub async fn run_ws_listener(self: Arc<Self>, cmd_tx: mpsc::Sender<WsAgentCommand>) {
        let ws_base = self
            .server_url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        let ws_url = format!("{ws_base}/ws/agent?agent_id={}", self.agent_id);

        let mut backoff_sec = 2u64;

        loop {
            let Ok(mut req) = ws_url.clone().into_client_request() else {
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            };
            if let Ok(hdr) = HeaderValue::from_str(&self.token) {
                req.headers_mut().insert("X-Agent-Token", hdr);
            }

            match tokio_tungstenite::connect_async(req).await {
                Ok((ws_stream, _)) => {
                    tracing::info!("Connected to server WebSocket /ws/agent");
                    backoff_sec = 2;
                    let (_write, mut read) = ws_stream.split();

                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(txt)) => {
                                if let Ok(cmd) = serde_json::from_str::<WsAgentCommand>(&txt) {
                                    let _ = cmd_tx.try_send(cmd);
                                }
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }
                    tracing::info!("WebSocket disconnected; will reconnect");
                }
                Err(e) => {
                    tracing::debug!(error = %e, backoff_sec, "WS connect failed");
                }
            }

            tokio::time::sleep(Duration::from_secs(backoff_sec)).await;
            backoff_sec = (backoff_sec * 2).min(60);
        }
    }
}
