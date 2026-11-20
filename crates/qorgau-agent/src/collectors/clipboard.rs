//! Event-driven Windows clipboard & messenger collector (`AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE`).
//! Replaces TWO competing polling threads from legacy Python with a single zero-CPU Win32 message listener.
//! Excludes password managers so credentials/OTPs are never harvested.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use qorgau_common::dlp::scan_builtin_dlp;
use serde_json::json;

use crate::collectors::win_util::get_foreground_info;
use crate::notifier::notify_user_once;
use crate::transport::AgentTransport;

const PASSWORD_MANAGERS: &[&str] = &[
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "lastpass.exe",
    "dashlane.exe",
    "enpass.exe",
    "roboform.exe",
];

const MESSENGERS: &[(&str, &str)] = &[
    ("telegram.exe", "telegram"),
    ("whatsapp.exe", "whatsapp"),
    ("slack.exe", "slack"),
    ("teams.exe", "teams"),
    ("ms-teams.exe", "teams"),
    ("viber.exe", "viber"),
    ("discord.exe", "discord"),
    ("signal.exe", "signal"),
];

pub fn is_password_manager(app_name: &str) -> bool {
    let lower = app_name.to_ascii_lowercase();
    PASSWORD_MANAGERS.iter().any(|pm| lower == *pm)
}

pub fn detect_messenger(app_name: &str, window_title: &str) -> Option<&'static str> {
    let lower = app_name.to_ascii_lowercase();
    for (exe, name) in MESSENGERS {
        if lower == *exe {
            return Some(name);
        }
    }
    let t = window_title.to_ascii_lowercase();
    if t.contains("whatsapp web") || t.contains("whatsapp") {
        return Some("whatsapp");
    }
    if t.contains("telegram web") {
        return Some("telegram");
    }
    None
}

pub struct ClipboardConfigFlags {
    pub mod_clipboard: AtomicBool,
    pub mod_messenger: AtomicBool,
    pub clipboard_block: bool,
    pub warn_user: bool,
}

#[cfg(windows)]
pub fn spawn_clipboard_listener(transport: Arc<AgentTransport>, flags: Arc<ClipboardConfigFlags>) {
    std::thread::Builder::new()
        .name("clipboard-listener".to_string())
        .spawn(move || {
            run_win32_clipboard_loop(transport, flags);
        })
        .ok();
}

#[cfg(not(windows))]
pub fn spawn_clipboard_listener(
    _transport: Arc<AgentTransport>,
    _flags: Arc<ClipboardConfigFlags>,
) {
}

#[cfg(windows)]
fn run_win32_clipboard_loop(transport: Arc<AgentTransport>, flags: Arc<ClipboardConfigFlags>) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, RemoveClipboardFormatListener,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
        TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE,
        WNDCLASSW,
    };

    struct Ctx {
        transport: Arc<AgentTransport>,
        flags: Arc<ClipboardConfigFlags>,
        last_text: String,
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLIPBOARDUPDATE {
            let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut Ctx;
            if !ptr.is_null() {
                let ctx = &mut *ptr;
                handle_clipboard_change(ctx);
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    fn handle_clipboard_change(ctx: &mut Ctx) {
        let clip_on = ctx.flags.mod_clipboard.load(Ordering::Relaxed);
        let msg_on = ctx.flags.mod_messenger.load(Ordering::Relaxed);
        if !clip_on && !msg_on {
            return;
        }

        let fg = get_foreground_info();
        if is_password_manager(&fg.app_name) {
            return;
        }

        let Some(text) = read_clipboard_unicode() else {
            return;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed == ctx.last_text {
            return;
        }
        ctx.last_text = trimmed.to_string();

        let matches = scan_builtin_dlp(trimmed);
        let should_block =
            ctx.flags.clipboard_block && matches.iter().any(|m| m.action == "block");

        if should_block {
            clear_clipboard();
            let rule_list: Vec<&str> = matches.iter().map(|m| m.rule_name.as_str()).collect();
            notify_user_once(
                ctx.flags.warn_user,
                "Qorgau DLP — Операция заблокирована",
                &format!(
                    "Копирование конфиденциальных данных заблокировано политикой ИБ:\n{}",
                    rule_list.join(", ")
                ),
            );
        }

        let snippet: String = trimmed.chars().take(2000).collect();

        if clip_on {
            ctx.transport.enqueue(
                "clipboard",
                json!({
                    "content": snippet,
                    "source_app": fg.app_name,
                    "blocked": should_block,
                }),
            );
        }

        if msg_on {
            if let Some(messenger) = detect_messenger(&fg.app_name, &fg.window_title) {
                ctx.transport.enqueue(
                    "messenger",
                    json!({
                        "messenger": messenger,
                        "direction": "sent",
                        "contact": fg.window_title.chars().take(120).collect::<String>(),
                        "content": trimmed.chars().take(1000).collect::<String>(),
                        "has_file": false,
                    }),
                );
            }
        }
    }

    unsafe {
        let class_name: Vec<u16> = "QorgauClipboardWnd\0".encode_utf16().collect();
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            hinstance,
            None,
        ) else {
            tracing::warn!("Failed to create clipboard message window");
            return;
        };

        let ctx = Box::new(Ctx {
            transport,
            flags,
            last_text: String::new(),
        });
        let ctx_ptr = Box::into_raw(ctx);
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ctx_ptr as isize,
        );

        let _ = AddClipboardFormatListener(hwnd);
        tracing::info!("Started event-driven Clipboard/Messenger listener (WM_CLIPBOARDUPDATE)");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, hwnd, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = RemoveClipboardFormatListener(hwnd);
        drop(Box::from_raw(ctx_ptr));
    }
}

#[cfg(windows)]
fn read_clipboard_unicode() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;

    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return None;
        }
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 && len < 65536 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let s = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(hglobal);
            Some(s)
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(windows)]
fn clear_clipboard() {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_managers_are_excluded() {
        assert!(is_password_manager("KeePassXC.exe"));
        assert!(is_password_manager("Bitwarden.exe"));
        assert!(!is_password_manager("notepad.exe"));
    }

    #[test]
    fn detects_messengers_by_exe_and_title() {
        assert_eq!(detect_messenger("Telegram.exe", "Chat"), Some("telegram"));
        assert_eq!(detect_messenger("chrome.exe", "WhatsApp Web - Google Chrome"), Some("whatsapp"));
        assert_eq!(detect_messenger("code.exe", "main.rs"), None);
    }
}
