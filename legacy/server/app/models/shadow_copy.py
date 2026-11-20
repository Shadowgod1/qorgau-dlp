"""
Теневая копия перехваченного файла (доказательная база расследования).

Агент загружает полную копию документа/файла, скопированного на USB
(или иным контролируемым каналом), когда файл представляет интерес.
Копия хранится на сервере и связывается с инцидентом по sha256.
"""
from sqlalchemy import Column, Integer, String, DateTime, BigInteger
from sqlalchemy.sql import func
from app.database import Base


class ShadowCopy(Base):
    __tablename__ = "shadow_copies"

    id            = Column(Integer, primary_key=True, index=True)
    agent_id      = Column(String, index=True)
    hostname      = Column(String)
    username      = Column(String, index=True)
    channel       = Column(String, default="usb")   # usb / print / ...
    original_path = Column(String)
    file_name     = Column(String)
    sha256        = Column(String, index=True)
    size_bytes    = Column(BigInteger)
    stored_path   = Column(String)                  # путь к копии на сервере
    incident_id   = Column(Integer, index=True, nullable=True)
    created_at    = Column(DateTime(timezone=True), server_default=func.now(), index=True)
