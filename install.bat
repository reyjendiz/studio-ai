@echo off
title Студия — Установка моделей
cd /d "%~dp0"

echo ================================================================
echo   Студия AI: Запуск мастера установки...
echo ================================================================
echo.

if exist "Setup.exe" (
    start "" "Setup.exe"
    exit /b 0
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "install.ps1"
pause
