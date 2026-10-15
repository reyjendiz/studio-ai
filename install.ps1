# ==============================================================================
# Студия AI — Скрипт автоматической установки моделей и компонентов
# ==============================================================================
param(
    [string]$TargetDir = $PSScriptRoot,
    [switch]$SkipZImage,
    [switch]$SkipNoctQ,
    [switch]$SkipUpscalers,
    [switch]$SkipTranslator,
    [switch]$NoShortcut,
    [switch]$LTX,            # video LTX-2.5 (~34 GB); needs a Hugging Face token and the accepted Lightricks/LTX-2.5 license
    [switch]$Lipsync,        # lip sync LatentSync 1.6 (~5.5 GB); open models, no token; needs ffmpeg on PATH
    [string]$HfToken = $env:HF_TOKEN
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
        [string]$Description,
        [string]$Token = ""      # sent to the first host only: curl drops custom Authorization headers on cross-host redirects
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
        $auth = @()
        if ($Token) { $auth = @("-H", "Authorization: Bearer $Token") }
        & curl.exe -L -f --retry 3 --retry-delay 3 -C - -o $part -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64)" @auth $Url
        if ($LASTEXITCODE -eq 0 -and (Test-Path $part)) {
            if (Test-Path $dest) { Remove-Item $dest -Force }
            Move-Item $part $dest -Force
            Write-Host " [✓ ГОТОВО] $Description" -ForegroundColor Green
            return
        }
    }

    if ($Token) { throw "Для загрузки с токеном нужен curl.exe (есть в Windows 10/11)." }

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

