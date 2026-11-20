from sqlalchemy import Column, Integer, String, DateTime, Boolean, Text
from sqlalchemy.sql import func
from app.database import Base

class MessengerEvent(Base):
    __tablename__ = "messenger_events"

    id           = Column(Integer, primary_key=True, index=True)
    agent_id     = Column(String, index=True)
    username     = Column(String, index=True)
    messenger    = Column(String, index=True)  # telegram / whatsapp / slack / teams / viber / discord
    direction    = Column(String)              # sent / received
    contact      = Column(String)             # имя контакта или номер
    content      = Column(Text)              # текст сообщения (макс 1000 символов)
    has_file     = Column(Boolean, default=False)
    file_name    = Column(String)
    flagged      = Column(Boolean, default=False)  # помечено DLP движком
    matched_rules = Column(String)
    created_at   = Column(DateTime(timezone=True), server_default=func.now(), index=True)
