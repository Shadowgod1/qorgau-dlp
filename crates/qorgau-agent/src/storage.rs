use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use qorgau_common::EventItem;

/// Crash-safe, size-capped disk-backed JSONL offline queue + persistent key-value cursor store.
/// Eliminates unbounded RAM growth during server outages and preserves queued events across restarts.
pub struct DiskQueue {
    queue_path: PathBuf,
    cursors_path: PathBuf,
    max_bytes: u64,
    lock: Mutex<()>,
}

impl DiskQueue {
    pub fn new(data_dir: &Path, max_mb: u64) -> anyhow::Result<Self> {
        fs::create_dir_all(data_dir)?;
        Ok(Self {
            queue_path: data_dir.join("events.queue.jsonl"),
            cursors_path: data_dir.join("cursors.json"),
            max_bytes: max_mb.max(1) * 1024 * 1024,
            lock: Mutex::new(()),
        })
    }

    pub fn push(&self, event: &EventItem) {
        let Ok(_g) = self.lock.lock() else { return };
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };

        // Enforce disk cap by trimming oldest 25% if over limit
        if let Ok(meta) = fs::metadata(&self.queue_path) {
            if meta.len() >= self.max_bytes {
                self.trim_oldest_locked();
            }
        }

        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.queue_path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// Reads up to `max_items` events without removing them until `ack_batch` is called.
    pub fn peek_batch(&self, max_items: usize) -> Vec<EventItem> {
        let Ok(_g) = self.lock.lock() else {
            return Vec::new();
        };
        let Ok(f) = fs::File::open(&self.queue_path) else {
            return Vec::new();
        };
        let reader = BufReader::new(f);
        let mut out = Vec::with_capacity(max_items.min(256));
        for line in reader.lines().take(max_items) {
            if let Ok(l) = line {
                if let Ok(ev) = serde_json::from_str::<EventItem>(&l) {
                    out.push(ev);
                }
            }
        }
        out
    }

    /// Removes the first `count` successfully transmitted lines atomically via temp file + rename.
    pub fn ack_batch(&self, count: usize) {
        if count == 0 {
            return;
        }
        let Ok(_g) = self.lock.lock() else { return };
        let Ok(f) = fs::File::open(&self.queue_path) else {
            return;
        };
        let reader = BufReader::new(f);
        let remaining: Vec<String> = reader.lines().skip(count).flatten().collect();

        if remaining.is_empty() {
            let _ = fs::remove_file(&self.queue_path);
            return;
        }

        let tmp = self.queue_path.with_extension("tmp");
        if let Ok(mut out) = fs::File::create(&tmp) {
            for line in &remaining {
                let _ = writeln!(out, "{line}");
            }
            let _ = out.sync_all();
            let _ = fs::rename(&tmp, &self.queue_path);
        }
    }

    fn trim_oldest_locked(&self) {
        let Ok(f) = fs::File::open(&self.queue_path) else {
            return;
        };
        let lines: Vec<String> = BufReader::new(f).lines().flatten().collect();
        let drop_n = (lines.len() / 4).max(1);
        let tmp = self.queue_path.with_extension("tmp");
        if let Ok(mut out) = fs::File::create(&tmp) {
            for line in lines.iter().skip(drop_n) {
                let _ = writeln!(out, "{line}");
            }
            let _ = fs::rename(&tmp, &self.queue_path);
        }
    }

    pub fn load_cursors(&self) -> HashMap<String, i64> {
        let Ok(_g) = self.lock.lock() else {
            return HashMap::new();
        };
        fs::read_to_string(&self.cursors_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_cursors(&self, map: &HashMap<String, i64>) {
        let Ok(_g) = self.lock.lock() else { return };
        if let Ok(json) = serde_json::to_string_pretty(map) {
            let tmp = self.cursors_path.with_extension("tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.cursors_path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[test]
    fn disk_queue_persists_and_acks_fifo() {
        let dir = std::env::temp_dir().join(format!("qorgau_q_{}", uuid::Uuid::new_v4()));
        let q = DiskQueue::new(&dir, 1).unwrap();
        for i in 0..5 {
            q.push(&EventItem {
                event_type: "app".to_string(),
                data: json!({ "idx": i }),
                timestamp: Utc::now(),
            });
        }

        // Simulate restart by reopening queue from same dir
        let q2 = DiskQueue::new(&dir, 1).unwrap();
        let batch = q2.peek_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].data["idx"], 0);
        q2.ack_batch(3);

        let rest = q2.peek_batch(10);
        assert_eq!(rest.len(), 2);
        assert_eq!(rest[0].data["idx"], 3);
        let _ = fs::remove_dir_all(&dir);
    }
}
