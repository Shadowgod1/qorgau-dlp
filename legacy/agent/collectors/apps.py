"""
Мониторинг активных приложений и учёт рабочего времени.

Для каждого окна в фокусе фиксируется:
  • duration_sec — сколько окно было активно (в фокусе)
  • active_sec   — из них с реальным вводом (клавиатура/мышь)
  • idle_sec     — из них простой (нет ввода дольше порога)

Простой определяется через WinAPI GetLastInputInfo (время с последнего
ввода). Это даёт корректный учёт: открытое окно без активности не
засчитывается как рабочее время.
"""
import ctypes
import logging
import time

import psutil
import win32gui
import win32process

from config import APP_POLL_INTERVAL, IDLE_THRESHOLD_SEC

log = logging.getLogger("apps")

MIN_DURATION_SEC = 5  # не отправлять если окно было активно меньше N секунд


class _LASTINPUTINFO(ctypes.Structure):
    _fields_ = [("cbSize", ctypes.c_uint), ("dwTime", ctypes.c_uint)]


def _idle_seconds() -> float:
    """Секунд с момента последнего ввода пользователя (клавиатура/мышь)."""
    try:
        info = _LASTINPUTINFO()
        info.cbSize = ctypes.sizeof(info)
        if ctypes.windll.user32.GetLastInputInfo(ctypes.byref(info)):
            millis = ctypes.windll.kernel32.GetTickCount() - info.dwTime
            return millis / 1000.0
    except Exception:
        pass
    return 0.0


class AppCollector:
    def __init__(self, sender):
        self._sender       = sender
        self._current_key  = ""
        self._current_data : dict = {}
        self._start_time   = 0.0
        self._last_poll    = 0.0
        self._active_sec   = 0.0
        self._idle_sec     = 0.0

    def run(self) -> None:
        self._last_poll = time.monotonic()
        while True:
            try:
                self._check()
            except Exception as e:
                log.error("AppCollector: %s", e)
            time.sleep(APP_POLL_INTERVAL)

    def _flush_current(self, now: float) -> None:
        """Отправляет накопленную статистику по текущему окну."""
        if not (self._current_key and self._start_time):
            return
        duration = int(now - self._start_time)
        if duration >= MIN_DURATION_SEC:
            self._sender.add_event("app", {
                **self._current_data,
                "duration_sec": duration,
                "active_sec":   int(self._active_sec),
                "idle_sec":     int(self._idle_sec),
            })

    def _check(self) -> None:
        now = time.monotonic()
        delta = now - self._last_poll
        self._last_poll = now

        # Классифицируем прошедший интервал: активный или простой
        if delta > 0 and self._current_key:
            if _idle_seconds() >= IDLE_THRESHOLD_SEC:
                self._idle_sec += delta
            else:
                self._active_sec += delta

        hwnd = win32gui.GetForegroundWindow()
        if not hwnd:
            return

        title = win32gui.GetWindowText(hwnd)
        _, pid = win32process.GetWindowThreadProcessId(hwnd)

        try:
            proc     = psutil.Process(pid)
            app_name = proc.name()
            exe_path = proc.exe()
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            app_name = "unknown"
            exe_path = ""

        key = f"{app_name}|{title}"

        if key != self._current_key:
            # Зафиксировать предыдущее окно
            self._flush_current(now)

            self._current_key  = key
            self._current_data = {
                "app_name":     app_name,
                "window_title": title[:300],
                "exe_path":     exe_path,
            }
            self._start_time = now
            self._active_sec = 0.0
            self._idle_sec   = 0.0
