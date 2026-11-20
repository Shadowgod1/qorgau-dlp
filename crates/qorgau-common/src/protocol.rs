use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Allowed pattern for agent_id: alphanumeric, underscore, hyphen, 8..=64 chars.
/// Prevents path traversal (../, /, \, NUL) across all storage endpoints.
pub fn validate_agent_id(agent_id: &str) -> Result<(), &'static str> {
    let len = agent_id.len();
    if !(8..=64).contains(&len) {
        return Err("agent_id length must be between 8 and 64 characters");
    }
    if !agent_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("agent_id may only contain ASCII letters, digits, '_' and '-'");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "critical" => Severity::Critical,
            "high" => Severity::High,
            "medium" => Severity::Medium,
            _ => Severity::Low,
        }
    }

    pub fn cef_level(&self) -> u8 {
        match self {
            Severity::Low => 3,
            Severity::Medium => 5,
            Severity::High => 8,
            Severity::Critical => 10,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Reviewed,
    Resolved,
    FalsePositive,
}

impl IncidentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IncidentStatus::Open => "open",
            IncidentStatus::Reviewed => "reviewed",
            IncidentStatus::Resolved => "resolved",
            IncidentStatus::FalsePositive => "false_positive",
        }
    }

    pub fn parse_strict(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Some(IncidentStatus::Open),
            "reviewed" => Some(IncidentStatus::Reviewed),
            "resolved" => Some(IncidentStatus::Resolved),
            "false_positive" => Some(IncidentStatus::FalsePositive),
            _ => None,
        }
    }
}

