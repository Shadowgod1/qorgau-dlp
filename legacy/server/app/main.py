from fastapi import FastAPI, WebSocket, WebSocketDisconnect, Depends
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager

from app.database import engine, Base, AsyncSessionLocal
from app.api import events, incidents, sessions, rules, reports, agents, web_activity, screenshots, timeline, auth, olap, configs, audit, shadow, activity, audio, webcam
from app.api.auth import get_current_user, require_admin, require_officer, seed_default_admin
from app.monitors_server import router as monitor_router
from app.ws_manager import manager


@asynccontextmanager
async def lifespan(_app: FastAPI):
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    # Bootstrap: создаём дефолтного админа + дефолтные категории продуктивности
    async with AsyncSessionLocal() as db:
        await seed_default_admin(db)
        from app.engine.productivity import seed_defaults
        await seed_defaults(db)
    yield


app = FastAPI(
    title="Qorgau DLP",
    description="Data Loss Prevention система для корпоративной защиты данных",
    version="1.0.0",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Auth — публичный (login), защита внутри роутера
app.include_router(auth.router,         prefix="/api/v1", tags=["Auth"])

# Управление через дашборд — требуется аутентификация
_authed = [Depends(get_current_user)]
_officer = [Depends(require_officer)]
_admin   = [Depends(require_admin)]

app.include_router(agents.router,       prefix="/api/v1", tags=["Agents"],      dependencies=_authed)
app.include_router(incidents.router,    prefix="/api/v1", tags=["Incidents"],   dependencies=_authed)
app.include_router(events.router,       prefix="/api/v1", tags=["Events"],      dependencies=_authed)
app.include_router(sessions.router,     prefix="/api/v1", tags=["Sessions"],    dependencies=_authed)
app.include_router(rules.router,        prefix="/api/v1", tags=["Rules"],       dependencies=_admin)
app.include_router(web_activity.router, prefix="/api/v1", tags=["WebActivity"], dependencies=_authed)
app.include_router(reports.router,      prefix="/api/v1", tags=["Reports"],     dependencies=_authed)
app.include_router(screenshots.router,  prefix="/api/v1", tags=["Screenshots"])  # защита на уровне эндпоинтов
app.include_router(timeline.router,     prefix="/api/v1", tags=["Timeline"],    dependencies=_authed)
app.include_router(olap.router,         prefix="/api/v1", tags=["OLAP"],        dependencies=_authed)
app.include_router(configs.router,      prefix="/api/v1", tags=["Configs"],     dependencies=_admin)
app.include_router(audit.router,        prefix="/api/v1", tags=["Audit"],       dependencies=_officer)
app.include_router(shadow.router,       prefix="/api/v1", tags=["Shadow"],      dependencies=_officer)
app.include_router(activity.router,     prefix="/api/v1", tags=["Activity"],    dependencies=_authed)
app.include_router(audio.router,        prefix="/api/v1", tags=["Audio"],       dependencies=_officer)
app.include_router(webcam.router,       prefix="/api/v1", tags=["Webcam"],      dependencies=_officer)

# Канал приёма данных от агентов
app.include_router(monitor_router.router, prefix="/api/v1", tags=["Monitor"])


@app.websocket("/ws")
async def websocket_endpoint(ws: WebSocket):
    await manager.connect(ws)
    try:
        while True:
            await ws.receive_text()
    except WebSocketDisconnect:
        manager.disconnect(ws)


@app.get("/health", tags=["System"])
async def health():
    return {"status": "ok", "service": "Qorgau DLP"}
