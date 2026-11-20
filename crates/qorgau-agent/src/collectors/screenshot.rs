//! Native Win32 GDI screen capture + JPEG compression (zero Pillow/mss overhead).

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::RgbImage;

#[cfg(windows)]
pub fn capture_primary_jpeg(max_w: u32, quality: u8) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0 || h <= 0 {
            return None;
        }

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return None;
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
        let old = SelectObject(hdc_mem, hbm);

        let blt_ok = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, 0, 0, SRCCOPY).is_ok();

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];
        let lines = if blt_ok {
            GetDIBits(
                hdc_mem,
                hbm,
                0,
                h as u32,
                Some(bgra.as_mut_ptr() as *mut _),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        if lines == 0 {
            return None;
        }

        // Convert BGRA -> RGB
        let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
        for px in bgra.chunks_exact(4) {
            rgb.push(px[2]);
            rgb.push(px[1]);
            rgb.push(px[0]);
        }

        let img = RgbImage::from_raw(w as u32, h as u32, rgb)?;
        encode_resized_jpeg(img, max_w, quality)
    }
}

#[cfg(not(windows))]
pub fn capture_primary_jpeg(_max_w: u32, _quality: u8) -> Option<Vec<u8>> {
    None
}

pub fn encode_resized_jpeg(img: RgbImage, max_w: u32, quality: u8) -> Option<Vec<u8>> {
    let final_img = if max_w > 0 && img.width() > max_w {
        let new_h = ((img.height() as f64) * (max_w as f64) / (img.width() as f64)).round() as u32;
        image::imageops::resize(&img, max_w, new_h.max(1), FilterType::Triangle)
    } else {
        img
    };

    let mut out = Cursor::new(Vec::with_capacity(128 * 1024));
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality.clamp(10, 95));
    enc.encode_image(&final_img).ok()?;
    Some(out.into_inner())
}
