//! Browser history collector (Chrome, Edge, Brave — all profiles; Firefox — all profiles).
//! Fixes legacy bugs:
//!  * Uses persistent visit_id cursors in `DiskQueue` (captures revisits, never replays on restart)
//!  * Copies DB together with `-wal` and `-shm` files and guarantees cleanup
//!  * Uses native Windows `winsqlite3.dll` (zero binary bloat)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::transport::AgentTransport;
use crate::winsqlite::query_history_rows;

pub async fn run(transport: Arc<AgentTransport>, mut enabled: tokio::sync::watch::Receiver<bool>) {
    let mut cursors = transport.queue.load_cursors();
    let mut ticker = tokio::time::interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }

        let changed = scan_all_browsers(&transport, &mut cursors);
        if changed {
            transport.queue.save_cursors(&cursors);
        }
    }
}

fn scan_all_browsers(transport: &AgentTransport, cursors: &mut HashMap<String, i64>) -> bool {
    let mut dirty = false;
    let local_app = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let app_data = std::env::var("APPDATA").unwrap_or_default();

    // Chromium-based browsers (scan Default + Profile *)
    let chromium_roots = [
        ("chrome", PathBuf::from(&local_app).join(r"Google\Chrome\User Data")),
        ("edge", PathBuf::from(&local_app).join(r"Microsoft\Edge\User Data")),
        (
            "brave",
            PathBuf::from(&local_app).join(r"BraveSoftware\Brave-Browser\User Data"),
        ),
    ];

    for (browser, root) in chromium_roots {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for ent in entries.flatten() {
                let name = ent.file_name().to_string_lossy().to_string();
                if name == "Default" || name.starts_with("Profile ") {
                    let hist = ent.path().join("History");
                    if hist.exists() {
                        let key = format!("web:{browser}:{name}");
                        if scan_chromium_db(transport, browser, &hist, &key, cursors) {
                            dirty = true;
                        }
                    }
                }
            }
        }
    }

    // Firefox (scan all profiles)
    let ff_root = PathBuf::from(&app_data).join(r"Mozilla\Firefox\Profiles");
    if let Ok(entries) = std::fs::read_dir(&ff_root) {
        for ent in entries.flatten() {
            let places = ent.path().join("places.sqlite");
            if places.exists() {
                let key = format!("web:firefox:{}", ent.file_name().to_string_lossy());
                if scan_firefox_db(transport, &places, &key, cursors) {
                    dirty = true;
                }
            }
        }
    }

    dirty
}

fn copy_sqlite_with_wal(src: &Path) -> Option<PathBuf> {
    let tmp_dir = std::env::temp_dir().join(format!("qorgau_hist_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let dst = tmp_dir.join("hist.db");
    std::fs::copy(src, &dst).ok()?;

    // Copy companion WAL/SHM files so recent visits aren't lost
    for suffix in ["-wal", "-shm"] {
        let mut s = src.as_os_str().to_os_string();
        s.push(suffix);
        let src_side = PathBuf::from(s);
        if src_side.exists() {
            let mut d = dst.as_os_str().to_os_string();
            d.push(suffix);
            let _ = std::fs::copy(&src_side, PathBuf::from(d));
        }
    }
    Some(dst)
}

fn cleanup_temp_db(dst: &Path) {
    if let Some(parent) = dst.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

fn scan_chromium_db(
    transport: &AgentTransport,
    browser: &str,
    db_path: &Path,
    cursor_key: &str,
    cursors: &mut HashMap<String, i64>,
) -> bool {
    let Some(tmp) = copy_sqlite_with_wal(db_path) else {
        return false;
    };

    let last_visit = *cursors.get(cursor_key).unwrap_or(&0);
    // First run: advance cursor to current max without flooding historical backlog
    let changed = if last_visit == 0 {
        let max_rows = query_history_rows(&tmp, "SELECT COALESCE(MAX(id), 0), '' FROM visits");
        if let Some((max_id, _)) = max_rows.first() {
            if *max_id > 0 {
                cursors.insert(cursor_key.to_string(), *max_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    } else {
        // Query visits.id > last_visit so revisits to existing URLs ARE captured!
        let sql = format!(
            "SELECT v.id, u.url FROM visits v JOIN urls u ON v.url = u.id \
             WHERE v.id > {last_visit} ORDER BY v.id ASC LIMIT 200"
        );
        let rows = query_history_rows(&tmp, &sql);
        let mut max_seen = last_visit;
        for (vid, url) in rows {
            if vid > max_seen {
                max_seen = vid;
            }
            let domain = extract_domain(&url);
            if !domain.is_empty() {
                transport.enqueue(
                    "web",
                    json!({
                        "url": url.chars().take(500).collect::<String>(),
                        "domain": domain,
                        "browser": browser,
                    }),
                );
            }
        }
        if max_seen > last_visit {
            cursors.insert(cursor_key.to_string(), max_seen);
            true
        } else {
            false
        }
    };

    cleanup_temp_db(&tmp);
    changed
}

fn scan_firefox_db(
    transport: &AgentTransport,
    db_path: &Path,
    cursor_key: &str,
    cursors: &mut HashMap<String, i64>,
) -> bool {
    let Some(tmp) = copy_sqlite_with_wal(db_path) else {
        return false;
    };

    let last_visit = *cursors.get(cursor_key).unwrap_or(&0);
    let changed = if last_visit == 0 {
        let max_rows =
            query_history_rows(&tmp, "SELECT COALESCE(MAX(id), 0), '' FROM moz_historyvisits");
        if let Some((max_id, _)) = max_rows.first() {
            if *max_id > 0 {
                cursors.insert(cursor_key.to_string(), *max_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    } else {
        let sql = format!(
            "SELECT v.id, p.url FROM moz_historyvisits v JOIN moz_places p ON v.place_id = p.id \
             WHERE v.id > {last_visit} ORDER BY v.id ASC LIMIT 200"
        );
        let rows = query_history_rows(&tmp, &sql);
        let mut max_seen = last_visit;
        for (vid, url) in rows {
            if vid > max_seen {
                max_seen = vid;
            }
            let domain = extract_domain(&url);
            if !domain.is_empty() {
                transport.enqueue(
                    "web",
                    json!({
                        "url": url.chars().take(500).collect::<String>(),
                        "domain": domain,
                        "browser": "firefox",
                    }),
                );
            }
        }
        if max_seen > last_visit {
            cursors.insert(cursor_key.to_string(), max_seen);
            true
        } else {
            false
        }
    };

    cleanup_temp_db(&tmp);
    changed
}

pub fn extract_domain(url: &str) -> String {
    let without_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    let host_port = without_scheme.split('/').next().unwrap_or("");
    let host = host_port
        .split('@')
        .last()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    host.trim().to_ascii_lowercase()
}
