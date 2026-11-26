---
title: Справочник API и WebSocket
tags: [qorgau-dlp, api, websocket, rest, reference]
---

# 🔌 Справочник API и WebSocket

← [[07 — Отчёты и комплаенс РК]] | Далее: [[09 — Сравнение с iBatyr и Roadmap]]

Базовый префикс всех REST-маршрутов: `/api/v1`.

## Аутентификация
- **Веб-консоль / Интеграции:** `Authorization: Bearer <JWT>` (получается через `POST /api/v1/auth/login`).
- **Агенты:** заголовок `X-Agent-Token: <AGENT_TOKEN>` (сравнение в постоянном времени).

---

## Основные группы REST-эндпоинтов

| Группа | Маршруты | Назначение |
|---|---|---|
| **Auth & RBAC** | `POST /auth/login`, `GET /auth/me`, `GET/POST /auth/users`, `PATCH/DELETE /auth/users/{id}` | JWT-авторизация, управление операторами (`admin`, `security_officer`, `viewer`) |
| **Агенты** | `GET /agents`, `GET/DELETE /agents/{agent_id}`, `GET /agent-download/windows` | Реестр ПК и скачивание бинарника `qorgau-agent.exe` |
| **Конфигурации** | `GET/POST /configs`, `GET/PUT/DELETE /configs/{id}`, `POST/DELETE /configs/{id}/assign` | Управление профилями и мгновенное назначение ПК |
| **Телеметрия агента** | `POST /monitor/register`, `POST /monitor/heartbeat`, `POST /monitor/events`, `GET /monitor/prevention-rules`, `POST /monitor/discovery`, `POST /monitor/inventory` | Приём батчей событий, скачивание правил блокировки/USB, отчёты краулера и инвентаря |
| **Инциденты SOC** | `GET/POST /incidents`, `PATCH /incidents/{id}`, `POST /incidents/{id}/comments`, `GET /incidents/stats` | Расследование, Legal Hold, комментарии, ручное создание |
| **Сотрудники и УРВ** | `GET /employees`, `GET /employees/{username}`, `GET /activity/worktime`, `GET /activity/summary`, `GET/PUT /settings/risk-threshold` | Досье сотрудников, Risk Score и учёт рабочего времени |
| **Активная защита** | `GET/POST /usb-devices`, `PATCH/DELETE /usb-devices/{id}`, `GET/POST /rules/apps`, `GET /events/files`, `GET /discovery`, `POST /discovery/trigger`, `GET /inventory`, `POST /inventory/trigger` | ВНИ (USB), запрещённое ПО, файловый мониторинг, краулер ПДн, инвентарь |
| **Медиа и Теневые копии** | `GET /screenshots`, `POST /screenshots/live-trigger`, `GET /shadow`, `GET /shadow/{id}/download`, `GET /audio`, `GET /webcam` | Скриншоты, Квадратор Live, теневые копии, аудио/видео |
| **Отчёты и Аудит** | `GET /reports/incidents`, `GET /reports/agents`, `GET /reports/compliance`, `GET /reports/export`, `GET /reports/cdr`, `GET /reports/activity/export`, `GET /audit`, `GET /audit/verify` | Аналитика, экспорт XLSX/CSV/JSON, крипто-проверка хэш-цепочки аудита |

---

## Протокол WebSocket

- **`GET /ws/agent?agent_id=<id>`** (заголовок `X-Agent-Token`):
  - Сервер → Агент (`WsAgentCommand`):
    - `ReloadConfig` — перечитать профиль конфигурации
    - `ReloadPreventionRules` — обновить USB-списки, блокировку ПО и `hosts`
    - `TakeScreenshot` — сделать скриншот сейчас
    - `LiveFrame` — отправить живой кадр для Квадратора
    - `RunDiscoveryScan` — запустить краулер ПДн
    - `RunInventoryScan` — собрать инвентаризацию железа/ПО
    - `TakeWebcamPhoto` / `RecordAudio` — команды наблюдения (если разрешены в профиле)
- **`GET /ws/dashboard?token=<JWT>`**:
  - Сервер → Браузер: `incident`, `screenshot`, `live_frame`, `audio`, `webcam`, `risk_alert`.
