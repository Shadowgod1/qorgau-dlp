//! Event-driven Windows session collector (`WTSRegisterSessionNotification` / `WM_WTSSESSION_CHANGE`).
//! Works under BOTH standard user and LocalSystem without requiring Security Event Log admin privileges,
//! and consumes 0% CPU while idle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;

use crate::transport::AgentTransport;

#[cfg(windows)]
pub fn spawn_session_listener(transport: Arc<AgentTransport>, enabled: Arc<AtomicBool>) {
    // Emit initial logon/agent_start event
    if enabled.load(Ordering::Relaxed) {
        transport.enqueue(
            "session",
            json!({
                "event_type": "logon",
                "logon_type": "Interactive",
                "ip_address": "",
            }),
        );
    }

    std::thread::Builder::new()
        .name("session-listener".to_string())
        .spawn(move || {
            run_wts_loop(transport, enabled);
        })
        .ok();
}

#[cfg(not(windows))]
pub fn spawn_session_listener(_transport: Arc<AgentTransport>, _enabled: Arc<AtomicBool>) {}

#[cfg(windows)]
fn run_wts_loop(transport: Arc<AgentTransport>, enabled: Arc<AtomicBool>) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        TranslateMessage, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_WTSSESSION_CHANGE, WNDCLASSW,
        WTS_CONSOLE_CONNECT, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_CONNECT, WTS_REMOTE_DISCONNECT,
        WTS_SESSION_LOCK, WTS_SESSION_LOGOFF, WTS_SESSION_LOGON, WTS_SESSION_UNLOCK,
    };

    struct Ctx {
        transport: Arc<AgentTransport>,
        enabled: Arc<AtomicBool>,
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_WTSSESSION_CHANGE {
            let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut Ctx;
            if !ptr.is_null() {
                let ctx = &*ptr;
                if ctx.enabled.load(Ordering::Relaxed) {
                    let (ev_type, logon_type) = match wparam.0 as u32 {
                        WTS_SESSION_LOGON => ("logon", "Interactive"),
                        WTS_SESSION_LOGOFF => ("logoff", "Interactive"),
                        WTS_SESSION_LOCK => ("lock", "Interactive"),
                        WTS_SESSION_UNLOCK => ("unlock", "Interactive"),
                        WTS_REMOTE_CONNECT => ("rdp_logon", "RemoteInteractive"),
                        WTS_REMOTE_DISCONNECT => ("rdp_disconnect", "RemoteInteractive"),
                        WTS_CONSOLE_CONNECT => ("console_connect", "Interactive"),
                        WTS_CONSOLE_DISCONNECT => ("console_disconnect", "Interactive"),
                        _ => ("session_change", "Unknown"),
                    };
                    ctx.transport.enqueue(
                        "session",
                        json!({
                            "event_type": ev_type,
                            "logon_type": logon_type,
                            "ip_address": "",
                        }),
                    );
                }
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe {
        let class_name: Vec<u16> = "QorgauSessionWnd\0".encode_utf16().collect();
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        // Note: WTSRegisterSessionNotification requires a top-level hidden window (not HWND_MESSAGE)
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        ) else {
            return;
        };

        let ctx_ptr = Box::into_raw(Box::new(Ctx { transport, enabled }));
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ctx_ptr as isize,
        );

        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        tracing::info!("Started event-driven Session listener (WM_WTSSESSION_CHANGE)");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, hwnd, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = WTSUnRegisterSessionNotification(hwnd);
        drop(Box::from_raw(ctx_ptr));
    }
}
