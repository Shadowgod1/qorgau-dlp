"""HTTP-клиент для отправки событий и скриншотов на сервер Qorgau DLP."""
import threading
import socket
import logging
from typing import Any

import requests

log = logging.getLogger("sender")


class Sender:
    def __init__(self, server_url: str, token: str, agent_id: str, hostname: str, username: str):
        self._base    = server_url.rstrip("/")
        self._headers = {"X-Agent-Token": token}
        self.agent_id = agent_id
        self.hostname = hostname
        self.username = username

        self._batch: list[dict[str, Any]] = []
        self._lock  = threading.Lock()

    # ── Регистрация ───────────────────────────────────────────────────────────

    def register(self, os_version: str, agent_version: str) -> None:
        payload = {
            "agent_id":      self.agent_id,
            "hostname":      self.hostname,
            "username":      self.username,
            "ip_address":    self._local_ip(),
            "os_version":    os_version,
            "agent_version": agent_version,
        }
        self._post("/api/v1/monitor/register", json=payload)

    # ── Очередь событий ──────────────────────────────────────────────────────

    def add_event(self, event_type: str, data: dict[str, Any]) -> None:
        with self._lock:
            self._batch.append({"type": event_type, "data": data})

    def flush_batch(self) -> None:
        with self._lock:
            if not self._batch:
                return
            batch, self._batch = self._batch[:], []

        payload = {
            "agent_id": self.agent_id,
            "hostname": self.hostname,
            "username": self.username,
            "events":   batch,
        }
        ok = self._post("/api/v1/monitor/events", json=payload)
        if not ok:
            # Вернуть события обратно в очередь
            with self._lock:
                self._batch = batch + self._batch

    # ── Heartbeat ─────────────────────────────────────────────────────────────

    def send_heartbeat(self) -> None:
        self._post("/api/v1/monitor/heartbeat", params={"agent_id": self.agent_id})

    # ── Конфигурация ──────────────────────────────────────────────────────────

    def fetch_config(self) -> dict[str, Any] | None:
        """Забирает конфигурацию агента с сервера. None при ошибке."""
        try:
            resp = requests.get(
                f"{self._base}/api/v1/monitor/config",
                headers=self._headers,
                params={"agent_id": self.agent_id},
                timeout=15,
            )
            resp.raise_for_status()
            return resp.json()
        except Exception as e:
            log.warning("Fetch config failed: %s", e)
            return None

    # ── Скриншоты ─────────────────────────────────────────────────────────────

    def upload_screenshot(self, jpeg_bytes: bytes) -> None:
        try:
            resp = requests.post(
                f"{self._base}/api/v1/monitor/screenshot",
                files={"file": ("screenshot.jpg", jpeg_bytes, "image/jpeg")},
                data={
                    "agent_id": self.agent_id,
                    "hostname": self.hostname,
                    "username": self.username,
                },
                headers=self._headers,
                timeout=30,
            )
            resp.raise_for_status()
        except Exception as e:
            log.warning("Screenshot upload failed: %s", e)

    # ── Теневые копии ─────────────────────────────────────────────────────────

    def upload_shadow_copy(
        self, file_bytes: bytes, file_name: str,
        original_path: str, sha256: str, channel: str = "usb",
    ) -> None:
        """Загружает полную копию перехваченного файла для расследования."""
        try:
            resp = requests.post(
                f"{self._base}/api/v1/monitor/shadow",
                files={"file": (file_name, file_bytes, "application/octet-stream")},
                data={
                    "agent_id":      self.agent_id,
                    "hostname":      self.hostname,
                    "username":      self.username,
                    "original_path": original_path,
                    "sha256":        sha256,
                    "channel":       channel,
                },
                headers=self._headers,
                timeout=60,
            )
            resp.raise_for_status()
        except Exception as e:
            log.warning("Shadow copy upload failed (%s): %s", file_name, e)

    # ── Аудиозаписи (микрофон) ───────────────────────────────────────────────

    def upload_audio(self, wav_bytes: bytes, duration_sec: int = 0) -> None:
        """Загружает аудиозапись с микрофона на сервер."""
        try:
            resp = requests.post(
                f"{self._base}/api/v1/monitor/audio",
                files={"file": ("recording.wav", wav_bytes, "audio/wav")},
                data={
                    "agent_id": self.agent_id,
                    "hostname": self.hostname,
                    "username": self.username,
                    "duration_sec": str(duration_sec),
                },
                headers=self._headers,
                timeout=60,
            )
            resp.raise_for_status()
        except Exception as e:
            log.warning("Audio upload failed: %s", e)

    # ── Фото с веб-камеры ────────────────────────────────────────────────────

    def upload_webcam_photo(self, jpeg_bytes: bytes) -> None:
        """Загружает снимок с веб-камеры на сервер."""
        try:
            resp = requests.post(
                f"{self._base}/api/v1/monitor/webcam",
                files={"file": ("webcam.jpg", jpeg_bytes, "image/jpeg")},
                data={
                    "agent_id": self.agent_id,
                    "hostname": self.hostname,
                    "username": self.username,
                },
                headers=self._headers,
                timeout=30,
            )
            resp.raise_for_status()
        except Exception as e:
            log.warning("Webcam photo upload failed: %s", e)

    # ── Вспомогательные ──────────────────────────────────────────────────────

    def _post(self, path: str, **kwargs) -> bool:
        try:
            resp = requests.post(
                f"{self._base}{path}",
                headers=self._headers,
                timeout=15,
                **kwargs,
            )
            resp.raise_for_status()
            return True
        except Exception as e:
            log.warning("POST %s failed: %s", path, e)
            return False

    @staticmethod
    def _local_ip() -> str:
        try:
            return socket.gethostbyname(socket.gethostname())
        except Exception:
            return "unknown"
