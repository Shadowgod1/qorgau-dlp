-- Qorgau DLP v2.1 (Variant A — iBatyr 5.6 feature parity)
-- Adds: Screen Watermarks config, Incident Legal Hold & SOC workflow, Comments, Risk Threshold alerts

ALTER TABLE agent_configs
    ADD COLUMN IF NOT EXISTS mod_watermark BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS watermark_opacity INTEGER NOT NULL DEFAULT 25,
    ADD COLUMN IF NOT EXISTS watermark_text TEXT NOT NULL DEFAULT '{username} • {hostname} • {time}',
    ADD COLUMN IF NOT EXISTS watermark_apps TEXT NOT NULL DEFAULT '';

ALTER TABLE incidents
    ADD COLUMN IF NOT EXISTS legal_hold BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS assigned_to VARCHAR(100),
    ADD COLUMN IF NOT EXISTS group_tag VARCHAR(120),
    ADD COLUMN IF NOT EXISTS resolution_note TEXT;

CREATE INDEX IF NOT EXISTS idx_incidents_legal_hold ON incidents(legal_hold) WHERE legal_hold = TRUE;
CREATE INDEX IF NOT EXISTS idx_incidents_assigned ON incidents(assigned_to);

CREATE TABLE IF NOT EXISTS incident_comments (
    id BIGSERIAL PRIMARY KEY,
    incident_id BIGINT NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    author VARCHAR(100) NOT NULL,
    author_role VARCHAR(32) NOT NULL DEFAULT '',
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_incident_comments_inc ON incident_comments(incident_id, created_at ASC);

CREATE TABLE IF NOT EXISTS alert_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    risk_threshold INTEGER NOT NULL DEFAULT 60,
    notify_on_risk_growth BOOLEAN NOT NULL DEFAULT TRUE,
    updated_by VARCHAR(100),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO alert_settings (id, risk_threshold, notify_on_risk_growth)
VALUES (1, 60, TRUE)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS user_risk_alerts (
    username VARCHAR(255) PRIMARY KEY,
    last_notified_score INTEGER NOT NULL DEFAULT 0,
    last_notified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
