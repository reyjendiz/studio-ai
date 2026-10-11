@echo off
cd /d "%~dp0"
set CSC=C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe
if not exist "%CSC%" set CSC=C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe

"%CSC%" /nologo /target:winexe /out:Studio.exe /win32icon:..\ui\studio.ico /win32manifest:app.manifest /r:Microsoft.Web.WebView2.Core.dll /r:Microsoft.Web.WebView2.WinForms.dll /r:System.Windows.Forms.dll /r:System.Drawing.dll Studio.cs

if %ERRORLEVEL% EQU 0 (
    echo [OK] app\Studio.exe compiled
) else (
    echo [ERROR] Compilation failed. Close Studio.exe first if it is running.
)
pause
