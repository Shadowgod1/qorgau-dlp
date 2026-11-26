---
title: Windows-агент Qorgau DLP (Rust)
tags: [qorgau-dlp, agent, windows, rust, win32]
---

# 💻 Windows-агент (`qorgau-agent.exe`)

← [[02 — Установка и ISO Appliance]] | Далее: [[04 — Стандартные конфигурации и политики]]

Агент написан на чистом **Rust** (`stable-x86_64-pc-windows-gnu`), компилируется в единый бинарник **2.94 МБ** и не требует установки Python, .NET или Visual C++ Redistributable.

---

## Архитектура модулей агента

| Модуль (`crates/qorgau-agent/src/`) | Механизм Win32 / ОС | Функциональность |
|---|---|---|
| `collectors/apps.rs` | `GetForegroundWindow`, `GetWindowTextW`, `QueryFullProcessImageNameW` | Трекинг активных окон, приложений и времени простоя (`GetLastInputInfo`). |
| `collectors/web.rs` + `winsqlite.rs` | Динамическая загрузка `C:\Windows\System32\winsqlite3.dll` | Чтение истории Chrome, Edge, Firefox без блокировки БД браузера. |
| `collectors/clipboard.rs` | `OpenClipboard` / `GetClipboardData(CF_UNICODETEXT)` | Перехват текста буфера обмена и отправка на DLP-анализ. |
| `collectors/usb.rs` | `STORAGE_DEVICE_DESCRIPTOR`, `FSCTL_LOCK_VOLUME`, `IOCTL_STORAGE_EJECT_MEDIA` | Чтение аппаратных `VID`/`PID`/`Serial`, анализ файлов и принудительное извлечение чужих флешек. |
| `collectors/blocker.rs` | `EnumProcesses`, `TerminateProcess`, `C:\Windows\System32\drivers\etc\hosts` | Активная блокировка запрещённых программ и доменов. |
| `collectors/files.rs` | Обход `Desktop`, `Documents`, `Downloads` + SHA-256 | Контроль локальных файловых операций (`FileModule`). |
| `collectors/discovery.rs` | Встроенный движок `qorgau_common::dlp` | Краулер ПДн на дисках (`Data at Rest`): ИИН, БИН, карты, IBAN, ДСП. |
| `collectors/inventory.rs` | Win32 System Info + Реестр `Uninstall` (x64/x86/HKCU) | Инвентаризация CPU/RAM/Дисков/ОС и установленного ПО. |
| `collectors/screen.rs` | GDI `BitBlt` + JPEG-компрессия | Периодические скриншоты + мгновенные Live-кадры для Квадратора. |
| `collectors/watermark.rs` | Прозрачное `WS_EX_LAYERED \| WS_EX_TRANSPARENT \| WS_EX_TOPMOST` окно | Неперекрываемые кликами водяные знаки поверх экрана. |
| `collectors/printer.rs` | WinSpool `EnumPrintersW` / `EnumJobsW` | Аудит очереди и заданий печати. |
| `collectors/network.rs` | `GetExtendedTcpTable` | Контроль активных TCP-соединений, RDP/SSH/SMB и Tor. |
| `collectors/audio.rs` / `webcam.rs` | WinMM / Media | **Строго выключены по умолчанию (`false`)**, включаются только явно в профиле. |

---

## Развёртывание на рабочих станциях

### 1. Скачивание агента
Скачайте `qorgau-agent.exe` прямо из веб-консоли:
- кнопка **«Агент (.exe)»** в шапке или на странице **Агенты** (`/agents`);
- или через API: `GET /api/v1/agent-download/windows`.

### 2. Тестовый запуск
```powershell
$env:QORGAU_SERVER_URL = "http://<IP_СЕРВЕРА>"
$env:QORGAU_AGENT_TOKEN = "<AGENT_TOKEN>"
.\qorgau-agent.exe
```

### 3. Установка как автозапускаемая служба Windows (от Администратора)
```powershell
New-Item -ItemType Directory -Force -Path "C:\ProgramData\QorgauDLP"
Copy-Item .\qorgau-agent.exe C:\ProgramData\QorgauDLP\qorgau-agent.exe

sc.exe create QorgauAgent `
  binPath= "C:\ProgramData\QorgauDLP\qorgau-agent.exe --service --server http://<IP_СЕРВЕРА> --token <AGENT_TOKEN>" `
  start= auto `
  DisplayName= "Qorgau DLP Endpoint Agent"

sc.exe start QorgauAgent
```

### 4. Сборка агента из исходников (на Windows)
```powershell
cargo build --release -p qorgau-agent
# Результат: target\release\qorgau-agent.exe (~2.94 МБ)
```
