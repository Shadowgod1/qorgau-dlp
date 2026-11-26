-- UEBA (User & Entity Behavior Analytics) detected anomalies
CREATE TABLE IF NOT EXISTS ueba_anomalies (
    id            BIGSERIAL PRIMARY KEY,
    username      TEXT NOT NULL,
    anomaly_type  TEXT NOT NULL,
    severity      TEXT NOT NULL DEFAULT 'medium',
    score_delta   INT NOT NULL DEFAULT 15,
    title         TEXT NOT NULL,
    details       JSONB NOT NULL DEFAULT '{}'::jsonb,
    status        TEXT NOT NULL DEFAULT 'open',
    detected_date DATE NOT NULL DEFAULT CURRENT_DATE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (username, anomaly_type, detected_date)
);

CREATE INDEX IF NOT EXISTS idx_ueba_anomalies_status_created
    ON ueba_anomalies (status, created_at DESC);
