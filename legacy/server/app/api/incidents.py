from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func, update
from pydantic import BaseModel
from typing import Optional

from app.database import get_db
from app.api.auth import get_current_user, CurrentUser
from app.audit import write_audit
from app.masking import mask_text
from app.models.operator import ROLE_VIEWER
from app.models.incident import Incident, IncidentStatus, Severity

router = APIRouter()


class IncidentStatusUpdate(BaseModel):
    status: IncidentStatus


def _serialize(i: Incident, mask: bool = False) -> dict:
    return {
        "id": i.id,
        "agent_id": i.agent_id,
        "hostname": i.hostname,
        "username": i.username,
        "channel": i.channel,
        "severity": i.severity,
        "status": i.status,
        "matched_rules": i.matched_rules,
        "snippet": mask_text(i.snippet) if mask else i.snippet,
        "meta": i.meta,
        "created_at": i.created_at.isoformat() if i.created_at else None,
        "updated_at": i.updated_at.isoformat() if i.updated_at else None,
    }


@router.get("/incidents/stats", summary="Статистика инцидентов по срезам")
async def incident_stats(db: AsyncSession = Depends(get_db)):
    by_severity = await db.execute(
        select(Incident.severity, func.count()).group_by(Incident.severity)
    )
    by_status = await db.execute(
        select(Incident.status, func.count()).group_by(Incident.status)
    )
    by_channel = await db.execute(
        select(Incident.channel, func.count()).group_by(Incident.channel)
    )
    return {
        "by_severity": {str(k): v for k, v in by_severity.all()},
        "by_status": {str(k): v for k, v in by_status.all()},
        "by_channel": dict(by_channel.all()),
    }


@router.get("/incidents", summary="Список инцидентов с фильтрацией")
async def list_incidents(
    severity: Optional[str] = None,
    status: Optional[str] = None,
    username: Optional[str] = None,
    channel: Optional[str] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
    user: CurrentUser = Depends(get_current_user),
):
    q = select(Incident).order_by(Incident.created_at.desc())
    if severity:
        q = q.where(Incident.severity == severity)
    if status:
        q = q.where(Incident.status == status)
    if username:
        q = q.where(Incident.username == username)
    if channel:
        q = q.where(Incident.channel == channel)

    mask = user.role == ROLE_VIEWER
    total = await db.scalar(select(func.count()).select_from(q.subquery()))
    result = await db.execute(q.limit(limit).offset(offset))
    return {"total": total, "items": [_serialize(i, mask) for i in result.scalars().all()]}


@router.get("/incidents/{incident_id}", summary="Детали инцидента")
async def get_incident(
    incident_id: int,
    db: AsyncSession = Depends(get_db),
    user: CurrentUser = Depends(get_current_user),
):
    result = await db.execute(select(Incident).where(Incident.id == incident_id))
    incident = result.scalar_one_or_none()
    if not incident:
        raise HTTPException(status_code=404, detail="Incident not found")

    # Неизменяемый журнал доступа к ПДн (Приказ МЦРиАП РК №410)
    await write_audit(
        db, actor=user.username, actor_role=user.role,
        action="view_incident", resource=f"incident:{incident_id}",
        details={"channel": str(incident.channel), "username": incident.username},
    )
    return _serialize(incident, mask=user.role == ROLE_VIEWER)


@router.patch("/incidents/{incident_id}", summary="Обновить статус инцидента")
async def update_incident_status(
    incident_id: int,
    body: IncidentStatusUpdate,
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(select(Incident).where(Incident.id == incident_id))
    if not result.scalar_one_or_none():
        raise HTTPException(status_code=404, detail="Incident not found")
    await db.execute(
        update(Incident).where(Incident.id == incident_id).values(status=body.status)
    )
    await db.commit()
    return {"status": "updated"}
