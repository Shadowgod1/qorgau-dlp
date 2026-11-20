use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentConfig {
    #[serde(default = "def_server_url")]
    pub server_url: String,
    #[serde(default = "def_token")]
    pub agent_token: String,
    #[serde(default = "def_batch_sec")]
    pub batch_interval_sec: u64,
    #[serde(default = "def_heartbeat_sec")]
    pub heartbeat_interval_sec: u64,
    #[serde(default = "def_queue_mb")]
    pub max_queue_mb: u64,
    #[serde(default = "def_shadow_mb")]
    pub shadow_max_mb: u64,
    #[serde(default = "def_true")]
    pub clipboard_block: bool,
    #[serde(default = "def_true")]
    pub warn_user_enabled: bool,
    #[serde(default)]
    pub install_autostart: bool,
    #[serde(default = "def_data_dir")]
    pub data_dir: PathBuf,
}

fn def_server_url() -> String {
    std::env::var("QORGAU_SERVER_URL").unwrap_or_else(|_| "http://localhost".to_string())
}
fn def_token() -> String {
    std::env::var("QORGAU_AGENT_TOKEN").unwrap_or_else(|_| "qorgau_agent_token_2024".to_string())
}
fn def_batch_sec() -> u64 {
    15
}
fn def_heartbeat_sec() -> u64 {
    60
}
fn def_queue_mb() -> u64 {
    50
}
fn def_shadow_mb() -> u64 {
    48
}
fn def_true() -> bool {
    true
}
fn def_data_dir() -> PathBuf {
    if let Ok(pd) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(pd).join("QorgauDLP")
    } else {
        std::env::temp_dir().join("QorgauDLP")
    }
}

impl Default for LocalAgentConfig {
    fn default() -> Self {
        Self {
            server_url: def_server_url(),
            agent_token: def_token(),
            batch_interval_sec: def_batch_sec(),
            heartbeat_interval_sec: def_heartbeat_sec(),
            max_queue_mb: def_queue_mb(),
            shadow_max_mb: def_shadow_mb(),
            clipboard_block: true,
            warn_user_enabled: true,
            install_autostart: false,
            data_dir: def_data_dir(),
        }
    }
}

impl LocalAgentConfig {
    pub fn load() -> Self {
        let candidates = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("config.toml"))),
            Some(PathBuf::from(r"C:\ProgramData\QorgauDLP\config.toml")),
            Some(PathBuf::from("config.toml")),
        ];
        for cand in candidates.into_iter().flatten() {
            if let Ok(raw) = std::fs::read_to_string(&cand) {
                if let Ok(mut parsed) = toml::from_str::<Self>(&raw) {
                    apply_env_overrides(&mut parsed);
                    return parsed;
                }
            }
        }
        let mut cfg = Self::default();
        apply_env_overrides(&mut cfg);
        cfg
    }
}

fn apply_env_overrides(cfg: &mut LocalAgentConfig) {
    if let Ok(v) = std::env::var("QORGAU_SERVER_URL") {
        if !v.trim().is_empty() {
            cfg.server_url = v.trim().trim_end_matches('/').to_string();
        }
    }
    if let Ok(v) = std::env::var("QORGAU_AGENT_TOKEN") {
        if !v.trim().is_empty() {
            cfg.agent_token = v.trim().to_string();
        }
    }
}

/// Reads Windows `MachineGuid` from HKLM\SOFTWARE\Microsoft\Cryptography,
/// falling back to a persistent local UUID file so `agent_id` never drifts across restarts.
pub fn resolve_machine_id(data_dir: &Path) -> String {
    #[cfg(windows)]
    if let Some(guid) = read_windows_machine_guid() {
        let cleaned: String = guid
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if cleaned.len() >= 8 {
            return cleaned;
        }
    }

    let id_file = data_dir.join("agent_id.txt");
    if let Ok(existing) = std::fs::read_to_string(&id_file) {
        let t = existing.trim();
        if qorgau_common::validate_agent_id(t).is_ok() {
            return t.to_string();
        }
    }

    let fresh = uuid::Uuid::new_v4().simple().to_string();
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(&id_file, &fresh);
    fresh
}

#[cfg(windows)]
fn read_windows_machine_guid() -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_WOW64_64KEY, REG_VALUE_TYPE,
    };

    unsafe {
        let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0".encode_utf16().collect();
        let val_name: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        )
        .is_err()
        {
            return None;
        }

        let mut buf = vec![0u8; 256];
        let mut size = buf.len() as u32;
        let mut rtype = REG_VALUE_TYPE::default();
        let ok = RegQueryValueExW(
            hkey,
            PCWSTR(val_name.as_ptr()),
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

pub fn current_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

pub fn current_username() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(windows)]
pub fn register_autostart_if_enabled(enabled: bool) {
    if !enabled {
        return;
    }
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
    };

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let val = format!("\"{}\"", exe.display());
    let val_wide: Vec<u16> = val.encode_utf16().chain(std::iter::once(0)).collect();
    let val_bytes: Vec<u8> = val_wide.iter().flat_map(|w| w.to_le_bytes()).collect();

    unsafe {
        let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
            .encode_utf16()
            .collect();
        let name: Vec<u16> = "QorgauAgent\0".encode_utf16().collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        )
        .is_ok()
        {
            let _ = RegSetValueExW(hkey, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(&val_bytes));
            let _ = RegCloseKey(hkey);
        }
    }
}

#[cfg(not(windows))]
pub fn register_autostart_if_enabled(_enabled: bool) {}
