//! Native Windows Media Foundation webcam capture (zero OpenCV / numpy bloat!).
//! Strictly FAIL-CLOSED: disabled unless explicitly enabled by server AgentConfig (`mod_webcam = true`).
//! Mutex-protected so periodic capture, one-shot WS command, and stream mode never collide on the camera handle.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static CAM_LOCK: Mutex<()> = Mutex::new(());
static CAM_NOTIFIED: AtomicBool = AtomicBool::new(false);

/// Captures a single JPEG frame from the default Windows video capture device via Media Foundation.
#[cfg(windows)]
pub fn capture_webcam_jpeg(max_w: u32, quality: u8, warn_user: bool) -> Option<Vec<u8>> {
    use image::RgbImage;
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, IMFMediaSource, IMFMediaType, IMFSourceReader, MFCreateAttributes,
        MFCreateMediaType, MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFShutdown,
        MFStartup, MFVideoFormat_RGB32, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
        MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION, MFMediaType_Video,
        MFSTARTUP_NOSOCKET,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };

    let Ok(_guard) = CAM_LOCK.try_lock() else {
        return None;
    };

    if warn_user && !CAM_NOTIFIED.swap(true, Ordering::SeqCst) {
        crate::notifier::notify_user_once(
            true,
            "Qorgau DLP — Уведомление (ТК РК ст. 23)",
            "В соответствии с политикой информационной безопасности активна фотофиксация с веб-камеры.",
        );
    }

    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let mf_ok = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).is_ok();

        let result = (|| -> Option<Vec<u8>> {
            if !mf_ok {
                return None;
            }
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1).ok()?;
            let attrs = attrs?;
            attrs
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .ok()?;

            let mut devices_ptr: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attrs, &mut devices_ptr, &mut count).ok()?;
            if count == 0 || devices_ptr.is_null() {
                return None;
            }

            let devices = std::slice::from_raw_parts(devices_ptr, count as usize);
            let source: IMFMediaSource = devices[0].as_ref()?.ActivateObject().ok()?;
            CoTaskMemFree(Some(devices_ptr as *const _));

            let reader: IMFSourceReader =
                MFCreateSourceReaderFromMediaSource(&source, None).ok()?;

            // Request RGB32 output so Media Foundation converts YUY2/NV12/MJPG natively
            let out_type: IMFMediaType = MFCreateMediaType().ok()?;
            out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
            let _ = reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &out_type,
            );

            let actual_type = reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .ok()?;
            let frame_size = actual_type.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
            let width = (frame_size >> 32) as u32;
            let height = (frame_size & 0xFFFF_FFFF) as u32;
            if width == 0 || height == 0 {
                return None;
            }

            // Warm up 3 frames so auto-exposure stabilizes
            let mut sample = None;
            for _ in 0..4 {
                let mut flags = 0u32;
                let mut s = None;
                if reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        None,
                        Some(&mut s),
                    )
                    .is_ok()
                    && s.is_some()
                {
                    sample = s;
                }
            }

            let sample = sample?;
            let buffer = sample.ConvertToContiguousBuffer().ok()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut cur_len = 0u32;
            buffer.Lock(&mut ptr, None, Some(&mut cur_len)).ok()?;

            let expected = (width as usize) * (height as usize) * 4;
            let jpeg = if !ptr.is_null() && (cur_len as usize) >= expected {
                let bgra = std::slice::from_raw_parts(ptr, expected);
                let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
                for px in bgra.chunks_exact(4) {
                    rgb.push(px[2]);
                    rgb.push(px[1]);
                    rgb.push(px[0]);
                }
                RgbImage::from_raw(width, height, rgb)
                    .and_then(|img| crate::collectors::screenshot::encode_resized_jpeg(img, max_w, quality))
            } else {
                None
            };

            let _ = buffer.Unlock();
            jpeg
        })();

        if mf_ok {
            let _ = MFShutdown();
        }
        if com_ok {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn capture_webcam_jpeg(_max_w: u32, _quality: u8, _warn_user: bool) -> Option<Vec<u8>> {
    None
}
