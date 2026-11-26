//! Active Prevention: Application Blocker (`TerminateProcess`) + Web Domain Blocker (`hosts` sync).
//! Both are strictly opt-in via server AgentConfig (`mod_app_blocker`, `mod_web_blocker`).

use std::sync::Arc;
use std::time::Duration;

use qorgau_common::PreventionRulesDto;
use serde_json::json;
use tokio::sync::RwLock;

use crate::notifier::notify_user_once;
use crate::transport::AgentTransport;

const HOSTS_BEGIN: &str = "# BEGIN QORGAU DLP BLOCKLIST";
const HOSTS_END: &str = "# END QORGAU DLP BLOCKLIST";

pub async fn run_app_blocker(
    transport: Arc<AgentTransport>,
    mut enabled: tokio::sync::watch::Receiver<bool>,
    rules: Arc<RwLock<PreventionRulesDto>>,
    warn_user: bool,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(3));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }
        let patterns = rules.read().await.blocked_apps.clone();
        if patterns.is_empty() {
            continue;
        }

        let killed = tokio::task::spawn_blocking(move || scan_and_terminate_blocked(&patterns))
            .await
            .unwrap_or_default();

        for (proc_name, exe_path, pat) in killed {
            tracing::warn!(%proc_name, %pat, "Terminated blocked application");
            notify_user_once(
                warn_user,
                "Qorgau DLP — Запуск приложения запрещён",
                &format!("Приложение «{proc_name}» заблокировано политикой информационной безопасности."),
            );
            transport.enqueue(
                "app_blocked",
                json!({
                    "app_name": proc_name,
                    "exe_path": exe_path,
                    "pattern": pat,
                    "reason": format!("Заблокирован запуск запрещённого ПО: {proc_name} (правило: {pat})"),
                }),
            );
        }
    }
}

/// Synchronizes blocked domains into the Windows `hosts` file when `mod_web_blocker` is enabled.
/// Gracefully skips if the agent does not have Administrator/SYSTEM write privileges.
pub fn sync_hosts_blocklist(enabled: bool, domains: &[String]) {
    #[cfg(windows)]
    let hosts_path = std::path::PathBuf::from(
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string()),
    )
    .join(r"System32\drivers\etc\hosts");

    #[cfg(not(windows))]
    let hosts_path = std::path::PathBuf::from("/etc/hosts");

    let Ok(existing) = std::fs::read_to_string(&hosts_path) else {
        return;
    };

    // Strip any previous Qorgau blocklist section
    let mut cleaned = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        if line.trim() == HOSTS_BEGIN {
            skipping = true;
            continue;
        }
        if line.trim() == HOSTS_END {
            skipping = false;
            continue;
        }
        if !skipping {
            cleaned.push(line.to_string());
        }
    }

    if enabled && !domains.is_empty() {
        cleaned.push(HOSTS_BEGIN.to_string());
        for d in domains {
            let dom = d.trim().to_ascii_lowercase();
            if !dom.is_empty() && !dom.contains(char::is_whitespace) {
                cleaned.push(format!("0.0.0.0 {dom}"));
                if !dom.starts_with("www.") {
                    cleaned.push(format!("0.0.0.0 www.{dom}"));
                }
            }
        }
        cleaned.push(HOSTS_END.to_string());
    }

    let final_text = cleaned.join("\r\n") + "\r\n";
    if final_text != existing {
        if let Err(e) = std::fs::write(&hosts_path, final_text) {
            tracing::debug!(error = %e, "Unable to update hosts file (requires admin/SYSTEM)");
        } else {
            tracing::info!(
                enabled,
                count = domains.len(),
                "Synchronized web domain blocklist"
            );
        }
    }
}

#[cfg(windows)]
fn scan_and_terminate_blocked(patterns: &[String]) -> Vec<(String, String, String)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    let mut pids = vec![0u32; 2048];
    let mut bytes_returned = 0u32;
    let ok = unsafe {
        EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut bytes_returned,
        )
        .is_ok()
    };
    if !ok {
        return Vec::new();
    }

    let count = (bytes_returned as usize) / std::mem::size_of::<u32>();
    let my_pid = unsafe { GetCurrentProcessId() };
    let mut terminated = Vec::new();

    for &pid in &pids[..count] {
        if pid == 0 || pid == 4 || pid == my_pid {
            continue;
        }
        let (name, path) = crate::collectors::win_util::get_process_name_and_path(pid);
        if name.is_empty() || name.starts_with("pid:") {
            continue;
        }
        let name_l = name.to_ascii_lowercase();
        let path_l = path.to_ascii_lowercase();

        for pat in patterns {
            let pat_l = pat.to_ascii_lowercase();
            if !pat_l.is_empty() && (name_l.contains(&pat_l) || path_l.contains(&pat_l)) {
                unsafe {
                    if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                        let _ = TerminateProcess(h, 1);
                        let _ = CloseHandle(h);
                        terminated.push((name, path, pat.clone()));
                    }
                }
                break;
            }
        }
    }
    terminated
}

#[cfg(not(windows))]
fn scan_and_terminate_blocked(_patterns: &[String]) -> Vec<(String, String, String)> {
    Vec::new()
}
