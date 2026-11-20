"""API неизменяемого audit-журнала."""
from fastapi import APIRouter, Depends, Query
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession
from typing import Optional

from app.database import get_db
from app.api.auth import require_officer, require_admin
from app.audit import verify_chain
from app.models.audit_log import AuditLog

router = APIRouter(prefix="/audit", tags=["Audit"])


def _serialize(a: AuditLog) -> dict:
    return {
        "id": a.id,
        "ts": a.ts.isoformat() if a.ts else None,
        "actor": a.actor,
        "actor_role": a.actor_role,
        "action": a.action,
        "resource": a.resource,
        "details": a.details,
        "entry_hash": a.entry_hash,
        "prev_hash": a.prev_hash,
    }


@router.get("", summary="Журнал доступа (append-only)", dependencies=[Depends(require_officer)])
async def list_audit(
    actor: Optional[str] = None,
    action: Optional[str] = None,
    limit: int = Query(100, le=1000),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(AuditLog).order_by(AuditLog.id.desc())
    if actor:
        q = q.where(AuditLog.actor == actor)
    if action:
        q = q.where(AuditLog.action == action)

    total = await db.scalar(select(func.count()).select_from(q.subquery()))
    result = await db.execute(q.limit(limit).offset(offset))
    return {"total": total, "items": [_serialize(a) for a in result.scalars().all()]}


@router.get("/verify", summary="Проверка целостности цепочки", dependencies=[Depends(require_admin)])
async def verify(db: AsyncSession = Depends(get_db)):
    """Пересчитывает hash-chain. valid=false → журнал был изменён/усечён."""
    return await verify_chain(db)
