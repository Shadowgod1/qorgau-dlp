from sqlalchemy import Column, Integer, String, DateTime, JSON
from sqlalchemy.sql import func
from app.database import Base

class UserSession(Base):
    __tablename__ = "user_sessions"

    id          = Column(Integer, primary_key=True, index=True)
    agent_id    = Column(String, index=True)
    hostname    = Column(String)
    username    = Column(String, index=True)
    ip_address  = Column(String)
    event_type  = Column(String)   # logon / logoff / lock / unlock / rdp_logon
    logon_type  = Column(String)   # Interactive / Network / RemoteInteractive
    meta        = Column(JSON)
    created_at  = Column(DateTime(timezone=True), server_default=func.now(), index=True)
