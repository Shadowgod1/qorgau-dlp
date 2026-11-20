# 🛡️ QORGAU DLP v2.0 (Rust)

## Что это за проект

**QORGAU DLP** — корпоративная система предотвращения утечек данных (Data Loss Prevention) и контроля активности сотрудников для казахстанских организаций, переписанная на **Rust** для минимального потребления CPU и RAM.

## Архитектура

```
qorgau_dlp/
├── Cargo.toml                 → Rust Cargo Workspace (opt-level="z", LTO, strip)
├── crates/
│   ├── qorgau-common/         → Протокол, валидация agent_id, DLP-детекторы РК (ИИН/БИН mod-11, Luhn, IBAN mod-97, CEF, маскирование)
│   ├── qorgau-server/         → Единый бинарник Axum + Tokio + SQLx (PostgreSQL 16) + встроенные воркеры (OCR, алерты, retention)
│   └── qorgau-agent/          → Нативный Windows x64 агент (~5-10 МБ, event-driven Win32, 11 каналов, дисковая WAL-очередь)
├── dashboard/                 → React 18 + TypeScript + Ant Design 5 дашборд
├── legacy/                    → Исходная Python-версия (архив)
├── Dockerfile                 → Multi-stage сборка qorgau-server + Tesseract OCR (rus/kaz/eng)
├── Dockerfile.nginx           → Multi-stage сборка dashboard + Nginx (REST + WSS Upgrade)
├── docker-compose.yml         → Лёгкий 3-контейнерный стек (PostgreSQL, Rust server, Nginx)
├── nginx.conf
└── .env.example
```

## Быстрый старт

### 1. Сервер + Дашборд + БД (Docker Compose)
```bash
cp .env.example .env           # задать пароли и SECRET_KEY
docker compose up -d --build   # → http://localhost
```

### 2. Локальная сборка и тесты (Rust)
```powershell
cargo test --workspace
cargo build --release -p qorgau-agent
# Готовый агент: target/release/qorgau-agent.exe
```

### 3. Запуск агента на Windows
Создайте `config.toml` рядом с `qorgau-agent.exe` (или задайте переменные `QORGAU_SERVER_URL`, `QORGAU_AGENT_TOKEN`):
```toml
server_url = "http://localhost"
agent_token = "your_agent_token"
```

## Ключевые принципы безопасности
1. **Fail-closed по умолчанию:** модули `mod_microphone` и `mod_webcam` всегда выключены при недоступности сервера (ТК РК ст. 23).
2. **Авторизованные WebSockets:** `/ws/dashboard` (JWT) и `/ws/agent` (`X-Agent-Token` + адресная доставка команд по `agent_id`).
3. **Защита от Path Traversal:** строгая валидация `agent_id` (`^[a-zA-Z0-9_-]{8,64}$`) + проверка канонических путей.
4. **RBAC + Маскирование ПДн:** роли `admin`, `security_officer`, `viewer`; для `viewer` маскируются `snippet` и `meta`, скрыты сырые мессенджеры/медиа.
5. **Hash-Chain Аудит (Приказ №410):** криптографическая цепочка SHA-256 с транзакционной блокировкой PostgreSQL (`pg_advisory_xact_lock`).
