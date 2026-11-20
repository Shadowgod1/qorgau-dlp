from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, delete
from pydantic import BaseModel
from typing import Optional

from app.database import get_db
from app.models.policy import Policy
from app.models.blocked_domain import BlockedDomain
from app.engine.web_filter import web_filter

router = APIRouter()


class PolicyCreate(BaseModel):
    name: str
    channel: str   # usb / email / web / clipboard / messenger / file
    action: str    # block / alert / log
    severity: str
    pattern: str
    enabled: bool = True
    created_by: Optional[str] = None


class DomainCreate(BaseModel):
    domain: str
    category: str = "custom"
    reason: Optional[str] = None
    created_by: Optional[str] = None


def _policy_dict(p: Policy) -> dict:
    return {
        "id": p.id, "name": p.name, "channel": p.channel,
        "action": p.action, "severity": p.severity, "pattern": p.pattern,
        "enabled": p.enabled, "created_by": p.created_by,
        "created_at": p.created_at.isoformat() if p.created_at else None,
    }


def _domain_dict(d: BlockedDomain) -> dict:
    return {
        "id": d.id, "domain": d.domain, "category": d.category,
        "reason": d.reason, "enabled": d.enabled, "created_by": d.created_by,
        "created_at": d.created_at.isoformat() if d.created_at else None,
    }


# ── Политики DLP ──────────────────────────────────────────────────────────────

@router.get("/rules", summary="Список всех политик DLP")
async def list_rules(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Policy).order_by(Policy.created_at.desc()))
    return [_policy_dict(p) for p in result.scalars().all()]


@router.post("/rules", status_code=201, summary="Создать политику DLP")
async def create_rule(body: PolicyCreate, db: AsyncSession = Depends(get_db)):
    policy = Policy(**body.model_dump())
    db.add(policy)
    await db.commit()
    await db.refresh(policy)
    return _policy_dict(policy)


@router.put("/rules/{rule_id}", summary="Обновить политику DLP")
async def update_rule(rule_id: int, body: PolicyCreate, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Policy).where(Policy.id == rule_id))
    policy = result.scalar_one_or_none()
    if not policy:
        raise HTTPException(status_code=404, detail="Rule not found")
    for k, v in body.model_dump().items():
        setattr(policy, k, v)
    await db.commit()
    return _policy_dict(policy)


@router.delete("/rules/{rule_id}", summary="Удалить политику DLP")
async def delete_rule(rule_id: int, db: AsyncSession = Depends(get_db)):
    await db.execute(delete(Policy).where(Policy.id == rule_id))
    await db.commit()
    return {"status": "deleted"}


# ── Заблокированные домены ────────────────────────────────────────────────────

@router.get("/rules/domains", summary="Список заблокированных доменов")
async def list_domains(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(BlockedDomain).order_by(BlockedDomain.created_at.desc()))
    return [_domain_dict(d) for d in result.scalars().all()]


@router.post("/rules/domains", status_code=201, summary="Добавить домен в блок-лист")
async def add_domain(body: DomainCreate, db: AsyncSession = Depends(get_db)):
    domain = BlockedDomain(**body.model_dump())
    db.add(domain)
    await db.commit()
    await web_filter.add_domain(body.domain)
    await db.refresh(domain)
    return _domain_dict(domain)


@router.delete("/rules/domains/{domain_id}", summary="Убрать домен из блок-листа")
async def remove_domain(domain_id: int, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(BlockedDomain).where(BlockedDomain.id == domain_id))
    domain = result.scalar_one_or_none()
    if not domain:
        raise HTTPException(status_code=404, detail="Domain not found")
    await web_filter.remove_domain(domain.domain)
    await db.execute(delete(BlockedDomain).where(BlockedDomain.id == domain_id))
    await db.commit()
    return {"status": "deleted"}
