' Runs Studio.ps1 with no visible window (a plain powershell -WindowStyle Hidden still flashes a console)
CreateObject("WScript.Shell").Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""E:\ComfyUI\Studio.ps1""", 0, False
