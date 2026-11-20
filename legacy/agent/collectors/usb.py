"""Мониторинг USB-устройств, копирования файлов и контентного анализа."""
import hashlib
import logging
import os
import threading
import time

import wmi
import win32file
import win32con

from config import (
    USB_POLL_INTERVAL, SHADOW_COPY_ENABLED, SHADOW_COPY_MAX_MB,
    USB_WARN_ON_SENSITIVE, WARN_USER_ENABLED,
)
from collectors.file_analyzer import extract_text
import local_dlp
from notifier import notify

log = logging.getLogger("usb")

_ANALYZABLE = {".docx", ".doc", ".pdf", ".txt", ".csv", ".xml", ".json", ".zip"}
# Типы файлов, для которых снимаем теневую копию (доказательная база)
_SHADOW_EXTS = {
    ".docx", ".doc", ".pdf", ".txt", ".csv", ".xml", ".json",
    ".xlsx", ".xls", ".pptx", ".ppt", ".zip", ".rar", ".7z",
    # Сканы/изображения — распознаются OCR на сервере
    ".jpg", ".jpeg", ".png", ".bmp", ".tif", ".tiff", ".webp",
}


def _sha256(path: str) -> str | None:
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(65536), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return None


class UsbCollector:
    def __init__(self, sender):
        self._sender       = sender
        self._known_drives: set[str] = set()
        self._watchers: dict[str, threading.Thread] = {}

    def run(self) -> None:
        c = wmi.WMI()
        while True:
            try:
                self._check(c)
            except Exception as e:
                log.error("UsbCollector: %s", e)
            time.sleep(USB_POLL_INTERVAL)

    def _check(self, c: wmi.WMI) -> None:
        current: set[str] = set()

        for disk in c.Win32_LogicalDisk(DriveType=2):
            drive_id = disk.DeviceID
            current.add(drive_id)
            if drive_id not in self._known_drives:
                self._sender.add_event("usb", {
                    "device_id":    drive_id,
                    "device_name":  disk.VolumeName or "USB Drive",
                    "action":       "connected",
                    "drive_letter": drive_id,
                    "size_gb":      round(int(disk.Size or 0) / 1e9, 2) if disk.Size else None,
                })
                self._start_watcher(drive_id, disk.VolumeName or "USB Drive")

        for drive_id in self._known_drives - current:
            self._sender.add_event("usb", {
                "device_id":    drive_id,
                "device_name":  "USB Drive",
                "action":       "disconnected",
                "drive_letter": drive_id,
            })

        self._known_drives = current

    def _upload_shadow(self, file_path: str, file_name: str, sha256: str) -> None:
        try:
            with open(file_path, "rb") as f:
                data = f.read()
            self._sender.upload_shadow_copy(
                file_bytes=data,
                file_name=file_name,
                original_path=file_path,
                sha256=sha256,
                channel="usb",
            )
            log.info("Shadow copy uploaded: %s (%d bytes)", file_name, len(data))
        except Exception as e:
            log.warning("Shadow copy failed for %s: %s", file_name, e)

    def _start_watcher(self, drive_id: str, device_name: str) -> None:
        t = threading.Thread(
            target=self._watch_drive,
            args=(drive_id, device_name),
            daemon=True,
            name=f"usb-watcher-{drive_id}",
        )
        self._watchers[drive_id] = t
        t.start()

    def _watch_drive(self, drive_id: str, device_name: str) -> None:
        path = drive_id + "\\"
        try:
            hDir = win32file.CreateFile(
                path, 1,
                win32con.FILE_SHARE_READ | win32con.FILE_SHARE_WRITE | win32con.FILE_SHARE_DELETE,
                None, win32con.OPEN_EXISTING, win32con.FILE_FLAG_BACKUP_SEMANTICS, None,
            )
        except Exception as e:
            log.warning("Cannot watch drive %s: %s", drive_id, e)
            return

        log.info("Watching USB drive %s", drive_id)
        try:
            while drive_id in self._known_drives:
                changes = win32file.ReadDirectoryChangesW(
                    hDir, 65536, True,
                    win32con.FILE_NOTIFY_CHANGE_FILE_NAME | win32con.FILE_NOTIFY_CHANGE_SIZE,
                    None, None,
                )
                for action, filename in changes:
                    if action != 1:   # FILE_ACTION_ADDED
                        continue
                    file_path = f"{drive_id}\\{filename}"
                    ext = os.path.splitext(filename)[1].lower()
                    size_bytes = 0
                    try:
                        size_bytes = os.path.getsize(file_path)
                    except Exception:
                        pass

                    event = {
                        "device_id":    drive_id,
                        "device_name":  device_name,
                        "action":       "file_copied",
                        "drive_letter": drive_id,
                        "file_path":    file_path,
                        "file_name":    filename,
                        "file_ext":     ext,
                        "size_bytes":   size_bytes,
                        "size_gb":      round(size_bytes / 1e9, 6) if size_bytes else None,
                        "sha256":       _sha256(file_path),
                        "content_snippet": None,
                    }

                    # Контентный анализ для офисных файлов и PDF
                    if ext in _ANALYZABLE:
                        text = extract_text(file_path)
                        if text:
                            event["content_snippet"] = text[:500]
                            log.info("Content extracted from %s: %d chars", filename, len(text))

                    # ── Локальная проверка: предупредить сотрудника ──────────
                    check_text = f"{filename}\n{event.get('content_snippet') or ''}"
                    rules, _ = local_dlp.check(check_text)
                    if rules:
                        event["local_rules"] = rules
                        if USB_WARN_ON_SENSITIVE and WARN_USER_ENABLED:
                            notify(
                                f"Копирование файла на съёмный носитель:\n{filename}\n\n"
                                f"Обнаружены конфиденциальные данные:\n• "
                                + "\n• ".join(rules)
                                + "\n\nДействие зафиксировано службой безопасности."
                            )
                        log.warning("USB DLP: %s | %s", filename, ", ".join(rules))

                    self._sender.add_event("usb", event)

                    # Теневая копия файла (доказательная база)
                    if (
                        SHADOW_COPY_ENABLED
                        and ext in _SHADOW_EXTS
                        and 0 < size_bytes <= SHADOW_COPY_MAX_MB * 1024 * 1024
                    ):
                        self._upload_shadow(file_path, filename, event.get("sha256") or "")
        except Exception as e:
            log.warning("Drive watcher stopped (%s): %s", drive_id, e)
        finally:
            win32file.CloseHandle(hDir)
