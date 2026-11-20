from sqlalchemy import Column, Integer, String, DateTime, BigInteger
from sqlalchemy.sql import func
from app.database import Base


class PrintJob(Base):
    __tablename__ = "print_jobs"

    id            = Column(Integer, primary_key=True, index=True)
    agent_id      = Column(String, index=True)
    username      = Column(String, index=True)
    printer_name  = Column(String)
    document_name = Column(String)
    pages         = Column(Integer)
    size_bytes    = Column(BigInteger)
    status        = Column(String)          # Printed / Error / Deleted
    created_at    = Column(DateTime(timezone=True), server_default=func.now(), index=True)
