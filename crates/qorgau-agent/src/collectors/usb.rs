//! Native Win32 USB monitor & Active USB Whitelist Enforcement (Менеджер ВНИ — iBatyr parity).
//! Detects BOTH removable flash drives AND external USB HDD/SSDs (`BusTypeUsb`), reads
//! real hardware VendorId / ProductId / SerialNumber from `STORAGE_DEVICE_DESCRIPTOR`,
//! dismounts/ejects unauthorized drives when `usb_whitelist_mode` or blocklist triggers,
//! and performs debounced content DLP analysis + shadow copying.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use qorgau_common::dlp::scan_builtin_dlp;
use qorgau_common::PreventionRulesDto;
use serde_json::json;
use tokio::sync::RwLock;

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
    vendor_id: String,
    product_id: String,
    serial_number: String,
    size_gb: f64,
}

pub async fn run(
    transport: Arc<AgentTransport>,
    mut enabled: tokio::sync::watch::Receiver<bool>,
    prevention_rules: Arc<RwLock<PreventionRulesDto>>,
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
        let rules = prevention_rules.read().await.clone();

        for drv in &current {
            // Check USB blocklist / whitelist enforcement
            let serial_l = drv.serial_number.to_ascii_lowercase();
            let explicitly_blocked = rules
                .blocked_usb_serials
                .iter()
                .any(|s| s.to_ascii_lowercase() == serial_l);
            let allowed_in_whitelist = rules
                .allowed_usb_serials
                .iter()
                .any(|s| s.to_ascii_lowercase() == serial_l);
            let unauthorized = explicitly_blocked || (rules.usb_whitelist_mode && !allowed_in_whitelist);

            if unauthorized {
                let letter_ch = drv.letter.chars().next().unwrap_or('E');
                let ejected = eject_usb_volume(letter_ch);
                if !known_drives.contains_key(&drv.letter) {
                    tracing::warn!(
                        drive = %drv.letter,
                        serial = %drv.serial_number,
                        ejected,
                        "Blocked unauthorized USB device"
                    );
                    notify_user_once(
                        warn_user,
                        "Qorgau DLP — USB-накопитель заблокирован",
                        &format!(
                            "Устройство {} (S/N: {}) не входит в белый список ВНИ и заблокировано политикой ИБ.",
                            drv.name, drv.serial_number
                        ),
                    );
                    transport.enqueue(
                        "usb_blocked",
                        json!({
                            "device_id": drv.serial_number,
                            "device_name": drv.name,
                            "vendor_id": drv.vendor_id,
                            "product_id": drv.product_id,
                            "drive_letter": drv.letter,
                            "ejected": ejected,
                            "reason": format!("Неавторизованный USB-носитель {} (S/N: {})", drv.name, drv.serial_number),
                        }),
                    );
                }
                if ejected {
                    continue;
                }
            }

            if !known_drives.contains_key(&drv.letter) {
                tracing::info!(
                    drive = %drv.letter,
                    name = %drv.name,
                    serial = %drv.serial_number,
                    "USB drive connected"
                );
                transport.enqueue(
                    "usb",
                    json!({
                        "device_id": drv.serial_number,
                        "device_name": drv.name,
                        "vendor_id": drv.vendor_id,
                        "product_id": drv.product_id,
                        "action": "connected",
                        "drive_letter": drv.letter,
                        "size_gb": drv.size_gb,
                    }),
                );
                snapshot_existing_files(Path::new(&format!("{}\\", drv.letter)), &mut seen_files, 0);
                known_drives.insert(drv.letter.clone(), drv.clone());
            }
        }

        // Disconnected drives
        let removed: Vec<String> = known_drives
            .keys()
            .filter(|k| !current_letters.contains(*k))
            .cloned()
            .collect();
        for letter in removed {
            if let Some(drv) = known_drives.remove(&letter) {
                transport.enqueue(
                    "usb",
                    json!({
                        "device_id": drv.serial_number,
                        "device_name": drv.name,
                        "action": "disconnected",
                        "drive_letter": drv.letter,
                        "size_gb": drv.size_gb,
                    }),
                );
                seen_files.retain(|p, _| !p.to_string_lossy().starts_with(&letter));
            }
        }

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
            "device_id": drv.serial_number,
            "device_name": drv.name,
            "action": "file_copied",
            "drive_letter": drv.letter,
            "file_path": fp_str,
            "size_gb": (analysis.size_bytes as f64) / (1024.0 * 1024.0 * 1024.0),
            "sha256": analysis.sha256,
            "content_snippet": analysis.text_snippet,
        }),
    );

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

