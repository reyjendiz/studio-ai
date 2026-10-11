# ==============================================================================
# Студия AI — Скрипт автоматической установки моделей и компонентов
# ==============================================================================
param(
    [string]$TargetDir = $PSScriptRoot,
    [switch]$SkipZImage,
    [switch]$SkipNoctQ,
    [switch]$SkipUpscalers,
    [switch]$SkipTranslator,
    [switch]$NoShortcut
)

$Host.UI.RawUI.WindowTitle = "Студия — Установка моделей"
Write-Host ""
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host "   ✦ СТУДИЯ AI — Автоматическая установка моделей и переводчика  " -ForegroundColor White
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host " Папка установки: $TargetDir" -ForegroundColor Gray
Write-Host ""

function Download-ModelFile {
    param(
        [string]$Url,
        [string]$RelativePath,
        [long]$ExpectedSize,
        [string]$Description
    )

    $dest = Join-Path $TargetDir $RelativePath
    $parent = Split-Path -Parent $dest
    if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }

    if (Test-Path $dest) {
        $size = (Get-Item $dest).Length
        if ($ExpectedSize -gt 0 -and $size -eq $ExpectedSize) {
            Write-Host " [✓ УЖЕ ЕСТЬ] $Description ($([math]::Round($size / 1MB, 1)) МБ)" -ForegroundColor Green
            return
        }
    }

    Write-Host " [↓ ЗАГРУЗКА] $Description..." -ForegroundColor Yellow
    Write-Host "              Файл: $RelativePath" -ForegroundColor DarkGray

    $part = "$dest.part"
    $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue

    if ($curl) {
        & curl.exe -L --retry 3 --retry-delay 3 -C - -o $part -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" $Url
        if ($LASTEXITCODE -eq 0 -and (Test-Path $part)) {
            if (Test-Path $dest) { Remove-Item $dest -Force }
            Move-Item $part $dest -Force
            Write-Host " [✓ ГОТОВО] $Description" -ForegroundColor Green
            return
        }
    }

    # Fallback to BITS / WebClient if curl is not present
    $wc = New-Object System.Net.WebClient
    $wc.Headers.Add("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
    $wc.DownloadFile($Url, $part)
    if (Test-Path $dest) { Remove-Item $dest -Force }
    Move-Item $part $dest -Force
    Write-Host " [✓ ГОТОВО] $Description" -ForegroundColor Green
}

# 1. Translator (Helsinki-NLP/opus-mt-ru-en)
if (-not $SkipTranslator) {
    Write-Host "`n>>> [1/4] Установка офлайн-переводчика RU -> EN (Helsinki-NLP/opus-mt-ru-en)..." -ForegroundColor Cyan
    $trBase = "https://huggingface.co/Helsinki-NLP/opus-mt-ru-en/resolve/main"
    Download-ModelFile "$trBase/pytorch_model.bin" "translate\opus-mt-ru-en\pytorch_model.bin" 306991893 "Веса модели перевода (307 МБ)"
    Download-ModelFile "$trBase/config.json" "translate\opus-mt-ru-en\config.json" 1337 "Конфиг модели перевода"
    Download-ModelFile "$trBase/generation_config.json" "translate\opus-mt-ru-en\generation_config.json" 304 "Конфиг генерации"
    Download-ModelFile "$trBase/source.spm" "translate\opus-mt-ru-en\source.spm" 1080169 "SPM модель источника"
    Download-ModelFile "$trBase/target.spm" "translate\opus-mt-ru-en\target.spm" 802781 "SPM модель цели"
    Download-ModelFile "$trBase/tokenizer_config.json" "translate\opus-mt-ru-en\tokenizer_config.json" 42 "Конфиг токенизатора"
    Download-ModelFile "$trBase/vocab.json" "translate\opus-mt-ru-en\vocab.json" 2601758 "Словарь"
}

# 2. Upscalers (Phips: Nomos2 hq dat2 for photos, HFA2k for anime)
if (-not $SkipUpscalers) {
    Write-Host "`n>>> [2/4] Установка моделей Апскейлера 2x (Nomos2 hq dat2 + HFA2k)..." -ForegroundColor Cyan
    Download-ModelFile "https://huggingface.co/Phips/4xNomos2_hq_dat2/resolve/main/4xNomos2_hq_dat2.safetensors" "ComfyUI\models\upscale_models\4xNomos2_hq_dat2.safetensors" 139792588 "4xNomos2 hq dat2 (Фото, люди, предметы, 139.8 МБ)"
    Download-ModelFile "https://huggingface.co/Phips/4xHFA2k/resolve/main/4xHFA2k.safetensors" "ComfyUI\models\upscale_models\4xHFA2k.safetensors" 33467822 "4xHFA2k (Аниме и арт, 33.5 МБ)"
}

