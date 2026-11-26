---
title: Установка и ISO Appliance
tags: [qorgau-dlp, deployment, iso, debian, docker]
---

# 💿 Установка и ISO Appliance

← [[01 — Архитектура системы]] | Далее: [[03 — Windows-агент (Rust)]]

Qorgau DLP v2.0 поддерживает два официальных режима развёртывания: **Docker Compose** (для серверов с существующей ОС) и **автономный загрузочный ISO-образ Debian 12 Appliance** (установка «на голое железо» или ВМ без доступа в интернет).

---

## Вариант 1. Автономный ISO-образ (Appliance)

- **Файл образа:** `dist/qorgau-dlp-2.0-amd64.iso` (**935 МБ**)
- **Контрольная сумма:** `dist/qorgau-dlp-2.0-amd64.iso.sha256`
- **Поддержка загрузки:** Гибридный **BIOS (ISOLINUX) + UEFI (GRUB)**
- **Требования к ВМ/серверу:** 2 vCPU, 2 ГБ RAM (рекомендуется 4 ГБ при активном OCR), диск от 20 ГБ (от 100 ГБ для архива скриншотов/теневых копий).

### Что вшито внутрь ISO
- Базовая ОС: `debian-12.15.0-amd64-netinst.iso` + автоматический `preseed.cfg` в `initrd.gz` (часовой пояс `Asia/Almaty`, авто-разметка диска, SSH-сервер).
- Оффлайн `.deb`-пакеты (78 пакетов): PostgreSQL, `nginx-light`, `tesseract-ocr` + языковые модели `rus`, `kaz`, `eng`.
- Скомпилированный Linux-бинарник `/usr/local/bin/qorgau-server` и systemd-сервис `qorgau-server.service`.
- Собранный фронтенд React Dashboard и конфиг Nginx (`/etc/nginx/sites-available/qorgau`).
- Windows-агент `/var/lib/qorgau/downloads/qorgau-agent.exe` (доступен для скачивания прямо из консоли).

### Процесс установки на ВМ (Hyper-V / VMware / VirtualBox / Proxmox)
1. Подключите `qorgau-dlp-2.0-amd64.iso` как виртуальный DVD-привод и загрузите ВМ.
2. Установка пройдёт **полностью автоматически** (5–7 минут), после чего сервер перезагрузится.
3. При установке скрипт `iso/post-install.sh` генерирует **уникальные случайные ключи** (`DB_PASSWORD`, `SECRET_KEY`, `AGENT_TOKEN`), сохраняет их в `/etc/qorgau/qorgau.env` (права `600`) и выводит на экран входа консоли (`/etc/issue`):
   - **Веб-консоль:** `http://<IP-сервера>/`
   - **Логин консоли:** `admin` / `qorgau_admin_2026` *(сразу смените пароль в Настройках!)*
   - **SSH-доступ к ОС:** `qorgau` / `qorgau2026`

### Пересборка ISO после внесения изменений
```powershell
cd C:\Users\anuarbekov_t.ILER\Desktop\qorgau_dlp
powershell -ExecutionPolicy Bypass -File .\iso\build-iso.ps1
```
> [!tip] Кэширование сборки
> Базовый ISO Debian 12 и `.deb`-пакеты кэшируются в `iso/cache/`. Повторная пересборка образа занимает **~30 секунд**.

---

## Вариант 2. Docker Compose

```powershell
cd C:\Users\anuarbekov_t.ILER\Desktop\qorgau_dlp
docker compose up -d --build
```

Конфигурация задаётся через файл `.env`:
- `DB_PASSWORD` — пароль PostgreSQL 16
- `SECRET_KEY` — ключ подписи JWT
- `AGENT_TOKEN` — общий секрет авторизации агентов (`X-Agent-Token`)
- `ADMIN_USERNAME` / `ADMIN_PASSWORD` — первичный администратор (создаётся при первом запуске)
- `OCR_CONCURRENCY` — число параллельных потоков Tesseract OCR (по умолчанию `2`)
