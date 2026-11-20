from sqlalchemy import Column, Integer, String, DateTime, Boolean
from sqlalchemy.sql import func
from app.database import Base


class User(Base):
    """Dimension-таблица пользователей (Star Schema)."""
    __tablename__ = "users"

    id          = Column(Integer, primary_key=True, index=True)
    username    = Column(String, unique=True, index=True)
    sid         = Column(String, unique=True, index=True)   # Windows SID
    domain      = Column(String)                             # AD домен
    ad_group    = Column(String)                             # группа AD
    department  = Column(String)
    email       = Column(String)
    is_active   = Column(Boolean, default=True)
    risk_score  = Column(Integer, default=0)                 # накопленный риск 0-100
    created_at  = Column(DateTime(timezone=True), server_default=func.now())
    updated_at  = Column(DateTime(timezone=True), onupdate=func.now())
