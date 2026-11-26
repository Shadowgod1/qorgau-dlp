use std::path::Path;

use qorgau_common::dlp::scan_builtin_dlp;
use serde_json::json;
use tokio::process::Command;

use crate::AgentClient;

/// Parses `ps -eo pid,comm,args` output into `(pid, comm, args)` tuples.
pub fn parse_ps_lines(raw: &str) -> Vec<(u32, String, String)> {
    let mut out = Vec::new();
    for line in raw.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(3, char::is_whitespace);
        let pid_str = parts.next().unwrap_or("");
        let comm = parts.next().unwrap_or("").trim();
        let args = parts.next().unwrap_or(comm).trim();
        if let Ok(pid) = pid_str.parse::<u32>() {
            if !comm.is_empty() {
                out.push((pid, comm.to_string(), args.to_string()));
            }
        }
    }
    out
}

/// Parses `/proc/mounts` lines and returns removable / external mountpoints (`/media`, `/mnt`, `/run/media`, `/Volumes`).
pub fn parse_removable_mounts(raw: &str) -> Vec<(String, String)> {
    let mut mounts = Vec::new();
    for line in raw.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        let dev = cols[0];
        let mp = cols[1];
        if mp.starts_with("/media/")
            || mp.starts_with("/run/media/")
            || mp.starts_with("/mnt/")
            || mp.starts_with("/Volumes/")
        {
            mounts.push((dev.to_string(), mp.to_string()));
        }
    }
    mounts
}

pub async fn collect_processes(client: &AgentClient) {
    let Ok(out) = Command::new("ps").args(["-eo", "pid,comm,args"]).output().await else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let procs = parse_ps_lines(&String::from_utf8_lossy(&out.stdout));
    let events: Vec<_> = procs
        .into_iter()
        .take(40)
        .map(|(_pid, comm, args)| {
            json!({
                "app_name": comm,
                "window_title": args.chars().take(160).collect::<String>(),
                "duration_sec": 30,
                "active_sec": 30,
                "idle_sec": 0,
                "productivity": "neutral",
            })
        })
        .collect();

    if !events.is_empty() {
        client
            .post_json(
                "/events/app",
                json!({
                    "agent_id": client.agent_id,
                    "username": client.username,
                    "events": events,
                }),
            )
            .await;
    }
}

pub async fn collect_mounts(client: &AgentClient) {
    let raw = tokio::fs::read_to_string("/proc/mounts").await.unwrap_or_default();
    let mounts = parse_removable_mounts(&raw);
    for (dev, mp) in mounts {
        client
            .post_json(
                "/events/usb",
                json!({
                    "agent_id": client.agent_id,
                    "username": client.username,
                    "device_id": dev,
                    "device_name": mp,
                    "action": "connected",
                    "file_path": mp,
                }),
            )
            .await;
    }
}

pub async fn collect_network(client: &AgentClient) {
    let Ok(out) = Command::new("ss").args(["-tun"]).output().await else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut conns = Vec::new();
    for line in text.lines().skip(1).take(30) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 6 {
            conns.push(json!({
                "process_name": "unix-socket",
                "remote_ip": cols[5],
                "remote_port": 0,
                "protocol": cols[0],
                "state": cols[1],
            }));
        }
    }
    if !conns.is_empty() {
        client
            .post_json(
                "/events/network",
                json!({
                    "agent_id": client.agent_id,
                    "username": client.username,
                    "connections": conns,
                }),
            )
            .await;
    }
}

pub async fn run_discovery(client: &AgentClient, paths_csv: &str) {
    let mut findings = Vec::new();
    for raw_dir in paths_csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let expanded = if let Some(rest) = raw_dir.strip_prefix("~/") {
            crate::dirs_home().join(rest)
        } else {
            Path::new(raw_dir).to_path_buf()
        };
        let Ok(mut rd) = tokio::fs::read_dir(&expanded).await else {
            continue;
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            if let Ok(content) = tokio::fs::read_to_string(&p).await {
                let hits = scan_builtin_dlp(&content);
                if !hits.is_empty() {
                    let labels: Vec<String> = hits.into_iter().map(|h| h.rule_name).collect();
                    findings.push(json!({
                        "file_path": p.to_string_lossy(),
                        "file_size": content.len(),
                        "matched_rules": labels,
                        "severity": "high",
                    }));
                }
            }
            if findings.len() >= 100 {
                break;
            }
        }
    }

    if !findings.is_empty() {
        client
            .post_json(
                "/discovery/report",
                json!({
                    "agent_id": client.agent_id,
                    "hostname": client.hostname,
                    "username": client.username,
                    "findings": findings,
                }),
            )
            .await;
    }
}

pub async fn run_inventory(client: &AgentClient) {
    let os_build = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    client
        .post_json(
            "/inventory/report",
            json!({
                "agent_id": client.agent_id,
                "hostname": client.hostname,
                "username": client.username,
                "cpu_model": std::env::consts::ARCH,
                "cpu_cores": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
                "ram_total_mb": 0,
                "disk_total_gb": 0,
                "disk_free_gb": 0,
                "os_build": os_build,
                "software": [],
            }),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ps_and_mounts_cleanly() {
        let ps = "  PID COMMAND         COMMAND\n  1234 firefox         /usr/lib/firefox/firefox --new-window\n";
        let procs = parse_ps_lines(ps);
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0].0, 1234);
        assert_eq!(procs[0].1, "firefox");

        let mounts = "/dev/sda1 / ext4 rw 0 0\n/dev/sdb1 /media/user/USB vfat rw 0 0\n";
        let rem = parse_removable_mounts(mounts);
        assert_eq!(rem.len(), 1);
        assert_eq!(rem[0].1, "/media/user/USB");
    }
}
