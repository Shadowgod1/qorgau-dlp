"""
Qorgau DLP Agent — точка входа.

Коллекторы:
  • SessionCollector    — события входа/выхода Windows
  • UsbCollector        — USB-устройства + файловый анализ
  • AppCollector        — активные окна / время фокуса
  • WebCollector        — история браузеров
  • MessengerCollector  — мессенджеры (clipboard + window titles)
  • ClipboardCollector  — буфер обмена (независимый DLP-канал)
  • NetworkCollector    — TCP/IP соединения
  • PrinterCollector    — задания печати
  • ScreenshotCapture   — скриншоты по таймеру + триггер от сервера
"""
import logging
import os
import platform
import socket
import sys
import threading
import time
import uuid
import winreg

import config
from sender import Sender
from screenshot import ScreenshotCapture
from collectors.sessions    import SessionCollector
from collectors.usb         import UsbCollector
from collectors.apps        import AppCollector
from collectors.web         import WebCollector
from collectors.messenger   import MessengerCollector
from collectors.clipboard   import ClipboardCollector
from collectors.network     import NetworkCollector
from collectors.printer     import PrinterCollector
from collectors.microphone  import MicrophoneCollector
from collectors.webcam      import WebcamCollector

# ── Логирование ───────────────────────────────────────────────────────────────
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    handlers=[
        logging.FileHandler(
            os.path.join(os.environ.get("TEMP", "."), "qorgau_agent.log"),
            encoding="utf-8",
        )
    ],
)
log = logging.getLogger("agent")


# ── Идентификатор машины ──────────────────────────────────────────────────────
def get_machine_guid() -> str:
    try:
        key  = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Cryptography")
        guid, _ = winreg.QueryValueEx(key, "MachineGuid")
        winreg.CloseKey(key)
        return guid.replace("-", "")
    except Exception:
        return str(uuid.uuid4()).replace("-", "")


# ── Автозапуск ────────────────────────────────────────────────────────────────
def add_to_startup() -> None:
    try:
        exe = sys.executable if getattr(sys, "frozen", False) else os.path.abspath(__file__)
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            0, winreg.KEY_SET_VALUE,
        )
        winreg.SetValueEx(key, "QorgauAgent", 0, winreg.REG_SZ, f'"{exe}"')
        winreg.CloseKey(key)
    except Exception as e:
        log.warning("Startup registration failed: %s", e)


# ── Запуск коллектора в потоке ────────────────────────────────────────────────
def _start(collector, name: str) -> None:
    t = threading.Thread(target=collector.run, daemon=True, name=name)
    t.start()
    log.info("Started: %s", name)


# ── Главная функция ───────────────────────────────────────────────────────────
def main() -> None:
    agent_id = get_machine_guid()
    hostname = socket.gethostname()
    username = os.environ.get("USERNAME", "unknown")

    log.info(
        "Qorgau DLP Agent v%s | host=%s user=%s id=%s",
        config.AGENT_VERSION, hostname, username, agent_id,
    )

    add_to_startup()

    sender = Sender(
        server_url   = config.SERVER_URL,
        token        = config.AGENT_TOKEN,
        agent_id     = agent_id,
        hostname     = hostname,
        username     = username,
    )

    sender.register(
        os_version    = platform.platform(),
        agent_version = config.AGENT_VERSION,
    )

    # ── Конфигурация с сервера (тумблеры модулей + настройки скриншотов) ───────
    cfg = sender.fetch_config() or {}
    if cfg:
        log.info("Loaded server config (id=%s)", cfg.get("id"))
    else:
        log.info("Using local defaults (server config unavailable)")

    def enabled(flag: str) -> bool:
        # По умолчанию модуль включён, если сервер не вернул флаг
        return cfg.get(flag, True)

    # ── Коллекторы ────────────────────────────────────────────────────────────

    # Микрофон (создаём заранее для передачи в ScreenshotCapture → WS-команды)
    mic_collector = None
    if enabled("mod_microphone"):
        mic_collector = MicrophoneCollector(
            sender,
            duration    = cfg.get("microphone_duration"),
            sample_rate = cfg.get("microphone_sample_rate"),
        )

    # Веб-камера
    webcam_collector = None
    if enabled("mod_webcam"):
        webcam_collector = WebcamCollector(
            sender,
            interval = cfg.get("webcam_interval"),
            quality  = cfg.get("webcam_quality"),
            max_w    = cfg.get("webcam_max_w"),
        )

    # Скриншоты (+ WS-хаб для микрофона и камеры)
    screenshot = ScreenshotCapture(
        sender,
        interval         = cfg.get("screenshot_interval"),
        quality          = cfg.get("screenshot_quality"),
        max_w            = cfg.get("screenshot_max_w"),
        mic_collector    = mic_collector,
        webcam_collector = webcam_collector,
    )

    candidates = [
        (SessionCollector,   "SessionCollector",   "mod_sessions"),
        (UsbCollector,       "UsbCollector",       "mod_usb"),
        (AppCollector,       "AppCollector",       "mod_apps"),
        (WebCollector,       "WebCollector",       "mod_web"),
        (MessengerCollector, "MessengerCollector", "mod_messenger"),
        (ClipboardCollector, "ClipboardCollector", "mod_clipboard"),
        (NetworkCollector,   "NetworkCollector",   "mod_network"),
        (PrinterCollector,   "PrinterCollector",   "mod_printer"),
    ]

    for cls, name, flag in candidates:
        if enabled(flag):
            _start(cls(sender), name)
        else:
            log.info("Skipped (disabled by config): %s", name)

    # Микрофон
    if mic_collector:
        _start(mic_collector, "MicrophoneCollector")
    else:
        log.info("Skipped (disabled by config): MicrophoneCollector")

    # Веб-камера
    if webcam_collector:
        _start(webcam_collector, "WebcamCollector")
    else:
        log.info("Skipped (disabled by config): WebcamCollector")

    if enabled("mod_screenshots"):
        _start(screenshot, "ScreenshotCapture")
    else:
        log.info("Skipped (disabled by config): ScreenshotCapture")

    # ── Основной цикл: flush + heartbeat ──────────────────────────────────────
    last_heartbeat = 0.0
    while True:
        time.sleep(config.BATCH_INTERVAL)
        sender.flush_batch()

        now = time.monotonic()
        if now - last_heartbeat >= config.HEARTBEAT_INTERVAL:
            sender.send_heartbeat()
            last_heartbeat = now


if __name__ == "__main__":
    main()
