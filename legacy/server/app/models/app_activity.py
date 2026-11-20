from sqlalchemy import Column, Integer, String, DateTime
from sqlalchemy.sql import func
from app.database import Base

class AppActivity(Base):
    __tablename__ = "app_activity"

    id           = Column(Integer, primary_key=True, index=True)
    agent_id     = Column(String, index=True)
    username     = Column(String, index=True)
    app_name     = Column(String, index=True)
    window_title = Column(String)
    exe_path     = Column(String)
    duration_sec = Column(Integer)
    active_sec   = Column(Integer, default=0)   # из duration — с реальным вводом
    idle_sec     = Column(Integer, default=0)   # из duration — простой
    productivity = Column(String, index=True, default="neutral")  # productive/unproductive/neutral
    created_at   = Column(DateTime(timezone=True), server_default=func.now(), index=True)
