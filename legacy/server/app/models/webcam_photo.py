from sqlalchemy import Column, Integer, String, DateTime
from datetime import datetime, timezone
from app.database import Base


class WebcamPhoto(Base):
    __tablename__ = "webcam_photos"

    id          = Column(Integer, primary_key=True)
    agent_id    = Column(String(64), nullable=False, index=True)
    hostname    = Column(String(255), nullable=False)
    username    = Column(String(255), nullable=False)
    file_path   = Column(String(512), nullable=False)  # путь к JPEG файлу на сервере
    file_size   = Column(Integer, nullable=False)       # размер в байтах
    created_at  = Column(DateTime, default=lambda: datetime.now(timezone.utc))
