"""
Мониторинг мессенджеров на корпоративных устройствах.

Подход: мониторинг через доступные Windows API методы —
читаем текст из активных окон мессенджеров (accessibility API).
Не расшифровываем трафик — только читаем видимый контент.

Поддерживаемые мессенджеры:
- Telegram Desktop
- WhatsApp Desktop
- Slack
- Microsoft Teams
- Viber Desktop
- Discord
"""

import win32gui
import win32process
import win32con
import win32api
import psutil
import threading
import time
import hashlib
import logging
import ctypes
from ctypes import wintypes
from sender import put_event

# Названия процессов мессенджеров
MESSENGER_PROCESSES = {
    "telegram.exe":        "telegram",
    "whatsapp.exe":        "whatsapp",
    "slack.exe":           "slack",
    "teams.exe":           "teams",
    "msteams.exe":         "teams",
    "viber.exe":           "viber",
    "discord.exe":         "discord",
}

MAX_CONTENT_LENGTH = 500  # символов — не храним полный текст

def get_running_messengers() -> dict[str, str]:
    """Возвращает {процесс: мессенджер} для запущенных мессенджеров."""
    running = {}
    for proc in psutil.process_iter(['name', 'pid']):
        try:
            name = proc.info['name'].lower()
            if name in MESSENGER_PROCESSES:
                running[name] = MESSENGER_PROCESSES[name]
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return running

def get_window_text_by_process(process_name: str) -> list[str]:
    """Получает текст из всех окон указанного процесса через WinAPI."""
    texts = []

    def callback(hwnd, results):
        try:
            _, pid = win32process.GetWindowThreadProcessId(hwnd)
            proc = psutil.Process(pid)
            if proc.name().lower() == process_name.lower():
                if win32gui.IsWindowVisible(hwnd):
                    text = win32gui.GetWindowText(hwnd)
                    if text and len(text) > 10:
                        results.append(text)
        except Exception:
            pass
        return True

    win32gui.EnumWindows(callback, texts)
    return texts

class MessengerMonitor:
    def __init__(self):
        self._seen_hashes: set[str] = set()
        self._lock = threading.Lock()

    def _hash(self, text: str) -> str:
        return hashlib.md5(text.encode()).hexdigest()

    def _is_new(self, text: str) -> bool:
        h = self._hash(text)
        with self._lock:
            if h in self._seen_hashes:
                return False
            self._seen_hashes.add(h)
            # Ограничиваем размер кэша
            if len(self._seen_hashes) > 10000:
                self._seen_hashes.clear()
            return True

    def scan(self):
        running = get_running_messengers()

        for process_name, messenger_name in running.items():
            try:
                texts = get_window_text_by_process(process_name)
                for text in texts:
                    if not self._is_new(text):
                        continue

                    snippet = text[:MAX_CONTENT_LENGTH]
                    put_event("messenger_activity", {
                        "messenger": messenger_name,
                        "process":   process_name,
                        "content":   snippet,
                        "truncated": len(text) > MAX_CONTENT_LENGTH,
                        "direction": "unknown",  # определить направление сложно без интеграции
                        "has_file":  any(ext in text.lower() for ext in [".pdf", ".docx", ".xlsx", ".zip", ".rar"])
                    })

            except Exception as e:
                logging.debug(f"Ошибка мониторинга {process_name}: {e}")

    def monitor_loop(self):
        logging.info("Мониторинг мессенджеров запущен")
        while True:
            try:
                self.scan()
            except Exception as e:
                logging.error(f"Ошибка в цикле мониторинга мессенджеров: {e}")
            time.sleep(10)

def start():
    monitor = MessengerMonitor()
    t = threading.Thread(target=monitor.monitor_loop, daemon=True)
    t.start()
