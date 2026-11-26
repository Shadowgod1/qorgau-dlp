-- Runtime SIEM & Alerting configuration (DB-backed, overrides or complements .env)
CREATE TABLE IF NOT EXISTS siem_settings (
    id               INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    enabled          BOOLEAN NOT NULL DEFAULT FALSE,
    webhook_url      TEXT NOT NULL DEFAULT '',
    webhook_token    TEXT NOT NULL DEFAULT '',
    syslog_host      TEXT NOT NULL DEFAULT '',
    syslog_port      INT NOT NULL DEFAULT 514,
    syslog_proto     TEXT NOT NULL DEFAULT 'udp',
    telegram_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    telegram_token   TEXT NOT NULL DEFAULT '',
    telegram_chat_id TEXT NOT NULL DEFAULT '',
    min_severity     TEXT NOT NULL DEFAULT 'low',
    updated_by       TEXT,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO siem_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
