from fastapi import APIRouter, Depends, Query
from fastapi.responses import Response
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from datetime import datetime, timedelta
from typing import Literal, Optional

from app.database import get_db
from app.models.incident import Incident, Severity
from app.models.agent import Agent
from app.models.usb import UsbEvent
from app.models.messenger import MessengerEvent
from app.models.web import WebActivity
from app.compliance.kz_law import get_requirements_report
from app.reports.generator import export_incidents_csv, export_incidents_json, export_incidents_xlsx
from app.reports.exporters import cdr_rows, cdr_to_csv, cdr_to_xlsx, to_xlsx

router = APIRouter()

_XLSX_MEDIA = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"


@router.get("/reports/incidents", summary="Сводный отчёт по инцидентам")
async def incident_report(
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    since = datetime.utcnow() - timedelta(days=days)

    by_severity = await db.execute(
        select(Incident.severity, func.count())
        .where(Incident.created_at >= since)
        .group_by(Incident.severity)
    )
    by_channel = await db.execute(
        select(Incident.channel, func.count())
        .where(Incident.created_at >= since)
        .group_by(Incident.channel)
    )
    top_users = await db.execute(
        select(Incident.username, func.count().label("count"))
        .where(Incident.created_at >= since)
        .group_by(Incident.username)
        .order_by(func.count().desc())
        .limit(10)
    )

    return {
        "period_days": days,
        "by_severity": {str(k): v for k, v in by_severity.all()},
        "by_channel": dict(by_channel.all()),
        "top_users": [{"username": r[0], "count": r[1]} for r in top_users.all()],
    }


@router.get("/reports/user/{username}", summary="Профиль риска конкретного пользователя")
async def user_report(
    username: str,
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    since = datetime.utcnow() - timedelta(days=days)

    incidents = await db.scalar(
        select(func.count()).where(Incident.username == username, Incident.created_at >= since)
    )
    critical = await db.scalar(
        select(func.count()).where(
            Incident.username == username,
            Incident.severity == Severity.CRITICAL,
            Incident.created_at >= since,
        )
    )
    usb_copies = await db.scalar(
        select(func.count()).where(
            UsbEvent.username == username,
            UsbEvent.action == "file_copied",
            UsbEvent.created_at >= since,
        )
    )
    blocked_sites = await db.scalar(
        select(func.count()).where(
            WebActivity.username == username,
            WebActivity.blocked == True,
            WebActivity.created_at >= since,
        )
    )
    flagged_messages = await db.scalar(
        select(func.count()).where(
            MessengerEvent.username == username,
            MessengerEvent.flagged == True,
            MessengerEvent.created_at >= since,
        )
    )

    return {
        "username": username,
        "period_days": days,
        "incidents_total": incidents,
        "incidents_critical": critical,
        "usb_file_copies": usb_copies,
        "blocked_site_attempts": blocked_sites,
        "flagged_messages": flagged_messages,
    }


@router.get("/reports/compliance", summary="Отчёт о соответствии законодательству РК")
async def compliance_report():
    return get_requirements_report()


@router.get("/reports/agents", summary="Сводка по агентам")
async def agents_report(db: AsyncSession = Depends(get_db)):
    total = await db.scalar(select(func.count()).select_from(Agent))
    online = await db.scalar(select(func.count()).where(Agent.is_online == True))
    return {
        "total_agents": total,
        "online_agents": online,
        "offline_agents": (total or 0) - (online or 0),
    }


@router.get("/reports/export", summary="Экспорт инцидентов (JSON/CSV/Excel)")
async def export_report(
    format: Literal["json", "csv", "xlsx"] = Query("json"),
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    if format == "csv":
        content = await export_incidents_csv(db, days)
        return Response(
            content=content,
            media_type="text/csv",
            headers={"Content-Disposition": f"attachment; filename=incidents_{days}d.csv"},
        )
    if format == "xlsx":
        content = await export_incidents_xlsx(db, days)
        return Response(
            content=content,
            media_type=_XLSX_MEDIA,
            headers={"Content-Disposition": f"attachment; filename=incidents_{days}d.xlsx"},
        )
    return await export_incidents_json(db, days)


@router.get("/reports/cdr", summary="CDR — детальные записи событий (CSV/Excel)")
async def export_cdr(
    format: Literal["csv", "xlsx"] = Query("csv"),
    days: int = Query(30, le=365),
    event_type: Optional[str] = None,
    username: Optional[str] = None,
    db: AsyncSession = Depends(get_db),
):
    rows = await cdr_rows(db, days=days, event_type=event_type, username=username)
    if format == "xlsx":
        return Response(
            content=cdr_to_xlsx(rows),
            media_type=_XLSX_MEDIA,
            headers={"Content-Disposition": f"attachment; filename=cdr_{days}d.xlsx"},
        )
    return Response(
        content=cdr_to_csv(rows),
        media_type="text/csv",
        headers={"Content-Disposition": f"attachment; filename=cdr_{days}d.csv"},
    )


@router.get("/reports/activity/export", summary="Экспорт учёта рабочего времени (Excel)")
async def export_activity(
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    from app.api.activity import worktime as _worktime
    rows = await _worktime(username=None, date_from=None, date_to=None, days=days, db=db)
    headers = ["День", "Пользователь", "Начало", "Конец", "Активно, с", "Простой, с",
               "Продуктивно, с", "Непродуктивно, с", "Продуктивность, %"]
    data = [
        [
            r["day"], r["username"],
            (r["first_seen"] or "")[11:16], (r["last_seen"] or "")[11:16],
            r["active_sec"], r["idle_sec"],
            r["productive_sec"], r["unproductive_sec"], r["productivity_pct"],
        ]
        for r in rows
    ]
    return Response(
        content=to_xlsx("Рабочее время", headers, data),
        media_type=_XLSX_MEDIA,
        headers={"Content-Disposition": f"attachment; filename=worktime_{days}d.xlsx"},
    )
