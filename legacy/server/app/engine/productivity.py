"""
Сервис категоризации продуктивности приложений.

Кэширует правила из БД (TTL 60с). Классифицирует app_name по подстроке.
При отсутствии совпадения — neutral.
"""
import time
import logging
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.models.activity_category import (
    ActivityCategory, PRODUCTIVE, UNPRODUCTIVE, NEUTRAL,
)

log = logging.getLogger("productivity")

# Дефолтные правила (подстрока имени процесса → класс)
DEFAULT_RULES: list[dict] = [
    # Продуктивные — рабочие инструменты
    {"pattern": "winword",     "productivity": PRODUCTIVE,   "label": "MS Word"},
    {"pattern": "excel",       "productivity": PRODUCTIVE,   "label": "MS Excel"},
    {"pattern": "powerpnt",    "productivity": PRODUCTIVE,   "label": "MS PowerPoint"},
    {"pattern": "outlook",     "productivity": PRODUCTIVE,   "label": "Outlook"},
    {"pattern": "onenote",     "productivity": PRODUCTIVE,   "label": "OneNote"},
    {"pattern": "code",        "productivity": PRODUCTIVE,   "label": "VS Code"},
    {"pattern": "devenv",      "productivity": PRODUCTIVE,   "label": "Visual Studio"},
    {"pattern": "pycharm",     "productivity": PRODUCTIVE,   "label": "PyCharm"},
    {"pattern": "1cv8",        "productivity": PRODUCTIVE,   "label": "1С:Предприятие"},
    {"pattern": "acad",        "productivity": PRODUCTIVE,   "label": "AutoCAD"},
    {"pattern": "photoshop",   "productivity": PRODUCTIVE,   "label": "Photoshop"},
    {"pattern": "teams",       "productivity": PRODUCTIVE,   "label": "MS Teams"},
    {"pattern": "putty",       "productivity": PRODUCTIVE,   "label": "PuTTY"},
    {"pattern": "notepad",     "productivity": PRODUCTIVE,   "label": "Notepad"},
    # Непродуктивные — развлечения
    {"pattern": "steam",       "productivity": UNPRODUCTIVE, "label": "Steam"},
    {"pattern": "game",        "productivity": UNPRODUCTIVE, "label": "Игры"},
    {"pattern": "telegram",    "productivity": UNPRODUCTIVE, "label": "Telegram"},
    {"pattern": "whatsapp",    "productivity": UNPRODUCTIVE, "label": "WhatsApp"},
    {"pattern": "vlc",         "productivity": UNPRODUCTIVE, "label": "VLC"},
    {"pattern": "spotify",     "productivity": UNPRODUCTIVE, "label": "Spotify"},
    {"pattern": "discord",     "productivity": UNPRODUCTIVE, "label": "Discord"},
    {"pattern": "epicgames",   "productivity": UNPRODUCTIVE, "label": "Epic Games"},
]


class _Cache:
    TTL = 60

    def __init__(self):
        self._rules: list[tuple[str, str]] = []   # (pattern, productivity)
        self._loaded_at = 0.0

    async def get(self, db: AsyncSession) -> list[tuple[str, str]]:
        if time.monotonic() - self._loaded_at > self.TTL:
            result = await db.execute(select(ActivityCategory))
            self._rules = [
                (c.pattern.lower(), c.productivity)
                for c in result.scalars().all() if c.pattern
            ]
            self._loaded_at = time.monotonic()
        return self._rules

    def invalidate(self):
        self._loaded_at = 0.0


_cache = _Cache()


def invalidate_cache():
    _cache.invalidate()


async def classify(app_name: str, db: AsyncSession) -> str:
    """Возвращает класс продуктивности для приложения."""
    if not app_name:
        return NEUTRAL
    name = app_name.lower()
    for pattern, productivity in await _cache.get(db):
        if pattern in name:
            return productivity
    return NEUTRAL


async def seed_defaults(db: AsyncSession) -> None:
    """Засеивает дефолтные правила, если таблица пуста."""
    result = await db.execute(select(ActivityCategory).limit(1))
    if result.scalar():
        return
    for r in DEFAULT_RULES:
        db.add(ActivityCategory(
            pattern=r["pattern"], productivity=r["productivity"], label=r["label"],
        ))
    await db.commit()
    invalidate_cache()
    log.info("Seeded %d default productivity rules", len(DEFAULT_RULES))
