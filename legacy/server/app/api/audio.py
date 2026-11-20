"""API-роутер для аудиозаписей с микрофона."""
import os
from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import FileResponse
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func, desc
from typing import Optional

from app.database import get_db
from app.models.audio_recording import AudioRecording

router = APIRouter(prefix="/audio", tags=["Audio"])


@router.get("", summary="Список аудиозаписей")
async def list_recordings(
    agent_id: Optional[str] = None,
    hostname: Optional[str] = None,
    limit: int = Query(50, ge=1, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(AudioRecording).order_by(desc(AudioRecording.created_at))

    if agent_id:
        q = q.where(AudioRecording.agent_id == agent_id)
    if hostname:
        q = q.where(AudioRecording.hostname.ilike(f"%{hostname}%"))

    # Count
    count_q = select(func.count()).select_from(q.subquery())
    total = (await db.execute(count_q)).scalar() or 0

    result = await db.execute(q.offset(offset).limit(limit))
    rows = result.scalars().all()

    return {
        "total": total,
        "items": [
            {
                "id": r.id,
                "agent_id": r.agent_id,
                "hostname": r.hostname,
                "username": r.username,
                "file_size": r.file_size,
                "duration_sec": r.duration_sec,
                "sample_rate": r.sample_rate,
                "created_at": r.created_at.isoformat() if r.created_at else None,
            }
            for r in rows
        ],
    }


@router.get("/{recording_id}/download", summary="Скачать аудиозапись")
async def download_recording(
    recording_id: int,
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(AudioRecording).where(AudioRecording.id == recording_id)
    )
    recording = result.scalar_one_or_none()
    if not recording:
        raise HTTPException(status_code=404, detail="Recording not found")

    if not os.path.isfile(recording.file_path):
        raise HTTPException(status_code=404, detail="File not found on disk")

    return FileResponse(
        recording.file_path,
        media_type="audio/wav",
        filename=f"recording_{recording.agent_id}_{recording.id}.wav",
    )


@router.delete("/{recording_id}", summary="Удалить аудиозапись")
async def delete_recording(
    recording_id: int,
    db: AsyncSession = Depends(get_db),
):
    result = await db.execute(
        select(AudioRecording).where(AudioRecording.id == recording_id)
    )
    recording = result.scalar_one_or_none()
    if not recording:
        raise HTTPException(status_code=404, detail="Recording not found")

    # Удаляем файл
    if os.path.isfile(recording.file_path):
        os.remove(recording.file_path)

    await db.delete(recording)
    await db.commit()

    return {"status": "ok"}
