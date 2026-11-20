"""API теневых копий (доказательная база). Доступ — только Офицер ИБ / админ."""
import os
from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import FileResponse
from sqlalchemy import select, func
from sqlalchemy.ext.asyncio import AsyncSession
from typing import Optional

from app.database import get_db
from app.api.auth import require_officer, get_current_user, CurrentUser
from app.audit import write_audit
from app.models.shadow_copy import ShadowCopy

router = APIRouter(prefix="/shadow", tags=["Shadow"])


def _serialize(s: ShadowCopy) -> dict:
    return {
        "id": s.id,
        "agent_id": s.agent_id,
        "hostname": s.hostname,
        "username": s.username,
        "channel": s.channel,
        "original_path": s.original_path,
        "file_name": s.file_name,
        "sha256": s.sha256,
        "size_bytes": s.size_bytes,
        "incident_id": s.incident_id,
        "created_at": s.created_at.isoformat() if s.created_at else None,
    }


@router.get("", summary="Список теневых копий", dependencies=[Depends(require_officer)])
async def list_shadow(
    username: Optional[str] = None,
    incident_id: Optional[int] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(ShadowCopy).order_by(ShadowCopy.created_at.desc())
    if username:
        q = q.where(ShadowCopy.username == username)
    if incident_id:
        q = q.where(ShadowCopy.incident_id == incident_id)

    total = await db.scalar(select(func.count()).select_from(q.subquery()))
    result = await db.execute(q.limit(limit).offset(offset))
    return {"total": total, "items": [_serialize(s) for s in result.scalars().all()]}


@router.get("/{shadow_id}/download", summary="Скачать теневую копию")
async def download_shadow(
    shadow_id: int,
    db: AsyncSession = Depends(get_db),
    user: CurrentUser = Depends(require_officer),
):
    result = await db.execute(select(ShadowCopy).where(ShadowCopy.id == shadow_id))
    shadow = result.scalar_one_or_none()
    if not shadow:
        raise HTTPException(status_code=404, detail="Shadow copy not found")
    if not os.path.exists(shadow.stored_path):
        raise HTTPException(status_code=410, detail="Файл копии удалён (истёк срок хранения)")

    # Скачивание вещдока — фиксируем в неизменяемом журнале
    await write_audit(
        db, actor=user.username, actor_role=user.role,
        action="download_shadow", resource=f"shadow:{shadow_id}",
        details={"file_name": shadow.file_name, "username": shadow.username,
                 "incident_id": shadow.incident_id},
    )

    return FileResponse(
        shadow.stored_path,
        filename=shadow.file_name,
        media_type="application/octet-stream",
    )