# 5. Video LTX-2.5 (GGUF): transformer is open, the rest is gated on Hugging Face
if ($LTX) {
    Write-Host "`n>>> [5/5] Установка видео LTX-2.5 (~34 ГБ)..." -ForegroundColor Cyan
    if (-not $HfToken) {
        $tokFile = Join-Path $env:USERPROFILE ".cache\huggingface\token"
        if (Test-Path $tokFile) { $HfToken = (Get-Content $tokFile -Raw).Trim() }
    }
    if (-not $HfToken) { throw "Нужен токен Hugging Face: -HfToken <токен> (права Read). Сначала примите лицензию на https://huggingface.co/Lightricks/LTX-2.5" }
    $lt = "https://huggingface.co/Lightricks/LTX-2.5/resolve/main"
    Download-ModelFile "https://huggingface.co/Abiray/LTX-2.5-Distilled-GGUF/resolve/main/LTX-2.5-Distilled-Q4_K_M.gguf" "ComfyUI\models\unet\LTX-2.5-Distilled-Q4_K_M.gguf" 15687639488 "LTX-2.5 Distilled Q4_K_M (14.6 ГБ)"
    Download-ModelFile "$lt/text_encoders/gemma4-12b-with-proj-ltx-2.5-comfy-int8-convrot.safetensors" "ComfyUI\models\text_encoders\gemma4-12b-with-proj-ltx-2.5-comfy-int8-convrot.safetensors" 15372969374 "Gemma 4 12B энкодер int8 (14.3 ГБ)" $HfToken
    Download-ModelFile "$lt/vae/ltx-2.5-video-vae-bf16.safetensors" "ComfyUI\models\vae\ltx-2.5-video-vae-bf16.safetensors" 1472223346 "LTX-2.5 видео VAE (1.4 ГБ)" $HfToken
    Download-ModelFile "$lt/vae/ltx-2.5-audio-vae-bf16.safetensors" "ComfyUI\models\vae\ltx-2.5-audio-vae-bf16.safetensors" 364866540 "LTX-2.5 аудио VAE (348 МБ)" $HfToken
    Download-ModelFile "$lt/latent_upscale_models/ltx-2.5-latent-spatial-upscaler-x2-bf16-1.0.safetensors" "ComfyUI\models\latent_upscale_models\ltx-2.5-latent-spatial-upscaler-x2-bf16-1.0.safetensors" 995778752 "Апскейлер латентов x2 (950 МБ)" $HfToken

    # ComfyUI-GGUF at the tested commit + loader.py patch (same change as patches\ComfyUI-GGUF-ltx25-gemma4.patch) + its Python deps
    $node = Join-Path $TargetDir "ComfyUI\custom_nodes\ComfyUI-GGUF"
    if (-not (Test-Path "$node\loader.py")) {
        $commit = "6ea2651e7df66d7585f6ffee804b20e92fb38b8a"
        $tmp = Join-Path $env:TEMP "studio_gguf_$commit"
        Invoke-WebRequest "https://github.com/city96/ComfyUI-GGUF/archive/$commit.zip" -OutFile "$tmp.zip" -UseBasicParsing
        Expand-Archive "$tmp.zip" -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Force (Split-Path $node) | Out-Null
        Move-Item "$tmp\ComfyUI-GGUF-$commit" $node
        Remove-Item "$tmp.zip", $tmp -Recurse -Force
    }
    $loader = Get-Content "$node\loader.py" -Raw
    if ($loader -notmatch '"gemma4"') {
        $loader = $loader.Replace("`r`n", "`n")
        $loader = $loader.Replace('TXT_ARCH_LIST = {"t5", "t5encoder", "llama", "qwen2vl", "qwen3", "qwen3vl", "gemma3"}', 'TXT_ARCH_LIST = {"t5", "t5encoder", "llama", "qwen2vl", "qwen3", "qwen3vl", "gemma3", "gemma4"}')
        $loader = $loader.Replace("VIS_TYPE_LIST = {`"clip-vision`", `"mmproj`"}`n", "VIS_TYPE_LIST = {`"clip-vision`", `"mmproj`"}`nLTXV_BF16_PARAMETERS = {`n    `"audio_embeddings_connector.learnable_registers`",`n    `"keyframes_abs_pos_embedding`",`n    `"video_embeddings_connector.learnable_registers`",`n}`n")
        $loader = $loader.Replace("        # 1D tensors shouldn't be quantized, this is a fix for BF16`n        if len(shape) <= 1 and tensor.tensor_type == gguf.GGMLQuantizationType.BF16:", "        # Raw LTXAV parameters are not loaded through GGMLOps, so their BF16 byte storage must be decoded here.`n        if tensor.tensor_type == gguf.GGMLQuantizationType.BF16 and (len(shape) <= 1 or (arch_str == `"ltxv`" and tensor_name in LTXV_BF16_PARAMETERS)):")
        if ($loader -notmatch 'LTXV_BF16_PARAMETERS in|tensor_name in LTXV_BF16_PARAMETERS') { throw "loader.py имеет неожиданный вид, патч не применён." }
        [IO.File]::WriteAllText("$node\loader.py", $loader, (New-Object Text.UTF8Encoding $false))
        Write-Host " [✓ ГОТОВО] ComfyUI-GGUF пропатчен для Gemma 4 / LTX" -ForegroundColor Green
    }
    $py = Join-Path $TargetDir "python_embeded\python.exe"
    if (Test-Path $py) { & $py -s -m pip install "gguf>=0.13.0" sentencepiece protobuf }
    else { Write-Host " [!] python_embeded не найден: установите gguf, sentencepiece, protobuf вручную" -ForegroundColor Yellow }
}

