"""Smoke/benchmark run of the app's workflow templates against a running ComfyUI (:8188). Prints time, peak VRAM and the result.
usage: python bench.py t2v|i2v WIDTH HEIGHT SECONDS [fps] [unet] [clip] [image]"""
import json, sys, time, subprocess, threading, urllib.request, pathlib, uuid

HERE = pathlib.Path(__file__).parent
COMFY = "http://127.0.0.1:8188"
mode, W, H, SEC = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), float(sys.argv[4])
FPS = int(sys.argv[5]) if len(sys.argv) > 5 else 24
UNET = sys.argv[6] if len(sys.argv) > 6 else "LTX-2.5-Distilled-Q4_K_M.gguf"
CLIP = sys.argv[7] if len(sys.argv) > 7 else "gemma4-12b-with-proj-ltx-2.5-Q5_K_M.gguf"
IMAGE = sys.argv[8] if len(sys.argv) > 8 else "example.png"
import os
PROMPT = os.environ.get("LTX_PROMPT") or {"t2v": "A red fox walks through a snowy pine forest at dawn, the camera slowly tracks alongside, soft golden light, cinematic, ambient forest sounds.",
          "i2v": "The subject slowly turns and looks at the camera, gentle natural motion, the camera pushes in slightly, soft light."}[mode]


def fill(n, v):
    if isinstance(n, list): return [fill(x, v) for x in n]
    if isinstance(n, dict): return {k: fill(x, v) for k, x in n.items()}
    return v[n[2:-2]] if isinstance(n, str) and n.startswith("{{") else n


length = round(SEC * FPS / 8) * 8 + 1
g = fill(json.loads((HERE / f"ltx_{mode}_api.json").read_text()), dict(
    unet=UNET, clip=CLIP, vae="ltx-2.5-video-vae-bf16.safetensors", audio_vae="ltx-2.5-audio-vae-bf16.safetensors",
    upscaler="ltx-2.5-latent-spatial-upscaler-x2-bf16-1.0.safetensors", prompt=PROMPT, negative="pc game, console game, video game, cartoon, childish, ugly",
    fps=FPS, half_width=W // 2, half_height=H // 2, length=length, seed=int(time.time()) % 2**31, cfg=1, image=IMAGE))
if CLIP.endswith(".gguf"): g["2"] = {"class_type": "CLIPLoaderGGUF", "inputs": {"clip_name": CLIP, "type": "ltxv"}}

peak, stop = [0], threading.Event()
def sample():
    while not stop.is_set():
        try: peak[0] = max(peak[0], int(subprocess.check_output(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"], text=True).split()[0]))
        except Exception: pass
        time.sleep(0.5)
threading.Thread(target=sample, daemon=True).start()

def call(path, body=None):
    req = urllib.request.Request(COMFY + path, json.dumps(body).encode() if body is not None else None, {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req))

t0 = time.time()
try:
    pid = call("/prompt", {"prompt": g, "client_id": uuid.uuid4().hex})["prompt_id"]
except urllib.error.HTTPError as e:
    print("REJECTED", e.read().decode()[:1500]); sys.exit(2)
while True:
    time.sleep(1)
    item = call("/history/" + pid).get(pid)
    if item and item["status"].get("completed") or item and item["status"]["status_str"] == "error":
        break
stop.set()
st = item["status"]
msg = next((m[1] for m in st["messages"] if m[0] == "execution_error"), {})
files = [i["filename"] for o in item["outputs"].values() for i in o.get("images", [])]
print(json.dumps({"mode": mode, "size": f"{W}x{H}", "frames": length, "sec": SEC, "fps": FPS, "unet": UNET, "clip": CLIP, "status": st["status_str"],
                  "seconds": round(time.time() - t0, 1), "peak_vram_mib": peak[0], "files": files,
                  "error": (msg.get("exception_type", "") + ": " + msg.get("exception_message", "")[:300]) if msg else None}, ensure_ascii=False))
