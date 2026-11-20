"""
Экспорт инцидентов в ОЦИБ/SIEM по Syslog + CEF.

CEF (Common Event Format, ArcSight) — требуемый «Едиными требованиями ИБ»
формат для передачи событий в Оперативный центр информационной безопасности.

Формат:
  CEF:0|Vendor|Product|Version|SignatureID|Name|Severity|Extension
обёрнутый в syslog-заголовок RFC 5424:
  <PRI>1 TIMESTAMP HOST APP PROCID MSGID CEF:...
"""
import socket
import logging
from datetime import datetime, timezone

log = logging.getLogger("siem_cef")

# CEF severity 0-10 из наших уровней
_CEF_SEVERITY = {"low": 3, "medium": 5, "high": 7, "critical": 10}


def _escape_header(value: str) -> str:
    # В заголовке CEF экранируются \ и |
    return value.replace("\\", "\\\\").replace("|", "\\|")


def _escape_ext(value: str) -> str:
    # В расширениях экранируются \ , = и переводы строк
    return (
        str(value)
        .replace("\\", "\\\\")
        .replace("=", "\\=")
        .replace("\n", " ")
        .replace("\r", " ")
    )


def build_cef(
    incident_id: int,
    severity: str,
    channel: str,
    hostname: str,
    username: str,
    snippet: str,
    matched_rules: list[str],
) -> str:
    cef_sev = _CEF_SEVERITY.get(severity, 5)
    name = _escape_header(f"DLP {channel} violation")
    ext_parts = {
        "externalId": incident_id,
        "cs1Label": "channel",   "cs1": _escape_ext(channel),
        "cs2Label": "rules",     "cs2": _escape_ext(", ".join(matched_rules or [])),
        "shost": _escape_ext(hostname),
        "suser": _escape_ext(username),
        "msg":   _escape_ext(snippet or ""),
        "cat":   "DLP",
    }
    extension = " ".join(f"{k}={v}" for k, v in ext_parts.items())

    return (
        f"CEF:0|Qorgau|DLP|1.0|{channel}-{severity}|{name}|{cef_sev}|{extension}"
    )


def _syslog_frame(cef: str, facility: int, severity_level: int, hostname: str) -> str:
    # PRI = facility*8 + severity
    pri = facility * 8 + severity_level
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    host = hostname or "qorgau"
    # RFC 5424: <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID MSG
    return f"<{pri}>1 {ts} {host} QorgauDLP - - - {cef}"


def send_syslog_cef(
    host: str,
    port: int,
    proto: str,
    facility: int,
    incident_id: int,
    severity: str,
    channel: str,
    hostname: str,
    username: str,
    snippet: str,
    matched_rules: list[str],
) -> None:
    """Отправляет один инцидент в syslog-коллектор в формате CEF."""
    cef = build_cef(incident_id, severity, channel, hostname, username, snippet, matched_rules)
    # severity уровень syslog: critical/high -> 2 (crit)/3 (err), medium -> 4, low -> 6
    sev_level = {"critical": 2, "high": 3, "medium": 4, "low": 6}.get(severity, 5)
    frame = _syslog_frame(cef, facility, sev_level, hostname)
    data = frame.encode("utf-8")

    if proto.lower() == "tcp":
        # RFC 6587 octet-counting framing для TCP
        payload = f"{len(data)} ".encode("utf-8") + data
        with socket.create_connection((host, port), timeout=10) as sock:
            sock.sendall(payload)
    else:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.sendto(data, (host, port))
