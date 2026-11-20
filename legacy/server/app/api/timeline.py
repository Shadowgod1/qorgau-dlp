from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, union_all, literal, cast, String, func, and_
from typing import Optional, List
from datetime import datetime

from app.database import get_db
from app.models.incident import Incident
from app.models.usb import UsbEvent
from app.models.messenger import MessengerEvent
from app.models.app_activity import AppActivity
from app.models.web import WebActivity
from app.models.Screenshot import Screenshot

router = APIRouter()

ALL_TYPES = {"incident", "usb", "messenger", "app", "web", "screenshot"}


def _ts(col):
    return func.coalesce(col, func.now())


@router.get("/events/timeline", summary="Единая лента событий всех типов")
async def get_timeline(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    event_types: Optional[str] = Query(None, description="Comma-separated: incident,usb,messenger,app,web,screenshot"),
    severity: Optional[str] = None,
    date_from: Optional[datetime] = None,
    date_to: Optional[datetime] = None,
    search: Optional[str] = None,
    limit: int = Query(100, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    types = set(event_types.split(",")) if event_types else ALL_TYPES

    queries = []

    # ── Incidents ─────────────────────────────────────────────────────────────
    if "incident" in types:
        q = select(
            Incident.id,
            literal("incident").label("type"),
            Incident.agent_id,
            func.coalesce(Incident.hostname, "").label("hostname"),
            func.coalesce(Incident.username, "").label("username"),
            Incident.created_at.label("ts"),
            func.coalesce(Incident.snippet, "").label("summary"),
            cast(Incident.severity, String).label("severity"),
            cast(Incident.channel, String).label("subtype"),
        ).where(Incident.id != None)  # noqa
        if agent_id:
            q = q.where(Incident.agent_id == agent_id)
        if username:
            q = q.where(Incident.username == username)
        if severity:
            q = q.where(cast(Incident.severity, String) == severity)
        if date_from:
            q = q.where(Incident.created_at >= date_from)
        if date_to:
            q = q.where(Incident.created_at <= date_to)
        if search:
            q = q.where(Incident.snippet.ilike(f"%{search}%"))
        queries.append(q)

    # ── USB ───────────────────────────────────────────────────────────────────
    if "usb" in types and not severity:
        q = select(
            UsbEvent.id,
            literal("usb").label("type"),
            UsbEvent.agent_id,
            literal("").label("hostname"),
            func.coalesce(UsbEvent.username, "").label("username"),
            UsbEvent.created_at.label("ts"),
            func.coalesce(UsbEvent.device_name, "").label("summary"),
            literal("").label("severity"),
            func.coalesce(UsbEvent.action, "").label("subtype"),
        ).where(UsbEvent.id != None)  # noqa
        if agent_id:
            q = q.where(UsbEvent.agent_id == agent_id)
        if username:
            q = q.where(UsbEvent.username == username)
        if date_from:
            q = q.where(UsbEvent.created_at >= date_from)
        if date_to:
            q = q.where(UsbEvent.created_at <= date_to)
        if search:
            q = q.where(
                UsbEvent.device_name.ilike(f"%{search}%") |
                UsbEvent.file_path.ilike(f"%{search}%")
            )
        queries.append(q)

    # ── Messenger ─────────────────────────────────────────────────────────────
    if "messenger" in types and not severity:
        q = select(
            MessengerEvent.id,
            literal("messenger").label("type"),
            MessengerEvent.agent_id,
            literal("").label("hostname"),
            func.coalesce(MessengerEvent.username, "").label("username"),
            MessengerEvent.created_at.label("ts"),
            func.coalesce(MessengerEvent.messenger, "").label("summary"),
            literal("").label("severity"),
            func.coalesce(MessengerEvent.direction, "").label("subtype"),
        ).where(MessengerEvent.id != None)  # noqa
        if agent_id:
            q = q.where(MessengerEvent.agent_id == agent_id)
        if username:
            q = q.where(MessengerEvent.username == username)
        if date_from:
            q = q.where(MessengerEvent.created_at >= date_from)
        if date_to:
            q = q.where(MessengerEvent.created_at <= date_to)
        if search:
            q = q.where(MessengerEvent.content.ilike(f"%{search}%"))
        queries.append(q)

    # ── App activity ──────────────────────────────────────────────────────────
    if "app" in types and not severity:
        q = select(
            AppActivity.id,
            literal("app").label("type"),
            AppActivity.agent_id,
            literal("").label("hostname"),
            func.coalesce(AppActivity.username, "").label("username"),
            AppActivity.created_at.label("ts"),
            func.coalesce(AppActivity.app_name, "").label("summary"),
            literal("").label("severity"),
            func.coalesce(AppActivity.app_name, "").label("subtype"),
        ).where(AppActivity.id != None)  # noqa
        if agent_id:
            q = q.where(AppActivity.agent_id == agent_id)
        if username:
            q = q.where(AppActivity.username == username)
        if date_from:
            q = q.where(AppActivity.created_at >= date_from)
        if date_to:
            q = q.where(AppActivity.created_at <= date_to)
        if search:
            q = q.where(
                AppActivity.app_name.ilike(f"%{search}%") |
                AppActivity.window_title.ilike(f"%{search}%")
            )
        queries.append(q)

    # ── Web activity ──────────────────────────────────────────────────────────
    if "web" in types and not severity:
        q = select(
            WebActivity.id,
            literal("web").label("type"),
            WebActivity.agent_id,
            literal("").label("hostname"),
            func.coalesce(WebActivity.username, "").label("username"),
            WebActivity.created_at.label("ts"),
            func.coalesce(WebActivity.domain, "").label("summary"),
            literal("").label("severity"),
            func.coalesce(WebActivity.category, "").label("subtype"),
        ).where(WebActivity.id != None)  # noqa
        if agent_id:
            q = q.where(WebActivity.agent_id == agent_id)
        if username:
            q = q.where(WebActivity.username == username)
        if date_from:
            q = q.where(WebActivity.created_at >= date_from)
        if date_to:
            q = q.where(WebActivity.created_at <= date_to)
        if search:
            q = q.where(
                WebActivity.url.ilike(f"%{search}%") |
                WebActivity.domain.ilike(f"%{search}%")
            )
        queries.append(q)

    # ── Screenshots ───────────────────────────────────────────────────────────
    if "screenshot" in types and not severity:
        q = select(
            Screenshot.id,
            literal("screenshot").label("type"),
            Screenshot.agent_id,
            func.coalesce(Screenshot.hostname, "").label("hostname"),
            func.coalesce(Screenshot.username, "").label("username"),
            Screenshot.created_at.label("ts"),
            func.coalesce(Screenshot.username, "").label("summary"),
            literal("").label("severity"),
            literal("screenshot").label("subtype"),
        ).where(Screenshot.id != None)  # noqa
        if agent_id:
            q = q.where(Screenshot.agent_id == agent_id)
        if username:
            q = q.where(Screenshot.username == username)
        if date_from:
            q = q.where(Screenshot.created_at >= date_from)
        if date_to:
            q = q.where(Screenshot.created_at <= date_to)
        queries.append(q)

    if not queries:
        return {"items": [], "total": 0}

    if len(queries) == 1:
        combined = queries[0]
    else:
        combined = union_all(*queries)

    combined_subq = combined.subquery()
    total_q = select(func.count()).select_from(combined_subq)
    total_res = await db.execute(total_q)
    total = total_res.scalar() or 0

    paged = (
        select(combined_subq)
        .order_by(combined_subq.c.ts.desc())
        .limit(limit)
        .offset(offset)
    )
    result = await db.execute(paged)
    rows = result.fetchall()

    items = []
    for row in rows:
        items.append({
            "id": row.id,
            "type": row.type,
            "agent_id": row.agent_id,
            "hostname": row.hostname,
            "username": row.username,
            "ts": row.ts.isoformat() if row.ts else None,
            "summary": row.summary,
            "severity": row.severity or None,
            "subtype": row.subtype,
        })

    return {"items": items, "total": total}


@router.get("/events/agent-stats", summary="Счётчики событий по агентам")
async def agent_event_stats(
    db: AsyncSession = Depends(get_db),
):
    """Returns per-agent event counts across all tables."""
    results = {}

    for model, etype in [
        (Incident, "incident"),
        (UsbEvent, "usb"),
        (MessengerEvent, "messenger"),
        (AppActivity, "app"),
        (WebActivity, "web"),
        (Screenshot, "screenshot"),
    ]:
        q = select(model.agent_id, func.count().label("cnt")).group_by(model.agent_id)
        res = await db.execute(q)
        for agent_id, cnt in res.fetchall():
            if agent_id not in results:
                results[agent_id] = {"agent_id": agent_id, "incident": 0, "usb": 0, "messenger": 0, "app": 0, "web": 0, "screenshot": 0, "total": 0}
            results[agent_id][etype] = cnt
            results[agent_id]["total"] += cnt

    return list(results.values())
