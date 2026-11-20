"""JWT-аутентификация и RBAC для UI-консоли."""
import os
import hmac
import hashlib
import time
import base64
import json as _json
from dataclasses import dataclass
from datetime import datetime, timezone

from fastapi import APIRouter, HTTPException, Depends
from fastapi.security import HTTPBearer, HTTPAuthorizationCredentials
from pydantic import BaseModel
from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.config import settings
from app.database import get_db
from app.models.operator import (
    Operator, ALL_ROLES,
    ROLE_ADMIN, ROLE_SECURITY_OFFICER, ROLE_VIEWER,
)

router = APIRouter(prefix="/auth", tags=["Auth"])
_bearer = HTTPBearer(auto_error=False)


# ── Хэширование паролей (PBKDF2-SHA256, без внешних зависимостей) ──────────────

_PBKDF2_ITERS = 200_000


def hash_password(password: str) -> str:
    salt = os.urandom(16)
    dk = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, _PBKDF2_ITERS)
    return f"pbkdf2_sha256${_PBKDF2_ITERS}${salt.hex()}${dk.hex()}"


def verify_password(password: str, stored: str) -> bool:
    try:
        algo, iters, salt_hex, hash_hex = stored.split("$")
        dk = hashlib.pbkdf2_hmac(
            "sha256", password.encode(), bytes.fromhex(salt_hex), int(iters)
        )
        return hmac.compare_digest(dk.hex(), hash_hex)
    except Exception:
        return False


# ── JWT (вручную, без python-jose) ────────────────────────────────────────────

def _b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def _sign(header_b64: str, payload_b64: str) -> str:
    msg = f"{header_b64}.{payload_b64}".encode()
    sig = hmac.new(settings.SECRET_KEY.encode(), msg, hashlib.sha256).digest()
    return _b64url(sig)


def create_token(username: str, role: str) -> str:
    header  = _b64url(_json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    payload = _b64url(_json.dumps({
        "sub":  username,
        "role": role,
        "exp":  int(time.time()) + settings.JWT_EXPIRE_MIN * 60,
        "iat":  int(time.time()),
    }).encode())
    sig = _sign(header, payload)
    return f"{header}.{payload}.{sig}"


@dataclass
class CurrentUser:
    username: str
    role: str


def _decode_token(token: str) -> CurrentUser:
    try:
        parts = token.split(".")
        if len(parts) != 3:
            raise ValueError("bad format")
        header, payload, sig = parts
        expected = _sign(header, payload)
        if not hmac.compare_digest(sig, expected):
            raise ValueError("bad signature")
        data = _json.loads(base64.urlsafe_b64decode(payload + "=="))
        if data["exp"] < int(time.time()):
            raise ValueError("expired")
        return CurrentUser(username=data["sub"], role=data.get("role", ROLE_VIEWER))
    except Exception as e:
        raise HTTPException(status_code=401, detail=f"Unauthorized: {e}")


def get_current_user(
    creds: HTTPAuthorizationCredentials | None = Depends(_bearer),
) -> CurrentUser:
    if not creds:
        raise HTTPException(status_code=401, detail="Missing Authorization header")
    return _decode_token(creds.credentials)


# ── RBAC-зависимости ──────────────────────────────────────────────────────────

def require_roles(*roles: str):
    """Фабрика зависимости: пропускает только указанные роли."""
    async def _dep(user: CurrentUser = Depends(get_current_user)) -> CurrentUser:
        if user.role not in roles:
            raise HTTPException(
                status_code=403,
                detail=f"Недостаточно прав (требуется одна из ролей: {', '.join(roles)})",
            )
        return user
    return _dep


# Офицер ИБ и админ видят контент; viewer — нет
require_admin   = require_roles(ROLE_ADMIN)
require_officer = require_roles(ROLE_ADMIN, ROLE_SECURITY_OFFICER)


# ── Bootstrap: дефолтный админ из настроек ────────────────────────────────────

async def seed_default_admin(db: AsyncSession) -> None:
    """Создаёт админа из настроек, если таблица операторов пуста."""
    result = await db.execute(select(Operator).limit(1))
    if result.scalar():
        return
    db.add(Operator(
        username=settings.ADMIN_USERNAME,
        password_hash=hash_password(settings.ADMIN_PASSWORD),
        role=ROLE_ADMIN,
        full_name="Администратор",
        is_active=True,
    ))
    await db.commit()


# ── Endpoints: аутентификация ─────────────────────────────────────────────────

class LoginRequest(BaseModel):
    username: str
    password: str


class TokenResponse(BaseModel):
    access_token: str
    token_type: str = "bearer"
    expires_in: int
    role: str


@router.post("/login", response_model=TokenResponse, summary="Получить JWT-токен")
async def login(body: LoginRequest, db: AsyncSession = Depends(get_db)):
    await seed_default_admin(db)

    result = await db.execute(select(Operator).where(Operator.username == body.username))
    op = result.scalar_one_or_none()
    if not op or not op.is_active or not verify_password(body.password, op.password_hash):
        raise HTTPException(status_code=401, detail="Неверный логин или пароль")

    op.last_login = datetime.now(timezone.utc)
    await db.commit()

    token = create_token(op.username, op.role)
    return TokenResponse(
        access_token=token,
        expires_in=settings.JWT_EXPIRE_MIN * 60,
        role=op.role,
    )


@router.get("/me", summary="Текущий пользователь")
async def me(user: CurrentUser = Depends(get_current_user)):
    return {"username": user.username, "role": user.role}


# ── Endpoints: управление операторами (только admin) ──────────────────────────

class OperatorCreate(BaseModel):
    username: str
    password: str
    role: str = ROLE_VIEWER
    full_name: str | None = None


class OperatorUpdate(BaseModel):
    role: str | None = None
    password: str | None = None
    is_active: bool | None = None
    full_name: str | None = None


def _serialize_op(op: Operator) -> dict:
    return {
        "id": op.id,
        "username": op.username,
        "role": op.role,
        "full_name": op.full_name,
        "is_active": op.is_active,
        "created_at": op.created_at.isoformat() if op.created_at else None,
        "last_login": op.last_login.isoformat() if op.last_login else None,
    }


@router.get("/users", summary="Список операторов", dependencies=[Depends(require_admin)])
async def list_operators(db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Operator).order_by(Operator.id))
    return [_serialize_op(o) for o in result.scalars().all()]


