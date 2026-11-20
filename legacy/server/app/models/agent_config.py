from sqlalchemy import Column, Integer, String, Boolean, DateTime, Text
from datetime import datetime
from app.database import Base


class AgentConfig(Base):
    __tablename__ = "agent_configs"

    id          = Column(Integer, primary_key=True)
    name        = Column(String(120), unique=True, nullable=False)
    description = Column(Text, nullable=True)

    # ── Коллекторы ─────────────────────────────────────────────────
    mod_sessions    = Column(Boolean, default=True)   # Вход/выход пользователей
    mod_apps        = Column(Boolean, default=True)   # Активные приложения
    mod_web         = Column(Boolean, default=True)   # Интернет / история браузера
    mod_messenger   = Column(Boolean, default=True)   # Мессенджеры
    mod_clipboard   = Column(Boolean, default=True)   # Буфер обмена
    mod_network     = Column(Boolean, default=True)   # Сетевые соединения
    mod_usb         = Column(Boolean, default=True)   # USB-устройства и файлы
    mod_printer     = Column(Boolean, default=True)   # Печать
    mod_screenshots = Column(Boolean, default=True)   # Скриншоты

    # ── Скриншоты ──────────────────────────────────────────────────
    screenshot_interval = Column(Integer, default=60)   # секунд
    screenshot_quality  = Column(Integer, default=60)   # JPEG 1-95
    screenshot_max_w    = Column(Integer, default=1920)

    # ── Сеть ───────────────────────────────────────────────────────
    net_alert_sensitive_ports = Column(Boolean, default=True)   # RDP, SSH, SMB, etc.
    net_alert_unknown_ext     = Column(Boolean, default=False)  # Неизвестные внешние IP
    net_block_tor             = Column(Boolean, default=True)   # Алерт при Tor-портах

    # ── Веб-фильтрация (алерты) ────────────────────────────────────
    web_alert_social    = Column(Boolean, default=False)
    web_alert_adult     = Column(Boolean, default=True)
    web_alert_gambling  = Column(Boolean, default=True)
    web_alert_streaming = Column(Boolean, default=False)
    web_alert_crypto    = Column(Boolean, default=False)

    # ── USB-политика ───────────────────────────────────────────────
    usb_alert_on_connect  = Column(Boolean, default=True)
    usb_analyze_content   = Column(Boolean, default=True)
    usb_alert_executables = Column(Boolean, default=True)

    # ── Микрофон (выключен по умолчанию — чувствительный модуль) ───
    mod_microphone       = Column(Boolean, default=False)
    microphone_duration  = Column(Integer, default=30)    # длительность фрагмента (сек)
    microphone_sample_rate = Column(Integer, default=16000)

    # ── Веб-камера (выключена по умолчанию — чувствительный модуль) ─
    mod_webcam           = Column(Boolean, default=False)
    webcam_interval      = Column(Integer, default=120)   # секунд между снимками
    webcam_quality       = Column(Integer, default=60)    # JPEG 1-95
    webcam_max_w         = Column(Integer, default=1280)  # макс. ширина кадра

    created_by = Column(String(100), nullable=True)
    created_at = Column(DateTime, default=datetime.utcnow)
    updated_at = Column(DateTime, default=datetime.utcnow, onupdate=datetime.utcnow)
