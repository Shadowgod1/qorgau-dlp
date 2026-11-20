from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    DATABASE_URL: str = "postgresql+asyncpg://qorgau_user:password@db/qorgau_dlp"
    REDIS_URL: str = "redis://redis:6379/0"
    SECRET_KEY: str = "change-me-in-production"
    AGENT_TOKEN: str = "change-agent-token"

    ALERT_TELEGRAM_TOKEN: str = ""
    ALERT_TELEGRAM_CHAT_ID: str = ""

    SERVER_DOMAIN: str = "localhost"

    SCREENSHOTS_DIR: str = "/app/screenshots"
    SCREENSHOTS_RETENTION_DAYS: int = 30

    # Теневое копирование (доказательная база)
    SHADOW_DIR: str = "/app/shadow"
    SHADOW_RETENTION_DAYS: int = 365
    SHADOW_MAX_MB: int = 50

    # UI auth
    ADMIN_USERNAME: str = "admin"
    ADMIN_PASSWORD: str = "qorgau_admin_2024"
    JWT_ALGORITHM: str  = "HS256"
    JWT_EXPIRE_MIN: int = 480    # 8 часов

    # SIEM (JSON-webhook)
    SIEM_WEBHOOK_URL: str = ""   # URL куда слать инциденты (Elastic/Splunk/etc)
    SIEM_WEBHOOK_TOKEN: str = ""

    # ОЦИБ / SIEM по Syslog + CEF (RFC 5424 / ArcSight CEF)
    SIEM_SYSLOG_HOST: str = ""          # адрес syslog-коллектора ОЦИБ; пусто = выкл
    SIEM_SYSLOG_PORT: int = 514
    SIEM_SYSLOG_PROTO: str = "udp"      # udp | tcp
    SIEM_SYSLOG_FACILITY: int = 13      # 13 = log audit (RFC 5424)

    # Микрофон — хранение записей
    AUDIO_DIR: str = "/app/audio"
    AUDIO_RETENTION_DAYS: int = 30

    # Веб-камера — хранение фото
    WEBCAM_DIR: str = "/app/webcam_photos"
    WEBCAM_RETENTION_DAYS: int = 30

    class Config:
        env_file = ".env"

settings = Settings()
