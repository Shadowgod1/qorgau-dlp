//! Native Win32 Hardware & Software Inventory collector (`InventoryModule` — iBatyr parity).
//! Reads CPU, RAM, Disk, OS build, and installed programs from Windows Registry (64-bit + 32-bit + HKCU)
//! with zero WMI overhead!

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;

use crate::transport::AgentTransport;

#[derive(Debug, Serialize)]
pub struct SoftwareItemPayload {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryPayload {
    pub agent_id: String,
    pub hostname: String,
    pub username: String,
    pub cpu_model: String,
    pub cpu_cores: i32,
    pub ram_total_mb: i64,
    pub disk_total_gb: f64,
    pub disk_free_gb: f64,
    pub os_build: String,
    pub software: Vec<SoftwareItemPayload>,
}

pub async fn collect_and_upload(transport: Arc<AgentTransport>) {
    let agent_id = transport.agent_id.clone();
    let hostname = transport.hostname.clone();
    let username = transport.username.clone();

    let payload = tokio::task::spawn_blocking(move || {
        collect_inventory_sync(agent_id, hostname, username)
    })
    .await;

    if let Ok(inv) = payload {
        tracing::info!(
            cpu = %inv.cpu_model,
            ram_mb = inv.ram_total_mb,
            software = inv.software.len(),
            "Collected workstation inventory"
        );
        transport.upload_inventory(inv).await;
    }
}

fn collect_inventory_sync(agent_id: String, hostname: String, username: String) -> InventoryPayload {
    let (cpu_model, cpu_cores, ram_total_mb, disk_total_gb, disk_free_gb, os_build) =
        probe_hardware();
    let software = enumerate_installed_software();

    InventoryPayload {
        agent_id,
        hostname,
        username,
        cpu_model,
        cpu_cores,
        ram_total_mb,
        disk_total_gb,
        disk_free_gb,
        os_build,
        software,
    }
}

#[cfg(windows)]
fn probe_hardware() -> (String, i32, i64, f64, f64, String) {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY};
    use windows::Win32::System::SystemInformation::{
        GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
    };

    let cpu_model = read_reg_str(
        HKEY_LOCAL_MACHINE,
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
        KEY_WOW64_64KEY.0,
    )
    .unwrap_or_else(|| "Unknown CPU".to_string());

    let product_name = read_reg_str(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductName",
        KEY_WOW64_64KEY.0,
    )
    .unwrap_or_else(|| "Windows".to_string());
    let build = read_reg_str(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
        KEY_WOW64_64KEY.0,
    )
    .unwrap_or_default();
    let os_build = if build.is_empty() {
        product_name
    } else {
        format!("{product_name} (Build {build})")
    };

    let cpu_cores = unsafe {
        let mut si = SYSTEM_INFO::default();
        GetSystemInfo(&mut si);
        si.dwNumberOfProcessors as i32
    };

    let ram_total_mb = unsafe {
        let mut ms = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut ms).is_ok() {
            (ms.ullTotalPhys / (1024 * 1024)) as i64
        } else {
            0
        }
    };

    let (disk_total_gb, disk_free_gb) = unsafe {
        let root: Vec<u16> = "C:\\\0".encode_utf16().collect();
        let mut free_avail = 0u64;
        let mut total = 0u64;
        let mut free = 0u64;
        if GetDiskFreeSpaceExW(
            PCWSTR(root.as_ptr()),
            Some(&mut free_avail),
            Some(&mut total),
            Some(&mut free),
        )
        .is_ok()
        {
            let gb = 1024.0 * 1024.0 * 1024.0;
            (
                ((total as f64 / gb) * 10.0).round() / 10.0,
                ((free as f64 / gb) * 10.0).round() / 10.0,
            )
        } else {
            (0.0, 0.0)
        }
    };

    (
        cpu_model.trim().to_string(),
        cpu_cores,
        ram_total_mb,
        disk_total_gb,
        disk_free_gb,
        os_build,
    )
}

#[cfg(not(windows))]
fn probe_hardware() -> (String, i32, i64, f64, f64, String) {
    ("Unknown".to_string(), 0, 0, 0.0, 0.0, std::env::consts::OS.to_string())
}

#[cfg(windows)]
fn enumerate_installed_software() -> Vec<SoftwareItemPayload> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS,
    };

    let hives = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_WOW64_64KEY.0,
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            KEY_WOW64_32KEY.0,
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            0,
        ),
    ];

    let mut dedup: BTreeMap<String, SoftwareItemPayload> = BTreeMap::new();

    for (root, path, wow_flag) in hives {
        unsafe {
            let sub_w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(
                root,
                PCWSTR(sub_w.as_ptr()),
                0,
                KEY_READ | REG_SAM_FLAGS(wow_flag),
                &mut hkey,
            )
            .is_err()
            {
                continue;
            }

            let mut idx = 0u32;
            loop {
                let mut name_buf = [0u16; 256];
                let mut name_len = name_buf.len() as u32;
                let res = RegEnumKeyExW(
                    hkey,
                    idx,
                    PWSTR(name_buf.as_mut_ptr()),
                    &mut name_len,
                    None,
                    PWSTR::null(),
                    None,
                    None,
                );
                if res.is_err() {
                    break;
                }
                idx += 1;
                let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let full_sub = format!("{path}\\{sub_name}");
                if let Some(disp) = read_reg_str(root, &full_sub, "DisplayName", wow_flag) {
                    let clean_name = disp.trim().to_string();
                    if clean_name.is_empty() {
                        continue;
                    }
                    let version = read_reg_str(root, &full_sub, "DisplayVersion", wow_flag)
                        .unwrap_or_default();
                    let publisher =
                        read_reg_str(root, &full_sub, "Publisher", wow_flag).unwrap_or_default();
                    let install_date =
                        read_reg_str(root, &full_sub, "InstallDate", wow_flag).unwrap_or_default();
                    dedup.entry(clean_name.clone()).or_insert(SoftwareItemPayload {
                        name: clean_name,
                        version,
                        publisher,
                        install_date,
                    });
                }
            }
            let _ = RegCloseKey(hkey);
        }
    }

    dedup.into_values().collect()
}

#[cfg(not(windows))]
fn enumerate_installed_software() -> Vec<SoftwareItemPayload> {
    Vec::new()
}

#[cfg(windows)]
fn read_reg_str(
    root: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value_name: &str,
    wow_flag: u32,
) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, KEY_READ, REG_SAM_FLAGS, REG_VALUE_TYPE,
    };

    unsafe {
        let sub_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let val_w: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            root,
            PCWSTR(sub_w.as_ptr()),
            0,
            KEY_READ | REG_SAM_FLAGS(wow_flag),
            &mut hkey,
        )
        .is_err()
        {
            return None;
        }
        let mut buf = vec![0u8; 512];
        let mut size = buf.len() as u32;
        let mut rtype = REG_VALUE_TYPE::default();
        let ok = RegQueryValueExW(
            hkey,
            PCWSTR(val_w.as_ptr()),
            None,
            Some(&mut rtype),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        if !ok || size < 2 {
            return None;
        }
        let u16s: Vec<u16> = buf[..(size as usize)]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&w| w != 0)
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    }
}
