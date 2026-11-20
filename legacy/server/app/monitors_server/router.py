import os
import aiofiles
from datetime import datetime, timezone
from fastapi import APIRouter, Depends, HTTPException, Header, UploadFile, File, Form
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, update

from app.database import get_db
from app.config import settings
from app.models.agent import Agent
from app.models.agent_config import AgentConfig
from app.models.session import UserSession
from app.models.usb import UsbEvent
from app.models.web import WebActivity
from app.models.app_activity import AppActivity
from app.models.messenger import MessengerEvent
from app.models.incident import Incident, Severity, IncidentStatus
from app.models.Screenshot import Screenshot
from app.models.network import NetworkConnection
from app.models.audio_recording import AudioRecording
from app.models.webcam_photo import WebcamPhoto
from app.models.clipboard import ClipboardEvent
from app.models.print_job import PrintJob
from app.engine.classifier import classifier
from app.engine.web_filter import web_filter
from app.ws_manager import manager
from app.monitors_server.policy_engine import match_policies, highest_severity, requires_block, invalidate_cache
from .schemas import AgentRegister, EventBatch

router = APIRouter(prefix="/monitor", tags=["Monitor"])


async def verify_token(x_agent_token: str = Header(...)):
    if x_agent_token != settings.AGENT_TOKEN:
        raise HTTPException(status_code=401, detail="Invalid agent token")


def _severity_from_str(s: str) -> Severity:
    try:
        return Severity(s)
    except ValueError:
        return Severity.MEDIUM


async def _create_incident(
    db, agent_id, hostname, username,
    channel, severity, rules, snippet, meta,
    trigger_screenshot: bool = False,
) -> Incident:
    incident = Incident(
        agent_id=agent_id,
        hostname=hostname,
        username=username,
        channel=channel,
        severity=severity,
        status=IncidentStatus.OPEN,
        matched_rules=rules,
        snippet=snippet,
        meta=meta,
    )
    db.add(incident)
    await db.flush()

    # Неизменяемый журнал: факт создания инцидента (доказательная база)
    from app.audit import write_audit
    await write_audit(
        db, actor="system", actor_role="agent",
        action="incident_created", resource=f"incident:{incident.id}",
        details={"channel": channel, "severity": str(severity), "hostname": hostname,
                 "username": username, "rules": rules},
    )

    sev_val = severity.value if hasattr(severity, "value") else str(severity)
    await manager.broadcast({
        "type": "incident",
        "id": incident.id,
        "severity": sev_val,
        "channel": channel,
        "hostname": hostname,
        "username": username,
        "snippet": (snippet or "")[:120],
    })

    # Триггерный скриншот при высоком инциденте
    if trigger_screenshot and sev_val in ("high", "critical"):
        await manager.broadcast({
            "type": "take_screenshot",
            "agent_id": agent_id,
        })

    return incident


async def _run_policy_engine(
    content: str,
    channel: str,
    db: AsyncSession,
    agent_id: str,
    hostname: str,
    username: str,
    extra_meta: dict,
    fallback_rules: list[str] | None = None,
    fallback_severity: Severity = Severity.MEDIUM,
) -> tuple[list[dict], bool]:
    """
    Прогоняет контент через Policy Engine.
    Возвращает (matches, blocked).
    """
    from app.workers.tasks import send_telegram_alert

    matches = await match_policies(content, channel, db)
    if not matches and fallback_rules:
        # Использовать хардкод-триггеры если нет политик
        matches = [{"name": r, "action": "alert", "severity": fallback_severity.value} for r in fallback_rules]

    if not matches:
        return [], False

    sev_str = highest_severity(matches)
    blocked = requires_block(matches)
    rule_names = [m["name"] for m in matches]
    snippet = (content or "")[:200]

    incident = await _create_incident(
        db, agent_id, hostname, username,
        channel, _severity_from_str(sev_str),
        rule_names, snippet, extra_meta,
        trigger_screenshot=True,
    )

    send_telegram_alert.delay(
        incident.id, sev_str, channel,
        hostname, username, snippet,
    )
    from app.workers.tasks import send_siem_webhook
    send_siem_webhook.delay(
        incident.id, sev_str, channel,
        hostname, username, snippet,
        [m["name"] for m in matches],
    )
    from app.workers.tasks import send_syslog_cef
    send_syslog_cef.delay(
        incident.id, sev_str, channel,
        hostname, username, snippet,
        [m["name"] for m in matches],
    )

    return matches, blocked


