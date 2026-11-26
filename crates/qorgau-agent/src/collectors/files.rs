//! Local File Operations Monitor (`FileModule` — iBatyr parity).
//! Watches user Desktop, Documents, and Downloads for newly created, modified, and deleted
//! files (debounced, low-CPU, non-locking), sending SHA-256 + text snippets for DLP analysis.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::json;

use crate::collectors::file_analyzer::analyze_file;
use crate::transport::AgentTransport;

const TRACKED_EXTS: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf", "txt", "csv", "json", "xml", "sql",
    "zip", "rar", "7z", "key", "pem", "pfx", "kdbx",
];

pub async fn run(transport: Arc<AgentTransport>, mut enabled: tokio::sync::watch::Receiver<bool>) {
    let roots = default_user_dirs();
    let mut snapshot: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();

    // Build initial baseline without emitting historic files
    for r in &roots {
        collect_snapshot(r, &mut snapshot, 0);
    }

    let mut ticker = tokio::time::interval(Duration::from_secs(12));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }

        let mut current: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();
        for r in &roots {
            collect_snapshot(r, &mut current, 0);
        }

        let now = SystemTime::now();
        // Created or modified
        for (path, (size, mt)) in &current {
            let age_ok = now
                .duration_since(*mt)
                .map(|d| d >= Duration::from_secs(2))
                .unwrap_or(true);
            if !age_ok || *size == 0 {
                continue;
            }
            let action = match snapshot.get(path) {
                None => Some("created"),
                Some(&(ps, pmt)) if ps != *size || pmt != *mt => Some("modified"),
                _ => None,
            };
            if let Some(act) = action {
                let p_clone = path.clone();
                if let Ok(Some(analysis)) =
                    tokio::task::spawn_blocking(move || analyze_file(&p_clone)).await
                {
                    transport.enqueue(
                        "file",
                        json!({
                            "action": act,
                            "file_path": path.to_string_lossy(),
                            "size_bytes": analysis.size_bytes,
                            "sha256": analysis.sha256,
                            "content_snippet": analysis.text_snippet,
                        }),
                    );
                }
            }
        }

        // Deleted files
        for (path, (size, _)) in &snapshot {
            if !current.contains_key(path) {
                transport.enqueue(
                    "file",
                    json!({
                        "action": "deleted",
                        "file_path": path.to_string_lossy(),
                        "size_bytes": size,
                        "sha256": "",
                        "content_snippet": "",
                    }),
                );
            }
        }

        snapshot = current;
    }
}

pub fn default_user_dirs() -> Vec<PathBuf> {
    let profile = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if profile.is_empty() {
        return Vec::new();
    }
    let base = PathBuf::from(profile);
    ["Desktop", "Documents", "Downloads"]
        .iter()
        .map(|d| base.join(d))
        .filter(|p| p.exists())
        .collect()
}

fn collect_snapshot(dir: &Path, out: &mut HashMap<PathBuf, (u64, SystemTime)>, depth: usize) {
    if depth > 3 || out.len() > 15_000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            let name = ent.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            collect_snapshot(&p, out, depth + 1);
        } else if meta.is_file() {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if TRACKED_EXTS.contains(&ext.as_str()) {
                let mt = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                out.insert(p, (meta.len(), mt));
            }
        }
    }
}
