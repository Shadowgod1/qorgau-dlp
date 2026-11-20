"""
Всплывающие уведомления сотруднику (Трудовой кодекс РК ст. 23 — работник
информируется о контроле; DLP-требование предупреждать при попытке
передачи конфиденциальных данных).

Используется системный MessageBox (ctypes) — без внешних зависимостей.
Показывается в отдельном потоке, чтобы не блокировать коллектор.
"""
import ctypes
import logging
import threading

log = logging.getLogger("notifier")

MB_ICONWARNING = 0x30
MB_SYSTEMMODAL = 0x1000
MB_SETFOREGROUND = 0x10000

_TITLE = "Qorgau DLP — Предупреждение безопасности"


def notify(message: str, title: str = _TITLE) -> None:
    """Показывает предупреждение сотруднику (не блокирует вызывающий поток)."""
    def _show():
        try:
            ctypes.windll.user32.MessageBoxW(
                0, message, title,
                MB_ICONWARNING | MB_SYSTEMMODAL | MB_SETFOREGROUND,
            )
        except Exception as e:
            log.debug("notify failed: %s", e)

    threading.Thread(target=_show, daemon=True).start()
