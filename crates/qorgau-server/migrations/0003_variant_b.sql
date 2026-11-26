-- Qorgau DLP v2.2 (Variant B — Active Prevention, USB Whitelist, File Monitor, Data Discovery, Inventory)

ALTER TABLE agent_configs
    ADD COLUMN IF NOT EXISTS usb_whitelist_mode BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS mod_file_monitor BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS mod_discovery BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS discovery_paths TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS mod_inventory BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS mod_app_blocker BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS mod_web_blocker BOOLEAN NOT NULL DEFAULT FALSE;

-- USB Device Manager (Менеджер ВНИ — white/black lists by VID/PID/Serial)
CREATE TABLE IF NOT EXISTS usb_devices (
    id BIGSERIAL PRIMARY KEY,
    serial_number VARCHAR(128) NOT NULL UNIQUE,
    vendor_id VARCHAR(32) NOT NULL DEFAULT '',
    product_id VARCHAR(32) NOT NULL DEFAULT '',
    label VARCHAR(255) NOT NULL DEFAULT '',
    status VARCHAR(16) NOT NULL DEFAULT 'allowed' CHECK (status IN ('allowed', 'blocked', 'readonly')),
    owner VARCHAR(128),
    comment TEXT,
    created_by VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Blocked Applications list
CREATE TABLE IF NOT EXISTS blocked_apps (
    id BIGSERIAL PRIMARY KEY,
    pattern VARCHAR(255) NOT NULL UNIQUE,
    action VARCHAR(16) NOT NULL DEFAULT 'block' CHECK (action IN ('block', 'alert')),
    reason TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_by VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Local File Operations (FileModule)
CREATE TABLE IF NOT EXISTS file_events (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    username VARCHAR(255) NOT NULL DEFAULT '',
    action VARCHAR(32) NOT NULL,
    file_path TEXT NOT NULL,
    new_path TEXT,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    sha256 VARCHAR(64) NOT NULL DEFAULT '',
    flagged BOOLEAN NOT NULL DEFAULT FALSE,
    matched_rules TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_file_events_created ON file_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_file_events_agent ON file_events(agent_id, created_at DESC);

-- Data Discovery / Crawler findings (Data at Rest)
CREATE TABLE IF NOT EXISTS discovery_findings (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL DEFAULT 0,
    sha256 VARCHAR(64) NOT NULL DEFAULT '',
    matched_rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    severity VARCHAR(16) NOT NULL DEFAULT 'medium' CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    snippet TEXT NOT NULL DEFAULT '',
    status VARCHAR(16) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved', 'ignored')),
    scanned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id, file_path)
);
CREATE INDEX IF NOT EXISTS idx_discovery_scanned ON discovery_findings(scanned_at DESC);
CREATE INDEX IF NOT EXISTS idx_discovery_status ON discovery_findings(status);

-- Hardware & Software Inventory (InventoryModule)
CREATE TABLE IF NOT EXISTS agent_inventory (
    agent_id VARCHAR(64) PRIMARY KEY,
    hostname VARCHAR(255) NOT NULL DEFAULT '',
    username VARCHAR(255) NOT NULL DEFAULT '',
    cpu_model VARCHAR(255) NOT NULL DEFAULT '',
    cpu_cores INTEGER NOT NULL DEFAULT 0,
    ram_total_mb BIGINT NOT NULL DEFAULT 0,
    disk_total_gb DOUBLE PRECISION NOT NULL DEFAULT 0,
    disk_free_gb DOUBLE PRECISION NOT NULL DEFAULT 0,
    os_build VARCHAR(255) NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS installed_software (
    id BIGSERIAL PRIMARY KEY,
    agent_id VARCHAR(64) NOT NULL,
    name VARCHAR(255) NOT NULL,
    version VARCHAR(128) NOT NULL DEFAULT '',
    publisher VARCHAR(255) NOT NULL DEFAULT '',
    install_date VARCHAR(64) NOT NULL DEFAULT '',
    status VARCHAR(16) NOT NULL DEFAULT 'installed' CHECK (status IN ('installed', 'removed')),
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id, name)
);
CREATE INDEX IF NOT EXISTS idx_software_agent ON installed_software(agent_id);
