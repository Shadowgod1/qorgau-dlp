from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base

class BlockedDomain(Base):
    __tablename__ = "blocked_domains"

    id         = Column(Integer, primary_key=True, index=True)
    domain     = Column(String, unique=True, index=True)
    category   = Column(String)   # social / games / adult / crypto / custom
    reason     = Column(String)
    enabled    = Column(Boolean, default=True)
    created_by = Column(String)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