@router.post("/register", summary="Регистрация агента")
async def register_agent(
    payload: AgentRegister,
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    result = await db.execute(select(Agent).where(Agent.agent_id == payload.agent_id))
    agent = result.scalar_one_or_none()
    now = datetime.now(timezone.utc)

    if agent:
        agent.hostname   = payload.hostname
        agent.username   = payload.username
        agent.ip_address = payload.ip_address
        agent.os_version = payload.os_version
        agent.agent_version = payload.agent_version
        agent.is_online  = True
        agent.last_seen  = now
    else:
        agent = Agent(
            agent_id=payload.agent_id,
            hostname=payload.hostname,
            username=payload.username,
            ip_address=payload.ip_address,
            os_version=payload.os_version,
            agent_version=payload.agent_version,
            is_online=True,
            last_seen=now,
        )
        db.add(agent)

    await db.commit()

    # Засеиваем дефолтные KZ-политики при первом подключении агента
    from app.monitors_server.policy_engine import seed_default_policies
    await seed_default_policies(db)

    return {"status": "ok", "agent_id": payload.agent_id}


@router.post("/events", summary="Приём пакета событий от агента")
async def receive_events(
    payload: EventBatch,
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    incidents_created = 0
    blocked_events: list[str] = []

    for event in payload.events:
        etype = event.type
        data  = event.data

        # ── Session ──────────────────────────────────────────────────────────
        if etype == "session":
            db.add(UserSession(
                agent_id   = payload.agent_id,
                hostname   = payload.hostname,
                username   = payload.username,
                ip_address = data.get("ip_address", ""),
                event_type = data.get("event_type", ""),
                logon_type = data.get("logon_type"),
                meta       = data.get("meta"),
            ))

        # ── USB ───────────────────────────────────────────────────────────────
        elif etype == "usb":
            file_path = data.get("file_path") or ""
            action    = data.get("action", "")

            db.add(UsbEvent(
                agent_id    = payload.agent_id,
                username    = payload.username,
                device_id   = data.get("device_id", ""),
                device_name = data.get("device_name", ""),
                action      = action,
                drive_letter= data.get("drive_letter"),
                file_path   = file_path,
                size_gb     = data.get("size_gb"),
            ))

            if action == "file_copied":
                # Policy Engine по имени файла
                matches, blocked = await _run_policy_engine(
                    file_path, "usb", db,
                    payload.agent_id, payload.hostname, payload.username,
                    data,
                    fallback_rules=["USB file copy detected"],
                    fallback_severity=Severity.HIGH,
                )
                if matches:
                    incidents_created += 1
                if blocked:
                    blocked_events.append(event.type)

        # ── Web ───────────────────────────────────────────────────────────────
        elif etype == "web":
            domain  = data.get("domain", "")
            url     = data.get("url", "")
            blocked_domain = await web_filter.is_blocked(domain)
            category = web_filter.categorize(domain)

            db.add(WebActivity(
                agent_id = payload.agent_id,
                username = payload.username,
                url      = url,
                domain   = domain,
                category = category,
                blocked  = blocked_domain,
                browser  = data.get("browser"),
            ))

            if blocked_domain:
                matches, _ = await _run_policy_engine(
                    url, "web", db,
                    payload.agent_id, payload.hostname, payload.username,
                    data,
                    fallback_rules=["Blocked domain access attempt"],
                    fallback_severity=Severity.MEDIUM,
                )
                if matches:
                    incidents_created += 1

        # ── App ───────────────────────────────────────────────────────────────
        elif etype == "app":
            window_title = data.get("window_title", "")
            app_name = data.get("app_name", "")
            from app.engine.productivity import classify
            productivity = await classify(app_name, db)
            db.add(AppActivity(
                agent_id    = payload.agent_id,
                username    = payload.username,
                app_name    = app_name,
                window_title= window_title,
                exe_path    = data.get("exe_path"),
                duration_sec= data.get("duration_sec"),
                active_sec  = data.get("active_sec", 0),
                idle_sec    = data.get("idle_sec", 0),
                productivity= productivity,
            ))
            # Анализируем заголовок окна на конфиденциальные слова
            matches, _ = await _run_policy_engine(
                window_title, "app", db,
                payload.agent_id, payload.hostname, payload.username,
                data,
            )
            if matches:
                incidents_created += 1

        # ── Messenger ─────────────────────────────────────────────────────────
        elif etype == "messenger":
            content = (data.get("content") or "")[:1000]

            # Старый classifier + новый Policy Engine
            old_matches = classifier.analyze(content) if content else []
            policy_matches, blocked = await _run_policy_engine(
                content, "messenger", db,
                payload.agent_id, payload.hostname, payload.username,
                {"messenger": data.get("messenger"), "direction": data.get("direction")},
            )

            all_rules = [m.rule_name for m in old_matches] + [m["name"] for m in policy_matches]
            flagged = len(all_rules) > 0

            db.add(MessengerEvent(
                agent_id    = payload.agent_id,
                username    = payload.username,
                messenger   = data.get("messenger", ""),
                direction   = data.get("direction", "sent"),
                contact     = data.get("contact"),
                content     = content,
                has_file    = data.get("has_file", False),
                file_name   = data.get("file_name"),
                flagged     = flagged,
                matched_rules= ", ".join(all_rules) if all_rules else None,
            ))
            if policy_matches:
                incidents_created += 1
            if blocked:
                blocked_events.append("messenger")

        # ── Clipboard ─────────────────────────────────────────────────────────
        elif etype == "clipboard":
            content = (data.get("content") or "")[:2000]
            source_app = data.get("source_app", "")

            policy_matches, blocked = await _run_policy_engine(
                content, "clipboard", db,
                payload.agent_id, payload.hostname, payload.username,
                {"source_app": source_app},
            )

            db.add(ClipboardEvent(
                agent_id   = payload.agent_id,
                username   = payload.username,
                content    = content,
                source_app = source_app,
                flagged    = len(policy_matches) > 0,
                matched_rules= ", ".join(m["name"] for m in policy_matches) if policy_matches else None,
            ))
            if policy_matches:
                incidents_created += 1
            if blocked:
                blocked_events.append("clipboard")

        # ── Network ───────────────────────────────────────────────────────────
        elif etype == "network":
            db.add(NetworkConnection(
                agent_id    = payload.agent_id,
                username    = payload.username,
                local_ip    = data.get("local_ip", ""),
                local_port  = data.get("local_port"),
                remote_ip   = data.get("remote_ip", ""),
                remote_port = data.get("remote_port"),
                protocol    = data.get("protocol", "tcp"),
                status      = data.get("status", ""),
                process_name= data.get("process_name", ""),
                exe_path    = data.get("exe_path"),
                bytes_sent  = data.get("bytes_sent"),
                bytes_recv  = data.get("bytes_recv"),
            ))

        # ── Print ─────────────────────────────────────────────────────────────
        elif etype == "print":
            doc_name = data.get("document_name", "")
            db.add(PrintJob(
                agent_id      = payload.agent_id,
                username      = payload.username,
                printer_name  = data.get("printer_name", ""),
                document_name = doc_name,
                pages         = data.get("pages"),
                size_bytes    = data.get("size_bytes"),
                status        = data.get("status", ""),
            ))
            # Любая печать — логируем через Policy Engine
            matches, _ = await _run_policy_engine(
                doc_name, "print", db,
                payload.agent_id, payload.hostname, payload.username,
                data,
                fallback_rules=["Document sent to printer"],
                fallback_severity=Severity.LOW,
            )
            if matches:
                incidents_created += 1

    await db.execute(
        update(Agent)
        .where(Agent.agent_id == payload.agent_id)
        .values(is_online=True, last_seen=datetime.now(timezone.utc))
    )
    await db.commit()

    return {
        "status": "ok",
        "processed": len(payload.events),
        "incidents": incidents_created,
        "blocked": blocked_events,
    }


@router.post("/screenshot", summary="Загрузка скриншота от агента")
async def upload_screenshot(
    agent_id: str  = Form(...),
    hostname: str  = Form(...),
    username: str  = Form(...),
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    agent_dir = os.path.join(settings.SCREENSHOTS_DIR, agent_id)
    os.makedirs(agent_dir, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S_%f")
    file_path = os.path.join(agent_dir, f"{timestamp}.jpg")

    content = await file.read()
    async with aiofiles.open(file_path, "wb") as f:
        await f.write(content)

    screenshot = Screenshot(
        agent_id  = agent_id,
        hostname  = hostname,
        username  = username,
        file_path = file_path,
        file_size = len(content),
    )
    db.add(screenshot)
    await db.commit()
    await db.refresh(screenshot)

    await manager.broadcast({
        "type": "screenshot",
        "agent_id": agent_id,
        "hostname": hostname,
        "username": username,
        "screenshot_id": screenshot.id,
    })

    return {"status": "ok", "screenshot_id": screenshot.id}


@router.post("/shadow", summary="Загрузка теневой копии файла от агента")
async def upload_shadow_copy(
    agent_id: str  = Form(...),
    hostname: str  = Form(...),
    username: str  = Form(...),
    original_path: str = Form(""),
    sha256: str    = Form(""),
    channel: str   = Form("usb"),
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    from app.models.shadow_copy import ShadowCopy

    content = await file.read()
    max_bytes = settings.SHADOW_MAX_MB * 1024 * 1024
    if len(content) > max_bytes:
        raise HTTPException(status_code=413, detail="Shadow copy too large")

    agent_dir = os.path.join(settings.SHADOW_DIR, agent_id)
    os.makedirs(agent_dir, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S_%f")
    safe_name = (file.filename or "file").replace("\\", "_").replace("/", "_")
    stored_path = os.path.join(agent_dir, f"{timestamp}_{safe_name}")

    async with aiofiles.open(stored_path, "wb") as f:
        await f.write(content)

    # Пытаемся привязать к недавнему инциденту того же файла (по sha256 в meta)
    incident_id = None
    if sha256:
        res = await db.execute(
            select(Incident)
            .where(Incident.channel == channel)
            .order_by(Incident.created_at.desc())
            .limit(20)
        )
        for inc in res.scalars().all():
            if inc.meta and inc.meta.get("sha256") == sha256:
                incident_id = inc.id
                break

    shadow = ShadowCopy(
        agent_id=agent_id,
        hostname=hostname,
        username=username,
        channel=channel,
        original_path=original_path,
        file_name=safe_name,
        sha256=sha256,
        size_bytes=len(content),
        stored_path=stored_path,
        incident_id=incident_id,
    )
    db.add(shadow)
    await db.commit()
    await db.refresh(shadow)

    # ── OCR: если это скан/изображение — распознаём текст и ищем ПДн ───────────
    ocr_incident = None
    from app.engine.ocr import is_image, ocr_image_bytes
    if is_image(safe_name):
        text = ocr_image_bytes(content)
        if text:
            matches, _ = await _run_policy_engine(
                text, channel, db,
                agent_id, hostname, username,
                {"source": "ocr", "file_name": safe_name, "sha256": sha256},
            )
            if matches:
                # Привязываем инцидент к теневой копии
                res = await db.execute(
                    select(Incident).order_by(Incident.created_at.desc()).limit(1)
                )
                inc = res.scalar_one_or_none()
                if inc:
                    ocr_incident = inc.id
                    shadow.incident_id = inc.id
                    await db.commit()

    return {
        "status": "ok",
        "shadow_id": shadow.id,
        "incident_id": incident_id or ocr_incident,
        "ocr_matched": bool(ocr_incident),
    }


@router.post("/heartbeat", summary="Пинг агента")
async def heartbeat(
    agent_id: str,
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    await db.execute(
        update(Agent)
        .where(Agent.agent_id == agent_id)
        .values(is_online=True, last_seen=datetime.now(timezone.utc))
    )
    await db.commit()
    return {"status": "ok"}


@router.post("/policy-sync", summary="Инвалидация кэша политик")
async def policy_sync(_=Depends(verify_token)):
    """Вызывается после изменения политик через UI."""
    invalidate_cache()
    return {"status": "ok"}


@router.post("/audio", summary="Загрузка аудиозаписи от агента")
async def upload_audio(
    agent_id: str  = Form(...),
    hostname: str  = Form(...),
    username: str  = Form(...),
    duration_sec: int = Form(0),
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    agent_dir = os.path.join(settings.AUDIO_DIR, agent_id)
    os.makedirs(agent_dir, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S_%f")
    file_path = os.path.join(agent_dir, f"{timestamp}.wav")

    content = await file.read()
    async with aiofiles.open(file_path, "wb") as f:
        await f.write(content)

    recording = AudioRecording(
        agent_id     = agent_id,
        hostname     = hostname,
        username     = username,
        file_path    = file_path,
        file_size    = len(content),
        duration_sec = duration_sec,
    )
    db.add(recording)
    await db.commit()
    await db.refresh(recording)

    await manager.broadcast({
        "type": "audio_recording",
        "agent_id": agent_id,
        "hostname": hostname,
        "username": username,
        "recording_id": recording.id,
        "duration_sec": duration_sec,
    })

    return {"status": "ok", "recording_id": recording.id}


@router.post("/webcam", summary="Загрузка фото с веб-камеры от агента")
async def upload_webcam_photo(
    agent_id: str  = Form(...),
    hostname: str  = Form(...),
    username: str  = Form(...),
    file: UploadFile = File(...),
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    agent_dir = os.path.join(settings.WEBCAM_DIR, agent_id)
    os.makedirs(agent_dir, exist_ok=True)

    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S_%f")
    file_path = os.path.join(agent_dir, f"{timestamp}.jpg")

    content = await file.read()
    async with aiofiles.open(file_path, "wb") as f:
        await f.write(content)

    photo = WebcamPhoto(
        agent_id  = agent_id,
        hostname  = hostname,
        username  = username,
        file_path = file_path,
        file_size = len(content),
    )
    db.add(photo)
    await db.commit()
    await db.refresh(photo)

    await manager.broadcast({
        "type": "webcam_photo",
        "agent_id": agent_id,
        "hostname": hostname,
        "username": username,
        "photo_id": photo.id,
    })

    return {"status": "ok", "photo_id": photo.id}


_DEFAULT_CONFIG = {
    "id": None,
    "mod_sessions": True, "mod_apps": True, "mod_web": True,
    "mod_messenger": True, "mod_clipboard": True, "mod_network": True,
    "mod_usb": True, "mod_printer": True, "mod_screenshots": True,
    "mod_microphone": False, "mod_webcam": False,
    "screenshot_interval": 60, "screenshot_quality": 60, "screenshot_max_w": 1920,
    "microphone_duration": 30, "microphone_sample_rate": 16000,
    "webcam_interval": 120, "webcam_quality": 60, "webcam_max_w": 1280,
    "net_alert_sensitive_ports": True, "net_alert_unknown_ext": False, "net_block_tor": True,
    "web_alert_social": False, "web_alert_adult": True, "web_alert_gambling": True,
    "web_alert_streaming": False, "web_alert_crypto": False,
    "usb_alert_on_connect": True, "usb_analyze_content": True, "usb_alert_executables": True,
}


@router.get("/config", summary="Конфигурация агента")
async def get_agent_config(
    agent_id: str,
    db: AsyncSession = Depends(get_db),
    _=Depends(verify_token),
):
    result = await db.execute(select(Agent).where(Agent.agent_id == agent_id))
    agent = result.scalar_one_or_none()
    if not agent or not agent.config_id:
        return _DEFAULT_CONFIG
    result2 = await db.execute(select(AgentConfig).where(AgentConfig.id == agent.config_id))
    cfg = result2.scalar_one_or_none()
    if not cfg:
        return _DEFAULT_CONFIG
    return {col.name: getattr(cfg, col.name) for col in AgentConfig.__table__.columns}
