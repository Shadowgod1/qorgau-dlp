import os
from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import FileResponse
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, delete, func
from typing import Optional

from app.database import get_db
from app.api.auth import require_officer, _decode_token, CurrentUser
from app.models.operator import ROLE_ADMIN, ROLE_SECURITY_OFFICER
from app.audit import write_audit
from app.models.Screenshot import Screenshot

router = APIRouter()


def _require_officer_token(token: str) -> CurrentUser:
    """Проверка токена из query-параметра (для <img>, который не шлёт заголовок)."""
    user = _decode_token(token)
    if user.role not in (ROLE_ADMIN, ROLE_SECURITY_OFFICER):
        raise HTTPException(status_code=403, detail="Недостаточно прав")
    return user


def _serialize(s: Screenshot) -> dict:
    return {
        "id": s.id,
        "agent_id": s.agent_id,
        "hostname": s.hostname,
        "username": s.username,
        "file_size": s.file_size,
        "created_at": s.created_at.isoformat() if s.created_at else None,
        "image_url": f"/api/v1/screenshots/{s.id}/image",
    }


@router.get("/screenshots", summary="Список скриншотов", dependencies=[Depends(require_officer)])
async def list_screenshots(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    limit: int = Query(50, le=200),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(Screenshot).order_by(Screenshot.created_at.desc())
    if agent_id:
        q = q.where(Screenshot.agent_id == agent_id)
    if username:
        q = q.where(Screenshot.username == username)

    total = await db.scalar(select(func.count()).select_from(q.subquery()))
    result = await db.execute(q.limit(limit).offset(offset))
    return {"total": total, "items": [_serialize(s) for s in result.scalars().all()]}


@router.get("/screenshots/latest", summary="Последний скриншот по каждому агенту", dependencies=[Depends(require_officer)])
async def latest_per_agent(db: AsyncSession = Depends(get_db)):
    subq = (
        select(Screenshot.agent_id, func.max(Screenshot.created_at).label("max_at"))
        .group_by(Screenshot.agent_id)
        .subquery()
    )
    result = await db.execute(
        select(Screenshot).join(
            subq,
            (Screenshot.agent_id == subq.c.agent_id)
            & (Screenshot.created_at == subq.c.max_at),
        )
    )
    return [_serialize(s) for s in result.scalars().all()]


@router.get("/screenshots/{screenshot_id}/image", summary="Получить изображение")
async def get_image(
    screenshot_id: int,
    token: str = Query(..., description="JWT (для <img>, который не шлёт заголовок)"),
    db: AsyncSession = Depends(get_db),
):
    user = _require_officer_token(token)
    result = await db.execute(select(Screenshot).where(Screenshot.id == screenshot_id))
    s = result.scalar_one_or_none()
    if not s or not os.path.exists(s.file_path):
        raise HTTPException(status_code=404, detail="Screenshot not found")

    # Просмотр скриншота — доступ к средству наблюдения, фиксируем в журнале
    await write_audit(
        db, actor=user.username, actor_role=user.role,
        action="view_screenshot", resource=f"screenshot:{screenshot_id}",
        details={"agent_id": s.agent_id, "username": s.username},
    )
    return FileResponse(s.file_path, media_type="image/jpeg")


@router.delete("/screenshots/{screenshot_id}", summary="Удалить скриншот", dependencies=[Depends(require_officer)])
async def delete_screenshot(screenshot_id: int, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Screenshot).where(Screenshot.id == screenshot_id))
    s = result.scalar_one_or_none()
    if s:
        if os.path.exists(s.file_path):
            os.remove(s.file_path)
        await db.execute(delete(Screenshot).where(Screenshot.id == screenshot_id))
        await db.commit()
    return {"status": "deleted"}
