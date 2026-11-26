mod collectors;
mod config;
mod notifier;
mod storage;
mod transport;
mod winsqlite;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use qorgau_common::{AgentConfigDto, WsAgentCommand};
use tokio::sync::{mpsc, watch, RwLock};
use tracing_subscriber::EnvFilter;

use crate::collectors::clipboard::ClipboardConfigFlags;
use crate::config::{
    current_hostname, current_username, register_autostart_if_enabled, resolve_machine_id,
    LocalAgentConfig,
};
use crate::storage::DiskQueue;
use crate::transport::AgentTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let local_cfg = LocalAgentConfig::load();
    std::fs::create_dir_all(&local_cfg.data_dir)?;

    // Daily rotating log file (fixes unbounded %TEMP%\qorgau_agent.log leak)
    let log_dir = local_cfg.data_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "qorgau-agent.log");
    let (non_blocking, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .init();

    let agent_id = resolve_machine_id(&local_cfg.data_dir);
    let hostname = current_hostname();
    let username = current_username();
    let version = env!("CARGO_PKG_VERSION");

    tracing::info!(
        %version,
        %hostname,
        %username,
        %agent_id,
        server = %local_cfg.server_url,
        "Starting Qorgau DLP Agent (Rust)"
    );

    register_autostart_if_enabled(local_cfg.install_autostart);

    let queue = Arc::new(DiskQueue::new(&local_cfg.data_dir, local_cfg.max_queue_mb)?);
    let transport = Arc::new(AgentTransport::new(
        local_cfg.server_url.clone(),
        local_cfg.agent_token.clone(),
        agent_id,
        hostname,
        username,
        queue,
    )?);

    let os_ver = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let mut registered = transport.register(&os_ver, version).await;

    // Fetch server config & active prevention rules (fail-closed defaults)
    let initial_cfg = transport.fetch_config().await.unwrap_or_default();
    let shared_cfg = Arc::new(RwLock::new(initial_cfg.clone()));
    let initial_prev = transport.fetch_prevention_rules().await.unwrap_or_default();
    if initial_cfg.mod_web_blocker {
        collectors::blocker::sync_hosts_blocklist(true, &initial_prev.blocked_domains);
    }
    let shared_prev = Arc::new(RwLock::new(initial_prev));

    // Watch channels for dynamic hot-reload of all collectors
    let (tx_apps, rx_apps) = watch::channel(initial_cfg.mod_apps);
    let (tx_web, rx_web) = watch::channel(initial_cfg.mod_web);
    let (tx_usb, rx_usb) = watch::channel(initial_cfg.mod_usb);
    let (tx_net, rx_net) = watch::channel(initial_cfg.mod_network);
    let (tx_print, rx_print) = watch::channel(initial_cfg.mod_printer);
    let (tx_files, rx_files) = watch::channel(initial_cfg.mod_file_monitor);
    let (tx_blocker, rx_blocker) = watch::channel(initial_cfg.mod_app_blocker);

    let session_flag = Arc::new(AtomicBool::new(initial_cfg.mod_sessions));
    let clip_flags = Arc::new(ClipboardConfigFlags {
        mod_clipboard: AtomicBool::new(initial_cfg.mod_clipboard),
        mod_messenger: AtomicBool::new(initial_cfg.mod_messenger),
        clipboard_block: local_cfg.clipboard_block,
        warn_user: local_cfg.warn_user_enabled,
    });

    // Event-driven Win32 listeners (0% idle CPU)
    collectors::sessions::spawn_session_listener(transport.clone(), session_flag.clone());
    collectors::clipboard::spawn_clipboard_listener(transport.clone(), clip_flags.clone());

    // Native transparent screen watermark overlay (0% idle CPU, hot-reloadable)
    let watermark_state = Arc::new(std::sync::RwLock::new(
        collectors::watermark::WatermarkSettings::from_config(
            initial_cfg.mod_watermark,
            initial_cfg.watermark_opacity,
            &initial_cfg.watermark_text,
            &initial_cfg.watermark_apps,
            &transport.username,
            &transport.hostname,
        ),
    ));
    collectors::watermark::spawn_watermark_overlay(watermark_state.clone());

    // Async collectors
    tokio::spawn(collectors::apps::run(transport.clone(), rx_apps));
    tokio::spawn(collectors::web::run(transport.clone(), rx_web));
    tokio::spawn(collectors::usb::run(
        transport.clone(),
        rx_usb,
        shared_prev.clone(),
        local_cfg.shadow_max_mb * 1024 * 1024,
        local_cfg.warn_user_enabled,
    ));
    tokio::spawn(collectors::network::run(transport.clone(), rx_net));
    tokio::spawn(collectors::printer::run(transport.clone(), rx_print));
    tokio::spawn(collectors::files::run(transport.clone(), rx_files));
    tokio::spawn(collectors::blocker::run_app_blocker(
        transport.clone(),
        rx_blocker,
        shared_prev.clone(),
        local_cfg.warn_user_enabled,
    ));

    // Initial Inventory scan (if enabled) + periodic 6h refresh
    let t_inv = transport.clone();
    let cfg_inv = shared_cfg.clone();
    tokio::spawn(async move {
        if cfg_inv.read().await.mod_inventory {
            collectors::inventory::collect_and_upload(t_inv.clone()).await;
        }
        let mut iv = tokio::time::interval(Duration::from_secs(3600 * 6));
        iv.tick().await;
        loop {
            iv.tick().await;
            if cfg_inv.read().await.mod_inventory {
                collectors::inventory::collect_and_upload(t_inv.clone()).await;
            }
        }
    });

    // Periodic Data Discovery crawler (if enabled in config)
    let t_disc = transport.clone();
    let cfg_disc = shared_cfg.clone();
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(Duration::from_secs(3600 * 12));
        iv.tick().await;
        loop {
            iv.tick().await;
            let (on, paths) = {
                let c = cfg_disc.read().await;
                (c.mod_discovery, c.discovery_paths.clone())
            };
            if on {
                collectors::discovery::run_discovery_scan(t_disc.clone(), paths).await;
            }
        }
    });

    // Periodic screenshots
    let t_scr = transport.clone();
    let cfg_scr = shared_cfg.clone();
    tokio::spawn(async move {
        loop {
            let (on, interval_s, max_w, quality) = {
                let c = cfg_scr.read().await;
                (
                    c.mod_screenshots,
                    c.screenshot_interval.clamp(10, 3600) as u64,
                    c.screenshot_max_w.clamp(640, 3840) as u32,
                    c.screenshot_quality.clamp(10, 95) as u8,
                )
            };
            tokio::time::sleep(Duration::from_secs(interval_s)).await;
            if on {
                if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                    collectors::screenshot::capture_primary_jpeg(max_w, quality)
                })
                .await
                {
                    t_scr.upload_screenshot(bytes).await;
                }
            }
        }
    });

    // Periodic webcam (STRICTLY FAIL-CLOSED: runs only if server explicitly sets mod_webcam = true)
    let webcam_streaming = Arc::new(AtomicBool::new(false));
    let t_cam = transport.clone();
    let cfg_cam = shared_cfg.clone();
    let warn_cam = local_cfg.warn_user_enabled;
    tokio::spawn(async move {
        loop {
            let (on, interval_s, max_w, quality) = {
                let c = cfg_cam.read().await;
                (
                    c.mod_webcam,
                    c.webcam_interval.clamp(15, 3600) as u64,
                    c.webcam_max_w.clamp(320, 1920) as u32,
                    c.webcam_quality.clamp(10, 95) as u8,
                )
            };
            tokio::time::sleep(Duration::from_secs(interval_s)).await;
            if on {
                if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                    collectors::webcam::capture_webcam_jpeg(max_w, quality, warn_cam)
                })
                .await
                {
                    t_cam.upload_webcam(bytes).await;
                }
            }
        }
    });

    // Periodic microphone (STRICTLY FAIL-CLOSED: runs only if mod_microphone = true)
    let mic_stop = Arc::new(AtomicBool::new(false));
    let mic_continuous = Arc::new(AtomicBool::new(false));
    let t_mic = transport.clone();
    let cfg_mic = shared_cfg.clone();
    let mic_stop_bg = mic_stop.clone();
    let warn_mic = local_cfg.warn_user_enabled;
    tokio::spawn(async move {
        loop {
            let (on, dur, rate) = {
                let c = cfg_mic.read().await;
                (
                    c.mod_microphone,
                    c.microphone_duration.clamp(5, 300) as u32,
                    c.microphone_sample_rate.clamp(8000, 48000) as u32,
                )
            };
            tokio::time::sleep(Duration::from_secs((dur as u64) * 2)).await;
            if on {
                let sf = mic_stop_bg.clone();
                sf.store(false, Ordering::SeqCst);
                if let Ok(Some(wav)) = tokio::task::spawn_blocking(move || {
                    collectors::microphone::record_wav(dur, rate, warn_mic, &sf)
                })
                .await
                {
                    t_mic.upload_audio(wav, dur as i32, rate as i32).await;
                }
            }
        }
    });

    // Single persistent WebSocket connection + command dispatcher
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<WsAgentCommand>(64);
    tokio::spawn(transport.clone().run_ws_listener(cmd_tx));

    let t_cmd = transport.clone();
    let cfg_cmd = shared_cfg.clone();
    let prev_cmd = shared_prev.clone();
    let cam_stream_flag = webcam_streaming.clone();
    let mic_cont_flag = mic_continuous.clone();
    let mic_stop_cmd = mic_stop.clone();
    let warn_cmd = local_cfg.warn_user_enabled;
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                WsAgentCommand::TakeScreenshot => {
                    let c = cfg_cmd.read().await.clone();
                    let t = t_cmd.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                            collectors::screenshot::capture_primary_jpeg(
                                c.screenshot_max_w as u32,
                                c.screenshot_quality as u8,
                            )
                        })
                        .await
                        {
                            t.upload_screenshot(bytes).await;
                        }
                    });
                }
                WsAgentCommand::TakeLiveFrame => {
                    let t = t_cmd.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                            collectors::screenshot::capture_primary_jpeg(900, 48)
                        })
                        .await
                        {
                            t.upload_live_frame(bytes).await;
                        }
                    });
                }
                WsAgentCommand::TakeWebcam => {
                    let c = cfg_cmd.read().await.clone();
                    let t = t_cmd.clone();
                    tokio::spawn(async move {
                        if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                            collectors::webcam::capture_webcam_jpeg(
                                c.webcam_max_w as u32,
                                c.webcam_quality as u8,
                                warn_cmd,
                            )
                        })
                        .await
                        {
                            t.upload_webcam(bytes).await;
                        }
                    });
                }
                WsAgentCommand::StartWebcamStream => {
                    if !cam_stream_flag.swap(true, Ordering::SeqCst) {
                        let flag = cam_stream_flag.clone();
                        let t = t_cmd.clone();
                        let cfg_s = cfg_cmd.clone();
                        tokio::spawn(async move {
                            while flag.load(Ordering::Relaxed) {
                                let c = cfg_s.read().await.clone();
                                if let Ok(Some(bytes)) = tokio::task::spawn_blocking(move || {
                                    collectors::webcam::capture_webcam_jpeg(
                                        c.webcam_max_w.min(640) as u32,
                                        50,
                                        warn_cmd,
                                    )
                                })
                                .await
                                {
                                    t.upload_webcam(bytes).await;
                                }
                                tokio::time::sleep(Duration::from_millis(1000)).await;
                            }
                        });
                    }
                }
                WsAgentCommand::StopWebcamStream => {
                    cam_stream_flag.store(false, Ordering::SeqCst);
                }
                WsAgentCommand::TakeRecording => {
                    let c = cfg_cmd.read().await.clone();
                    let t = t_cmd.clone();
                    let sf = mic_stop_cmd.clone();
                    sf.store(false, Ordering::SeqCst);
                    tokio::spawn(async move {
                        if let Ok(Some(wav)) = tokio::task::spawn_blocking(move || {
                            collectors::microphone::record_wav(
                                c.microphone_duration as u32,
                                c.microphone_sample_rate as u32,
                                warn_cmd,
                                &sf,
                            )
                        })
                        .await
                        {
                            t.upload_audio(wav, c.microphone_duration, c.microphone_sample_rate)
                                .await;
                        }
                    });
                }
                WsAgentCommand::StartRecording => {
                    mic_stop_cmd.store(false, Ordering::SeqCst);
                    if !mic_cont_flag.swap(true, Ordering::SeqCst) {
                        let cont = mic_cont_flag.clone();
                        let sf = mic_stop_cmd.clone();
                        let t = t_cmd.clone();
                        let cfg_m = cfg_cmd.clone();
                        tokio::spawn(async move {
                            while cont.load(Ordering::Relaxed) && !sf.load(Ordering::Relaxed) {
                                let c = cfg_m.read().await.clone();
                                let sf2 = sf.clone();
                                if let Ok(Some(wav)) = tokio::task::spawn_blocking(move || {
                                    collectors::microphone::record_wav(
                                        c.microphone_duration as u32,
                                        c.microphone_sample_rate as u32,
                                        warn_cmd,
                                        &sf2,
                                    )
                                })
                                .await
                                {
                                    t.upload_audio(
                                        wav,
                                        c.microphone_duration,
                                        c.microphone_sample_rate,
                                    )
                                    .await;
                                }
                            }
                        });
                    }
                }
                WsAgentCommand::StopRecording => {
                    mic_cont_flag.store(false, Ordering::SeqCst);
                    mic_stop_cmd.store(true, Ordering::SeqCst);
                }
                WsAgentCommand::ReloadConfig => {
                    if let Some(fresh) = t_cmd.fetch_config().await {
                        apply_config(
                            &fresh,
                            &cfg_cmd,
                            &tx_apps,
                            &tx_web,
                            &tx_usb,
                            &tx_net,
                            &tx_print,
                            &tx_files,
                            &tx_blocker,
                            &session_flag,
                            &clip_flags,
                            &watermark_state,
                            &t_cmd.username,
                            &t_cmd.hostname,
                        )
                        .await;
                        let domains = prev_cmd.read().await.blocked_domains.clone();
                        collectors::blocker::sync_hosts_blocklist(fresh.mod_web_blocker, &domains);
                        tracing::info!(name = %fresh.name, "Hot-reloaded server config");
                    }
                }
                WsAgentCommand::ReloadPreventionRules => {
                    if let Some(fresh_rules) = t_cmd.fetch_prevention_rules().await {
                        let web_blk = cfg_cmd.read().await.mod_web_blocker;
                        collectors::blocker::sync_hosts_blocklist(web_blk, &fresh_rules.blocked_domains);
                        *prev_cmd.write().await = fresh_rules;
                        tracing::info!("Hot-reloaded prevention rules (USB/Apps/Domains)");
                    }
                }
                WsAgentCommand::RunDiscoveryScan => {
                    let paths = cfg_cmd.read().await.discovery_paths.clone();
                    let t = t_cmd.clone();
                    tokio::spawn(async move {
                        collectors::discovery::run_discovery_scan(t, paths).await;
                    });
                }
                WsAgentCommand::RunInventoryScan => {
                    let t = t_cmd.clone();
                    tokio::spawn(async move {
                        collectors::inventory::collect_and_upload(t).await;
                    });
                }
            }
        }
    });

    // Main loop: batch flush + heartbeat + registration retry
    let mut flush_iv = tokio::time::interval(Duration::from_secs(local_cfg.batch_interval_sec.max(5)));
    let mut hb_iv = tokio::time::interval(Duration::from_secs(local_cfg.heartbeat_interval_sec.max(15)));

    loop {
        tokio::select! {
            _ = flush_iv.tick() => {
                transport.flush_batch().await;
            }
            _ = hb_iv.tick() => {
                if !registered {
                    registered = transport.register(&os_ver, version).await;
                } else {
                    transport.send_heartbeat().await;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutting down Qorgau Agent");
                transport.flush_batch().await;
                break;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn apply_config(
    fresh: &AgentConfigDto,
    shared_cfg: &RwLock<AgentConfigDto>,
    tx_apps: &watch::Sender<bool>,
    tx_web: &watch::Sender<bool>,
    tx_usb: &watch::Sender<bool>,
    tx_net: &watch::Sender<bool>,
    tx_print: &watch::Sender<bool>,
    tx_files: &watch::Sender<bool>,
    tx_blocker: &watch::Sender<bool>,
    session_flag: &AtomicBool,
    clip_flags: &ClipboardConfigFlags,
    watermark_state: &std::sync::RwLock<collectors::watermark::WatermarkSettings>,
    username: &str,
    hostname: &str,
) {
    *shared_cfg.write().await = fresh.clone();
    let _ = tx_apps.send(fresh.mod_apps);
    let _ = tx_web.send(fresh.mod_web);
    let _ = tx_usb.send(fresh.mod_usb);
    let _ = tx_net.send(fresh.mod_network);
    let _ = tx_print.send(fresh.mod_printer);
    let _ = tx_files.send(fresh.mod_file_monitor);
    let _ = tx_blocker.send(fresh.mod_app_blocker);
    session_flag.store(fresh.mod_sessions, Ordering::Relaxed);
    clip_flags
        .mod_clipboard
        .store(fresh.mod_clipboard, Ordering::Relaxed);
    clip_flags
        .mod_messenger
        .store(fresh.mod_messenger, Ordering::Relaxed);
    if let Ok(mut wm) = watermark_state.write() {
        *wm = collectors::watermark::WatermarkSettings::from_config(
            fresh.mod_watermark,
            fresh.watermark_opacity,
            &fresh.watermark_text,
            &fresh.watermark_apps,
            username,
            hostname,
        );
    }
}
