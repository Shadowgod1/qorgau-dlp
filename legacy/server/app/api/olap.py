"""
OLAP / Star Schema — аналитические срезы.

Реализовано через PostgreSQL-представления (VIEW) поверх существующих таблиц.
Не ломает текущую схему — только добавляет витрины данных.

Endpoints:
  GET /api/v1/olap/facts          — главная fact-таблица (срез по измерениям)
  GET /api/v1/olap/dim/agents     — измерение Агент
  GET /api/v1/olap/dim/users      — измерение Пользователь
  GET /api/v1/olap/dim/apps       — измерение Приложение
  GET /api/v1/olap/dim/sites      — измерение Сайт
  GET /api/v1/olap/cube/activity  — OLAP-куб активности (агент × день × тип)
  GET /api/v1/olap/cube/risk      — риск-скор по пользователям
"""
from fastapi import APIRouter, Depends, Query
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import text
from typing import Optional
from datetime import datetime

from app.database import get_db

router = APIRouter(prefix="/olap", tags=["OLAP"])


# ── Главная Fact-таблица (UNION всех каналов) ─────────────────────────────────

FACT_SQL = """
SELECT
    'incident'          AS event_type,
    i.id                AS event_id,
    i.agent_id,
    a.hostname,
    i.username,
    i.channel           AS subtype,
    i.severity::text    AS severity,
    i.created_at        AS ts,
    length(i.snippet)   AS metric_int,
    NULL::float         AS metric_float
FROM incidents i
LEFT JOIN agents a ON a.agent_id = i.agent_id

UNION ALL

SELECT 'usb', u.id, u.agent_id, a.hostname, u.username,
       u.action, NULL, u.created_at,
       NULL, u.size_gb
FROM usb_events u
LEFT JOIN agents a ON a.agent_id = u.agent_id

UNION ALL

SELECT 'messenger', m.id, m.agent_id, a.hostname, m.username,
       m.messenger, NULL, m.created_at,
       length(m.content), NULL
FROM messenger_events m
LEFT JOIN agents a ON a.agent_id = m.agent_id

UNION ALL

SELECT 'app', ap.id, ap.agent_id, a.hostname, ap.username,
       ap.app_name, NULL, ap.created_at,
       ap.duration_sec, NULL
FROM app_activity ap
LEFT JOIN agents a ON a.agent_id = ap.agent_id

UNION ALL

SELECT 'web', w.id, w.agent_id, a.hostname, w.username,
       w.category, NULL, w.created_at,
       NULL, NULL
FROM web_activity w
LEFT JOIN agents a ON a.agent_id = w.agent_id

UNION ALL

SELECT 'clipboard', c.id, c.agent_id, a.hostname, c.username,
       c.source_app, NULL, c.created_at,
       length(c.content), NULL
FROM clipboard_events c
LEFT JOIN agents a ON a.agent_id = c.agent_id

UNION ALL

SELECT 'network', n.id, n.agent_id, a.hostname, n.username,
       n.process_name, NULL, n.created_at,
       n.remote_port, NULL
FROM network_connections n
LEFT JOIN agents a ON a.agent_id = n.agent_id

UNION ALL

SELECT 'print', p.id, p.agent_id, a.hostname, p.username,
       p.printer_name, NULL, p.created_at,
       p.pages, p.size_bytes
FROM print_jobs p
LEFT JOIN agents a ON a.agent_id = p.agent_id

UNION ALL

SELECT 'screenshot', s.id, s.agent_id, a.hostname, s.username,
       'screenshot', NULL, s.created_at,
       s.file_size, NULL
FROM screenshots s
LEFT JOIN agents a ON a.agent_id = s.agent_id
"""


@router.get("/facts", summary="Fact-таблица (Star Schema срез)")
async def get_facts(
    event_type: Optional[str]     = None,
    agent_id:   Optional[str]     = None,
    username:   Optional[str]     = None,
    severity:   Optional[str]     = None,
    date_from:  Optional[datetime] = None,
    date_to:    Optional[datetime] = None,
    limit: int  = Query(200, le=1000),
    offset: int = Query(0, ge=0),
    db: AsyncSession = Depends(get_db),
):
    filters = ["1=1"]
    params: dict = {}

    if event_type:
        filters.append("event_type = :event_type")
        params["event_type"] = event_type
    if agent_id:
        filters.append("agent_id = :agent_id")
        params["agent_id"] = agent_id
    if username:
        filters.append("username = :username")
        params["username"] = username
    if severity:
        filters.append("severity = :severity")
        params["severity"] = severity
    if date_from:
        filters.append("ts >= :date_from")
        params["date_from"] = date_from
    if date_to:
        filters.append("ts <= :date_to")
        params["date_to"] = date_to

    where = " AND ".join(filters)
    sql = text(f"""
        WITH facts AS ({FACT_SQL})
        SELECT * FROM facts
        WHERE {where}
        ORDER BY ts DESC
        LIMIT :limit OFFSET :offset
    """)
    params["limit"] = limit
    params["offset"] = offset

    result = await db.execute(sql, params)
    rows = result.mappings().all()
    return {"items": [dict(r) for r in rows], "count": len(rows)}


# ── Измерения (Dimensions) ────────────────────────────────────────────────────

