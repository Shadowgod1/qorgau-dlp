---
title: Qorgau DLP v2.0 — Главная (MOC)
tags: [qorgau-dlp, moc, dlp, rust, security]
status: active
version: 2.0
updated: 2026-11-20
---

# 🛡️ Qorgau DLP v2.0 — База знаний проекта

> [!info] О системе
> **Qorgau DLP v2.0** — корпоративная система предотвращения утечек данных (DLP) и контроля активности персонала (УРВ), полностью переписанная на **Rust** (сервер + Windows-агент) с веб-консолью **SOC Command Center** на React + Ant Design 5 и автономным установочным **ISO-образом Debian 12**. Разработана с учётом требований законодательства Республики Казахстан (Закон о ПДн №94-V, ТК РК ст. 23, Приказ №410).

---

## 🗺️ Карта документации (Map of Content)

1. [[01 — Архитектура системы]] — схема компонентов, стек Rust/PostgreSQL/Nginx/React, потребление ресурсов.
2. [[02 — Установка и ISO Appliance]] — Docker Compose, автономный гибридный BIOS+UEFI ISO (`935 МБ`), preseed и пересборка.
3. [[03 — Windows-агент (Rust)]] — модули сбора, Win32 API, `winsqlite3.dll`, установка как службы Windows.
4. [[04 — Стандартные конфигурации и политики]] — 5 готовых профилей защиты, конструктор конфигураций, правила DLP (ИИН/БИН, IBAN, карты, ДСП).
5. [[05 — Активная защита (USB ВНИ, ПО, Веб, Краулер)]] — Менеджер ВНИ (Whitelist/Eject), блокировка процессов и доменов, FileModule, Data Discovery и инвентаризация.
6. [[06 — Руководство администратора и SOC-аналитика]] — ежедневная работа с Дашбордом, Квадратором Live, инцидентами, Legal Hold, Risk Score и теневыми копиями.
7. [[07 — Отчёты и комплаенс РК]] — 5 видов отчётов, экспорт XLSX/CSV/JSON/CDR/PDF и матрица соответствия законам РК.
8. [[08 — Справочник API и WebSocket]] — REST API `/api/v1/*`, аутентификация JWT и `X-Agent-Token`, WebSocket-команды реального времени.
9. [[09 — Сравнение с iBatyr и Roadmap]] — сравнительная матрица с iBatyr Monitor 5.6 и дорожная карта развития (Вариант C).

---

## 📊 Ключевые показатели v2.0

| Параметр | Значение |
|---|---|
| **Сервер (`qorgau-server`)** | Rust (Axum 0.8 + SQLx + Tokio), бинарник **5.57 МБ**, ~**24 МБ RAM** под нагрузкой |
| **Windows-агент (`qorgau-agent.exe`)** | Rust + Win32 API + `winsqlite3.dll`, бинарник **2.94 МБ**, ~**6.6 МБ RAM** |
| **Стек Docker** | 3 контейнера: `postgres:16-alpine`, `qorgau_server`, `qorgau_nginx` (без Redis/Celery/Grafana) |
| **Автономный ISO Appliance** | `dist/qorgau-dlp-2.0-amd64.iso` (**935 МБ**), оффлайн-установка Debian 12 + OCR `rus/kaz/eng` |
| **Модули наблюдения** | `mod_microphone` и `mod_webcam` строго **OFF по умолчанию** (fail-closed, ТК РК ст. 23) |
| **Тесты** | `cargo test --workspace` — 19/19 пройдено, TypeScript + Vite — 0 ошибок |

---

## 🚀 Быстрый старт (Локальный стенд)

```powershell
cd C:\Users\anuarbekov_t.ILER\Desktop\qorgau_dlp
docker compose up -d --build
# Веб-консоль: http://localhost
# Логин по умолчанию: admin / qorgau_admin_2024
```

> [!tip] Быстрая навигация в веб-консоли
> В любой точке консоли нажмите `Ctrl + K` для мгновенного перехода к разделу, агенту, сотруднику или скачиванию `qorgau-agent.exe`.