@router.post("/users", summary="Создать оператора", dependencies=[Depends(require_admin)])
async def create_operator(body: OperatorCreate, db: AsyncSession = Depends(get_db)):
    if body.role not in ALL_ROLES:
        raise HTTPException(status_code=400, detail=f"Недопустимая роль: {body.role}")
    exists = await db.execute(select(Operator).where(Operator.username == body.username))
    if exists.scalar_one_or_none():
        raise HTTPException(status_code=409, detail="Пользователь уже существует")

    op = Operator(
        username=body.username,
        password_hash=hash_password(body.password),
        role=body.role,
        full_name=body.full_name,
        is_active=True,
    )
    db.add(op)
    await db.commit()
    await db.refresh(op)
    return _serialize_op(op)


@router.patch("/users/{op_id}", summary="Изменить оператора", dependencies=[Depends(require_admin)])
async def update_operator(op_id: int, body: OperatorUpdate, db: AsyncSession = Depends(get_db)):
    result = await db.execute(select(Operator).where(Operator.id == op_id))
    op = result.scalar_one_or_none()
    if not op:
        raise HTTPException(status_code=404, detail="Оператор не найден")

    if body.role is not None:
        if body.role not in ALL_ROLES:
            raise HTTPException(status_code=400, detail=f"Недопустимая роль: {body.role}")
        op.role = body.role
    if body.password is not None:
        op.password_hash = hash_password(body.password)
    if body.is_active is not None:
        op.is_active = body.is_active
    if body.full_name is not None:
        op.full_name = body.full_name

    await db.commit()
    return _serialize_op(op)


@router.delete("/users/{op_id}", summary="Удалить оператора", dependencies=[Depends(require_admin)])
async def delete_operator(
    op_id: int,
    db: AsyncSession = Depends(get_db),
    current: CurrentUser = Depends(require_admin),
):
    result = await db.execute(select(Operator).where(Operator.id == op_id))
    op = result.scalar_one_or_none()
    if not op:
        raise HTTPException(status_code=404, detail="Оператор не найден")
    if op.username == current.username:
        raise HTTPException(status_code=400, detail="Нельзя удалить самого себя")

    await db.delete(op)
    await db.commit()
    return {"status": "deleted"}
