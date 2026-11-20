use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: String,
    pub database_url: String,
    pub secret_key: String,
    pub jwt_expire_min: i64,
    pub agent_token: String,
    pub cors_origins: Vec<String>,

    pub admin_username: String,
    pub admin_password: String,

    pub screenshots_dir: PathBuf,
    pub screenshots_retention_days: i64,
    pub shadow_dir: PathBuf,
    pub shadow_retention_days: i64,
    pub shadow_max_bytes: usize,
    pub audio_dir: PathBuf,
    pub audio_retention_days: i64,
    pub audio_max_bytes: usize,
    pub webcam_dir: PathBuf,
    pub webcam_retention_days: i64,
    pub media_max_bytes: usize,
    pub data_retention_days: i64,

    pub telegram_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub siem_webhook_url: Option<String>,
    pub siem_webhook_token: Option<String>,
    pub siem_syslog_host: Option<String>,
    pub siem_syslog_port: u16,
    pub siem_syslog_proto: String,
    pub ocr_concurrency: usize,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let raw_db_url = env_str(
            "DATABASE_URL",
            "postgres://qorgau_user:qorgau_secret_2024@localhost:5432/qorgau_dlp",
        );
        // Normalize SQLAlchemy-style postgresql+asyncpg:// -> postgres://
        let database_url = raw_db_url
            .replace("postgresql+asyncpg://", "postgres://")
            .replace("postgresql://", "postgres://");

        let cors_raw = env_str(
            "CORS_ORIGINS",
            "http://localhost,http://localhost:5173,http://127.0.0.1:5173",
        );
        let cors_origins: Vec<String> = cors_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let shadow_max_mb: usize = env_parse("SHADOW_MAX_MB", 50);

        Self {
            bind_addr: env_str("BIND_ADDR", "0.0.0.0:8000"),
            database_url,
            secret_key: env_str("SECRET_KEY", "change-me-in-production-min-32-chars!!"),
            jwt_expire_min: env_parse("JWT_EXPIRE_MIN", 480),
            agent_token: env_str("AGENT_TOKEN", "qorgau_agent_token_2024"),
            cors_origins,

            admin_username: env_str("ADMIN_USERNAME", "admin"),
            admin_password: env_str("ADMIN_PASSWORD", "qorgau_admin_2024"),

            screenshots_dir: PathBuf::from(env_str("SCREENSHOTS_DIR", "./data/screenshots")),
            screenshots_retention_days: env_parse("SCREENSHOTS_RETENTION_DAYS", 30),
            shadow_dir: PathBuf::from(env_str("SHADOW_DIR", "./data/shadow")),
            shadow_retention_days: env_parse("SHADOW_RETENTION_DAYS", 365),
            shadow_max_bytes: shadow_max_mb.saturating_mul(1024 * 1024),
            audio_dir: PathBuf::from(env_str("AUDIO_DIR", "./data/audio")),
            audio_retention_days: env_parse("AUDIO_RETENTION_DAYS", 30),
            audio_max_bytes: env_parse::<usize>("AUDIO_MAX_MB", 20).saturating_mul(1024 * 1024),
            webcam_dir: PathBuf::from(env_str("WEBCAM_DIR", "./data/webcam_photos")),
            webcam_retention_days: env_parse("WEBCAM_RETENTION_DAYS", 30),
            media_max_bytes: env_parse::<usize>("MEDIA_MAX_MB", 10).saturating_mul(1024 * 1024),
            data_retention_days: env_parse("DATA_RETENTION_DAYS", 365),

            telegram_token: env_opt("ALERT_TELEGRAM_TOKEN"),
            telegram_chat_id: env_opt("ALERT_TELEGRAM_CHAT_ID"),
            siem_webhook_url: env_opt("SIEM_WEBHOOK_URL"),
            siem_webhook_token: env_opt("SIEM_WEBHOOK_TOKEN"),
            siem_syslog_host: env_opt("SIEM_SYSLOG_HOST"),
            siem_syslog_port: env_parse("SIEM_SYSLOG_PORT", 514),
            siem_syslog_proto: env_str("SIEM_SYSLOG_PROTO", "udp").to_ascii_lowercase(),
            ocr_concurrency: env_parse("OCR_CONCURRENCY", 2).clamp(1, 8),
        }
    }
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}
