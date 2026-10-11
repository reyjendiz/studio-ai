# Starts ComfyUI + the UI server hidden, shows the UI in a chromeless Edge window, stops what it started when the window closes.
$root = 'E:\ComfyUI'
$py   = "$root\python_embeded\python.exe"   # python.exe hidden + logs to files (pythonw has no stdout, which ComfyUI needs)
$edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
if (-not (Test-Path $edge)) { $edge = "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe" }
$logs = "$root\logs"; New-Item -ItemType Directory -Force $logs | Out-Null
function Log($m) { Add-Content "$logs\studio.log" "$(Get-Date -Format 'HH:mm:ss') $m" }

function Test-Port($port) {
  $c = New-Object Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $port); $true } catch { $false } finally { $c.Close() }
}
function Start-Hidden($name, $argList) {
  Start-Process $py -ArgumentList $argList -WorkingDirectory $root -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput "$logs\$name.out.log" -RedirectStandardError "$logs\$name.err.log"
}
$started = @()   # only processes we launched get stopped at the end

# ComfyUI is started by the UI server on the first generation and stopped when idle
if (-not (Test-Port 8189)) { $started += Start-Hidden 'ui' @('ui\server.py'); Log 'started UI server' }

# the page only needs the UI server; ComfyUI can keep loading while the window opens
$t = 0; while (-not (Test-Port 8189) -and $t -lt 60) { Start-Sleep -Milliseconds 500; $t++ }
Log "ui port ready after $($t * 0.5)s"

$app = Start-Process $edge -ArgumentList '--app=http://127.0.0.1:8189', "--user-data-dir=$root\ui\profile", '--start-maximized', '--no-first-run' -PassThru
Log "edge pid $($app.Id)"
$app.WaitForExit()
Log "edge exited ($($app.ExitCode))"

foreach ($p in $started) { if (-not $p.HasExited) { & taskkill /T /F /PID $p.Id | Out-Null } }
# also stop servers that were already running (UI server and ComfyUI), so nothing outlives the window
Get-NetTCPConnection -LocalPort 8188,8189 -State Listen -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
Log 'stopped servers'
