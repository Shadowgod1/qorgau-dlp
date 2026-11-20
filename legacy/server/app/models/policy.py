from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base

class Policy(Base):
    __tablename__ = "policies"

    id         = Column(Integer, primary_key=True, index=True)
    name       = Column(String, unique=True)
    channel    = Column(String)   # usb / email / web / clipboard / messenger / file
    action     = Column(String)   # block / alert / log
    severity   = Column(String)
    pattern    = Column(String)   # regex или keyword
    enabled    = Column(Boolean, default=True)
    created_by = Column(String)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
