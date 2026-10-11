@echo off
cd /d "%~dp0"
echo Compiling Setup.exe...
set CSC=C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe
if not exist "%CSC%" set CSC=C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe

"%CSC%" /nologo /target:winexe /win32manifest:app.manifest /win32icon:..\ui\studio.ico /r:System.dll,System.Windows.Forms.dll,System.Drawing.dll,System.Core.dll /out:..\Setup.exe Setup.cs

if %ERRORLEVEL% EQU 0 (
    echo [OK] Setup.exe compiled successfully: E:\ComfyUI\Setup.exe
) else (
    echo [ERROR] Compilation failed.
)
pause
