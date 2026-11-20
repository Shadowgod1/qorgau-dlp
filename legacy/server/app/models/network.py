from sqlalchemy import Column, Integer, String, DateTime, BigInteger
from sqlalchemy.sql import func
from app.database import Base


class NetworkConnection(Base):
    __tablename__ = "network_connections"

    id           = Column(Integer, primary_key=True, index=True)
    agent_id     = Column(String, index=True)
    username     = Column(String, index=True)
    local_ip     = Column(String)
    local_port   = Column(Integer)
    remote_ip    = Column(String, index=True)
    remote_port  = Column(Integer)
    protocol     = Column(String)          # tcp / udp
    status       = Column(String)          # ESTABLISHED / TIME_WAIT / etc.
    process_name = Column(String, index=True)
    exe_path     = Column(String)
    bytes_sent   = Column(BigInteger)
    bytes_recv   = Column(BigInteger)
    created_at   = Column(DateTime(timezone=True), server_default=func.now(), index=True)
