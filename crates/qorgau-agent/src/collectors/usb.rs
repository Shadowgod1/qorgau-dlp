//! Native Win32 USB monitor (detects both removable flash drives AND external USB HDD/SSDs via BusTypeUsb).
//! Zero WMI / COM dependencies! Uses debounced file scanning so copied files finish writing
//! before SHA-256, content analysis, local DLP check, and shadow copy upload.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use qorgau_common::dlp::scan_builtin_dlp;
use serde_json::json;

use crate::collectors::file_analyzer::analyze_file;
use crate::notifier::notify_user_once;
use crate::transport::AgentTransport;

const SHADOW_EXTS: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf", "txt", "csv", "json", "xml", "sql",
    "zip", "rar", "7z", "png", "jpg", "jpeg", "bmp", "tif", "tiff",
];

#[derive(Debug, Clone)]
struct DriveInfo {
    letter: String,
    name: String,
    size_gb: f64,
}

pub async fn run(
    transport: Arc<AgentTransport>,
    mut enabled: tokio::sync::watch::Receiver<bool>,
    shadow_max_bytes: u64,
    warn_user: bool,
) {
    let mut known_drives: HashMap<String, DriveInfo> = HashMap::new();
    let mut seen_files: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(4));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }

        let current = enumerate_usb_drives();
        let current_letters: HashSet<String> = current.iter().map(|d| d.letter.clone()).collect();

        // Detect newly connected USB drives
        for drv in &current {
            if !known_drives.contains_key(&drv.letter) {
                tracing::info!(drive = %drv.letter, name = %drv.name, "USB drive connected");
                transport.enqueue(
                    "usb",
                    json!({
                        "device_id": format!("USB_{}", drv.letter),
                        "device_name": drv.name,
                        "action": "connected",
                        "drive_letter": drv.letter,
                        "size_gb": drv.size_gb,
                    }),
                );
                // Baseline existing files on newly connected drive so we only alert on newly copied files
                snapshot_existing_files(Path::new(&format!("{}\\", drv.letter)), &mut seen_files, 0);
                known_drives.insert(drv.letter.clone(), drv.clone());
            }
        }

        // Detect disconnected USB drives
        let removed: Vec<String> = known_drives
            .keys()
            .filter(|k| !current_letters.contains(*k))
            .cloned()
            .collect();
        for letter in removed {
            if let Some(drv) = known_drives.remove(&letter) {
                tracing::info!(drive = %drv.letter, "USB drive disconnected");
                transport.enqueue(
                    "usb",
                    json!({
                        "device_id": format!("USB_{}", drv.letter),
                        "device_name": drv.name,
                        "action": "disconnected",
                        "drive_letter": drv.letter,
                        "size_gb": drv.size_gb,
                    }),
                );
                seen_files.retain(|p, _| !p.to_string_lossy().starts_with(&letter));
            }
        }

        // Scan mounted USB drives for newly written/modified files (debounced: mtime >= 1.5s old)
        for drv in known_drives.values() {
            let root = PathBuf::from(format!("{}\\", drv.letter));
            let new_files = find_new_or_modified_files(&root, &mut seen_files, 0);
            for fpath in new_files {
                process_usb_file(&transport, drv, &fpath, shadow_max_bytes, warn_user).await;
            }
        }
    }
}

async fn process_usb_file(
    transport: &AgentTransport,
    drv: &DriveInfo,
    fpath: &Path,
    shadow_max_bytes: u64,
    warn_user: bool,
) {
    let fpath_owned = fpath.to_path_buf();
    let Some(analysis) = tokio::task::spawn_blocking(move || analyze_file(&fpath_owned))
        .await
        .ok()
        .flatten()
    else {
        return;
    };

    let matches = scan_builtin_dlp(&analysis.text_snippet);
    if matches.iter().any(|m| m.action == "block") {
        let names: Vec<&str> = matches.iter().map(|m| m.rule_name.as_str()).collect();
        notify_user_once(
            warn_user,
            "Qorgau DLP — Обнаружена утечка на USB",
            &format!(
                "В файле {} обнаружены конфиденциальные данные:\n{}",
                fpath.file_name().and_then(|s| s.to_str()).unwrap_or(""),
                names.join(", ")
            ),
        );
    }

    let fp_str = fpath.to_string_lossy().to_string();
    transport.enqueue(
        "usb",
        json!({
            "device_id": format!("USB_{}", drv.letter),
            "device_name": drv.name,
            "action": "file_copied",
            "drive_letter": drv.letter,
            "file_path": fp_str,
            "size_gb": (analysis.size_bytes as f64) / (1024.0 * 1024.0 * 1024.0),
            "sha256": analysis.sha256,
            "content_snippet": analysis.text_snippet,
        }),
    );

    // Flush event batch BEFORE uploading shadow copy so incident is created first for linkage!
    let _ = transport.flush_batch().await;

    let ext = fpath
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if analysis.size_bytes > 0
        && analysis.size_bytes <= shadow_max_bytes
        && SHADOW_EXTS.contains(&ext.as_str())
    {
        if let Ok(bytes) = tokio::fs::read(fpath).await {
            let fname = fpath
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file.bin")
                .to_string();
            transport.upload_shadow("usb", &fp_str, &fname, bytes).await;
        }
    }
}

