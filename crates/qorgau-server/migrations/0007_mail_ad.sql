-- Mail DLP events (SMTP / IMAP / Outlook / Exchange relay)
CREATE TABLE IF NOT EXISTS mail_events (
    id           BIGSERIAL PRIMARY KEY,
    agent_id     TEXT NOT NULL DEFAULT 'mail-relay',
    username     TEXT NOT NULL DEFAULT '',
    direction    TEXT NOT NULL DEFAULT 'outbound',
    protocol     TEXT NOT NULL DEFAULT 'smtp',
    sender       TEXT NOT NULL,
    recipients   JSONB NOT NULL DEFAULT '[]'::jsonb,
    subject      TEXT NOT NULL DEFAULT '',
    body_preview TEXT NOT NULL DEFAULT '',
    attachments  JSONB NOT NULL DEFAULT '[]'::jsonb,
    blocked      BOOLEAN NOT NULL DEFAULT FALSE,
    incident_id  BIGINT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_mail_events_created
    ON mail_events (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_mail_events_username
    ON mail_events (username, created_at DESC);

-- Active Directory / LDAP Directory settings & synced employee org structure
CREATE TABLE IF NOT EXISTS ad_settings (
    id                INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    enabled           BOOLEAN NOT NULL DEFAULT FALSE,
    server_url        TEXT NOT NULL DEFAULT '',
    base_dn           TEXT NOT NULL DEFAULT '',
    bind_dn           TEXT NOT NULL DEFAULT '',
    corporate_domains TEXT NOT NULL DEFAULT '',
    sync_interval_min INT NOT NULL DEFAULT 60,
    last_sync_at      TIMESTAMPTZ,
    last_sync_status  TEXT NOT NULL DEFAULT '',
    synced_count      INT NOT NULL DEFAULT 0,
    updated_by        TEXT,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO ad_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS ad_users (
    username           TEXT PRIMARY KEY,
    full_name          TEXT NOT NULL DEFAULT '',
    email              TEXT NOT NULL DEFAULT '',
    department         TEXT NOT NULL DEFAULT '',
    title              TEXT NOT NULL DEFAULT '',
    manager            TEXT NOT NULL DEFAULT '',
    is_enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    distinguished_name TEXT NOT NULL DEFAULT '',
    synced_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
