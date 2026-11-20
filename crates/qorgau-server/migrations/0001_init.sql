-- Qorgau DLP v2 (Rust) — Clean PostgreSQL 16 Schema
-- Fixes legacy issues:
--  * Uniform TIMESTAMPTZ everywhere (no naive/aware crashes on audio/webcam)
--  * Lowercase TEXT + CHECK constraints for severity/status (no PG ENUM case mismatches)
--  * Proper foreign keys, unique constraints, and composite indexes

CREATE TABLE IF NOT EXISTS agent_configs (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(120) NOT NULL UNIQUE,
    description TEXT,

    mod_sessions BOOLEAN NOT NULL DEFAULT TRUE,
    mod_apps BOOLEAN NOT NULL DEFAULT TRUE,
    mod_web BOOLEAN NOT NULL DEFAULT TRUE,
    mod_messenger BOOLEAN NOT NULL DEFAULT TRUE,
    mod_clipboard BOOLEAN NOT NULL DEFAULT TRUE,
    mod_network BOOLEAN NOT NULL DEFAULT TRUE,
    mod_usb BOOLEAN NOT NULL DEFAULT TRUE,
    mod_printer BOOLEAN NOT NULL DEFAULT TRUE,
    mod_screenshots BOOLEAN NOT NULL DEFAULT TRUE,

    screenshot_interval INTEGER NOT NULL DEFAULT 60,
    screenshot_quality INTEGER NOT NULL DEFAULT 60,
    screenshot_max_w INTEGER NOT NULL DEFAULT 1920,

    net_alert_sensitive_ports BOOLEAN NOT NULL DEFAULT TRUE,
    net_alert_unknown_ext BOOLEAN NOT NULL DEFAULT FALSE,
    net_block_tor BOOLEAN NOT NULL DEFAULT TRUE,

    web_alert_social BOOLEAN NOT NULL DEFAULT FALSE,
    web_alert_adult BOOLEAN NOT NULL DEFAULT TRUE,
    web_alert_gambling BOOLEAN NOT NULL DEFAULT TRUE,
    web_alert_streaming BOOLEAN NOT NULL DEFAULT FALSE,
    web_alert_crypto BOOLEAN NOT NULL DEFAULT FALSE,

    usb_alert_on_connect BOOLEAN NOT NULL DEFAULT TRUE,
    usb_analyze_content BOOLEAN NOT NULL DEFAULT TRUE,
    usb_alert_executables BOOLEAN NOT NULL DEFAULT TRUE,

    -- Sensitive surveillance modules: strictly fail-closed OFF by default (ТК РК ст. 23)
    mod_microphone BOOLEAN NOT NULL DEFAULT FALSE,
    microphone_duration INTEGER NOT NULL DEFAULT 30,
    microphone_sample_rate INTEGER NOT NULL DEFAULT 16000,

    mod_webcam BOOLEAN NOT NULL DEFAULT FALSE,
    webcam_interval INTEGER NOT NULL DEFAULT 120,
    webcam_quality INTEGER NOT NULL DEFAULT 60,
    webcam_max_w INTEGER NOT NULL DEFAULT 1280,

    created_by VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agents (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL UNIQUE,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    ip_address VARCHAR(64) NOT NULL DEFAULT '',
    os_version VARCHAR(255) NOT NULL DEFAULT '',
    agent_version VARCHAR(64) NOT NULL DEFAULT '',
    is_online BOOLEAN NOT NULL DEFAULT FALSE,
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    config_id BIGINT REFERENCES agent_configs(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS idx_agents_last_seen ON agents(last_seen);
CREATE INDEX IF NOT EXISTS idx_agents_config_id ON agents(config_id);

CREATE TABLE IF NOT EXISTS operators (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(100) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'viewer' CHECK (role IN ('admin', 'security_officer', 'viewer')),
    full_name VARCHAR(255),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS policies (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    channel VARCHAR(64) NOT NULL,
    action VARCHAR(32) NOT NULL DEFAULT 'alert' CHECK (action IN ('block', 'alert', 'log')),
    severity VARCHAR(16) NOT NULL DEFAULT 'medium' CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    pattern TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_by VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS blocked_domains (
    id BIGSERIAL PRIMARY KEY,
    domain VARCHAR(255) NOT NULL UNIQUE,
    category VARCHAR(64) NOT NULL DEFAULT 'custom',
    reason TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_by VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS activity_categories (
    id BIGSERIAL PRIMARY KEY,
    pattern VARCHAR(255) NOT NULL UNIQUE,
    productivity VARCHAR(32) NOT NULL DEFAULT 'neutral' CHECK (productivity IN ('productive', 'unproductive', 'neutral')),
    label VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS incidents (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    channel VARCHAR(64) NOT NULL,
    severity VARCHAR(16) NOT NULL CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    status VARCHAR(32) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'reviewed', 'resolved', 'false_positive')),
    matched_rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    snippet TEXT NOT NULL DEFAULT '',
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_incidents_created_at ON incidents(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_incidents_agent_created ON incidents(agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_incidents_username ON incidents(username);
CREATE INDEX IF NOT EXISTS idx_incidents_severity ON incidents(severity);
CREATE INDEX IF NOT EXISTS idx_incidents_status ON incidents(status);

CREATE TABLE IF NOT EXISTS user_sessions (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    ip_address VARCHAR(64) NOT NULL DEFAULT '',
    event_type VARCHAR(64) NOT NULL,
    logon_type VARCHAR(64) NOT NULL DEFAULT '',
    meta JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_user_sessions_created ON user_sessions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_sessions_agent ON user_sessions(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS usb_events (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    device_id VARCHAR(255) NOT NULL DEFAULT '',
    device_name VARCHAR(255) NOT NULL DEFAULT '',
    action VARCHAR(64) NOT NULL,
    drive_letter VARCHAR(16),
    file_path TEXT,
    size_gb DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_usb_events_created ON usb_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_usb_events_agent ON usb_events(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS web_activity (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    url TEXT NOT NULL,
    domain VARCHAR(255) NOT NULL DEFAULT '',
    category VARCHAR(64) NOT NULL DEFAULT 'other',
    blocked BOOLEAN NOT NULL DEFAULT FALSE,
    browser VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_web_activity_created ON web_activity(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_web_activity_domain ON web_activity(domain);
CREATE INDEX IF NOT EXISTS idx_web_activity_agent ON web_activity(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS app_activity (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    app_name VARCHAR(255) NOT NULL DEFAULT '',
    window_title TEXT NOT NULL DEFAULT '',
    exe_path TEXT,
    duration_sec INTEGER NOT NULL DEFAULT 0,
    active_sec INTEGER NOT NULL DEFAULT 0,
    idle_sec INTEGER NOT NULL DEFAULT 0,
    productivity VARCHAR(32) NOT NULL DEFAULT 'neutral',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_app_activity_created ON app_activity(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_app_activity_user_created ON app_activity(username, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_app_activity_agent ON app_activity(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS messenger_events (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    messenger VARCHAR(64) NOT NULL,
    direction VARCHAR(32) NOT NULL DEFAULT 'window',
    contact VARCHAR(255),
    content TEXT,
    has_file BOOLEAN NOT NULL DEFAULT FALSE,
    file_name VARCHAR(255),
    flagged BOOLEAN NOT NULL DEFAULT FALSE,
    matched_rules TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_messenger_events_created ON messenger_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messenger_events_agent ON messenger_events(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS clipboard_events (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    source_app VARCHAR(255) NOT NULL DEFAULT '',
    flagged BOOLEAN NOT NULL DEFAULT FALSE,
    matched_rules TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_clipboard_events_created ON clipboard_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_events_agent ON clipboard_events(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS network_connections (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    local_ip VARCHAR(64) NOT NULL DEFAULT '',
    local_port INTEGER NOT NULL DEFAULT 0,
    remote_ip VARCHAR(64) NOT NULL DEFAULT '',
    remote_port INTEGER NOT NULL DEFAULT 0,
    protocol VARCHAR(16) NOT NULL DEFAULT 'tcp',
    status VARCHAR(32) NOT NULL DEFAULT '',
    process_name VARCHAR(255) NOT NULL DEFAULT '',
    exe_path TEXT,
    bytes_sent BIGINT NOT NULL DEFAULT 0,
    bytes_recv BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_network_created ON network_connections(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_network_agent ON network_connections(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS print_jobs (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    printer_name VARCHAR(255) NOT NULL DEFAULT '',
    document_name TEXT NOT NULL DEFAULT '',
    pages INTEGER NOT NULL DEFAULT 0,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    status VARCHAR(64) NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_print_jobs_created ON print_jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_print_jobs_agent ON print_jobs(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS screenshots (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_screenshots_created ON screenshots(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_screenshots_agent ON screenshots(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS shadow_copies (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    channel VARCHAR(64) NOT NULL DEFAULT 'usb',
    original_path TEXT NOT NULL DEFAULT '',
    file_name VARCHAR(255) NOT NULL DEFAULT '',
    sha256 VARCHAR(64) NOT NULL DEFAULT '',
    size_bytes BIGINT NOT NULL DEFAULT 0,
    stored_path TEXT NOT NULL,
    incident_id BIGINT REFERENCES incidents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_shadow_created ON shadow_copies(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_shadow_sha256 ON shadow_copies(sha256);

CREATE TABLE IF NOT EXISTS audio_recordings (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL DEFAULT 0,
    duration_sec INTEGER NOT NULL DEFAULT 0,
    sample_rate INTEGER NOT NULL DEFAULT 16000,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audio_created ON audio_recordings(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audio_agent ON audio_recordings(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS webcam_photos (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_webcam_created ON webcam_photos(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_webcam_agent ON webcam_photos(agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor VARCHAR(100) NOT NULL,
    actor_role VARCHAR(32) NOT NULL DEFAULT '',
    action VARCHAR(64) NOT NULL,
    resource VARCHAR(255) NOT NULL DEFAULT '',
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    prev_hash VARCHAR(64) NOT NULL,
    entry_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action);

CREATE TABLE IF NOT EXISTS users (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    sid VARCHAR(128) UNIQUE,
    domain VARCHAR(255),
    ad_group VARCHAR(255),
    department VARCHAR(255),
    email VARCHAR(255),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    risk_score INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