fn snapshot_existing_files(
    dir: &Path,
    seen: &mut HashMap<PathBuf, (u64, SystemTime)>,
    depth: usize,
) {
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
fn read_cstr_at(buf: &[u8], offset: u32) -> String {
    let off = offset as usize;
    if off == 0 || off >= buf.len() {
        return String::new();
    }
    let slice = &buf[off..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

#[cfg(windows)]
fn query_storage_descriptor(ch: char) -> Option<(bool, String, String, String)> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
        STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    let dev_path: Vec<u16> = format!("\\\\.\\{ch}:\0").encode_utf16().collect();
    unsafe {
        let h = CreateFileW(
            PCWSTR(dev_path.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
        .ok()?;

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut buf = [0u8; 1024];
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
            return None;
        }
        let desc = &*(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);
        let is_usb = desc.BusType.0 == 7; // BusTypeUsb
        let vendor = read_cstr_at(&buf[..returned as usize], desc.VendorIdOffset);
        let product = read_cstr_at(&buf[..returned as usize], desc.ProductIdOffset);
        let serial = read_cstr_at(&buf[..returned as usize], desc.SerialNumberOffset);
        Some((is_usb, vendor, product, serial))
    }
}

/// Actively locks, dismounts, and ejects an unauthorized USB volume.
#[cfg(windows)]
fn eject_usb_volume(ch: char) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, IOCTL_STORAGE_EJECT_MEDIA,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    let dev_path: Vec<u16> = format!("\\\\.\\{ch}:\0").encode_utf16().collect();
    unsafe {
        let Ok(h) = CreateFileW(
            PCWSTR(dev_path.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        ) else {
            return false;
        };
        let mut br = 0u32;
        let _ = DeviceIoControl(h, FSCTL_LOCK_VOLUME, None, 0, None, 0, Some(&mut br), None);
        let _ = DeviceIoControl(h, FSCTL_DISMOUNT_VOLUME, None, 0, None, 0, Some(&mut br), None);
        let ok = DeviceIoControl(h, IOCTL_STORAGE_EJECT_MEDIA, None, 0, None, 0, Some(&mut br), None).is_ok();
        let _ = CloseHandle(h);
        ok
    }
}

#[cfg(not(windows))]
fn eject_usb_volume(_ch: char) -> bool {
    false
}

#[cfg(windows)]
fn enumerate_usb_drives() -> Vec<DriveInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if (mask & (1 << i)) == 0 {
            continue;
        }
        let ch = (b'A' + i as u8) as char;
        if ch == 'C' {
            continue;
        }
        let letter = format!("{ch}:");
        let root_w: Vec<u16> = format!("{ch}:\\\0").encode_utf16().collect();
        let dtype = unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) };

        let desc = query_storage_descriptor(ch);
        let is_usb = if dtype == DRIVE_REMOVABLE {
            true
        } else if dtype == DRIVE_FIXED {
            desc.as_ref().map(|d| d.0).unwrap_or(false)
        } else {
            false
        };
        if !is_usb {
            continue;
        }

        let (vendor_id, product_id, mut serial_number) = desc
            .map(|(_, v, p, s)| (v, p, s))
            .unwrap_or_default();

        let mut vol_name = [0u16; 128];
        let mut vol_serial = 0u32;
        let label = unsafe {
            if GetVolumeInformationW(
                PCWSTR(root_w.as_ptr()),
                Some(&mut vol_name),
                Some(&mut vol_serial),
                None,
                None,
                None,
            )
            .is_ok()
            {
                let end = vol_name.iter().position(|&c| c == 0).unwrap_or(vol_name.len());
                let s = String::from_utf16_lossy(&vol_name[..end]);
                if s.is_empty() {
                    format!("{vendor_id} {product_id}").trim().to_string()
                } else {
                    s
                }
            } else {
                format!("USB Drive ({letter})")
            }
        };

        if serial_number.is_empty() {
            serial_number = format!("VOL_{:08X}", vol_serial);
        }

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
            name: if label.is_empty() {
                format!("USB ({ch}:)")
            } else {
                label
            },
            vendor_id,
            product_id,
            serial_number,
            size_gb: (size_gb * 100.0).round() / 100.0,
        });
    }
    out
}

#[cfg(not(windows))]
fn enumerate_usb_drives() -> Vec<DriveInfo> {
    Vec::new()
}
