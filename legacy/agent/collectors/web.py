"""Чтение истории браузеров (Chrome, Edge, Firefox) из SQLite-баз."""
import logging
import os
import shutil
import sqlite3
import tempfile
import time
from urllib.parse import urlparse

from config import WEB_POLL_INTERVAL

log = logging.getLogger("web")

APPDATA_LOCAL  = os.environ.get("LOCALAPPDATA", "")
APPDATA_ROAMING = os.environ.get("APPDATA", "")

CHROMIUM_PROFILES = {
    "chrome": os.path.join(APPDATA_LOCAL,  r"Google\Chrome\User Data\Default\History"),
    "edge":   os.path.join(APPDATA_LOCAL,  r"Microsoft\Edge\User Data\Default\History"),
    "brave":  os.path.join(APPDATA_LOCAL,  r"BraveSoftware\Brave-Browser\User Data\Default\History"),
}

FIREFOX_ROOT = os.path.join(APPDATA_ROAMING, "Mozilla", "Firefox", "Profiles")


def _firefox_db() -> str | None:
    if not os.path.isdir(FIREFOX_ROOT):
        return None
    for entry in os.listdir(FIREFOX_ROOT):
        candidate = os.path.join(FIREFOX_ROOT, entry, "places.sqlite")
        if os.path.exists(candidate):
            return candidate
    return None


def _extract_domain(url: str) -> str:
    try:
        return urlparse(url).netloc.lower()
    except Exception:
        return url[:80]


class WebCollector:
    def __init__(self, sender):
        self._sender       = sender
        self._last_ids: dict[str, int] = {}

    def run(self) -> None:
        while True:
            try:
                self._poll_all()
            except Exception as e:
                log.error("WebCollector: %s", e)
            time.sleep(WEB_POLL_INTERVAL)

    def _poll_all(self) -> None:
        for browser, db_path in CHROMIUM_PROFILES.items():
            if os.path.exists(db_path):
                self._read_chromium(browser, db_path)

        ff_db = _firefox_db()
        if ff_db:
            self._read_firefox(ff_db)

    def _read_chromium(self, browser: str, db_path: str) -> None:
        tmp = self._copy_db(db_path)
        if not tmp:
            return
        try:
            conn   = sqlite3.connect(tmp)
            cursor = conn.cursor()
            last   = self._last_ids.get(browser, 0)
            cursor.execute(
                "SELECT id, url FROM urls WHERE id > ? ORDER BY id ASC LIMIT 200",
                (last,),
            )
            rows = cursor.fetchall()
            conn.close()

            for row_id, url in rows:
                if not url.startswith("http"):
                    continue
                self._sender.add_event("web", {
                    "url":     url[:500],
                    "domain":  _extract_domain(url),
                    "browser": browser,
                })
                self._last_ids[browser] = row_id
        except Exception as e:
            log.warning("Chromium history (%s): %s", browser, e)
        finally:
            try:
                os.unlink(tmp)
            except OSError:
                pass

    def _read_firefox(self, db_path: str) -> None:
        tmp = self._copy_db(db_path)
        if not tmp:
            return
        try:
            conn   = sqlite3.connect(tmp)
            cursor = conn.cursor()
            last   = self._last_ids.get("firefox", 0)
            cursor.execute(
                "SELECT p.id, p.url FROM moz_places p "
                "JOIN moz_historyvisits h ON h.place_id = p.id "
                "WHERE p.id > ? ORDER BY p.id ASC LIMIT 200",
                (last,),
            )
            rows = cursor.fetchall()
            conn.close()

            for row_id, url in rows:
                if not url.startswith("http"):
                    continue
                self._sender.add_event("web", {
                    "url":     url[:500],
                    "domain":  _extract_domain(url),
                    "browser": "firefox",
                })
                self._last_ids["firefox"] = row_id
        except Exception as e:
            log.warning("Firefox history: %s", e)
        finally:
            try:
                os.unlink(tmp)
            except OSError:
                pass

    @staticmethod
    def _copy_db(src: str) -> str | None:
        """Копировать заблокированный SQLite-файл во временный."""
        try:
            fd, tmp = tempfile.mkstemp(suffix=".db")
            os.close(fd)
            shutil.copy2(src, tmp)
            return tmp
        except Exception as e:
            log.debug("Cannot copy DB %s: %s", src, e)
            return None
