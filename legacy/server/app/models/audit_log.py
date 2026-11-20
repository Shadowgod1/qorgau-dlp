"""
Неизменяемый audit-журнал (append-only, hash-chain).

Каждая запись содержит хэш предыдущей записи (prev_hash) и собственный
хэш (entry_hash = SHA256(prev_hash | полезная нагрузка)). Любое удаление
или изменение записи разрывает цепочку — это детектируется проверкой
целостности (GET /api/v1/audit/verify).

Соответствие: Приказ МЦРиАП РК №410 — журнал доступа к персональным
данным; требование доказательной базы (защищённый архив событий).

ВАЖНО (production): для полной защиты от удаления администратором БД
таблица должна вестись под отдельной учётной записью Postgres без прав
DELETE/UPDATE (только INSERT/SELECT), либо выгружаться в WORM-хранилище.
Hash-chain гарантирует *детектируемость* нарушения целостности.
"""
from sqlalchemy import Column, Integer, String, DateTime, JSON
from sqlalchemy.sql import func
from app.database import Base

GENESIS_HASH = "0" * 64   # хэш «нулевой» записи для первой строки цепочки


class AuditLog(Base):
    __tablename__ = "audit_log"

    id         = Column(Integer, primary_key=True, index=True)
    ts         = Column(DateTime(timezone=True), index=True)
    actor      = Column(String, index=True)   # username оператора или "system"
    actor_role = Column(String)
    action     = Column(String, index=True)   # view_incident / export / incident_created / ...
    resource   = Column(String)               # incident:123 / messenger / screenshot:5 / ...
    details    = Column(JSON)
    prev_hash  = Column(String, nullable=False)
    entry_hash = Column(String, nullable=False, index=True)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
