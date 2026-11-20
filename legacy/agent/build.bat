@echo off
echo ============================================
echo  Qorgau DLP Agent — сборка .exe
echo ============================================
echo.

python --version >nul 2>&1
if errorlevel 1 (
    echo [ОШИБКА] Python не найден. Установи Python 3.11+
    pause
    exit /b 1
)

echo [1/3] Устанавливаем зависимости...
python -m pip install -r requirements.txt --quiet
python -m pip install pyinstaller --quiet

echo.
echo [2/3] Собираем QorgauAgent.exe...
python -m PyInstaller ^
    --onefile ^
    --noconsole ^
    --name QorgauAgent ^
    --hidden-import win32timezone ^
    --hidden-import win32api ^
    --hidden-import win32con ^
    --hidden-import win32gui ^
    --hidden-import win32process ^
    --hidden-import win32evtlog ^
    --hidden-import win32evtlogutil ^
    --hidden-import win32security ^
    --hidden-import win32file ^
    --hidden-import win32clipboard ^
    --hidden-import win32ts ^
    --hidden-import wmi ^
    --hidden-import pywintypes ^
    --hidden-import mss ^
    --collect-all wmi ^
    --distpath dist ^
    --workpath build ^
    --specpath . ^
    --clean ^
    agent.py

echo.
if exist dist\QorgauAgent.exe (
    echo [3/3] Готово!
    echo.
    echo  Файл: %~dp0dist\QorgauAgent.exe
    for %%A in (dist\QorgauAgent.exe) do echo  Размер: %%~zA байт
) else (
    echo [ОШИБКА] Сборка не удалась. Смотри лог выше.
)
echo.
pause
