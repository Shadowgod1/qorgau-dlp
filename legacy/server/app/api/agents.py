from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, delete
from datetime import datetime, timezone, timedelta

from app.database import get_db
from app.models.agent import Agent
from app.models.policy import Policy

router = APIRouter()

# Агент считается онлайн если последний сигнал был < 3 минуты назад
_ONLINE_THRESHOLD = timedelta(minutes=3)


def _is_online(a: Agent) -> bool:
    if not a.last_seen:
        return False
    last = a.last_seen
    if last.tzinfo is None:
        last = last.replace(tzinfo=timezone.utc)
    return (datetime.now(timezone.utc) - last) < _ONLINE_THRESHOLD


def _agent_dict(a: Agent) -> dict:
    return {
        "id": a.id,
        "agent_id": a.agent_id,
        "hostname": a.hostname,
        "username": a.username,
        "ip_address": a.ip_address,
        "os_version": a.os_version,
        "agent_version": a.agent_version,
        "is_online": _is_online(a),
        "last_seen": a.last_seen.isoformat() if a.last_seen else None,
        "registered_at": a.registered_at.isoformat() if a.registered_at else None,
    }


@router.get("/agents", summary="Список всех агентов")
async def list_agents(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Agent).order_by(Agent.last_seen.desc()))
    return [_agent_dict(a) for a in result.scalars().all()]


@router.get("/agents/{agent_id}", summary="Детали агента")
async def get_agent(agent_id: str, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Agent).where(Agent.agent_id == agent_id))
    agent = result.scalar_one_or_none()
    if not agent:
        raise HTTPException(status_code=404, detail="Agent not found")
    return _agent_dict(agent)


@router.delete("/agents/{agent_id}", summary="Удалить агента")
async def delete_agent(agent_id: str, db: AsyncSession = Depends(get_db)):
    await db.execute(delete(Agent).where(Agent.agent_id == agent_id))
    await db.commit()
    return {"status": "deleted"}


@router.get("/agents/{agent_id}/policies", summary="Активные политики для агента")
async def get_agent_policies(agent_id: str, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Policy).where(Policy.enabled == True))
    return [
        {
            "id": p.id, "name": p.name, "channel": p.channel,
            "action": p.action, "severity": p.severity, "pattern": p.pattern,
        }
        for p in result.scalars().all()
    ]
