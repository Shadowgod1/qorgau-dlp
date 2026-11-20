"""
Network-коллектор.

Собирает активные TCP/UDP соединения через psutil.
Пропускает loopback и уже виденные соединения (дедупликация по ключу).
Особо отмечает соединения на чувствительные порты (22, 3389, 445, 25, 587).
"""
import logging
import time
import socket
from collections import defaultdict

import psutil

log = logging.getLogger("network")

_POLL          = 15   # сек между сканами
_SENSITIVE_PORTS = {22, 23, 25, 53, 110, 143, 443, 445, 587, 993, 995, 3306, 3389, 5432, 6379}
_SKIP_LOCAL    = {"127.0.0.1", "::1", "0.0.0.0", ""}


class NetworkCollector:
    def __init__(self, sender):
        self._sender  = sender
        # Кэш: ключ→timestamp последней отправки
        self._seen: dict[str, float] = {}
        self._TTL = 300  # повторно отправлять если соединение живёт >5 мин

    def run(self) -> None:
        log.info("NetworkCollector started")
        while True:
            try:
                self._scan()
            except Exception as e:
                log.debug("NetworkCollector: %s", e)
            time.sleep(_POLL)

    def _scan(self) -> None:
        now = time.monotonic()
        # Собираем pid→имя процесса заранее (одно обращение к OS)
        pid_names: dict[int, tuple[str, str]] = {}
        for proc in psutil.process_iter(["pid", "name", "exe"]):
            try:
                pid_names[proc.pid] = (proc.info["name"] or "", proc.info["exe"] or "")
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass

        events = []
        for conn in psutil.net_connections(kind="inet"):
            try:
                # Пропускаем незавершённые / loopback
                if conn.status not in ("ESTABLISHED", "CLOSE_WAIT", "SYN_SENT"):
                    continue
                if not conn.raddr:
                    continue
                remote_ip = conn.raddr.ip
                if remote_ip in _SKIP_LOCAL or remote_ip.startswith("169.254"):
                    continue

                local_ip   = conn.laddr.ip if conn.laddr else ""
                local_port = conn.laddr.port if conn.laddr else 0
                remote_port= conn.raddr.port
                proto      = "tcp" if conn.type == socket.SOCK_STREAM else "udp"
                pid        = conn.pid or 0
                proc_name, exe_path = pid_names.get(pid, ("", ""))

                key = f"{local_ip}:{local_port}-{remote_ip}:{remote_port}-{pid}"

                # Дедупликация с TTL
                last_sent = self._seen.get(key, 0)
                if now - last_sent < self._TTL:
                    continue
                self._seen[key] = now

                events.append({
                    "local_ip":    local_ip,
                    "local_port":  local_port,
                    "remote_ip":   remote_ip,
                    "remote_port": remote_port,
                    "protocol":    proto,
                    "status":      conn.status,
                    "process_name": proc_name,
                    "exe_path":    exe_path,
                    "sensitive":   remote_port in _SENSITIVE_PORTS,
                })
            except Exception:
                continue

        for ev in events:
            self._sender.add_event("network", ev)

        # Чистим старые ключи из кэша
        if len(self._seen) > 10000:
            cutoff = now - self._TTL * 2
            self._seen = {k: v for k, v in self._seen.items() if v > cutoff}

        if events:
            log.info("NetworkCollector: %d new connections", len(events))
