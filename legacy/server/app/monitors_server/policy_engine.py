"""
Policy Engine — ядро DLP.

Алгоритм:
1. Загружает активные политики из БД (кэш 60 сек).
2. Для каждого входящего события прогоняет контент через regex-паттерны.
3. Возвращает список сработавших политик.
4. Вызывающий код создаёт инцидент и решает: block / alert / log.
"""
import re
import time
import logging
from typing import Optional
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select

from app.models.policy import Policy

log = logging.getLogger("policy_engine")

# ── Казахстанские и универсальные DLP-паттерны ────────────────────────────────
# Встроены как дефолтные правила при инициализации БД

KZ_DEFAULT_POLICIES: list[dict] = [
    # Персональные данные РК
    {
        "name": "ИИН Казахстана",
        "channel": "all",
        "pattern": r"\b\d{12}\b",
        "action": "alert",
        "severity": "high",
        "description": "Индивидуальный идентификационный номер (12 цифр)",
    },
    {
        "name": "БИН / ИИН организации",
        "channel": "all",
        "pattern": r"\b[0-9]{2}(0[1-9]|1[0-2])(0[1-9]|[12][0-9]|3[01])\d{6}\b",
        "action": "alert",
        "severity": "high",
        "description": "Бизнес-идентификационный номер РК",
    },
    {
        "name": "Паспорт РК",
        "channel": "all",
        "pattern": r"\b[A-ZА-Я]{1}\d{7}\b",
        "action": "alert",
        "severity": "high",
        "description": "Номер паспорта гражданина РК",
    },
    {
        "name": "IBAN Казахстан",
        "channel": "all",
        "pattern": r"\bKZ\d{2}[A-Z0-9]{3}\d{13}\b",
        "action": "alert",
        "severity": "critical",
        "description": "Банковский счёт IBAN (KZ)",
    },
    # Банковские данные
    {
        "name": "Номер банковской карты",
        "channel": "all",
        "pattern": r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b",
        "action": "block",
        "severity": "critical",
        "description": "Visa / MasterCard / Amex / Discover",
    },
    {
        "name": "CVV/CVC код карты",
        "channel": "all",
        "pattern": r"\b(?:CVV|CVC|CVV2|CVC2|CID)[\s:=]?\d{3,4}\b",
        "action": "block",
        "severity": "critical",
        "description": "Код безопасности банковской карты",
    },
    # Грифы секретности
    {
        "name": "Гриф секретности",
        "channel": "all",
        "pattern": r"\b(?:секретно|совершенно\s+секретно|конфиденциально|коммерческая\s+тайна|для\s+служебного\s+пользования|ДСП|КТ)\b",
        "action": "alert",
        "severity": "high",
        "description": "Документ содержит гриф конфиденциальности",
    },
    # Сетевые учётные данные
    {
        "name": "Пароль в открытом виде",
        "channel": "all",
        "pattern": r"\b(?:password|passwd|пароль|pwd)\s*[=:]\s*\S{6,}",
        "action": "alert",
        "severity": "high",
        "description": "Пароль передаётся в открытом виде",
    },
    {
        "name": "API ключ / токен",
        "channel": "all",
        "pattern": r"\b(?:api[_-]?key|token|secret|bearer)\s*[=:]\s*[A-Za-z0-9+/]{20,}",
        "action": "alert",
        "severity": "high",
        "description": "Потенциальная утечка API-ключа или токена",
    },
    # Контактные данные
    {
        "name": "Email-адрес",
        "channel": "messenger",
        "pattern": r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b",
        "action": "log",
        "severity": "low",
        "description": "Передача email-адреса через мессенджер",
    },
    {
        "name": "Телефон РК",
        "channel": "all",
        "pattern": r"\b(?:\+7|8)[\s\-]?\(?\d{3}\)?[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}\b",
        "action": "log",
        "severity": "low",
        "description": "Номер телефона формата РК/РФ",
    },
    # Файловые операции
    {
        "name": "Архив с потенциальными данными на USB",
        "channel": "usb",
        "pattern": r"\.(zip|rar|7z|tar|gz|bz2)$",
        "action": "alert",
        "severity": "medium",
        "description": "Копирование архива на съёмный носитель",
    },
    {
        "name": "Документ на USB",
        "channel": "usb",
        "pattern": r"\.(docx?|xlsx?|pdf|pptx?|odt|ods)$",
        "action": "alert",
        "severity": "medium",
        "description": "Копирование офисного документа на USB",
    },
]


class PolicyCache:
    """Кэш политик с TTL 60 секунд."""

    TTL = 60

    def __init__(self):
        self._policies: list[Policy] = []
        self._loaded_at: float = 0.0
        # Скомпилированные regex: {policy.id: re.Pattern}
        self._compiled: dict[int, re.Pattern] = {}

    def _expired(self) -> bool:
        return time.monotonic() - self._loaded_at > self.TTL

    async def get(self, db: AsyncSession) -> list[Policy]:
        if self._expired():
            result = await db.execute(
                select(Policy).where(Policy.enabled == True)  # noqa
            )
            self._policies = list(result.scalars().all())
            self._compiled = {}
            for p in self._policies:
                try:
                    self._compiled[p.id] = re.compile(
                        p.pattern, re.IGNORECASE | re.UNICODE
                    )
                except re.error as e:
                    log.warning("Bad regex in policy %d (%s): %s", p.id, p.name, e)
            self._loaded_at = time.monotonic()
            log.debug("Policy cache refreshed: %d policies", len(self._policies))
        return self._policies

    def invalidate(self):
        self._loaded_at = 0.0


# Singleton
_cache = PolicyCache()


def invalidate_cache():
    _cache.invalidate()


async def match_policies(
    content: str,
    channel: str,
    db: AsyncSession,
) -> list[dict]:
    """
    Проверяет content против всех активных политик канала.

    Возвращает список сработавших политик:
    [{"id": int, "name": str, "action": str, "severity": str}, ...]
    """
    if not content or not content.strip():
        return []

    policies = await _cache.get(db)
    matches: list[dict] = []

    for policy in policies:
        # Канал "all" подходит под любой канал
        if policy.channel not in ("all", channel):
            continue

        pattern = _cache._compiled.get(policy.id)
        if pattern is None:
            continue

        try:
            if pattern.search(content):
                matches.append({
                    "id": policy.id,
                    "name": policy.name,
                    "action": policy.action,   # block / alert / log
                    "severity": policy.severity,
                })
        except Exception as e:
            log.debug("Pattern match error policy %d: %s", policy.id, e)

    return matches


def highest_severity(matches: list[dict]) -> str:
    """Возвращает наивысший уровень severity из списка совпадений."""
    order = {"critical": 4, "high": 3, "medium": 2, "low": 1}
    if not matches:
        return "low"
    return max(matches, key=lambda m: order.get(m["severity"], 0))["severity"]


def requires_block(matches: list[dict]) -> bool:
    return any(m["action"] == "block" for m in matches)


async def seed_default_policies(db: AsyncSession) -> None:
    """Добавляет дефолтные KZ-политики если таблица пустая."""
    from app.models.policy import Policy as PolicyModel
    result = await db.execute(select(PolicyModel).limit(1))
    if result.scalar():
        return  # Уже есть политики

    for p in KZ_DEFAULT_POLICIES:
        db.add(PolicyModel(
            name=p["name"],
            channel=p["channel"],
            pattern=p["pattern"],
            action=p["action"],
            severity=p["severity"],
            enabled=True,
            created_by="system",
        ))
    await db.commit()
    log.info("Seeded %d default KZ policies", len(KZ_DEFAULT_POLICIES))
    invalidate_cache()
