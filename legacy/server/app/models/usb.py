from sqlalchemy import Column, Integer, String, DateTime, Float
from sqlalchemy.sql import func
from app.database import Base

class UsbEvent(Base):
    __tablename__ = "usb_events"

    id          = Column(Integer, primary_key=True, index=True)
    agent_id    = Column(String, index=True)
    username    = Column(String, index=True)
    device_id   = Column(String)
    device_name = Column(String)
    action      = Column(String)    # connected / disconnected / file_copied / drive_mounted
    drive_letter = Column(String)
    file_path   = Column(String)
    size_gb     = Column(Float)
    created_at  = Column(DateTime(timezone=True), server_default=func.now(), index=True)
