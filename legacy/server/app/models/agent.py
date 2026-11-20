from sqlalchemy import Column, Integer, String, DateTime, Boolean, ForeignKey
from sqlalchemy.sql import func
from app.database import Base

class Agent(Base):
    __tablename__ = "agents"

    id            = Column(Integer, primary_key=True, index=True)
    agent_id      = Column(String, unique=True, index=True)
    hostname      = Column(String)
    username      = Column(String)
    ip_address    = Column(String)
    os_version    = Column(String)
    agent_version = Column(String)
    is_online     = Column(Boolean, default=False)
    last_seen     = Column(DateTime(timezone=True))
    registered_at = Column(DateTime(timezone=True), server_default=func.now())
    config_id     = Column(Integer, ForeignKey("agent_configs.id", ondelete="SET NULL"), nullable=True)
