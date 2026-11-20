//! Lightweight native Win32 helpers shared by collectors (zero WMI overhead).

#[derive(Debug, Clone, Default)]
pub struct ForegroundInfo {
    pub app_name: String,
    pub exe_path: String,
    pub window_title: String,
}

#[cfg(windows)]
pub fn get_foreground_info() -> ForegroundInfo {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return ForegroundInfo::default();
        }

        let len = GetWindowTextLengthW(hwnd);
        let title = if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let n = GetWindowTextW(hwnd, &mut buf);
            String::from_utf16_lossy(&buf[..n as usize])
        } else {
            String::new()
        };

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let (app_name, exe_path) = get_process_name_and_path(pid);

        ForegroundInfo {
            app_name,
            exe_path,
            window_title: title.chars().take(300).collect(),
        }
    }
}

#[cfg(not(windows))]
pub fn get_foreground_info() -> ForegroundInfo {
    ForegroundInfo::default()
}

#[cfg(windows)]
pub fn get_process_name_and_path(pid: u32) -> (String, String) {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    if pid == 0 {
        return ("system".to_string(), String::new());
    }

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (format!("pid:{pid}"), String::new());
        };
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);

        if !ok || size == 0 {
            return (format!("pid:{pid}"), String::new());
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        (name, path)
    }
}

#[cfg(not(windows))]
pub fn get_process_name_and_path(pid: u32) -> (String, String) {
    (format!("pid:{pid}"), String::new())
}

/// Returns user idle time in seconds using 64-bit `GetTickCount64` (fixes legacy 49.7-day rollover bug).
#[cfg(windows)]
pub fn get_idle_seconds() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount64;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    unsafe {
        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut lii).as_bool() {
            return 0;
        }
        let now64 = GetTickCount64();
        let now32 = (now64 & 0xFFFF_FFFF) as u32;
        let diff_ms = now32.wrapping_sub(lii.dwTime) as u64;
        diff_ms / 1000
    }
}

#[cfg(not(windows))]
pub fn get_idle_seconds() -> u64 {
    0
}
