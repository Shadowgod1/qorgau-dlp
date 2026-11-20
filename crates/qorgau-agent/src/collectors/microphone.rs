//! Native Windows WASAPI microphone recorder (zero PortAudio / sounddevice / numpy bloat).
//! Strictly FAIL-CLOSED: disabled unless explicitly enabled by server AgentConfig (`mod_microphone = true`).
//! Notifies user once per session in compliance with ТК РК ст. 23.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static MIC_LOCK: Mutex<()> = Mutex::new(());
static MIC_NOTIFIED: AtomicBool = AtomicBool::new(false);

/// Records `duration_sec` seconds from the default Windows capture device via WASAPI,
/// downmixing/resampling to 16-bit mono WAV at `target_rate` Hz.
#[cfg(windows)]
pub fn record_wav(
    duration_sec: u32,
    target_rate: u32,
    warn_user: bool,
    stop_flag: &AtomicBool,
) -> Option<Vec<u8>> {
    use std::time::{Duration, Instant};
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    let Ok(_guard) = MIC_LOCK.try_lock() else {
        return None;
    };

    if warn_user && !MIC_NOTIFIED.swap(true, Ordering::SeqCst) {
        crate::notifier::notify_user_once(
            true,
            "Qorgau DLP — Уведомление (ТК РК ст. 23)",
            "В соответствии с политикой информационной безопасности активна запись аудио с микрофона.",
        );
    }

    unsafe {
        let hr_init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let com_inited = hr_init.is_ok();

        let result = (|| -> Option<Vec<u8>> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;

            let fmt_ptr: *mut WAVEFORMATEX = client.GetMixFormat().ok()?;
            let fmt = *fmt_ptr;
            let src_rate = fmt.nSamplesPerSec;
            let src_channels = fmt.nChannels.max(1) as usize;
            let bits = fmt.wBitsPerSample;
            let format_tag = fmt.wFormatTag;

            // Initialize shared-mode 1-second buffer
            let hns_duration: i64 = 10_000_000;
            let init_res = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                0,
                hns_duration,
                0,
                fmt_ptr,
                None,
            );
            CoTaskMemFree(Some(fmt_ptr as *const _));
            init_res.ok()?;

            let capture: IAudioCaptureClient = client.GetService().ok()?;
            client.Start().ok()?;

            let deadline = Instant::now() + Duration::from_secs(duration_sec.clamp(1, 300) as u64);
            let mut mono_f32: Vec<f32> = Vec::with_capacity((src_rate * duration_sec) as usize);

            while Instant::now() < deadline && !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
                loop {
                    let Ok(pkt_len) = capture.GetNextPacketSize() else {
                        break;
                    };
                    if pkt_len == 0 {
                        break;
                    }
                    let mut data_ptr: *mut u8 = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    if capture
                        .GetBuffer(&mut data_ptr, &mut frames, &mut flags, None, None)
                        .is_err()
                    {
                        break;
                    }

                    let silent = (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0;
                    if silent || data_ptr.is_null() {
                        mono_f32.extend(std::iter::repeat_n(0.0f32, frames as usize));
                    } else if bits == 32 && (format_tag == 3 || format_tag == 0xFFFE) {
                        let samples = std::slice::from_raw_parts(
                            data_ptr as *const f32,
                            (frames as usize) * src_channels,
                        );
                        for chs in samples.chunks_exact(src_channels) {
                            let avg: f32 = chs.iter().sum::<f32>() / (src_channels as f32);
                            mono_f32.push(avg);
                        }
                    } else if bits == 16 {
                        let samples = std::slice::from_raw_parts(
                            data_ptr as *const i16,
                            (frames as usize) * src_channels,
                        );
                        for chs in samples.chunks_exact(src_channels) {
                            let avg: f32 = chs.iter().map(|&s| (s as f32) / 32768.0).sum::<f32>()
                                / (src_channels as f32);
                            mono_f32.push(avg);
                        }
                    }

                    let _ = capture.ReleaseBuffer(frames);
                }
            }

            let _ = client.Stop();

            if mono_f32.is_empty() {
                return None;
            }

            // Linear resample to target_rate (e.g. 16000 Hz)
            let out_rate = target_rate.clamp(8000, 48000);
            let resampled = resample_linear(&mono_f32, src_rate, out_rate);
            encode_wav_pcm16(&resampled, out_rate)
        })();

        if com_inited {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn record_wav(
    _duration_sec: u32,
    _target_rate: u32,
    _warn_user: bool,
    _stop_flag: &AtomicBool,
) -> Option<Vec<u8>> {
    None
}

fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.len() < 2 {
        return input.to_vec();
    }
    let ratio = (src_rate as f64) / (dst_rate as f64);
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = (i as f64) * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn encode_wav_pcm16(samples: &[f32], sample_rate: u32) -> Option<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::with_capacity(samples.len() * 2 + 64));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).ok()?;
        for &s in samples {
            let clamped = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(clamped).ok()?;
        }
        writer.finalize().ok()?;
    }
    Some(cursor.into_inner())
}
