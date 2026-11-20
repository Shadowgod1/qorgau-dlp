from sqlalchemy import Column, Integer, String, DateTime, JSON, Enum as SAEnum
from sqlalchemy.sql import func
from app.database import Base
import enum

class Severity(str, enum.Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"

class IncidentStatus(str, enum.Enum):
    OPEN = "open"
    REVIEWED = "reviewed"
    RESOLVED = "resolved"
    FALSE_POSITIVE = "false_positive"

class Incident(Base):
    __tablename__ = "incidents"

    id           = Column(Integer, primary_key=True, index=True)
    agent_id     = Column(String, index=True)
    hostname     = Column(String)
    username     = Column(String, index=True)
    channel      = Column(String)          # usb / email / clipboard / messenger / browser / file
    severity     = Column(SAEnum(Severity), index=True)
    status       = Column(SAEnum(IncidentStatus), default=IncidentStatus.OPEN)
    matched_rules = Column(JSON)
    snippet      = Column(String)          # фрагмент (не весь контент)
    meta         = Column(JSON)
    created_at   = Column(DateTime(timezone=True), server_default=func.now(), index=True)
    updated_at   = Column(DateTime(timezone=True), onupdate=func.now())
