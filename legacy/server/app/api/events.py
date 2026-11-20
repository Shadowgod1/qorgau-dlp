from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select
from typing import Optional

from app.database import get_db
from app.api.auth import require_officer, get_current_user, CurrentUser
from app.audit import write_audit
from app.models.usb import UsbEvent
from app.models.messenger import MessengerEvent
from app.models.app_activity import AppActivity

router = APIRouter()


@router.get("/events/usb", summary="USB-события: подключения, копирование файлов")
async def list_usb_events(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    action: Optional[str] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(UsbEvent).order_by(UsbEvent.created_at.desc())
    if agent_id:
        q = q.where(UsbEvent.agent_id == agent_id)
    if username:
        q = q.where(UsbEvent.username == username)
    if action:
        q = q.where(UsbEvent.action == action)

    result = await db.execute(q.limit(limit).offset(offset))
    return [
        {
            "id": e.id, "agent_id": e.agent_id, "username": e.username,
            "device_id": e.device_id, "device_name": e.device_name,
            "action": e.action, "drive_letter": e.drive_letter,
            "file_path": e.file_path, "size_gb": e.size_gb,
            "created_at": e.created_at.isoformat() if e.created_at else None,
        }
        for e in result.scalars().all()
    ]


@router.get(
    "/events/messenger",
    summary="Сообщения мессенджеров",
    dependencies=[Depends(require_officer)],
)
async def list_messenger_events(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    messenger: Optional[str] = None,
    flagged: Optional[bool] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
    user: CurrentUser = Depends(get_current_user),
):
    await write_audit(
        db, actor=user.username, actor_role=user.role,
        action="view_messenger", resource="messenger",
        details={"filter_username": username, "filter_messenger": messenger},
    )
    q = select(MessengerEvent).order_by(MessengerEvent.created_at.desc())
    if agent_id:
        q = q.where(MessengerEvent.agent_id == agent_id)
    if username:
        q = q.where(MessengerEvent.username == username)
    if messenger:
        q = q.where(MessengerEvent.messenger == messenger)
    if flagged is not None:
        q = q.where(MessengerEvent.flagged == flagged)

    result = await db.execute(q.limit(limit).offset(offset))
    return [
        {
            "id": e.id, "agent_id": e.agent_id, "username": e.username,
            "messenger": e.messenger, "direction": e.direction,
            "contact": e.contact, "content": e.content,
            "has_file": e.has_file, "file_name": e.file_name,
            "flagged": e.flagged, "matched_rules": e.matched_rules,
            "created_at": e.created_at.isoformat() if e.created_at else None,
        }
        for e in result.scalars().all()
    ]


@router.get("/events/app", summary="Активность приложений")
async def list_app_events(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    app_name: Optional[str] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(AppActivity).order_by(AppActivity.created_at.desc())
    if agent_id:
        q = q.where(AppActivity.agent_id == agent_id)
    if username:
        q = q.where(AppActivity.username == username)
    if app_name:
        q = q.where(AppActivity.app_name == app_name)

    result = await db.execute(q.limit(limit).offset(offset))
    return [
        {
            "id": e.id, "agent_id": e.agent_id, "username": e.username,
            "app_name": e.app_name, "window_title": e.window_title,
            "exe_path": e.exe_path, "duration_sec": e.duration_sec,
            "created_at": e.created_at.isoformat() if e.created_at else None,
        }
        for e in result.scalars().all()
    ]
