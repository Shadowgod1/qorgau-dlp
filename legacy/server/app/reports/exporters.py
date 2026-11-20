"""
Экспортёры отчётов: Excel (xlsx) и CDR (детальные записи событий).

CDR (Communication/Call Detail Records) — построчный журнал по каждому
событию всех каналов (метаданные, без сырого контента ПДн). Формат — CSV
(стандарт CDR) либо Excel.
"""
import io
import csv
from datetime import datetime, timedelta
from typing import Optional

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

from app.api.olap import FACT_SQL


# ── Excel ─────────────────────────────────────────────────────────────────────

def to_xlsx(sheet_name: str, headers: list[str], rows: list[list]) -> bytes:
    """Формирует xlsx из заголовков и строк. Требует openpyxl."""
    from openpyxl import Workbook
    from openpyxl.styles import Font, PatternFill

    wb = Workbook()
    ws = wb.active
    ws.title = sheet_name[:31]

    header_font = Font(bold=True, color="FFFFFF")
    header_fill = PatternFill("solid", fgColor="3B82F6")
    ws.append(headers)
    for cell in ws[1]:
        cell.font = header_font
        cell.fill = header_fill

    for row in rows:
        ws.append(row)

    # Автоширина по содержимому (грубая)
    for col_idx, header in enumerate(headers, 1):
        max_len = len(str(header))
        for row in rows:
            if col_idx - 1 < len(row):
                max_len = max(max_len, len(str(row[col_idx - 1])))
        ws.column_dimensions[ws.cell(row=1, column=col_idx).column_letter].width = min(max_len + 2, 60)

    buf = io.BytesIO()
    wb.save(buf)
    return buf.getvalue()


# ── CDR: детальные записи событий ─────────────────────────────────────────────

CDR_HEADERS = [
    "timestamp", "event_type", "event_id", "agent_id",
    "hostname", "username", "subtype", "severity", "metric",
]


async def cdr_rows(
    db: AsyncSession,
    days: int = 30,
    event_type: Optional[str] = None,
    username: Optional[str] = None,
    date_from: Optional[datetime] = None,
    date_to: Optional[datetime] = None,
    limit: int = 100000,
) -> list[dict]:
    filters = ["1=1"]
    params: dict = {"limit": limit}

    if date_from:
        filters.append("ts >= :date_from"); params["date_from"] = date_from
    else:
        filters.append("ts >= :since")
        params["since"] = datetime.utcnow() - timedelta(days=days)
    if date_to:
        filters.append("ts <= :date_to"); params["date_to"] = date_to
    if event_type:
        filters.append("event_type = :event_type"); params["event_type"] = event_type
    if username:
        filters.append("username = :username"); params["username"] = username

    where = " AND ".join(filters)
    sql = text(f"""
        WITH facts AS ({FACT_SQL})
        SELECT event_type, event_id, agent_id, hostname, username,
               subtype, severity, ts,
               COALESCE(metric_int, metric_float, 0) AS metric
        FROM facts
        WHERE {where}
        ORDER BY ts DESC
        LIMIT :limit
    """)
    result = await db.execute(sql, params)
    rows = []
    for r in result.mappings().all():
        d = dict(r)
        d["timestamp"] = d.pop("ts").isoformat() if d.get("ts") else None
        rows.append(d)
    return rows


def cdr_to_csv(rows: list[dict]) -> str:
    output = io.StringIO()
    writer = csv.writer(output, delimiter=";")   # CDR обычно ; — разделитель
    writer.writerow(CDR_HEADERS)
    for r in rows:
        writer.writerow([
            r.get("timestamp"), r.get("event_type"), r.get("event_id"),
            r.get("agent_id"), r.get("hostname"), r.get("username"),
            r.get("subtype"), r.get("severity"), r.get("metric"),
        ])
    return output.getvalue()


def cdr_to_xlsx(rows: list[dict]) -> bytes:
    data = [
        [r.get("timestamp"), r.get("event_type"), r.get("event_id"),
         r.get("agent_id"), r.get("hostname"), r.get("username"),
         r.get("subtype"), r.get("severity"), r.get("metric")]
        for r in rows
    ]
    return to_xlsx("CDR", CDR_HEADERS, data)
