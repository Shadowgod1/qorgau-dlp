"""Мониторинг сессий пользователей через Windows Event Log (Security).

Event ID:
  4624 — успешный вход
  4634 — выход
  4800 — блокировка экрана
  4801 — разблокировка
  4778 — подключение RDP-сессии
  4779 — отключение RDP-сессии
"""
import logging
import time

import win32evtlog

log = logging.getLogger("sessions")

EVENT_MAP = {
    4624: "logon",
    4634: "logoff",
    4800: "lock",
    4801: "unlock",
    4778: "rdp_logon",
    4779: "logoff",
}

LOGON_TYPE_MAP = {
    2: "Interactive",
    3: "Network",
    4: "Batch",
    5: "Service",
    7: "Unlock",
    10: "RemoteInteractive",
    11: "CachedInteractive",
}

_POLL = 10  # секунд между опросами


class SessionCollector:
    def __init__(self, sender):
        self._sender     = sender
        self._last_record = 0

    def run(self) -> None:
        while True:
            try:
                self._poll()
            except Exception as e:
                log.error("SessionCollector: %s", e)
            time.sleep(_POLL)

    def _poll(self) -> None:
        handle = win32evtlog.OpenEventLog(None, "Security")
        flags  = win32evtlog.EVENTLOG_BACKWARDS_READ | win32evtlog.EVENTLOG_SEQUENTIAL_READ

        try:
            events = win32evtlog.ReadEventLog(handle, flags, 0)
        finally:
            win32evtlog.CloseEventLog(handle)

        if not events:
            return

        new_records: list[tuple[int, object]] = []
        for ev in events:
            if ev.RecordNumber <= self._last_record:
                break
            new_records.append((ev.RecordNumber, ev))

        # Обрабатываем в хронологическом порядке
        for rec_num, ev in reversed(new_records):
            event_id = ev.EventID & 0xFFFF
            if event_id in EVENT_MAP:
                strings = ev.StringInserts or []
                logon_type_raw = strings[8] if len(strings) > 8 else None
                try:
                    logon_type = LOGON_TYPE_MAP.get(int(logon_type_raw), logon_type_raw)
                except (TypeError, ValueError):
                    logon_type = None

                self._sender.add_event("session", {
                    "event_type": EVENT_MAP[event_id],
                    "logon_type": logon_type,
                    "meta":       {"event_id": event_id},
                })

        if new_records:
            self._last_record = new_records[0][0]