fn snapshot_existing_files(dir: &Path, seen: &mut HashMap<PathBuf, (u64, SystemTime)>, depth: usize) {
    if depth > 4 || seen.len() > 20_000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if let Ok(meta) = ent.metadata() {
            if meta.is_dir() {
                snapshot_existing_files(&p, seen, depth + 1);
            } else if meta.is_file() {
                let mt = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                seen.insert(p, (meta.len(), mt));
            }
        }
    }
}

fn find_new_or_modified_files(
    dir: &Path,
    seen: &mut HashMap<PathBuf, (u64, SystemTime)>,
    depth: usize,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if depth > 4 {
        return found;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    let now = SystemTime::now();
    for ent in entries.flatten() {
        let p = ent.path();
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            let name = ent.file_name().to_string_lossy().to_ascii_lowercase();
            if name == "system volume information" || name == "$recycle.bin" {
                continue;
            }
            found.extend(find_new_or_modified_files(&p, seen, depth + 1));
        } else if meta.is_file() {
            let size = meta.len();
            let mt = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            // Debounce: wait until file hasn't changed for >= 1.5s so writer finishes
            let age_ok = now
                .duration_since(mt)
                .map(|d| d >= Duration::from_millis(1500))
                .unwrap_or(true);
            if !age_ok || size == 0 {
                continue;
            }
            let changed = match seen.get(&p) {
                Some(&(prev_size, prev_mt)) => prev_size != size || prev_mt != mt,
                None => true,
            };
            if changed {
                seen.insert(p.clone(), (size, mt));
                found.push(p);
            }
        }
    }
    found
}

#[cfg(windows)]
fn enumerate_usb_drives() -> Vec<DriveInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if (mask & (1 << i)) == 0 {
            continue;
        }
        let ch = (b'A' + i as u8) as char;
        // Skip C: system drive
        if ch == 'C' {
            continue;
        }
        let letter = format!("{ch}:");
        let root_w: Vec<u16> = format!("{ch}:\\\0").encode_utf16().collect();
        let dtype = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };

        let is_usb = if dtype == DRIVE_REMOVABLE {
            true
        } else if dtype == DRIVE_FIXED {
            // Check IOCTL_STORAGE_QUERY_PROPERTY for BusTypeUsb (catches external USB HDD/SSD!)
            is_usb_bus_drive(ch)
        } else {
            false
        };

        if !is_usb {
            continue;
        }

        let mut vol_name = [0u16; 128];
        let label = unsafe {
            if GetVolumeInformationW(
                PCWSTR(root_w.as_ptr()),
                Some(&mut vol_name),
                None,
                None,
                None,
                None,
            )
            .is_ok()
            {
                let end = vol_name.iter().position(|&c| c == 0).unwrap_or(vol_name.len());
                let s = String::from_utf16_lossy(&vol_name[..end]);
                if s.is_empty() {
                    format!("USB Drive ({letter})")
                } else {
                    s
                }
            } else {
                format!("USB Drive ({letter})")
            }
        };

        let mut total_bytes = 0u64;
        unsafe {
            let _ = GetDiskFreeSpaceExW(
                PCWSTR(root_w.as_ptr()),
                None,
                Some(&mut total_bytes),
                None,
            );
        }
        let size_gb = (total_bytes as f64) / (1024.0 * 1024.0 * 1024.0);

        out.push(DriveInfo {
            letter,
            name: label,
            size_gb: (size_gb * 100.0).round() / 100.0,
        });
    }

    fn is_usb_bus_drive(ch: char) -> bool {
        let dev_path: Vec<u16> = format!("\\\\.\\{ch}:\0").encode_utf16().collect();
        unsafe {
            let Ok(h) = CreateFileW(
                PCWSTR(dev_path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            ) else {
                return false;
            };

            let query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0],
            };
            let mut buf = [0u8; 512];
            let mut returned = 0u32;
            let ok = DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(&query as *const _ as *const _),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
            .is_ok();
            let _ = CloseHandle(h);

            if !ok || (returned as usize) < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
                return false;
            }
            let desc = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
            desc.BusType.0 == 7 // BusTypeUsb
        }
    }

    out
}

#[cfg(not(windows))]
fn enumerate_usb_drives() -> Vec<DriveInfo> {
    Vec::new()
}
