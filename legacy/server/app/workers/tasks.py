from celery import Celery
from celery.schedules import crontab
from sqlalchemy import create_engine, update, delete
from sqlalchemy.orm import Session
from datetime import datetime, timedelta
import httpx

from app.config import settings

celery_app = Celery(
    "qorgau",
    broker=settings.REDIS_URL,
    backend=settings.REDIS_URL,
)

celery_app.conf.timezone = "Asia/Almaty"
celery_app.conf.beat_schedule = {
    "check-agent-health-every-5min": {
        "task": "app.workers.tasks.check_agent_health",
        "schedule": 300.0,
    },
    "cleanup-old-data-daily": {
        "task": "app.workers.tasks.cleanup_old_data",
        "schedule": crontab(hour=2, minute=0),
    },
    "cleanup-screenshots-daily": {
        "task": "app.workers.tasks.cleanup_screenshots",
        "schedule": crontab(hour=3, minute=0),
    },
    "cleanup-shadow-daily": {
        "task": "app.workers.tasks.cleanup_shadow_copies",
        "schedule": crontab(hour=3, minute=30),
    },
}

# Синхронный движок для Celery-воркеров (psycopg2, без asyncpg)
_sync_url = settings.DATABASE_URL.replace("postgresql+asyncpg://", "postgresql+psycopg2://")
_sync_engine = create_engine(_sync_url, pool_pre_ping=True)


@celery_app.task(name="app.workers.tasks.send_telegram_alert", bind=True, max_retries=3)
def send_telegram_alert(
    self,
    incident_id: int,
    severity: str,
    channel: str,
    hostname: str,
    username: str,
    snippet: str,
):
    if not settings.ALERT_TELEGRAM_TOKEN or not settings.ALERT_TELEGRAM_CHAT_ID:
        return

    emoji = {"low": "🟡", "medium": "🟠", "high": "🔴", "critical": "🚨"}.get(severity, "⚠️")
    text = (
        f"{emoji} *DLP Инцидент \\#{incident_id}*\n"
        f"Серьёзность: `{severity.upper()}`\n"
        f"Канал: `{channel}`\n"
        f"Хост: `{hostname}`\n"
        f"Пользователь: `{username}`\n"
        f"Фрагмент: `{snippet[:150]}`"
    )

    try:
        with httpx.Client(timeout=10) as client:
            resp = client.post(
                f"https://api.telegram.org/bot{settings.ALERT_TELEGRAM_TOKEN}/sendMessage",
                json={
                    "chat_id": settings.ALERT_TELEGRAM_CHAT_ID,
                    "text": text,
                    "parse_mode": "MarkdownV2",
                },
            )
            resp.raise_for_status()
    except Exception as exc:
        raise self.retry(exc=exc, countdown=60)


@celery_app.task(name="app.workers.tasks.send_siem_webhook", bind=True, max_retries=3)
def send_siem_webhook(
    self,
    incident_id: int,
    severity: str,
    channel: str,
    hostname: str,
    username: str,
    snippet: str,
    matched_rules: list,
):
    """Отправляет инцидент в SIEM (Elasticsearch, Splunk, любой webhook)."""
    if not settings.SIEM_WEBHOOK_URL:
        return

    payload = {
        "source": "qorgau_dlp",
        "incident_id": incident_id,
        "severity": severity,
        "channel": channel,
        "hostname": hostname,
        "username": username,
        "snippet": snippet[:500],
        "matched_rules": matched_rules,
        "timestamp": datetime.utcnow().isoformat() + "Z",
    }
    headers = {}
    if settings.SIEM_WEBHOOK_TOKEN:
        headers["Authorization"] = f"Bearer {settings.SIEM_WEBHOOK_TOKEN}"

    try:
        with httpx.Client(timeout=15) as client:
            resp = client.post(settings.SIEM_WEBHOOK_URL, json=payload, headers=headers)
            resp.raise_for_status()
    except Exception as exc:
        raise self.retry(exc=exc, countdown=120)


@celery_app.task(name="app.workers.tasks.send_syslog_cef", bind=True, max_retries=3)
def send_syslog_cef(
    self,
    incident_id: int,
    severity: str,
    channel: str,
    hostname: str,
    username: str,
    snippet: str,
    matched_rules: list,
):
    """Отправляет инцидент в ОЦИБ/SIEM по Syslog в формате CEF."""
    if not settings.SIEM_SYSLOG_HOST:
        return

    from app.siem_cef import send_syslog_cef as _send
    try:
        _send(
            host=settings.SIEM_SYSLOG_HOST,
            port=settings.SIEM_SYSLOG_PORT,
            proto=settings.SIEM_SYSLOG_PROTO,
            facility=settings.SIEM_SYSLOG_FACILITY,
            incident_id=incident_id,
            severity=severity,
            channel=channel,
            hostname=hostname,
            username=username,
            snippet=snippet,
            matched_rules=matched_rules or [],
        )
    except Exception as exc:
        raise self.retry(exc=exc, countdown=60)


@celery_app.task(name="app.workers.tasks.check_agent_health")
def check_agent_health():
    from app.models.agent import Agent

    threshold = datetime.utcnow() - timedelta(minutes=10)
    with Session(_sync_engine) as session:
        session.execute(
            update(Agent)
            .where(Agent.last_seen < threshold)
            .values(is_online=False)
        )
        session.commit()


@celery_app.task(name="app.workers.tasks.cleanup_old_data")
def cleanup_old_data():
    from app.models.incident import Incident
    from app.models.session import UserSession
    from app.models.usb import UsbEvent
    from app.models.web import WebActivity
    from app.models.app_activity import AppActivity
    from app.models.messenger import MessengerEvent

    # Закон РК о персональных данных — хранить не более 3 лет
    cutoff = datetime.utcnow() - timedelta(days=1095)
    with Session(_sync_engine) as session:
        for model in [Incident, UserSession, UsbEvent, WebActivity, AppActivity, MessengerEvent]:
            session.execute(delete(model).where(model.created_at < cutoff))
        session.commit()


@celery_app.task(name="app.workers.tasks.cleanup_screenshots")
def cleanup_screenshots():
    import os
    from sqlalchemy import select
    from app.models.Screenshot import Screenshot

    cutoff = datetime.utcnow() - timedelta(days=settings.SCREENSHOTS_RETENTION_DAYS)
    with Session(_sync_engine) as session:
        old = session.execute(
            select(Screenshot).where(Screenshot.created_at < cutoff)
        ).scalars().all()
        for s in old:
            if os.path.exists(s.file_path):
                os.remove(s.file_path)
        session.execute(delete(Screenshot).where(Screenshot.created_at < cutoff))
        session.commit()


@celery_app.task(name="app.workers.tasks.cleanup_shadow_copies")
def cleanup_shadow_copies():
    import os
    from sqlalchemy import select
    from app.models.shadow_copy import ShadowCopy

    cutoff = datetime.utcnow() - timedelta(days=settings.SHADOW_RETENTION_DAYS)
    with Session(_sync_engine) as session:
        old = session.execute(
            select(ShadowCopy).where(ShadowCopy.created_at < cutoff)
        ).scalars().all()
        for s in old:
            if s.stored_path and os.path.exists(s.stored_path):
                os.remove(s.stored_path)
        session.execute(delete(ShadowCopy).where(ShadowCopy.created_at < cutoff))
        session.commit()
