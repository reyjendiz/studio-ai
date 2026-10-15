@echo off
rem Starts the native Studio app (ComfyUI is started by it on the first generation and stopped when idle).
cd /d %~dp0
start "" "%~dp0app\Studio.exe"
