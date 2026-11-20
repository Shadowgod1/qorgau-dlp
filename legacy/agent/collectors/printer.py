"""
Printer-коллектор.

Мониторинг заданий печати через WMI Win32_PrintJob.
Перехватывает: имя документа, принтер, количество страниц, размер, статус.
"""
import logging
import time

import wmi

log = logging.getLogger("printer")

_POLL = 10  # сек между проверками


class PrinterCollector:
    def __init__(self, sender):
        self._sender   = sender
        self._seen_ids: set[int] = set()
        self._wmi: wmi.WMI | None = None

    def _connect(self) -> bool:
        try:
            self._wmi = wmi.WMI()
            return True
        except Exception as e:
            log.error("WMI connect failed: %s", e)
            return False

    def run(self) -> None:
        log.info("PrinterCollector started")
        if not self._connect():
            log.warning("PrinterCollector: WMI unavailable, printer monitoring disabled")
            return

        while True:
            try:
                self._scan()
            except Exception as e:
                log.debug("PrinterCollector: %s", e)
                # Переподключение при потере WMI
                time.sleep(30)
                self._connect()
            time.sleep(_POLL)

    def _scan(self) -> None:
        try:
            jobs = self._wmi.Win32_PrintJob()
        except Exception as e:
            log.debug("Win32_PrintJob query failed: %s", e)
            return

        for job in jobs:
            try:
                job_id = int(job.JobId or 0)
                if job_id in self._seen_ids:
                    continue
                self._seen_ids.add(job_id)

                doc_name     = job.Document or ""
                printer_name = job.Name or ""
                pages        = int(job.TotalPages or job.PagesPrinted or 0)
                size_bytes   = int(job.Size or 0)
                status       = job.Status or ""
                owner        = job.Owner or ""

                log.info("PrintJob: [%s] '%s' (%d pages, %d bytes) by %s",
                         printer_name, doc_name, pages, size_bytes, owner)

                self._sender.add_event("print", {
                    "printer_name":  printer_name,
                    "document_name": doc_name,
                    "pages":         pages,
                    "size_bytes":    size_bytes,
                    "status":        status,
                    "owner":         owner,
                })
            except Exception as e:
                log.debug("PrintJob parse error: %s", e)

        # Ограничиваем размер множества
        if len(self._seen_ids) > 5000:
            self._seen_ids.clear()
