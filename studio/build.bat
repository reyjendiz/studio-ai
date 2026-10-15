@echo off
rem Builds the native Studio app and installs Studio.exe and studio-cli.exe into ..\app
cd /d "%~dp0"
cargo build --release || (echo [ERROR] build failed & exit /b 1)
copy /y target\release\Studio.exe ..\app\Studio.exe >nul || (echo [ERROR] Studio.exe is running, close it first & exit /b 1)
copy /y target\release\studio-cli.exe ..\app\studio-cli.exe >nul || (echo [ERROR] studio-cli.exe is in use: run "studio-cli shutdown" & exit /b 1)
echo [OK] app\Studio.exe and app\studio-cli.exe updated
