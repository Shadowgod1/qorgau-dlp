use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use qorgau_common::{AgentConfigDto, WsAgentCommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing_subscriber::EnvFilter;

mod collectors;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UnixAgentConfig {
    server_url: String,
    agent_token: String,
    data_dir: PathBuf,
    poll_interval_sec: u64,
}

impl UnixAgentConfig {
    fn load() -> Self {
        let env_url = std::env::var("QORGAU_SERVER_URL").ok();
        let env_tok = std::env::var("QORGAU_AGENT_TOKEN").ok();

        let cfg_candidates = [
            PathBuf::from("/etc/qorgau-agent/agent.toml"),
            dirs_home().join(".config/qorgau-agent/agent.toml"),
        ];

        let mut server_url = "http://127.0.0.1".to_string();
        let mut agent_token = "qorgau_agent_token_2024".to_string();
        let mut data_dir = PathBuf::from("/var/lib/qorgau-agent");
        let mut poll_interval_sec = 30u64;

        for p in cfg_candidates {
            if let Ok(raw) = std::fs::read_to_string(&p) {
                if let Ok(val) = toml::from_str::<toml::Value>(&raw) {
                    if let Some(u) = val.get("server_url").and_then(|v| v.as_str()) {
                        server_url = u.trim_end_matches('/').to_string();
                    }
                    if let Some(t) = val.get("agent_token").and_then(|v| v.as_str()) {
                        agent_token = t.to_string();
                    }
                    if let Some(d) = val.get("data_dir").and_then(|v| v.as_str()) {
                        data_dir = PathBuf::from(d);
                    }
                    if let Some(i) = val.get("poll_interval_sec").and_then(|v| v.as_integer()) {
                        poll_interval_sec = (i as u64).clamp(10, 600);
                    }
                }
                break;
            }
        }

        if let Some(u) = env_url.filter(|s| !s.trim().is_empty()) {
            server_url = u.trim_end_matches('/').to_string();
        }
        if let Some(t) = env_tok.filter(|s| !s.trim().is_empty()) {
            agent_token = t;
        }
        if !data_dir.exists() && std::fs::create_dir_all(&data_dir).is_err() {
            data_dir = dirs_home().join(".local/share/qorgau-agent");
        }

        Self {
            server_url,
            agent_token,
            data_dir,
            poll_interval_sec,
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn resolve_agent_id(data_dir: &Path) -> String {
    let id_file = data_dir.join("agent_id");
    if let Ok(existing) = std::fs::read_to_string(&id_file) {
        let trimmed = existing.trim();
        if qorgau_common::validate_agent_id(trimmed).is_ok() {
            return trimmed.to_string();
        }
    }
    let generated = format!("unix-{}", uuid::Uuid::new_v4().simple());
    let _ = std::fs::write(&id_file, &generated);
    generated
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn current_hostname() -> String {
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.trim().is_empty() {
            return h.trim().to_string();
        }
    }
    if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
        if !h.trim().is_empty() {
            return h.trim().to_string();
        }
    }
    "unix-host".to_string()
}

pub struct AgentClient {
    pub http: reqwest::Client,
    pub base_url: String,
    pub token: String,
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
}

impl AgentClient {
    pub async fn post_json(&self, path: &str, body: Value) {
        let url = format!("{}/api/v1{}", self.base_url, path);
        let res = self
            .http
            .post(&url)
            .header("X-Agent-Token", &self.token)
            .json(&body)
            .send()
            .await;
        if let Err(e) = res {
            tracing::warn!(error = %e, %path, "POST failed");
        }
    }

    pub async fn register(&self) {
        let os_ver = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        self.post_json(
            "/agents/register",
            json!({
                "agent_id": self.agent_id,
                "hostname": self.hostname,
                "username": self.username,
                "os_version": os_ver,
                "agent_version": env!("CARGO_PKG_VERSION"),
            }),
        )
        .await;
    }

    pub async fn fetch_config(&self) -> Option<AgentConfigDto> {
        let url = format!(
            "{}/api/v1/agents/{}/config",
            self.base_url, self.agent_id
        );
        let resp = self
            .http
            .get(&url)
            .header("X-Agent-Token", &self.token)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<AgentConfigDto>().await.ok()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = UnixAgentConfig::load();
    let _ = std::fs::create_dir_all(&cfg.data_dir);

    let agent_id = resolve_agent_id(&cfg.data_dir);
    let hostname = current_hostname();
    let username = current_username();

    tracing::info!(
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        %agent_id,
        %hostname,
        %username,
        server = %cfg.server_url,
        "Starting Qorgau DLP Unix Agent (Linux/macOS)"
    );

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .build()?;

    let client = Arc::new(AgentClient {
        http,
        base_url: cfg.server_url.clone(),
        token: cfg.agent_token.clone(),
        agent_id: agent_id.clone(),
        hostname,
        username,
    });

    client.register().await;
    let shared_cfg = Arc::new(RwLock::new(client.fetch_config().await.unwrap_or_default()));

    // WebSocket command loop
    let ws_client = client.clone();
    let ws_cfg = shared_cfg.clone();
    tokio::spawn(async move {
        let ws_base = ws_client
            .base_url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        let ws_url = format!("{ws_base}/ws/agent?agent_id={}", ws_client.agent_id);

        loop {
            if let Ok(mut req) = ws_url.as_str().into_client_request() {
                if let Ok(hv) = ws_client.token.parse() {
                    req.headers_mut().insert("X-Agent-Token", hv);
                }
                if let Ok((stream, _)) = tokio_tungstenite::connect_async(req).await {
                    tracing::info!("Connected to server WebSocket");
                    let (_, mut read) = stream.split();
                    while let Some(Ok(msg)) = read.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
                            if let Ok(cmd) = serde_json::from_str::<WsAgentCommand>(&txt) {
                                match cmd {
                                    WsAgentCommand::ReloadConfig => {
                                        if let Some(new_cfg) = ws_client.fetch_config().await {
                                            *ws_cfg.write().await = new_cfg;
                                        }
                                    }
                                    WsAgentCommand::RunDiscoveryScan => {
                                        let c = ws_cfg.read().await.clone();
                                        collectors::run_discovery(&ws_client, &c.discovery_paths).await;
                                    }
                                    WsAgentCommand::RunInventoryScan => {
                                        collectors::run_inventory(&ws_client).await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    // Periodic telemetry & DLP loop
    let mut tick = tokio::time::interval(Duration::from_secs(cfg.poll_interval_sec));
    loop {
        tick.tick().await;
        client.register().await;
        let current = shared_cfg.read().await.clone();
        if current.mod_apps {
            collectors::collect_processes(&client).await;
        }
        if current.mod_usb {
            collectors::collect_mounts(&client).await;
        }
        if current.mod_network {
            collectors::collect_network(&client).await;
        }
    }
}