@router.get("/dim/agents", summary="Измерение: Агенты")
async def dim_agents(db: AsyncSession = Depends(get_db)):
    sql = text("""
        SELECT
            a.agent_id, a.hostname, a.username, a.ip_address,
            a.os_version, a.agent_version, a.is_online, a.last_seen,
            COUNT(DISTINCT i.id)  AS total_incidents,
            COUNT(DISTINCT s.id)  AS total_screenshots,
            COUNT(DISTINCT u.id)  AS total_usb_events
        FROM agents a
        LEFT JOIN incidents   i ON i.agent_id = a.agent_id
        LEFT JOIN screenshots s ON s.agent_id = a.agent_id
        LEFT JOIN usb_events  u ON u.agent_id = a.agent_id
        GROUP BY a.agent_id, a.hostname, a.username, a.ip_address,
                 a.os_version, a.agent_version, a.is_online, a.last_seen
        ORDER BY total_incidents DESC
    """)
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]


@router.get("/dim/users", summary="Измерение: Пользователи с риск-скором")
async def dim_users(db: AsyncSession = Depends(get_db)):
    sql = text("""
        SELECT
            username,
            COUNT(DISTINCT agent_id)                                 AS agents_count,
            COUNT(*) FILTER (WHERE severity = 'critical')            AS critical_incidents,
            COUNT(*) FILTER (WHERE severity = 'high')                AS high_incidents,
            COUNT(*) FILTER (WHERE severity = 'medium')              AS medium_incidents,
            COUNT(*) FILTER (WHERE severity = 'low')                 AS low_incidents,
            COUNT(*) FILTER (WHERE status = 'open')                  AS open_incidents,
            -- Риск-скор: критично*40 + high*15 + medium*5 + low*1
            (COUNT(*) FILTER (WHERE severity='critical') * 40 +
             COUNT(*) FILTER (WHERE severity='high')     * 15 +
             COUNT(*) FILTER (WHERE severity='medium')   *  5 +
             COUNT(*) FILTER (WHERE severity='low')      *  1) AS risk_score,
            MAX(created_at) AS last_incident_at
        FROM incidents
        GROUP BY username
        ORDER BY risk_score DESC
    """)
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]


@router.get("/dim/apps", summary="Измерение: Приложения")
async def dim_apps(db: AsyncSession = Depends(get_db)):
    sql = text("""
        SELECT
            app_name,
            COUNT(DISTINCT agent_id)       AS agents_using,
            COUNT(DISTINCT username)        AS users_count,
            COUNT(*)                        AS total_events,
            SUM(duration_sec)              AS total_duration_sec,
            ROUND(AVG(duration_sec), 1)    AS avg_duration_sec,
            MAX(created_at)                AS last_seen_at
        FROM app_activity
        WHERE app_name IS NOT NULL AND app_name != ''
        GROUP BY app_name
        ORDER BY total_duration_sec DESC NULLS LAST
        LIMIT 100
    """)
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]


@router.get("/dim/sites", summary="Измерение: Сайты")
async def dim_sites(db: AsyncSession = Depends(get_db)):
    sql = text("""
        SELECT
            domain,
            category,
            COUNT(DISTINCT agent_id)    AS agents_count,
            COUNT(DISTINCT username)     AS users_count,
            COUNT(*)                     AS visit_count,
            SUM(CASE WHEN blocked THEN 1 ELSE 0 END) AS blocked_count,
            MAX(created_at)              AS last_visit_at
        FROM web_activity
        WHERE domain IS NOT NULL AND domain != ''
        GROUP BY domain, category
        ORDER BY visit_count DESC
        LIMIT 200
    """)
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]


# ── OLAP-кубы ─────────────────────────────────────────────────────────────────

@router.get("/cube/activity", summary="OLAP-куб: активность агент × день × тип")
async def cube_activity(
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    sql = text(f"""
        WITH facts AS ({FACT_SQL})
        SELECT
            agent_id,
            hostname,
            DATE_TRUNC('day', ts)::date AS day,
            event_type,
            COUNT(*)                     AS event_count
        FROM facts
        WHERE ts >= NOW() - INTERVAL ':days days'
        GROUP BY agent_id, hostname, DATE_TRUNC('day', ts), event_type
        ORDER BY day DESC, agent_id, event_type
    """.replace(":days", str(days)))
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]


@router.get("/cube/risk", summary="OLAP-куб: риск-скор пользователей по дням")
async def cube_risk(
    days: int = Query(30, le=365),
    db: AsyncSession = Depends(get_db),
):
    sql = text(f"""
        SELECT
            username,
            DATE_TRUNC('day', created_at)::date AS day,
            SUM(CASE severity WHEN 'critical' THEN 40
                               WHEN 'high'     THEN 15
                               WHEN 'medium'   THEN  5
                               ELSE                  1 END) AS risk_score,
            COUNT(*)                                          AS incident_count,
            array_agg(DISTINCT channel)                       AS channels
        FROM incidents
        WHERE created_at >= NOW() - INTERVAL '{days} days'
        GROUP BY username, DATE_TRUNC('day', created_at)
        ORDER BY day DESC, risk_score DESC
    """)
    result = await db.execute(sql)
    return [dict(r) for r in result.mappings().all()]
