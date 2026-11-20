from sqlalchemy import Column, Integer, String, DateTime
from datetime import datetime, timezone
from app.database import Base


class AudioRecording(Base):
    __tablename__ = "audio_recordings"

    id          = Column(Integer, primary_key=True)
    agent_id    = Column(String(64), nullable=False, index=True)
    hostname    = Column(String(255), nullable=False)
    username    = Column(String(255), nullable=False)
    file_path   = Column(String(512), nullable=False)  # путь к WAV файлу на сервере
    file_size   = Column(Integer, nullable=False)       # размер в байтах
    duration_sec = Column(Integer, default=0)           # длительность записи
    sample_rate = Column(Integer, default=16000)
    created_at  = Column(DateTime, default=lambda: datetime.now(timezone.utc))
