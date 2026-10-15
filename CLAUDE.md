# Студия (Studio): local AI studio, native Rust app

Windows app that runs ComfyUI (in `ComfyUI\`, with `python_embeded\`) as a background engine for: image generation, photo edit,
2x upscale, LTX-2.5 video (text/photo to video) and LatentSync lip sync (photo or video + audio). The app itself is Rust and
needs no Python; only ComfyUI is Python.

## Controlling Studio from Claude Code (automation)

Use `studio-cli` (in `app\studio-cli.exe`, built from `studio\`). Every command prints JSON on stdout and progress on stderr.
It uses the open Studio window if there is one, otherwise it starts a hidden server that stops itself (and ComfyUI) when idle.

```
studio-cli status | models | jobs | job ID | wait ID | cancel ID | cancel-all | engine-stop | shutdown
studio-cli gen      --prompt "..." [--model NAME] [--size 1344x768] [--count 1-4] [--seed N] [--neg ".."] [--photo IN.png --strength 0.9] [--out FILE.jpg]
studio-cli edit     --prompt "..." --photo A.png [--photo B.png] [--model NAME] [--count N] [--strength 0.85] [--out FILE]
studio-cli upscale  --image IN.png [--kind photo|anime] [--out FILE]
studio-cli video    --prompt "..." [--image FIRST_FRAME.png] [--size 1024x576] [--seconds 5] [--fps 24] [--no-audio] [--out FILE.mp4]
studio-cli lipsync  --media FACE.mp4|FACE.png --audio SPEECH.wav [--fast] [--loop] [--out FILE.mp4]
studio-cli list photos|videos|lipsync [--limit N]    open FILE    delete FILE...    delete-all photos|videos|lipsync --yes
studio-cli translate "русский текст"
```

- `--out`: one result is copied to FILE, several to FILE_1, FILE_2...; images are converted by extension (.jpg .png .webp).
- `--no-wait` returns the job id immediately; then `wait ID`. Several jobs queue and run one at a time (one GPU).
- Russian prompts are translated to English locally (`--no-translate` to skip). Prompts are best written in English anyway.
- Timing on the RTX 4080 SUPER 16 GB: image 5-50 s; first call after a pause +1 min (ComfyUI starts); video 3-5 min, but the first one after ComfyUI
  starts takes 6-10 min (loads ~30 GB); lip sync about 45 s (`--fast`) or 2 min (default) per second of result, max 60 s. Use long timeouts or `--no-wait`.
- Sizes for video are multiples of 64 (256-1920), 1-10 s. Lip sync needs a clear frontal face and `ffmpeg` on PATH.
- Look at results (Read tool) before using them. Save into the user's project, not into `ComfyUI\output`.
- `delete`, `delete-all` and `engine-stop` change the user's disk or stop their running jobs: ask first. `delete-all` requires `--yes`.
- Do not start `ui\server.py` (the old Python UI server): it uses the same port 8189.

### HTTP API (127.0.0.1:8189, JSON; hosted by the window or `studio-cli serve`)

Local programs only: requests with an `Origin` header, or POSTs that are not `Content-Type: application/json`, get 403.

| Method and path | Meaning |
|---|---|
| `GET /api/status`, `GET /api/models` | state; installed image models, LTX files, lipsync/ffmpeg availability |
| `POST /api/jobs` | submit `{"kind":"image"\|"edit"\|"upscale"\|"video"\|"lipsync", ...}` (fields as in `src/jobs.rs`), returns `{"id"}` |
| `GET /api/jobs`, `GET /api/jobs/{id}` | state `queued/running/done/error/cancelled`, `label`, `pct`, `files`, `error` |
| `POST /api/jobs/{id}/cancel`, `POST /api/jobs/cancel-all` | cancel (lip sync cancel restarts ComfyUI) |
| `GET /api/results/{photos\|videos\|lipsync}` | finished files with their saved settings |
| `POST /api/results/delete` `{"path"}`, `POST /api/results/delete-all` `{"kind"}` | delete (only inside `ComfyUI\output`) |
| `POST /api/open` `{"path"}` | open a result in the Windows default app |
| `POST /api/translate` `{"text"}` | RU to EN |
| `POST /api/engine/stop`, `POST /api/shutdown` | stop ComfyUI; stop the headless server (the window refuses shutdown) |

## Code map (`studio\`, Rust 2024, egui/eframe 0.36)

- `src\engine.rs` ComfyUI process lifecycle (start on demand, stop when idle: 60 s, 300 s after video/lip sync), websocket progress, upload.
- `src\jobs.rs` the single job queue used by window, API and CLI; `src\workflows.rs` ComfyUI graphs (templates in `ui\ltx`, `ui\lipsync`).
- `src\models.rs` model files and registry (`ui\models.json`), `src\results.rs` output folders, `src\media.rs` ffmpeg and Windows shell.
- `src\translate.rs` offline RU-EN (Marian on CPU with candle; reads `translate\opus-mt-ru-en`).
- `src\api.rs` local API, `src\bin\cli.rs` studio-cli, `src\bin\gui.rs` window, `src\ui\` interface.
- UI rules: all user-visible text goes through `t("русский текст")` and needs a row in `src\ui\i18n_table.rs` (Ukrainian, English); a unit test fails otherwise.
  Controls are pill-shaped (`theme::PILL`), cards use `theme::SURFACE_R`; the header is one `HEADER_H` line; accent colour follows the tab
  (photo chartreuse, video teal, lip sync amber); card opacity and language are in Settings. Font: Manrope (bundled, OFL).
- Build: `studio\build.bat` (release build, copies `Studio.exe` and `studio-cli.exe` into `app\`). Tests: `cargo test --release`.
  Layout check: `cargo test --release --test layout -- --ignored` writes screenshots to `logs\ui\`; look at them after UI changes.
- Round icon: `studio\tools\make_icon.py` (background made with Studio itself).

## Known limits

LatentSync needs ~16 GB and is slow here; one face, frontal; no cartoon faces. LTX video max tested 1920x1088 x 5 s and 1280x704 x 10 s.
The Gemma GGUF text encoder follows prompts poorly; the int8 one is used. ComfyUI-GGUF and LatentSyncWrapper carry local patches (see README).
