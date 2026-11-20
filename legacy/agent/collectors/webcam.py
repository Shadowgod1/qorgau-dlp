"""
Коллектор веб-камеры — снимки и стрим с камеры компьютера.

Режимы работы:
1. Периодический — делает снимок каждые WEBCAM_INTERVAL секунд.
2. Триггерный — сервер шлёт WS-команду {"type":"take_webcam","agent_id":"..."}.
3. Стрим — сервер активирует непрерывную отправку кадров (MJPEG через WS).

Формат: JPEG (настраиваемое качество).
Файлы отправляются через sender.upload_webcam_photo().

Правовое основание (ТК РК ст. 23): сотрудник уведомляется
о включении камеры через всплывающее окно.
"""
import io
import json
import logging
import threading
import time

import cv2
import numpy as np

from config import (
    SERVER_URL, AGENT_TOKEN,
    WEBCAM_INTERVAL, WEBCAM_QUALITY, WEBCAM_MAX_W,
    WARN_USER_ENABLED,
)

log = logging.getLogger("webcam")

_WS_RECONNECT = 10  # сек между попытками переподключения


class WebcamCollector:
    """Периодический / триггерный захват камеры + живой стрим."""

    def __init__(
        self,
        sender,
        interval: int | None = None,
        quality: int | None = None,
        max_w: int | None = None,
    ):
        self._sender = sender
        self._trigger = threading.Event()     # разовый снимок
        self._stream_active = threading.Event()  # стрим
        self._stop_stream = threading.Event()

        self._interval = interval if interval is not None else WEBCAM_INTERVAL
        self._quality = quality if quality is not None else WEBCAM_QUALITY
        self._max_w = max_w if max_w is not None else WEBCAM_MAX_W

        self._notified = False  # уведомление показано один раз

    # ── Публичный метод: один снимок по запросу ──────────────────────────────
    def take_now(self) -> None:
        """Делает один снимок по внешнему триггеру."""
        try:
            self._capture_and_upload()
            log.info("Triggered webcam photo uploaded")
        except Exception as e:
            log.error("Triggered webcam photo error: %s", e)

    def start_stream(self) -> None:
        """Включает стрим камеры."""
        self._stop_stream.clear()
        self._stream_active.set()
        log.info("Webcam stream activated")

    def stop_stream(self) -> None:
        """Выключает стрим камеры."""
        self._stop_stream.set()
        self._stream_active.clear()
        log.info("Webcam stream deactivated")

    # ── Периодический поток ──────────────────────────────────────────────────
    def run(self) -> None:
        """Запускается в daemon-потоке из agent.py."""
        # WS-слушатель для команд от сервера
        threading.Thread(
            target=self._ws_listener,
            daemon=True,
            name="webcam-ws",
        ).start()

        # Стрим-поток (ждёт активации)
        threading.Thread(
            target=self._stream_loop,
            daemon=True,
            name="webcam-stream",
        ).start()

        while True:
            try:
                self._capture_and_upload()
            except Exception as e:
                log.error("Webcam capture error: %s", e)

            # Ждём таймер ИЛИ триггер
            self._trigger.wait(timeout=self._interval)
            self._trigger.clear()

    # ── WebSocket-слушатель команд от сервера ─────────────────────────────────
    def _ws_listener(self) -> None:
        import websocket  # websocket-client

        agent_id = self._sender.agent_id
        ws_url = SERVER_URL.replace("http://", "ws://").replace("https://", "wss://")
        ws_url = f"{ws_url}/ws"

        while True:
            try:
                ws = websocket.create_connection(
                    ws_url,
                    timeout=30,
                    header={"X-Agent-Token": AGENT_TOKEN},
                )
                log.info("Webcam WS connected")
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

                        if msg_type == "take_webcam":
                            log.info("Take webcam command received")
                            self._trigger.set()

                        elif msg_type == "start_webcam_stream":
                            self.start_stream()

                        elif msg_type == "stop_webcam_stream":
                            self.stop_stream()

                    except Exception:
                        pass
            except Exception as e:
                log.debug("Webcam WS error: %s — reconnecting in %ds", e, _WS_RECONNECT)
                time.sleep(_WS_RECONNECT)

    # ── Стрим-петля (MJPEG через HTTP POST) ──────────────────────────────────
    def _stream_loop(self) -> None:
        """Отправляет кадры пока стрим активен (2-3 fps)."""
        while True:
            self._stream_active.wait()  # Блокируемся пока стрим не активирован
            log.info("Starting webcam stream loop")

            self._notify_employee()

            cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
            if not cap.isOpened():
                log.warning("Cannot open webcam for streaming")
                self._stream_active.clear()
                continue

            try:
                while self._stream_active.is_set() and not self._stop_stream.is_set():
                    ret, frame = cap.read()
                    if not ret:
                        break

                    jpeg_bytes = self._encode_frame(frame)
                    if jpeg_bytes:
                        self._sender.upload_webcam_photo(jpeg_bytes)

                    time.sleep(0.5)  # ~2 fps для стрима
            finally:
                cap.release()
                log.info("Webcam stream stopped")

    # ── Захват одного кадра и загрузка ───────────────────────────────────────
    def _capture_and_upload(self) -> None:
        self._notify_employee()

        cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
        if not cap.isOpened():
            log.warning("Cannot open webcam — device not available")
            return

        try:
            ret, frame = cap.read()
            if not ret or frame is None:
                log.warning("Empty frame from webcam")
                return

            jpeg_bytes = self._encode_frame(frame)
            if jpeg_bytes:
                self._sender.upload_webcam_photo(jpeg_bytes)
                log.info("Webcam photo uploaded: %d bytes", len(jpeg_bytes))
        finally:
            cap.release()

    # ── Кодирование кадра в JPEG ─────────────────────────────────────────────
    def _encode_frame(self, frame: np.ndarray) -> bytes | None:
        h, w = frame.shape[:2]

        # Масштабирование
        if w > self._max_w:
            ratio = self._max_w / w
            new_w = self._max_w
            new_h = int(h * ratio)
            frame = cv2.resize(frame, (new_w, new_h), interpolation=cv2.INTER_AREA)

        ok, buf = cv2.imencode(".jpg", frame, [cv2.IMWRITE_JPEG_QUALITY, self._quality])
        if not ok:
            log.warning("JPEG encoding failed")
            return None

        return buf.tobytes()

    # ── Уведомление сотрудника ───────────────────────────────────────────────
    def _notify_employee(self) -> None:
        if self._notified or not WARN_USER_ENABLED:
            return
        self._notified = True
        from notifier import notify
        notify(
            "Qorgau DLP: веб-камера активирована администратором безопасности.\n"
            "Периодически выполняется захват изображения.",
        )

    # ── Проверка доступности камеры ──────────────────────────────────────────
    @staticmethod
    def is_available() -> bool:
        """Проверяет наличие веб-камеры."""
        try:
            cap = cv2.VideoCapture(0, cv2.CAP_DSHOW)
            if cap.isOpened():
                cap.release()
                return True
        except Exception:
            pass
        return False
