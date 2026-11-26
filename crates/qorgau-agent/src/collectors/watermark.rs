//! Native Win32 transparent click-through screen watermark overlay (iBatyr 5.6 parity).
//! Displays diagonal `username • hostname • timestamp` across the screen to deter
//! smartphone screen photography & screenshot leaks — ~0% CPU, <1 MB RAM.

use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct WatermarkSettings {
    pub enabled: bool,
    pub opacity_pct: u8,
    pub template: String,
    pub target_apps: Vec<String>,
    pub username: String,
    pub hostname: String,
}

impl WatermarkSettings {
    pub fn from_config(
        enabled: bool,
        opacity: i32,
        text: &str,
        apps_csv: &str,
        username: &str,
        hostname: &str,
    ) -> Self {
        let target_apps = apps_csv
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let tpl = if text.trim().is_empty() {
            "{username} • {hostname} • {time}".to_string()
        } else {
            text.trim().to_string()
        };
        Self {
            enabled,
            opacity_pct: opacity.clamp(5, 80) as u8,
            template: tpl,
            target_apps,
            username: username.to_string(),
            hostname: hostname.to_string(),
        }
    }

    pub fn format_label(&self) -> String {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        self.template
            .replace("{username}", &self.username)
            .replace("{hostname}", &self.hostname)
            .replace("{time}", &now)
    }

    pub fn should_display_for(&self, app_name: &str, window_title: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.target_apps.is_empty() {
            return true;
        }
        let app_l = app_name.to_ascii_lowercase();
        let title_l = window_title.to_ascii_lowercase();
        self.target_apps
            .iter()
            .any(|k| app_l.contains(k) || title_l.contains(k))
    }
}

#[cfg(windows)]
pub fn spawn_watermark_overlay(settings: Arc<RwLock<WatermarkSettings>>) {
    std::thread::Builder::new()
        .name("watermark-overlay".to_string())
        .spawn(move || run_win32_overlay(settings))
        .ok();
}

#[cfg(not(windows))]
pub fn spawn_watermark_overlay(_settings: Arc<RwLock<WatermarkSettings>>) {}

#[cfg(windows)]
fn run_win32_overlay(settings: Arc<RwLock<WatermarkSettings>>) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
        InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW, CLIP_DEFAULT_PRECIS,
        DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, FF_SWISS, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
        TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        GetWindowLongPtrW, RegisterClassW, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW,
        ShowWindow, TranslateMessage, GWLP_USERDATA, LWA_ALPHA, LWA_COLORKEY, MSG, SM_CXSCREEN,
        SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    // Magic color key (pure magenta RGB(255, 0, 255)) becomes 100% transparent click-through
    const KEY_COLOR: COLORREF = COLORREF(0x00FF_00FF);
    const TIMER_ID: usize = 7001;

    struct Ctx {
        settings: Arc<RwLock<WatermarkSettings>>,
        visible: bool,
        last_label: String,
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ctx;
        if !ptr.is_null() {
            let ctx = &mut *ptr;
            match msg {
                WM_TIMER => {
                    let s = ctx.settings.read().map(|g| g.clone()).ok();
                    if let Some(cfg) = s {
                        let show = if !cfg.enabled {
                            false
                        } else if cfg.target_apps.is_empty() {
                            true
                        } else {
                            let fg = crate::collectors::win_util::get_foreground_info();
                            cfg.should_display_for(&fg.app_name, &fg.window_title)
                        };

                        let alpha = ((cfg.opacity_pct as u32) * 255 / 100).clamp(15, 210) as u8;
                        let _ = SetLayeredWindowAttributes(
                            hwnd,
                            KEY_COLOR,
                            alpha,
                            LWA_COLORKEY | LWA_ALPHA,
                        );

                        if show != ctx.visible {
                            ctx.visible = show;
                            let _ = ShowWindow(hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
                        }

                        if show {
                            let label = cfg.format_label();
                            if label != ctx.last_label {
                                ctx.last_label = label;
                                let _ = InvalidateRect(hwnd, None, true);
                            }
                        }
                    }
                    return LRESULT(0);
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);
                    let w = GetSystemMetrics(SM_CXSCREEN);
                    let h = GetSystemMetrics(SM_CYSCREEN);
                    let rect = RECT {
                        left: 0,
                        top: 0,
                        right: w,
                        bottom: h,
                    };
                    let bg_brush = CreateSolidBrush(KEY_COLOR);
                    FillRect(hdc, &rect, bg_brush);
                    let _ = DeleteObject(bg_brush);

                    let label = ctx
                        .settings
                        .read()
                        .map(|g| g.format_label())
                        .unwrap_or_default();
                    let label_w: Vec<u16> = label.encode_utf16().collect();
                    let font_name: Vec<u16> = "Segoe UI\0".encode_utf16().collect();

                    // 25-degree diagonal watermark font (escapement in tenths of a degree)
                    let hfont = CreateFontW(
                        24,
                        0,
                        250,
                        250,
                        600,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET.0 as u32,
                        OUT_DEFAULT_PRECIS.0 as u32,
                        CLIP_DEFAULT_PRECIS.0 as u32,
                        DEFAULT_QUALITY.0 as u32,
                        (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
                        PCWSTR(font_name.as_ptr()),
                    );
                    let old_font = SelectObject(hdc, hfont);
                    SetBkMode(hdc, TRANSPARENT);
                    // Neutral gray that stands out softly on both light and dark screens
                    SetTextColor(hdc, COLORREF(0x0080_8080));

                    let step_x = 460;
                    let step_y = 260;
                    let mut y = 80;
                    let mut row = 0;
                    while y < h + 200 {
                        let mut x = if row % 2 == 0 { 40 } else { -180 };
                        while x < w + 200 {
                            let _ = TextOutW(hdc, x, y, &label_w);
                            x += step_x;
                        }
                        y += step_y;
                        row += 1;
                    }

                    SelectObject(hdc, old_font);
                    let _ = DeleteObject(hfont);
                    let _ = EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe {
        let class_name: Vec<u16> = "QorgauWatermarkOverlay\0".encode_utf16().collect();
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_POPUP,
            0,
            0,
            w,
            h,
            None,
            None,
            hinst,
            None,
        ) else {
            return;
        };

        let ctx_ptr = Box::into_raw(Box::new(Ctx {
            settings,
            visible: false,
            last_label: String::new(),
        }));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, ctx_ptr as isize);
        let _ = SetLayeredWindowAttributes(hwnd, KEY_COLOR, 64, LWA_COLORKEY | LWA_ALPHA);
        SetTimer(hwnd, TIMER_ID, 2000, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, hwnd, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        drop(Box::from_raw(ctx_ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_watermark_matches_configured_apps() {
        let s = WatermarkSettings::from_config(
            true,
            30,
            "{username} | {hostname}",
            "excel, 1cv8, bitrix",
            "ivanov",
            "PC-01",
        );
        assert!(s.should_display_for("EXCEL.EXE", "Budget.xlsx"));
        assert!(s.should_display_for("chrome.exe", "Bitrix24 - CRM"));
        assert!(!s.should_display_for("notepad.exe", "notes.txt"));
        assert_eq!(s.format_label(), "ivanov | PC-01");
    }
}
