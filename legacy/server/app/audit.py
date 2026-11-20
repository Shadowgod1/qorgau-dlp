"""
Сервис неизменяемого audit-журнала.

write_audit()  — добавляет запись в hash-chain.
verify_chain() — проверяет целостность всей цепочки.
"""
import json
import hashlib
import asyncio
from datetime import datetime, timezone
from typing import Optional

from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.audit_log import AuditLog, GENESIS_HASH

# Сериализация записей в цепочку не терпит гонок — глобальный лок на процесс.
# (Для мульти-воркерной установки нужен advisory-lock БД; см. README.)
_chain_lock = asyncio.Lock()


def _compute_hash(prev_hash: str, ts: datetime, actor: str, action: str,
                  resource: str, details: Optional[dict]) -> str:
    payload = "|".join([
        prev_hash,
        ts.isoformat(),
        actor or "",
        action or "",
        resource or "",
        json.dumps(details or {}, sort_keys=True, ensure_ascii=False),
    ])
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


async def write_audit(
    db: AsyncSession,
    actor: str,
    action: str,
    resource: str = "",
    details: Optional[dict] = None,
    actor_role: str = "",
) -> None:
    """Добавляет запись в неизменяемый журнал. Никогда не бросает наружу —
    сбой аудита не должен ронять основной запрос (но логируется)."""
    try:
        async with _chain_lock:
            last = await db.execute(
                select(AuditLog.entry_hash).order_by(AuditLog.id.desc()).limit(1)
            )
            prev_hash = last.scalar_one_or_none() or GENESIS_HASH

            ts = datetime.now(timezone.utc)
            entry_hash = _compute_hash(prev_hash, ts, actor, action, resource, details)

            db.add(AuditLog(
                ts=ts,
                actor=actor,
                actor_role=actor_role,
                action=action,
                resource=resource,
                details=details,
                prev_hash=prev_hash,
                entry_hash=entry_hash,
            ))
            await db.commit()
    except Exception:
        import logging
        logging.getLogger("audit").exception("Failed to write audit entry")
        try:
            await db.rollback()
        except Exception:
            pass


async def verify_chain(db: AsyncSession) -> dict:
    """
    Пересчитывает всю цепочку и находит первое нарушение.
    Возвращает {"valid": bool, "total": int, "broken_at": id|None}.
    """
    result = await db.execute(select(AuditLog).order_by(AuditLog.id))
    rows = list(result.scalars().all())

    prev_hash = GENESIS_HASH
    for row in rows:
        expected = _compute_hash(
            prev_hash, row.ts, row.actor, row.action, row.resource, row.details
        )
        if row.prev_hash != prev_hash or row.entry_hash != expected:
            return {"valid": False, "total": len(rows), "broken_at": row.id}
        prev_hash = row.entry_hash

    return {"valid": True, "total": len(rows), "broken_at": None}


async def count_entries(db: AsyncSession) -> int:
    return await db.scalar(select(func.count()).select_from(AuditLog)) or 0
