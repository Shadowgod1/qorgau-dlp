//! Local Disk Data Discovery Crawler (iBatyr «Краулер» parity).
//! Scans configured directories (or Desktop/Documents/Downloads) for files containing
//! Kazakhstan PII / financial data / secrecy markings and reports findings to the server.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use qorgau_common::dlp::scan_builtin_dlp;
use qorgau_common::Severity;
use serde::Serialize;

use crate::collectors::file_analyzer::analyze_file;
use crate::collectors::files::default_user_dirs;
use crate::transport::AgentTransport;

static SCAN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

const SCAN_EXTS: &[&str] = &[
    "txt", "csv", "json", "xml", "sql", "log", "docx", "xlsx", "pptx", "pdf", "zip",
];

#[derive(Debug, Serialize)]
pub struct DiscoveryFindingPayload {
    pub file_path: String,
    pub file_size: i64,
    pub sha256: String,
    pub matched_rules: Vec<String>,
    pub severity: String,
    pub snippet: String,
}

pub async fn run_discovery_scan(transport: Arc<AgentTransport>, custom_paths_csv: String) {
    if SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return;
    }

    let roots: Vec<PathBuf> = if custom_paths_csv.trim().is_empty() {
        default_user_dirs()
    } else {
        custom_paths_csv
            .split(',')
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.exists())
            .collect()
    };

    tracing::info!(roots = roots.len(), "Starting Data Discovery crawl");

    let findings = tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        let mut visited = 0usize;
        for r in &roots {
            crawl_dir(r, 0, &mut visited, &mut results);
        }
        results
    })
    .await
    .unwrap_or_default();

    tracing::info!(findings = findings.len(), "Data Discovery crawl finished");
    if !findings.is_empty() {
        transport.upload_discovery(findings).await;
    }

    SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
}

fn crawl_dir(
    dir: &Path,
    depth: usize,
    visited: &mut usize,
    findings: &mut Vec<DiscoveryFindingPayload>,
) {
    if depth > 4 || *visited >= 5000 || findings.len() >= 300 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        if *visited >= 5000 || findings.len() >= 300 {
            break;
        }
        let p = ent.path();
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            let name = ent.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | "windows" | "program files") {
                continue;
            }
            crawl_dir(&p, depth + 1, visited, findings);
        } else if meta.is_file() {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if !SCAN_EXTS.contains(&ext.as_str()) {
                continue;
            }
            *visited += 1;
            if let Some(analysis) = analyze_file(&p) {
                let matches = scan_builtin_dlp(&analysis.text_snippet);
                // Ignore purely low-severity phone-only hits to keep discovery high-signal
                let important: Vec<_> = matches
                    .into_iter()
                    .filter(|m| m.severity >= Severity::Medium)
                    .collect();
                if !important.is_empty() {
                    let max_sev = important
                        .iter()
                        .map(|m| m.severity)
                        .max()
                        .unwrap_or(Severity::Medium);
                    findings.push(DiscoveryFindingPayload {
                        file_path: p.to_string_lossy().to_string(),
                        file_size: analysis.size_bytes as i64,
                        sha256: analysis.sha256,
                        matched_rules: important.into_iter().map(|m| m.rule_name).collect(),
                        severity: max_sev.as_str().to_string(),
                        snippet: analysis.text_snippet.chars().take(300).collect(),
                    });
                }
            }
        }
    }
}
