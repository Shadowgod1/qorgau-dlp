use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::AppResult;

const AUDIT_ADVISORY_LOCK_KEY: i64 = 0x514F_5247_4155_0001; // "QORGAU\0\1"
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn compute_entry_hash(
    prev_hash: &str,
    ts_iso: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> String {
    let mut hasher = Sha256::new();
    let payload = format!(
        "{}|{}|{}|{}|{}|{}",
        prev_hash,
        ts_iso,
        actor,
        action,
        resource,
        canonical_json(details)
    );
    hasher.update(payload.as_bytes());
    hex::encode(hasher.finalize())
}

fn canonical_json(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = String::from("{");
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                out.push_str(&canonical_json(&map[*k]));
            }
            out.push('}');
            out
        }
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", items.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

/// Appends a tamper-evident entry to `audit_log` inside an isolated transaction
/// protected by a PostgreSQL advisory lock (multi-worker & concurrent-safe, never corrupts caller tx).
pub async fn write_audit(
    db: &PgPool,
    actor: &str,
    actor_role: &str,
    action: &str,
    resource: &str,
    details: Value,
) {
    if let Err(e) = write_audit_inner(db, actor, actor_role, action, resource, details).await {
        tracing::error!(error = %e, actor, action, resource, "Failed to write audit log entry");
    }
}

async fn write_audit_inner(
    db: &PgPool,
    actor: &str,
    actor_role: &str,
    action: &str,
    resource: &str,
    details: Value,
) -> AppResult<()> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(AUDIT_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let prev: Option<(String,)> =
        sqlx::query_as("SELECT entry_hash FROM audit_log ORDER BY id DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?;
    let prev_hash = prev.map(|r| r.0).unwrap_or_else(|| GENESIS_HASH.to_string());

    // Truncate to microsecond precision to match PostgreSQL TIMESTAMPTZ storage exactly
    let now = Utc::now();
    let ts_micros = now.timestamp_micros();
    let ts = DateTime::from_timestamp_micros(ts_micros).unwrap_or(now);
    let ts_iso = ts.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    let entry_hash = compute_entry_hash(&prev_hash, &ts_iso, actor, action, resource, &details);

    sqlx::query(
        "INSERT INTO audit_log (ts, actor, actor_role, action, resource, details, prev_hash, entry_hash) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(ts)
    .bind(actor)
    .bind(actor_role)
    .bind(action)
    .bind(resource)
    .bind(&details)
    .bind(&prev_hash)
    .bind(&entry_hash)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct AuditVerifyResult {
    pub valid: bool,
    pub total_entries: i64,
    pub broken_at_id: Option<i64>,
    pub message: String,
}

/// Verifies the entire `audit_log` hash chain in bounded pages (constant memory).
pub async fn verify_chain(db: &PgPool) -> AppResult<AuditVerifyResult> {
    let mut last_id: i64 = 0;
    let mut expected_prev = GENESIS_HASH.to_string();
    let mut total: i64 = 0;

    loop {
        let rows: Vec<(i64, DateTime<Utc>, String, String, String, Value, String, String)> =
            sqlx::query_as(
                "SELECT id, ts, actor, action, resource, details, prev_hash, entry_hash \
                 FROM audit_log WHERE id > $1 ORDER BY id ASC LIMIT 1000",
            )
            .bind(last_id)
            .fetch_all(db)
            .await?;

        if rows.is_empty() {
            break;
        }

        for (id, ts, actor, action, resource, details, prev_hash, entry_hash) in rows {
            total += 1;
            last_id = id;

            if prev_hash != expected_prev {
                return Ok(AuditVerifyResult {
                    valid: false,
                    total_entries: total,
                    broken_at_id: Some(id),
                    message: format!("Hash chain broken at entry #{id}: prev_hash mismatch"),
                });
            }

            let ts_iso = ts.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
            let recomputed =
                compute_entry_hash(&prev_hash, &ts_iso, &actor, &action, &resource, &details);
            if recomputed != entry_hash {
                return Ok(AuditVerifyResult {
                    valid: false,
                    total_entries: total,
                    broken_at_id: Some(id),
                    message: format!("Tampering detected at entry #{id}: entry_hash mismatch"),
                });
            }
            expected_prev = entry_hash;
        }
    }

    Ok(AuditVerifyResult {
        valid: true,
        total_entries: total,
        broken_at_id: None,
        message: format!("Audit chain intact ({total} entries verified)"),
    })
}
