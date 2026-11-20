use std::sync::atomic::{AtomicBool, Ordering};

static POPUP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Shows a non-stacking Windows notification dialog (prevents infinite modal popup spam).
pub fn notify_user_once(enabled: bool, title: &str, message: &str) {
    if !enabled {
        return;
    }
    if POPUP_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let t = title.to_string();
    let m = message.to_string();
    std::thread::spawn(move || {
        #[cfg(windows)]
        show_msgbox(&t, &m);
        #[cfg(not(windows))]
        tracing::info!(title = %t, message = %m, "User notification");

        POPUP_ACTIVE.store(false, Ordering::SeqCst);
    });
}

#[cfg(windows)]
fn show_msgbox(title: &str, message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
    };

    let tw: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mw: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(mw.as_ptr()),
            PCWSTR(tw.as_ptr()),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        );
    }
}
