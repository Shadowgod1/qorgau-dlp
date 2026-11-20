from pydantic import BaseModel, Field
from typing import Optional, Any
from datetime import datetime


class AgentRegister(BaseModel):
    agent_id: str
    hostname: str
    username: str
    ip_address: str
    os_version: str
    agent_version: str


class EventItem(BaseModel):
    type: str  # session / usb / web / app / messenger
    timestamp: Optional[datetime] = None
    data: dict[str, Any]


class EventBatch(BaseModel):
    agent_id: str
    hostname: str
    username: str
    events: list[EventItem] = Field(default_factory=list)
