# 🛡️ QORGAU DLP — Инструкция для агента

> Прочитай этот файл ПЕРВЫМ при работе с проектом.

## Что это за проект

**QORGAU DLP** — корпоративная система предотвращения утечек данных (Data Loss Prevention) для казахстанских организаций. Система мониторит рабочие компьютеры сотрудников, обнаруживает утечки конфиденциальных данных и оповещает офицера безопасности.

## Архитектура

Проект состоит из **3 компонентов** + **инфраструктура**:

```
qorgau_dlp/
├── agent/         → Python-агент для Windows ПК
├── server/        → FastAPI REST-сервер
├── dashboard/     → React + TypeScript дашборд
├── docs/          → Документация (этот vault)
├── docker-compose.yml
├── nginx.conf
└── .env
```

## Быстрый старт

### 1. Сервер + БД (Docker)
```bash
cp .env.example .env      # настроить переменные
docker-compose up -d       # поднять PostgreSQL, Redis, API, Celery, Nginx, Grafana
```

### 2. Дашборд (dev-сервер)
```bash
cd dashboard
npm install
npm run dev                # → http://localhost:5173
```

### 3. Агент (Windows)
```bash
cd agent
pip install -r requirements.txt
python agent.py            # запуск в dev-режиме
# или собрать .exe:
python -m PyInstaller QorgauAgent.spec
```

## Связанные документы

| Документ | Описание |
|----------|----------|
| [architecture.md](architecture.md) | Архитектура системы, потоки данных, диаграммы |
| [agent.md](agent.md) | Подробно о Python-агенте (коллекторы, DLP, сборка) |
| [server.md](server.md) | FastAPI-сервер: модели, API, Policy Engine |
| [dashboard.md](dashboard.md) | React-дашборд: страницы, компоненты, роутинг |
| [api-reference.md](api-reference.md) | Справочник всех API endpoints |
| [deployment.md](deployment.md) | Docker, .env, production, SSL, hardening |
| [testing.md](testing.md) | Тесты: как запускать, что проверяется |

## Ключевые концепции

- **Policy Engine** — ядро DLP. regex-паттерны в БД, кэш 60 сек
- **Коллекторы** — daemon-потоки в агенте, каждый мониторит свой канал
- **Sender** — HTTP-клиент с очередью и batch-отправкой
- **RBAC** — 3 роли: admin, security_officer, viewer
- **Казахстанская специфика** — ИИН/БИН, IBAN KZ, грифы секретности, Syslog CEF для ОЦИБ

## Правила разработки

1. **Не удаляй существующие комментарии** — они содержат юридические ссылки (ТК РК, GDPR)
2. **Агент работает ТОЛЬКО на Windows** (ctypes, winreg, win32gui)
3. **Модули микрофона/камеры выключены по умолчанию** — это чувствительные функции
4. **Все изменения политик** должны проходить через `invalidate_cache()` в Policy Engine
5. **JWT токены** — 8 часов, хранятся в `localStorage` под ключом `qorgau_token`
6. **Серверный config** — `server/app/config.py` (pydantic-settings), агентский — `agent/config.py`

## Стек технологий

| Слой | Технология |
|------|-----------|
| Агент | Python 3.11+, pywin32, mss, sounddevice, opencv |
| Сервер | FastAPI, SQLAlchemy (async), Celery, Redis |
| БД | PostgreSQL 16 |
| Дашборд | React 18, TypeScript, Ant Design 5, Recharts, Vite |
| Инфраструктура | Docker Compose, Nginx, Grafana |
