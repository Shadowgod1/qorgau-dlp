-- Qorgau DLP v2.3 — Built-in standard configuration templates (idempotent seed)
-- Security invariant: mod_microphone and mod_webcam remain strictly FALSE by default (ТК РК ст. 23).

INSERT INTO agent_configs (
    name, description,
    mod_sessions, mod_apps, mod_web, mod_messenger, mod_clipboard, mod_network,
    mod_usb, mod_printer, mod_screenshots,
    screenshot_interval, screenshot_quality, screenshot_max_w,
    mod_watermark, watermark_opacity, watermark_text, watermark_apps,
    net_alert_sensitive_ports, net_alert_unknown_ext, net_block_tor,
    web_alert_social, web_alert_adult, web_alert_gambling, web_alert_streaming, web_alert_crypto,
    usb_alert_on_connect, usb_analyze_content, usb_alert_executables, usb_whitelist_mode,
    mod_file_monitor, mod_discovery, discovery_paths, mod_inventory,
    mod_app_blocker, mod_web_blocker,
    mod_microphone, mod_webcam, created_by
)
VALUES
(
    'Стандартный (Офис)',
    'Сбалансированный профиль для большинства офисных рабочих станций: полный аудит каналов утечек, скриншоты раз в 60 сек, контроль файлов и инвентаризация ПО без жёстких блокировок.',
    TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE,
    60, 60, 1920,
    FALSE, 22, '{username} • {hostname} • {time}', '',
    TRUE, FALSE, TRUE,
    FALSE, TRUE, TRUE, FALSE, FALSE,
    TRUE, TRUE, TRUE, FALSE,
    TRUE, FALSE, '', TRUE,
    FALSE, FALSE,
    FALSE, FALSE, 'system'
),
(
    'Усиленный ИБ (Бухгалтерия / Финансы)',
    'Строгий профиль для отделов, работающих с ПДн, ИИН/БИН и банковскими реквизитами: частые скриншоты (30 сек), водяные знаки на экране, белый список USB-носителей (ВНИ) и краулер ПДн.',
    TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE,
    30, 65, 1920,
    TRUE, 26, '{username} • {hostname} • {time}', '',
    TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE, FALSE, TRUE,
    TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, '', TRUE,
    FALSE, TRUE,
    FALSE, FALSE, 'system'
),
(
    'Максимальная защита (Гриф ДСП)',
    'Режим максимальной защиты для критичных узлов и руководителей: активная блокировка запрещённого ПО и сайтов, белый список ВНИ, заметные водяные знаки (32%), скриншоты каждые 20 сек и краулер дисков.',
    TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE,
    20, 70, 1920,
    TRUE, 32, 'КОНФИДЕНЦИАЛЬНО • {username} • {hostname} • {time}', '',
    TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, '', TRUE,
    TRUE, TRUE,
    FALSE, FALSE, 'system'
),
(
    'Только УРВ (Учёт рабочего времени)',
    'Деликатный профиль контроля продуктивности: фиксирует только сессии входа/выхода, активные приложения, посещённые сайты и инвентаризацию ПО. Без скриншотов, буфера обмена и перехвата файлов.',
    TRUE, TRUE, TRUE, FALSE, FALSE, FALSE,
    FALSE, FALSE, FALSE,
    120, 50, 1280,
    FALSE, 20, '{username} • {hostname} • {time}', '',
    FALSE, FALSE, FALSE,
    FALSE, TRUE, TRUE, FALSE, FALSE,
    FALSE, FALSE, FALSE, FALSE,
    FALSE, FALSE, '', TRUE,
    FALSE, FALSE,
    FALSE, FALSE, 'system'
),
(
    'Удалённый сотрудник (VPN / Home Office)',
    'Оптимизирован для удалённой работы: защита от фото экрана водяными знаками, контроль мессенджеров, буфера, файлов и RDP/Tor-соединений; без жёсткого извлечения домашних USB-устройств.',
    TRUE, TRUE, TRUE, TRUE, TRUE, TRUE,
    TRUE, TRUE, TRUE,
    45, 55, 1600,
    TRUE, 22, 'REMOTE • {username} • {hostname} • {time}', '',
    TRUE, TRUE, TRUE,
    FALSE, TRUE, TRUE, FALSE, TRUE,
    TRUE, TRUE, TRUE, FALSE,
    TRUE, FALSE, '', TRUE,
    FALSE, TRUE,
    FALSE, FALSE, 'system'
)
ON CONFLICT (name) DO NOTHING;
