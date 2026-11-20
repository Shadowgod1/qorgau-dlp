from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from typing import Optional

from app.database import get_db
from app.models.session import UserSession

router = APIRouter()


@router.get("/sessions", summary="Сессии пользователей: логон/логоф/блокировка/RDP")
async def list_sessions(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    event_type: Optional[str] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(UserSession).order_by(UserSession.created_at.desc())
    if agent_id:
        q = q.where(UserSession.agent_id == agent_id)
    if username:
        q = q.where(UserSession.username == username)
    if event_type:
        q = q.where(UserSession.event_type == event_type)

    result = await db.execute(q.limit(limit).offset(offset))
    return [
        {
            "id": s.id, "agent_id": s.agent_id, "hostname": s.hostname,
            "username": s.username, "ip_address": s.ip_address,
            "event_type": s.event_type, "logon_type": s.logon_type,
            "meta": s.meta,
            "created_at": s.created_at.isoformat() if s.created_at else None,
        }
        for s in result.scalars().all()
    ]
