"""
Оператор консоли DLP (RBAC).

Отделён от app.models.user.User — тот является dimension-таблицей
мониторимых Windows-пользователей (Star Schema). Operator — это
учётная запись для входа в веб-консоль с ролью.

Роли (Приказ МЦРиАП РК №410 — разграничение доступа):
  • admin            — полный доступ + управление пользователями и конфигом
  • security_officer — Офицер ИБ: видит контент инцидентов, переписку, скриншоты
  • viewer           — только агрегаты и статистика, ПДн маскируются
"""
from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base

ROLE_ADMIN            = "admin"
ROLE_SECURITY_OFFICER = "security_officer"
ROLE_VIEWER           = "viewer"

ALL_ROLES = (ROLE_ADMIN, ROLE_SECURITY_OFFICER, ROLE_VIEWER)


class Operator(Base):
    __tablename__ = "operators"

    id            = Column(Integer, primary_key=True, index=True)
    username      = Column(String, unique=True, index=True, nullable=False)
    password_hash = Column(String, nullable=False)
    role          = Column(String, nullable=False, default=ROLE_VIEWER)
    full_name     = Column(String)
    is_active     = Column(Boolean, default=True)
    created_at    = Column(DateTime(timezone=True), server_default=func.now())
    last_login    = Column(DateTime(timezone=True))
