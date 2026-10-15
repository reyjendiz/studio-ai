---
name: studio
description: Drive the user's local AI studio (E:\ComfyUI, native Rust app "Студия") with studio-cli, without Python: generate or edit images, upscale 2x, make LTX-2.5 videos (text or photo to video) and lip-sync a photo or video to an audio file, list/open/delete results. Use whenever the user wants an image, video or lip-sync made locally on their GPU, wants this automated, or asks to run Studio from a script or from Claude Code. Also use instead of the older local-image-gen skill when the Rust app is installed.
---

# Studio (Rust) via studio-cli

Read `E:\ComfyUI\CLAUDE.md` first: it lists every command, the timing numbers and the HTTP API. Short version:

```
E:\ComfyUI\app\studio-cli.exe gen --prompt "..." --size 1344x768 --out <project>\assets\hero.jpg
E:\ComfyUI\app\studio-cli.exe upscale --image in.png --out big.png
E:\ComfyUI\app\studio-cli.exe video --prompt "..." --seconds 5 --out clip.mp4          # long: use --no-wait then `wait ID`
E:\ComfyUI\app\studio-cli.exe lipsync --media face.mp4 --audio speech.wav --out synced.mp4   # long: 45-120 s per result second
```

Rules:
1. Run long commands in the background or with a long timeout; poll with `studio-cli job ID` (progress is `pct` and `label`).
2. Check every result with the Read tool (images) before using it; videos: check `state` and file size, and for lip sync the user can score them with `ui\lipsync\score.py`.
3. Ask the user before `delete`, `delete-all` and `engine-stop`. Never run `ui\server.py`.
4. Prompts in English work best; Russian is translated locally. Keep generated files in the user's project folder.
5. If `studio-cli` says the port 8189 is busy, another program holds it: tell the user instead of retrying.
