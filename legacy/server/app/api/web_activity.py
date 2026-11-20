from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from typing import Optional

from app.database import get_db
from app.models.web import WebActivity

router = APIRouter()


@router.get("/web-activity", summary="Веб-активность сотрудников")
async def list_web_activity(
    agent_id: Optional[str] = None,
    username: Optional[str] = None,
    domain: Optional[str] = None,
    category: Optional[str] = None,
    blocked: Optional[bool] = None,
    limit: int = Query(50, le=500),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    q = select(WebActivity).order_by(WebActivity.created_at.desc())
    if agent_id:
        q = q.where(WebActivity.agent_id == agent_id)
    if username:
        q = q.where(WebActivity.username == username)
    if domain:
        q = q.where(WebActivity.domain.contains(domain))
    if category:
        q = q.where(WebActivity.category == category)
    if blocked is not None:
        q = q.where(WebActivity.blocked == blocked)

    result = await db.execute(q.limit(limit).offset(offset))
    return [
        {
            "id": w.id, "agent_id": w.agent_id, "username": w.username,
            "url": w.url, "domain": w.domain, "category": w.category,
            "blocked": w.blocked, "browser": w.browser,
            "created_at": w.created_at.isoformat() if w.created_at else None,
        }
        for w in result.scalars().all()
    ]


@router.get("/web-activity/stats", summary="Топ доменов и разбивка по категориям")
async def web_activity_stats(db: AsyncSession = Depends(get_db)):
    top_domains = await db.execute(
        select(WebActivity.domain, func.count().label("visits"))
        .group_by(WebActivity.domain)
        .order_by(func.count().desc())
        .limit(20)
    )
    by_category = await db.execute(
        select(WebActivity.category, func.count()).group_by(WebActivity.category)
    )
    blocked_count = await db.scalar(
        select(func.count()).where(WebActivity.blocked == True)
    )
    return {
        "top_domains": [{"domain": r[0], "visits": r[1]} for r in top_domains.all()],
        "by_category": dict(by_category.all()),
        "blocked_attempts": blocked_count,
    }
