import csv
import io
from datetime import datetime, timedelta
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.models.incident import Incident
from app.masking import mask_text


async def export_incidents_csv(db: AsyncSession, days: int = 30) -> str:
    since = datetime.utcnow() - timedelta(days=days)
    result = await db.execute(
        select(Incident)
        .where(Incident.created_at >= since)
        .order_by(Incident.created_at.desc())
    )
    incidents = result.scalars().all()

    output = io.StringIO()
    writer = csv.writer(output)
    writer.writerow(["id", "hostname", "username", "channel", "severity", "status", "snippet", "created_at"])
    for i in incidents:
        writer.writerow([
            i.id, i.hostname, i.username, i.channel,
            i.severity, i.status, mask_text(i.snippet),
            i.created_at.isoformat() if i.created_at else "",
        ])
    return output.getvalue()


async def export_incidents_xlsx(db: AsyncSession, days: int = 30) -> bytes:
    from app.reports.exporters import to_xlsx
    since = datetime.utcnow() - timedelta(days=days)
    result = await db.execute(
        select(Incident)
        .where(Incident.created_at >= since)
        .order_by(Incident.created_at.desc())
    )
    headers = ["ID", "Хост", "Пользователь", "Канал", "Серьёзность", "Статус", "Фрагмент", "Дата"]
    rows = [
        [
            i.id, i.hostname, i.username, i.channel,
            str(i.severity), str(i.status), mask_text(i.snippet),
            i.created_at.isoformat() if i.created_at else "",
        ]
        for i in result.scalars().all()
    ]
    return to_xlsx("Инциденты", headers, rows)


async def export_incidents_json(db: AsyncSession, days: int = 30) -> list:
    since = datetime.utcnow() - timedelta(days=days)
    result = await db.execute(
        select(Incident)
        .where(Incident.created_at >= since)
        .order_by(Incident.created_at.desc())
    )
    return [
        {
            "id": i.id, "hostname": i.hostname, "username": i.username,
            "channel": i.channel, "severity": str(i.severity),
            "status": str(i.status), "snippet": mask_text(i.snippet),
            "matched_rules": i.matched_rules, "meta": i.meta,
            "created_at": i.created_at.isoformat() if i.created_at else None,
        }
        for i in result.scalars().all()
    ]
