from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base

class WebActivity(Base):
    __tablename__ = "web_activity"

    id         = Column(Integer, primary_key=True, index=True)
    agent_id   = Column(String, index=True)
    username   = Column(String, index=True)
    url        = Column(String)
    domain     = Column(String, index=True)
    category   = Column(String)    # work / social / games / news / adult / crypto / other
    blocked    = Column(Boolean, default=False)
    browser    = Column(String)    # chrome / firefox / edge
    created_at = Column(DateTime(timezone=True), server_default=func.now(), index=True)
