"""
Clipboard DLP-коллектор.

Независимый канал — перехватывает ВСЁ содержимое буфера обмена,
не зависит от активного окна. Определяет приложение-источник.
"""
import hashlib
import logging
import time

import psutil
import win32clipboard
import win32gui
import win32process

import local_dlp
from notifier import notify
from config import WARN_USER_ENABLED, CLIPBOARD_BLOCK

log = logging.getLogger("clipboard")

_POLL        = 1.5   # сек между проверками
_MIN_LEN     = 5     # минимум символов для отправки
_MAX_CONTENT = 2000  # максимум символов в событии


def _sha1(text: str) -> str:
    return hashlib.sha1(text.encode("utf-8", errors="ignore")).hexdigest()


def _get_clipboard_text() -> str | None:
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


def _clear_clipboard() -> bool:
    """Очищает буфер обмена — предотвращает вставку чувствительных данных."""
    try:
        win32clipboard.OpenClipboard()
        try:
            win32clipboard.EmptyClipboard()
            return True
        finally:
            win32clipboard.CloseClipboard()
    except Exception as e:
        log.warning("Failed to clear clipboard: %s", e)
        return False


def _foreground_app() -> str:
    """Имя процесса активного окна."""
    try:
        hwnd = win32gui.GetForegroundWindow()
        if hwnd:
            _, pid = win32process.GetWindowThreadProcessId(hwnd)
            return psutil.Process(pid).name()
    except Exception:
        pass
    return ""


class ClipboardCollector:
    def __init__(self, sender):
        self._sender   = sender
        self._last_hash = ""

    def run(self) -> None:
        log.info("ClipboardCollector started")
        while True:
            try:
                self._check()
            except Exception as e:
                log.debug("ClipboardCollector: %s", e)
            time.sleep(_POLL)

    def _check(self) -> None:
        text = _get_clipboard_text()
        if not text or len(text.strip()) < _MIN_LEN:
            return

        h = _sha1(text)
        if h == self._last_hash:
            return
        self._last_hash = h

        source_app = _foreground_app()
        content    = text[:_MAX_CONTENT]

        # ── Локальная проверка: предупредить / заблокировать немедленно ───────
        rules, block = local_dlp.check(text)
        blocked = False
        if rules:
            if block and CLIPBOARD_BLOCK:
                blocked = _clear_clipboard()
                self._last_hash = ""  # буфер очищен — сбрасываем дедуп
            if WARN_USER_ENABLED:
                action = "СКОПИРОВАННЫЕ ДАННЫЕ УДАЛЕНЫ из буфера обмена." if blocked \
                    else "Действие зафиксировано службой безопасности."
                notify(
                    f"Обнаружены конфиденциальные данные:\n• "
                    + "\n• ".join(rules)
                    + f"\n\n{action}"
                )
            log.warning("Clipboard DLP: %s | blocked=%s", ", ".join(rules), blocked)

        log.info("Clipboard: %d chars from %s", len(text), source_app)
        self._sender.add_event("clipboard", {
            "content":    content,
            "source_app": source_app,
            "length":     len(text),
            "truncated":  len(text) > _MAX_CONTENT,
            "local_rules": rules,
            "blocked":    blocked,
        })
