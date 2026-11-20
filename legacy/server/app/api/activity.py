"""
API контроля активности и учёта рабочего времени.

  GET  /activity/worktime    — рабочее время по пользователю/дню (актив/простой/продуктивность)
  GET  /activity/summary     — сводка по пользователям за период
  GET  /activity/categories  — правила продуктивности
  POST/PATCH/DELETE /activity/categories — управление (admin)
"""
from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy import select, text
from sqlalchemy.ext.asyncio import AsyncSession
from pydantic import BaseModel
from typing import Optional
from datetime import datetime

from app.database import get_db
from app.api.auth import require_admin
from app.models.activity_category import ActivityCategory, CLASSES
from app.engine.productivity import invalidate_cache

router = APIRouter(prefix="/activity", tags=["Activity"])


def _pct(productive, active) -> float:
    return round(100.0 * (productive or 0) / active, 1) if active else 0.0


@router.get("/worktime", summary="Рабочее время по пользователю и дню")
async def worktime(
    username: Optional[str] = None,
    date_from: Optional[datetime] = None,
    date_to: Optional[datetime] = None,
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    filters = ["duration_sec IS NOT NULL"]
    params: dict = {}
    if username:
        filters.append("username = :username")
        params["username"] = username
    if date_from:
        filters.append("created_at >= :date_from")
        params["date_from"] = date_from
    else:
        filters.append("created_at >= NOW() - make_interval(days => :days)")
        params["days"] = days
    if date_to:
        filters.append("created_at <= :date_to")
        params["date_to"] = date_to

    where = " AND ".join(filters)
    sql = text(f"""
        SELECT
            username,
            DATE_TRUNC('day', created_at)::date AS day,
            MIN(created_at) AS first_seen,
            MAX(created_at) AS last_seen,
            COALESCE(SUM(duration_sec), 0) AS total_sec,
            COALESCE(SUM(active_sec), 0)   AS active_sec,
            COALESCE(SUM(idle_sec), 0)     AS idle_sec,
            COALESCE(SUM(active_sec) FILTER (WHERE productivity = 'productive'), 0)   AS productive_sec,
            COALESCE(SUM(active_sec) FILTER (WHERE productivity = 'unproductive'), 0) AS unproductive_sec,
            COALESCE(SUM(active_sec) FILTER (WHERE productivity = 'neutral'), 0)      AS neutral_sec
        FROM app_activity
        WHERE {where}
        GROUP BY username, DATE_TRUNC('day', created_at)
        ORDER BY day DESC, active_sec DESC
    """)
    result = await db.execute(sql, params)
    rows = []
    for r in result.mappings().all():
        d = dict(r)
        d["first_seen"] = d["first_seen"].isoformat() if d["first_seen"] else None
        d["last_seen"] = d["last_seen"].isoformat() if d["last_seen"] else None
        d["day"] = d["day"].isoformat() if d["day"] else None
        d["productivity_pct"] = _pct(d["productive_sec"], d["active_sec"])
        rows.append(d)
    return rows


@router.get("/summary", summary="Сводка активности по пользователям")
async def summary(
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    sql = text("""
        SELECT
            username,
            COUNT(DISTINCT DATE_TRUNC('day', created_at)) AS work_days,
            COALESCE(SUM(active_sec), 0) AS active_sec,
            COALESCE(SUM(idle_sec), 0)   AS idle_sec,
            COALESCE(SUM(active_sec) FILTER (WHERE productivity = 'productive'), 0)   AS productive_sec,
            COALESCE(SUM(active_sec) FILTER (WHERE productivity = 'unproductive'), 0) AS unproductive_sec
        FROM app_activity
        WHERE created_at >= NOW() - make_interval(days => :days)
        GROUP BY username
        ORDER BY active_sec DESC
    """)
    result = await db.execute(sql, {"days": days})
    rows = []
    for r in result.mappings().all():
        d = dict(r)
        d["productivity_pct"] = _pct(d["productive_sec"], d["active_sec"])
        rows.append(d)
    return rows


# ── Управление категориями продуктивности ─────────────────────────────────────

class CategoryIn(BaseModel):
    pattern: str
    productivity: str
    label: Optional[str] = None


def _serialize_cat(c: ActivityCategory) -> dict:
    return {"id": c.id, "pattern": c.pattern, "productivity": c.productivity, "label": c.label}


@router.get("/categories", summary="Правила продуктивности")
async def list_categories(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(ActivityCategory).order_by(ActivityCategory.productivity, ActivityCategory.pattern))
    return [_serialize_cat(c) for c in result.scalars().all()]


@router.post("/categories", summary="Добавить правило", dependencies=[Depends(require_admin)])
async def create_category(body: CategoryIn, db: AsyncSession = Depends(get_db)):
    if body.productivity not in CLASSES:
        raise HTTPException(status_code=400, detail=f"Недопустимый класс: {body.productivity}")
    cat = ActivityCategory(pattern=body.pattern.lower(), productivity=body.productivity, label=body.label)
    db.add(cat)
    await db.commit()
    await db.refresh(cat)
    invalidate_cache()
    return _serialize_cat(cat)


@router.patch("/categories/{cat_id}", summary="Изменить правило", dependencies=[Depends(require_admin)])
async def update_category(cat_id: int, body: CategoryIn, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(ActivityCategory).where(ActivityCategory.id == cat_id))
    cat = result.scalar_one_or_none()
    if not cat:
        raise HTTPException(status_code=404, detail="Правило не найдено")
    if body.productivity not in CLASSES:
        raise HTTPException(status_code=400, detail=f"Недопустимый класс: {body.productivity}")
    cat.pattern = body.pattern.lower()
    cat.productivity = body.productivity
    cat.label = body.label
    await db.commit()
    invalidate_cache()
    return _serialize_cat(cat)


@router.delete("/categories/{cat_id}", summary="Удалить правило", dependencies=[Depends(require_admin)])
async def delete_category(cat_id: int, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(ActivityCategory).where(ActivityCategory.id == cat_id))
    cat = result.scalar_one_or_none()
    if not cat:
        raise HTTPException(status_code=404, detail="Правило не найдено")
    await db.delete(cat)
    await db.commit()
    invalidate_cache()
    return {"status": "deleted"}
