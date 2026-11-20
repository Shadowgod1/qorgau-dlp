"""
Мониторинг мессенджеров.

Три механизма:
1. Буфер обмена — следим всегда, не только в фокусе мессенджера.
   Если буфер изменился и любой мессенджер запущен — пишем событие.
2. Заголовки окон — GetWindowText по всем окнам запущенных мессенджеров.
   Даёт имя собеседника/чата и хэш для дедупликации.
3. Фокус — фиксируем момент когда мессенджер получил / потерял фокус.
"""
import hashlib
import logging
import time
import threading

import psutil
import win32clipboard
import win32gui
import win32process

log = logging.getLogger("messenger")

MESSENGER_MAP: dict[str, str] = {
    "telegram.exe":   "Telegram",
    "whatsapp.exe":   "WhatsApp",
    "slack.exe":      "Slack",
    "teams.exe":      "Teams",
    "msteams.exe":    "Teams",
    "viber.exe":      "Viber",
    "discord.exe":    "Discord",
    "skype.exe":      "Skype",
    "element.exe":    "Element",
    "signal.exe":     "Signal",
    "zoom.exe":       "Zoom",
}

_CLIPBOARD_POLL = 2    # сек
_WINDOW_POLL    = 5    # сек
_RECENT_WINDOW  = 60   # сек — считаем мессенджер «недавно активным»


def _sha1(text: str) -> str:
    return hashlib.sha1(text.encode("utf-8", errors="ignore")).hexdigest()


def _get_clipboard() -> str | None:
    try:
        win32clipboard.OpenClipboard()
        try:
            return win32clipboard.GetClipboardData(win32clipboard.CF_UNICODETEXT)
        except Exception:
            return None
        finally:
            win32clipboard.CloseClipboard()
    except Exception:
        return None


def _running_messengers() -> dict[str, str]:
    """Возвращает {exe_name: display_name} для запущенных мессенджеров."""
    found: dict[str, str] = {}
    for proc in psutil.process_iter(["name"]):
        try:
            name = proc.info["name"].lower()
            if name in MESSENGER_MAP:
                found[name] = MESSENGER_MAP[name]
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return found


def _window_titles(exe_name: str) -> list[str]:
    """Собирает видимые заголовки окон процесса."""
    titles: list[str] = []

    def _cb(hwnd, _):
        try:
            if not win32gui.IsWindowVisible(hwnd):
                return True
            _, pid = win32process.GetWindowThreadProcessId(hwnd)
            proc = psutil.Process(pid)
            if proc.name().lower() == exe_name:
                t = win32gui.GetWindowText(hwnd)
                if t and len(t.strip()) > 2:
                    titles.append(t.strip())
        except Exception:
            pass
        return True

    win32gui.EnumWindows(_cb, None)
    return titles


def _foreground_exe() -> str | None:
    try:
        hwnd = win32gui.GetForegroundWindow()
        if not hwnd:
            return None
        _, pid = win32process.GetWindowThreadProcessId(hwnd)
        return psutil.Process(pid).name().lower()
    except Exception:
        return None


class MessengerCollector:
    def __init__(self, sender):
        self._sender = sender
        self._clipboard_hash = ""
        self._seen_titles: set[str] = set()
        self._last_active: dict[str, float] = {}  # exe -> timestamp последнего фокуса
        self._lock = threading.Lock()

    # ── Поток 1: буфер обмена (всегда) ───────────────────────────────────────

    def _clipboard_loop(self) -> None:
        while True:
            try:
                self._check_clipboard()
            except Exception as e:
                log.debug("clipboard_loop: %s", e)
            time.sleep(_CLIPBOARD_POLL)

    def _check_clipboard(self) -> None:
        text = _get_clipboard()
        if not text or len(text.strip()) < 5:
            return

        h = _sha1(text)
        if h == self._clipboard_hash:
            return
        self._clipboard_hash = h

        running = _running_messengers()
        if not running:
            return

        # Ищем последний активный мессенджер
        now = time.monotonic()
        with self._lock:
            recent = {
                exe: ts for exe, ts in self._last_active.items()
                if now - ts < _RECENT_WINDOW and exe in running
            }

        if recent:
            # Берём самый последний
            exe = max(recent, key=lambda e: recent[e])
            messenger = running[exe]
            direction = "sent"  # копировал пока мессенджер был активен → скорее всего отправка
        else:
            # Мессенджер запущен, но не в фокусе последние 60 сек
            messenger = next(iter(running.values()))
            direction = "received"  # скорее скопировал входящее в другом окне

        log.info("Clipboard captured for %s: %d chars", messenger, len(text))
        self._sender.add_event("messenger", {
            "messenger": messenger,
            "direction": direction,
            "content":   text[:1000],
            "contact":   None,
            "has_file":  False,
        })

    # ── Поток 2: заголовки окон ───────────────────────────────────────────────

    def _window_loop(self) -> None:
        while True:
            try:
                self._check_windows()
            except Exception as e:
                log.debug("window_loop: %s", e)
            time.sleep(_WINDOW_POLL)

    def _check_windows(self) -> None:
        running = _running_messengers()
        for exe, messenger in running.items():
            for title in _window_titles(exe):
                # Пропускаем дефолтные заголовки приложений (не несут информации)
                skip = {
                    "telegram", "whatsapp", "slack", "discord",
                    "microsoft teams", "viber", "skype", "zoom",
                }
                if title.lower() in skip:
                    continue
                # Пропускаем числа и очень короткие строки
                if len(title) < 3:
                    continue

                key = _sha1(exe + title)
                with self._lock:
                    if key in self._seen_titles:
                        continue
                    self._seen_titles.add(key)
                    if len(self._seen_titles) > 5000:
                        self._seen_titles.clear()

                log.info("Window title [%s]: %s", messenger, title)
                self._sender.add_event("messenger", {
                    "messenger": messenger,
                    "direction": "window",
                    "content":   title[:500],
                    "contact":   title[:100],
                    "has_file":  any(
                        ext in title.lower()
                        for ext in [".pdf", ".doc", ".xls", ".zip", ".rar", ".jpg", ".png"]
                    ),
                })

    # ── Поток 3: трекинг фокуса ───────────────────────────────────────────────

    def _focus_loop(self) -> None:
        prev_exe = ""
        while True:
            try:
                exe = _foreground_exe() or ""
                if exe in MESSENGER_MAP and exe != prev_exe:
                    with self._lock:
                        self._last_active[exe] = time.monotonic()
                    log.info("Messenger focused: %s", MESSENGER_MAP[exe])
                prev_exe = exe
            except Exception as e:
                log.debug("focus_loop: %s", e)
            time.sleep(1)

    # ── Точка входа ───────────────────────────────────────────────────────────

    def run(self) -> None:
        threading.Thread(target=self._clipboard_loop, daemon=True, name="messenger-clipboard").start()
        threading.Thread(target=self._window_loop,    daemon=True, name="messenger-windows").start()
        # Основной поток = фокус трекинг
        self._focus_loop()
