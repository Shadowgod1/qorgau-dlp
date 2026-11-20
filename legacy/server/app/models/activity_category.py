"""
Категория продуктивности приложения/сайта.

Сопоставляет имя приложения (или подстроку) с классом продуктивности:
  • productive    — рабочие инструменты (офис, IDE, 1С, CRM…)
  • unproductive  — развлечения (игры, соцсети, стриминг…)
  • neutral       — прочее
"""
from sqlalchemy import Column, Integer, String, DateTime
from sqlalchemy.sql import func
from app.database import Base

PRODUCTIVE   = "productive"
UNPRODUCTIVE = "unproductive"
NEUTRAL      = "neutral"
CLASSES = (PRODUCTIVE, UNPRODUCTIVE, NEUTRAL)


class ActivityCategory(Base):
    __tablename__ = "activity_categories"

    id           = Column(Integer, primary_key=True, index=True)
    pattern      = Column(String, index=True)   # подстрока имени приложения (lowercase)
    productivity = Column(String)               # productive / unproductive / neutral
    label        = Column(String)               # человекочитаемое название
    created_at   = Column(DateTime(timezone=True), server_default=func.now())