# 3. Z-Image Ultimate NSFW v2.0
if (-not $SkipZImage) {
    Write-Host "`n>>> [3/4] Установка AI Модели: Z-Image Ultimate NSFW v2.0 (~17 ГБ)..." -ForegroundColor Cyan
    Download-ModelFile "https://civitai.com/api/download/models/2827368" "ComfyUI\models\diffusion_models\zImageUltimateNSFW_v20.safetensors" 12309876360 "Z-Image Ultimate NSFW v2.0 Diffusion Model (11.5 ГБ)"
    Download-ModelFile "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/text_encoders/qwen_3_4b_fp8_mixed.safetensors" "ComfyUI\models\text_encoders\qwen_3_4b_fp8_mixed.safetensors" 5631994051 "Qwen 3.4B Text Encoder (5.25 ГБ)"
    Download-ModelFile "https://huggingface.co/Comfy-Org/z_image_turbo/resolve/main/split_files/vae/ae.safetensors" "ComfyUI\models\vae\ae.safetensors" 335304388 "Z-Image Autoencoder VAE (320 МБ)"
}

# 4. NoctQ V4 int8 (Qwen-Image 2.1)
if (-not $SkipNoctQ) {
    Write-Host "`n>>> [4/4] Установка AI Модели: NoctQ V4 int8 (Qwen-Image 2.1) (~16.1 ГБ)..." -ForegroundColor Cyan
    $noctBase = "https://huggingface.co/Noctaluna/Noct-Q-Uncensored-Qwen-Image-2.1/resolve/main"
    Download-ModelFile "$noctBase/NoctQ_V4_int8_convrot.safetensors" "ComfyUI\models\diffusion_models\NoctQ_V4_int8_convrot.safetensors" 7256784368 "NoctQ V4 INT8 Diffusion Model (6.76 ГБ)"
    Download-ModelFile "$noctBase/qwen3vl_8b_int8_convrot.safetensors" "ComfyUI\models\text_encoders\qwen3vl_8b_int8_convrot.safetensors" 9350798360 "Qwen 3VL 8B Text Encoder (8.71 ГБ)"
    Download-ModelFile "$noctBase/qwen_image_2.1_vae_bf16.safetensors" "ComfyUI\models\vae\qwen_image_2.1_vae_bf16.safetensors" 675509688 "Qwen-Image 2.1 VAE BF16 (644 МБ)"
}

# Sync models.json
$regPath = Join-Path $TargetDir "ui\models.json"
$reg = @{
    "NoctQ_V4_int8_convrot.safetensors" = "qwen21"
    "zImageUltimateNSFW_v20.safetensors" = "zimage"
}
if (Test-Path $regPath) {
    try {
        $existing = Get-Content $regPath -Raw | ConvertFrom-Json
        foreach ($prop in $existing.PSObject.Properties) { $reg[$prop.Name] = $prop.Value }
    } catch {}
}
$reg | ConvertTo-Json -Compress:$false | Set-Content $regPath -Encoding UTF8
Write-Host "`n✓ Реестр моделей сохранен: ui\models.json" -ForegroundColor Green

# Desktop shortcut
if (-not $NoShortcut) {
    $desktop = [Environment]::GetFolderPath("Desktop")
    $shortcutPath = Join-Path $desktop "Студия.lnk"
    $wsh = New-Object -ComObject WScript.Shell
    $sc = $wsh.CreateShortcut($shortcutPath)
    $exeApp = Join-Path $TargetDir "app\Studio.exe"
    $ps1App = Join-Path $TargetDir "Studio.ps1"
    if (Test-Path $exeApp) {
        $sc.TargetPath = $exeApp
    } else {
        $sc.TargetPath = "powershell.exe"
        $sc.Arguments = "-ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ps1App`""
    }
    $sc.WorkingDirectory = $TargetDir
    $ico = Join-Path $TargetDir "ui\studio.ico"
    if (Test-Path $ico) { $sc.IconLocation = "$ico,0" }
    $sc.Description = "Студия AI: Генерация, правка фото и апскейл"
    $sc.Save()
    Write-Host "✓ Ярлык создан на Рабочем столе: Студия.lnk" -ForegroundColor Green
}

Write-Host "`n ================================================================" -ForegroundColor Cyan
Write-Host "   ✦ УСТАНОВКА УСПЕШНО ЗАВЕРШЕНА!                                " -ForegroundColor Green
Write-Host "   Для запуска откройте ярлык «Студия» или запустите Studio.ps1  " -ForegroundColor White
Write-Host " ================================================================`n" -ForegroundColor Cyan