impl std::fmt::Display for IncidentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    SecurityOfficer,
    Viewer,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::SecurityOfficer => "security_officer",
            Role::Viewer => "viewer",
        }
    }

    pub fn parse_strict(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "security_officer" => Some(Role::SecurityOfficer),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn is_officer_or_admin(&self) -> bool {
        matches!(self, Role::Admin | Role::SecurityOfficer)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub agent_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventItem {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: Value,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatchRequest {
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub events: Vec<EventItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatchResponse {
    pub status: String,
    pub processed: usize,
    pub incidents: usize,
    pub blocked: Vec<String>,
}

/// Complete agent configuration DTO shared between server, agent, and dashboard.
/// Security invariant: `mod_microphone` and `mod_webcam` default strictly to `false` (fail-closed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigDto {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default = "default_config_name")]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,

    // Core monitoring modules
    #[serde(default = "default_true")]
    pub mod_sessions: bool,
    #[serde(default = "default_true")]
    pub mod_apps: bool,
    #[serde(default = "default_true")]
    pub mod_web: bool,
    #[serde(default = "default_true")]
    pub mod_messenger: bool,
    #[serde(default = "default_true")]
    pub mod_clipboard: bool,
    #[serde(default = "default_true")]
    pub mod_network: bool,
    #[serde(default = "default_true")]
    pub mod_usb: bool,
    #[serde(default = "default_true")]
    pub mod_printer: bool,
    #[serde(default = "default_true")]
    pub mod_screenshots: bool,

    // Sensitive surveillance modules: ALWAYS fail-closed false by default (ТК РК ст. 23)
    #[serde(default)]
    pub mod_microphone: bool,
    #[serde(default)]
    pub mod_webcam: bool,

    // Screenshot settings
    #[serde(default = "default_screenshot_interval")]
    pub screenshot_interval: i32,
    #[serde(default = "default_quality")]
    pub screenshot_quality: i32,
    #[serde(default = "default_screenshot_max_w")]
    pub screenshot_max_w: i32,

    // Microphone settings
    #[serde(default = "default_mic_duration")]
    pub microphone_duration: i32,
    #[serde(default = "default_mic_sample_rate")]
    pub microphone_sample_rate: i32,

    // Webcam settings
    #[serde(default = "default_webcam_interval")]
    pub webcam_interval: i32,
    #[serde(default = "default_quality")]
    pub webcam_quality: i32,
    #[serde(default = "default_webcam_max_w")]
    pub webcam_max_w: i32,

    // Network alerts
    #[serde(default = "default_true")]
    pub net_alert_sensitive_ports: bool,
    #[serde(default)]
    pub net_alert_unknown_ext: bool,
    #[serde(default = "default_true")]
    pub net_block_tor: bool,

    // Web alerts
    #[serde(default)]
    pub web_alert_social: bool,
    #[serde(default = "default_true")]
    pub web_alert_adult: bool,
    #[serde(default = "default_true")]
    pub web_alert_gambling: bool,
    #[serde(default)]
    pub web_alert_streaming: bool,
    #[serde(default)]
    pub web_alert_crypto: bool,

    // USB alerts
    #[serde(default = "default_true")]
    pub usb_alert_on_connect: bool,
    #[serde(default = "default_true")]
    pub usb_analyze_content: bool,
    #[serde(default = "default_true")]
    pub usb_alert_executables: bool,

    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub agent_count: Option<i64>,
    #[serde(default)]
    pub agents: Option<Vec<AssignedAgentRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignedAgentRef {
    pub agent_id: String,
    pub hostname: String,
}

impl Default for AgentConfigDto {
    fn default() -> Self {
        Self {
            id: None,
            name: default_config_name(),
            description: None,
            mod_sessions: true,
            mod_apps: true,
            mod_web: true,
            mod_messenger: true,
            mod_clipboard: true,
            mod_network: true,
            mod_usb: true,
            mod_printer: true,
            mod_screenshots: true,
            // Fail-closed: strictly false
            mod_microphone: false,
            mod_webcam: false,
            screenshot_interval: default_screenshot_interval(),
            screenshot_quality: default_quality(),
            screenshot_max_w: default_screenshot_max_w(),
            microphone_duration: default_mic_duration(),
            microphone_sample_rate: default_mic_sample_rate(),
            webcam_interval: default_webcam_interval(),
            webcam_quality: default_quality(),
            webcam_max_w: default_webcam_max_w(),
            net_alert_sensitive_ports: true,
            net_alert_unknown_ext: false,
            net_block_tor: true,
            web_alert_social: false,
            web_alert_adult: true,
            web_alert_gambling: true,
            web_alert_streaming: false,
            web_alert_crypto: false,
            usb_alert_on_connect: true,
            usb_analyze_content: true,
            usb_alert_executables: true,
            created_by: None,
            created_at: None,
            updated_at: None,
            agent_count: None,
            agents: None,
        }
    }
}

fn default_config_name() -> String {
    "default".to_string()
}
fn default_true() -> bool {
    true
}
fn default_screenshot_interval() -> i32 {
    60
}
fn default_quality() -> i32 {
    60
}
fn default_screenshot_max_w() -> i32 {
    1920
}
fn default_mic_duration() -> i32 {
    30
}
fn default_mic_sample_rate() -> i32 {
    16000
}
fn default_webcam_interval() -> i32 {
    120
}
fn default_webcam_max_w() -> i32 {
    1280
}

/// Commands pushed from server to a targeted agent over `/ws/agent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsAgentCommand {
    TakeScreenshot,
    TakeWebcam,
    StartWebcamStream,
    StopWebcamStream,
    TakeRecording,
    StartRecording,
    StopRecording,
    ReloadConfig,
}

/// Real-time events pushed from server to authenticated operators over `/ws/dashboard`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsDashboardEvent {
    Incident {
        id: i64,
        agent_id: String,
        hostname: String,
        username: String,
        channel: String,
        severity: Severity,
        snippet: String,
    },
    Screenshot {
        agent_id: String,
        screenshot_id: i64,
    },
    Audio {
        agent_id: String,
        recording_id: i64,
    },
    Webcam {
        agent_id: String,
        photo_id: i64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_validation_blocks_path_traversal() {
        assert!(validate_agent_id("a1b2c3d4e5f67890").is_ok());
        assert!(validate_agent_id("HOST_01-abc").is_ok());
        assert!(validate_agent_id("short").is_err());
        assert!(validate_agent_id("../../etc/passwd").is_err());
        assert!(validate_agent_id("C:\\Windows\\System32").is_err());
        assert!(validate_agent_id("valid123/evil").is_err());
        assert!(validate_agent_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn default_agent_config_is_fail_closed_for_mic_and_webcam() {
        let cfg = AgentConfigDto::default();
        assert!(!cfg.mod_microphone);
        assert!(!cfg.mod_webcam);
        assert!(cfg.mod_usb);
        assert!(cfg.mod_clipboard);
    }

    #[test]
    fn deserialized_empty_config_is_fail_closed() {
        let cfg: AgentConfigDto = serde_json::from_str("{}").unwrap();
        assert!(!cfg.mod_microphone);
        assert!(!cfg.mod_webcam);
    }
}
