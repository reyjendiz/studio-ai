@echo off
rem Starts the prompt UI and opens it in the browser. ComfyUI is started by the UI on the first generation and stopped when idle.
cd /d %~dp0
start "Z-Image UI" /min python_embeded\python.exe ui\server.py
timeout /t 3 >nul
start http://127.0.0.1:8189
