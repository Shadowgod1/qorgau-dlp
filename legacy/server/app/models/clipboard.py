from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base


class ClipboardEvent(Base):
    __tablename__ = "clipboard_events"

    id            = Column(Integer, primary_key=True, index=True)
    agent_id      = Column(String, index=True)
    username      = Column(String, index=True)
    content       = Column(String)          # до 2000 символов
    source_app    = Column(String)          # имя процесса-источника
    flagged       = Column(Boolean, default=False)
    matched_rules = Column(String)
    created_at    = Column(DateTime(timezone=True), server_default=func.now(), index=True)
