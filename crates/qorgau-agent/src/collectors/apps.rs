use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::collectors::win_util::{get_foreground_info, get_idle_seconds, ForegroundInfo};
use crate::transport::AgentTransport;

const IDLE_THRESHOLD_SEC: u64 = 60;
const MAX_SLICE_SEC: u64 = 300; // Flush long uninterrupted focus slices every 5 min

/// Tracks active foreground windows + active/idle time split.
/// Uses 64-bit `GetTickCount64` and flushes periodically so long sessions are never lost on shutdown.
pub async fn run(transport: Arc<AgentTransport>, mut enabled: tokio::sync::watch::Receiver<bool>) {
    let mut current: Option<ForegroundInfo> = None;
    let mut slice_start = Instant::now();
    let mut active_sec: u64 = 0;
    let mut idle_sec: u64 = 0;

    let mut ticker = tokio::time::interval(Duration::from_secs(3));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            current = None;
            continue;
        }

        let fg = get_foreground_info();
        if fg.app_name.is_empty() {
            continue;
        }

        let is_idle = get_idle_seconds() >= IDLE_THRESHOLD_SEC;

        match &current {
            Some(prev)
                if prev.app_name == fg.app_name
                    && prev.window_title == fg.window_title
                    && slice_start.elapsed().as_secs() < MAX_SLICE_SEC =>
            {
                if is_idle {
                    idle_sec += 3;
                } else {
                    active_sec += 3;
                }
            }
            Some(prev) => {
                let dur = active_sec + idle_sec;
                if dur >= 5 {
                    transport.enqueue(
                        "app",
                        json!({
                            "app_name": prev.app_name,
                            "window_title": prev.window_title,
                            "exe_path": prev.exe_path,
                            "duration_sec": dur,
                            "active_sec": active_sec,
                            "idle_sec": idle_sec,
                        }),
                    );
                }
                current = Some(fg);
                slice_start = Instant::now();
                active_sec = if is_idle { 0 } else { 3 };
                idle_sec = if is_idle { 3 } else { 0 };
            }
            None => {
                current = Some(fg);
                slice_start = Instant::now();
                active_sec = 0;
                idle_sec = 0;
            }
        }
    }
}
