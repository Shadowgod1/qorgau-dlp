"""
CRUD для конфигураций агентов и назначения конфигураций на агентов.

GET  /configs                  — список всех конфигураций
POST /configs                  — создать конфигурацию
GET  /configs/{id}             — детали конфигурации
PUT  /configs/{id}             — обновить конфигурацию
DELETE /configs/{id}           — удалить конфигурацию
POST /configs/{id}/assign      — назначить конфигурацию агентам
GET  /configs/agent/{agent_id} — текущая конфигурация агента
"""
from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, delete, update
from pydantic import BaseModel
from typing import Optional, List
from datetime import datetime

from app.database import get_db
from app.models.agent_config import AgentConfig
from app.models.agent import Agent

router = APIRouter(prefix="/configs", tags=["Configs"])


# ── Схемы ─────────────────────────────────────────────────────────────────────

class ConfigBody(BaseModel):
    name: str
    description: Optional[str] = None

    mod_sessions:    bool = True
    mod_apps:        bool = True
    mod_web:         bool = True
    mod_messenger:   bool = True
    mod_clipboard:   bool = True
    mod_network:     bool = True
    mod_usb:         bool = True
    mod_printer:     bool = True
    mod_screenshots: bool = True

    screenshot_interval: int = 60
    screenshot_quality:  int = 60
    screenshot_max_w:    int = 1920

    net_alert_sensitive_ports: bool = True
    net_alert_unknown_ext:     bool = False
    net_block_tor:             bool = True

    web_alert_social:    bool = False
    web_alert_adult:     bool = True
    web_alert_gambling:  bool = True
    web_alert_streaming: bool = False
    web_alert_crypto:    bool = False

    usb_alert_on_connect:  bool = True
    usb_analyze_content:   bool = True
    usb_alert_executables: bool = True

    created_by: Optional[str] = None


class AssignBody(BaseModel):
    agent_ids: List[str]  # список agent_id строк


# ── Сериализация ──────────────────────────────────────────────────────────────

def _cfg(c: AgentConfig) -> dict:
    return {
        "id":          c.id,
        "name":        c.name,
        "description": c.description,

        "mod_sessions":    c.mod_sessions,
        "mod_apps":        c.mod_apps,
        "mod_web":         c.mod_web,
        "mod_messenger":   c.mod_messenger,
        "mod_clipboard":   c.mod_clipboard,
        "mod_network":     c.mod_network,
        "mod_usb":         c.mod_usb,
        "mod_printer":     c.mod_printer,
        "mod_screenshots": c.mod_screenshots,

        "screenshot_interval": c.screenshot_interval,
        "screenshot_quality":  c.screenshot_quality,
        "screenshot_max_w":    c.screenshot_max_w,

        "net_alert_sensitive_ports": c.net_alert_sensitive_ports,
        "net_alert_unknown_ext":     c.net_alert_unknown_ext,
        "net_block_tor":             c.net_block_tor,

        "web_alert_social":    c.web_alert_social,
        "web_alert_adult":     c.web_alert_adult,
        "web_alert_gambling":  c.web_alert_gambling,
        "web_alert_streaming": c.web_alert_streaming,
        "web_alert_crypto":    c.web_alert_crypto,

        "usb_alert_on_connect":  c.usb_alert_on_connect,
        "usb_analyze_content":   c.usb_analyze_content,
        "usb_alert_executables": c.usb_alert_executables,

        "created_by": c.created_by,
        "created_at": c.created_at.isoformat() if c.created_at else None,
        "updated_at": c.updated_at.isoformat() if c.updated_at else None,
    }


# ── Эндпоинты ─────────────────────────────────────────────────────────────────

@router.get("", summary="Список конфигураций")
async def list_configs(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(AgentConfig).order_by(AgentConfig.created_at.desc()))
    configs = result.scalars().all()

    # Для каждой конфигурации посчитать кол-во агентов
    out = []
    for c in configs:
        agents_res = await db.execute(
            select(Agent).where(Agent.config_id == c.id)
        )
        agents = agents_res.scalars().all()
        row = _cfg(c)
        row["agent_count"] = len(agents)
        row["agents"] = [{"agent_id": a.agent_id, "hostname": a.hostname} for a in agents]
        out.append(row)
    return out


@router.post("", status_code=201, summary="Создать конфигурацию")
async def create_config(body: ConfigBody, db: AsyncSession = Depends(get_db)):
    cfg = AgentConfig(**body.model_dump())
    db.add(cfg)
    await db.commit()
    await db.refresh(cfg)
    return _cfg(cfg)


@router.get("/{config_id}", summary="Детали конфигурации")
async def get_config(config_id: int, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(AgentConfig).where(AgentConfig.id == config_id))
    cfg = result.scalar_one_or_none()
    if not cfg:
        raise HTTPException(404, "Config not found")
    return _cfg(cfg)


@router.put("/{config_id}", summary="Обновить конфигурацию")
async def update_config(config_id: int, body: ConfigBody, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(AgentConfig).where(AgentConfig.id == config_id))
    cfg = result.scalar_one_or_none()
    if not cfg:
        raise HTTPException(404, "Config not found")
    for k, v in body.model_dump(exclude={"created_by"}).items():
        setattr(cfg, k, v)
    cfg.updated_at = datetime.utcnow()
    await db.commit()
    await db.refresh(cfg)
    return _cfg(cfg)


@router.delete("/{config_id}", summary="Удалить конфигурацию")
async def delete_config(config_id: int, db: AsyncSession = Depends(get_db)):
    await db.execute(
        update(Agent).where(Agent.config_id == config_id).values(config_id=None)
    )
    await db.execute(delete(AgentConfig).where(AgentConfig.id == config_id))
    await db.commit()
    return {"status": "deleted"}


@router.post("/{config_id}/assign", summary="Назначить конфигурацию агентам")
async def assign_config(config_id: int, body: AssignBody, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(AgentConfig).where(AgentConfig.id == config_id))
    if not result.scalar_one_or_none():
        raise HTTPException(404, "Config not found")

    await db.execute(
        update(Agent)
        .where(Agent.agent_id.in_(body.agent_ids))
        .values(config_id=config_id)
    )
    await db.commit()
    return {"status": "assigned", "count": len(body.agent_ids)}


@router.delete("/{config_id}/assign", summary="Снять конфигурацию с агентов")
async def unassign_config(config_id: int, body: AssignBody, db: AsyncSession = Depends(get_db)):
    await db.execute(
        update(Agent)
        .where(Agent.agent_id.in_(body.agent_ids))
        .values(config_id=None)
    )
    await db.commit()
    return {"status": "unassigned"}


@router.get("/agent/{agent_id}", summary="Конфигурация конкретного агента")
async def get_agent_config(agent_id: str, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Agent).where(Agent.agent_id == agent_id))
    agent = result.scalar_one_or_none()
    if not agent or not agent.config_id:
        # Возвращаем дефолтную конфигурацию
        return _cfg(AgentConfig())
    result2 = await db.execute(select(AgentConfig).where(AgentConfig.id == agent.config_id))
    cfg = result2.scalar_one_or_none()
    return _cfg(cfg) if cfg else _cfg(AgentConfig())