# 6. Lip sync LatentSync 1.6: UNet + Whisper tiny + Stable Diffusion VAE, the wrapper node at the tested commit, two patched files, Python packages
if ($Lipsync) {
    Write-Host "`n>>> [6/6] Установка липсинка LatentSync 1.6 (~5.5 ГБ)..." -ForegroundColor Cyan
    $ckpt = "ComfyUI\custom_nodes\ComfyUI-LatentSyncWrapper\checkpoints"
    $ls = "https://huggingface.co/ByteDance/LatentSync-1.6/resolve/main"
    $vae = "https://huggingface.co/stabilityai/sd-vae-ft-mse/resolve/main"
    Download-ModelFile "$ls/latentsync_unet.pt" "$ckpt\latentsync_unet.pt" 5072222488 "LatentSync 1.6 UNet (4.7 ГБ)"
    Download-ModelFile "$ls/whisper/tiny.pt" "$ckpt\whisper\tiny.pt" 75572083 "Whisper tiny, аудио-энкодер (72 МБ)"
    Download-ModelFile "$vae/diffusion_pytorch_model.safetensors" "$ckpt\vae\diffusion_pytorch_model.safetensors" 334643276 "Stable Diffusion VAE (319 МБ)"
    Download-ModelFile "$vae/config.json" "$ckpt\vae\config.json" 547 "Конфиг VAE"

    $node = Join-Path $TargetDir "ComfyUI\custom_nodes\ComfyUI-LatentSyncWrapper"
    if (-not (Test-Path "$node\nodes.py")) {
        $commit = "360d5283d7276aee68b4237b1387e594e4ce640e"
        $tmp = Join-Path $env:TEMP "studio_lipsync_$commit"
        Invoke-WebRequest "https://github.com/ShmuelRonen/ComfyUI-LatentSyncWrapper/archive/$commit.zip" -OutFile "$tmp.zip" -UseBasicParsing
        Expand-Archive "$tmp.zip" -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Force $node | Out-Null
        Copy-Item "$tmp\ComfyUI-LatentSyncWrapper-$commit\*" $node -Recurse -Force   # next to the checkpoints just downloaded
        Remove-Item "$tmp.zip", $tmp -Recurse -Force
    }
    # the tested wrapper needs two small fixes for current PyTorch/PyAV (patches\ComfyUI-LatentSyncWrapper-local.patch); the patched files are in this repository
    foreach ($rel in @("nodes.py", "latentsync/utils/util.py")) {
        $path = Join-Path $node ($rel -replace '/', '\')
        if ((Test-Path $path) -and ((Get-Content $path -Raw) -match "local patch")) { continue }
        $text = (Invoke-WebRequest "https://raw.githubusercontent.com/reyjendiz/studio-ai/main/patches/LatentSyncWrapper/$rel" -UseBasicParsing).Content
        if ($text -is [byte[]]) { $text = [Text.Encoding]::UTF8.GetString($text) }
        if ($text -notmatch "local patch") { throw "Получен неожиданный файл вместо патча: $rel" }
        [IO.File]::WriteAllText($path, $text, (New-Object Text.UTF8Encoding $false))
        Write-Host " [✓ ГОТОВО] LatentSync пропатчен: $rel" -ForegroundColor Green
    }
    $py = Join-Path $TargetDir "python_embeded\python.exe"
    $pkgs = "diffusers==0.41.0 omegaconf==2.4.0 einops==0.8.2 mediapipe==1.1.1 face-alignment==1.5.0 decord==0.6.0 ffmpeg-python==0.2.0 soundfile==0.14.0 DeepCache==0.1.1 python_speech_features==0.6 scenedetect==0.5.6.1 kornia==0.8.3 opencv-python==5.0.0.93" -split " "
    if (Test-Path $py) { & $py -s -m pip install @pkgs }
    else { Write-Host " [!] python_embeded не найден: установите вручную pip install $($pkgs -join ' ')" -ForegroundColor Yellow }
    if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) { Write-Host " [!] ffmpeg не найден в PATH: он нужен для липсинка (winget install Gyan.FFmpeg)" -ForegroundColor Yellow }
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
    if (Test-Path $exeApp) { $sc.IconLocation = "$exeApp,0" } elseif (Test-Path $ico) { $sc.IconLocation = "$ico,0" }   # the exe carries the icon
    $sc.Description = "Студия AI: фото, видео и липсинк"
    $sc.Save()
    Write-Host "✓ Ярлык создан на Рабочем столе: Студия.lnk" -ForegroundColor Green
}

Write-Host "`n ================================================================" -ForegroundColor Cyan
Write-Host "   ✦ УСТАНОВКА УСПЕШНО ЗАВЕРШЕНА!                                " -ForegroundColor Green
Write-Host "   Для запуска откройте ярлык «Студия» или запустите Studio.ps1  " -ForegroundColor White
Write-Host " ================================================================`n" -ForegroundColor Cyan
