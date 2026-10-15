"""UI server on :8189. Serves index.html, proxies the ComfyUI API on :8188, and manages model files (/api/models*).
ComfyUI is started on the first generation and stopped again once nothing has been generating for IDLE seconds."""
import os, shutil, http.server, urllib.request, urllib.error, urllib.parse, pathlib, json, re, threading, uuid, subprocess, time, mimetypes, gc

COMFY = "http://127.0.0.1:8188"
HERE = pathlib.Path(__file__).parent
IDLE_VIDEO = 300     # a video run leaves ~30 GB of weights loaded; reloading them takes 5-10 min, so keep ComfyUI a bit longer after one
IDLE = 60            # seconds without generation before ComfyUI (and the translation model) are released
START_TIMEOUT = 240  # how long a request may wait for ComfyUI to finish loading
ON_DEMAND = ("/prompt", "/upload/image")   # the only calls that start ComfyUI; everything else works (or fails quietly) without it
COMFY_DIR = HERE.parent / "ComfyUI"
MODELS = COMFY_DIR / "models"
OUTPUT_DIR = COMFY_DIR / "output"
TEMP_DIR = COMFY_DIR / "temp"
INPUT_DIR = COMFY_DIR / "input"
IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".webp"}
REGISTRY = HERE / "models.json"   # file name -> family; the page picks the workflow from it
PROXIED = ("/prompt", "/history", "/queue", "/interrupt", "/upload/image", "/free")  # shortcut: whitelist, add paths here if the page needs more
FOLDER = {"zimage": "diffusion_models", "klein": "diffusion_models", "qwen21": "diffusion_models", "sdxl": "checkpoints"}
NAME_RE = re.compile(r"^[\w\-. ()\[\]]{1,120}\.safetensors$")   # .safetensors only: pickle formats (.ckpt/.pt) can run code on load
UA = {"User-Agent": "Mozilla/5.0"}
jobs, lock = {}, threading.Lock()

# --- local RU->EN translation (Helsinki-NLP/opus-mt-ru-en on CPU, so it never competes with the image model for VRAM) ---
TR_DIR = HERE.parent / "translate" / "opus-mt-ru-en"
CYR = re.compile(r"[А-Яа-яЁё]")
SENT = re.compile(r"(?<=[.!?])\s+|\n+")
# photo/art terms the generic model mangles ("глубина резкости" -> "depth of sharpness"); replaced before translation
GLOSSARY = [(re.compile(p, re.I), e) for p, e in [
    (r"(?:малая|небольшая|малой|небольшой) глубин\w+ резкости", "shallow depth of field"),
    (r"глубин\w+ резкости", "depth of field"),
    (r"боке", "bokeh"),
    (r"пл[её]ночн\w+ зерн\w+", "film grain"),
    (r"золот\w+ час\w*", "golden hour"),
    (r"крупн\w+ план\w*", "close-up shot"),
    (r"общ\w+ план\w*", "wide shot"),
    (r"фотореалистичн\w+", "photorealistic"),
    (r"кинематографичн\w+", "cinematic"),
    (r"мягк\w+ (?:свет\w*|освещени\w+)", "soft light"),
    (r"естественн\w+ текстур\w+ кожи", "natural skin texture"),
    (r"резк\w+ фокус\w*", "sharp focus"),
]]
_tr, tr_lock = {}, threading.Lock()
tr_last = [0.0]


# --- ComfyUI lifecycle ---
comfy = {"proc": None, "last": time.time()}
comfy_lock = threading.Lock()


def comfy_up():
    try:
        urllib.request.urlopen(COMFY + "/system_stats", timeout=1).close()
        return True
    except OSError:
        return False


def ensure_comfy():
    """Starts ComfyUI if it is not answering and waits until it is; raises if it never comes up."""
    with comfy_lock:
        comfy["last"] = time.time()
        if comfy_up():
            return
        p = comfy["proc"]
        if p is None or p.poll() is not None:
            (HERE.parent / "logs").mkdir(exist_ok=True)
            log = open(HERE.parent / "logs" / "comfyui.out.log", "wb")
            comfy["proc"] = p = subprocess.Popen(
                [str(HERE.parent / "python_embeded" / "python.exe"), "-s", str(COMFY_DIR / "main.py"), "--windows-standalone-build", "--disable-auto-launch"],
                cwd=HERE.parent, stdout=log, stderr=subprocess.STDOUT, creationflags=0x08000000,
                env={**os.environ, "PYTHONIOENCODING": "utf-8"})   # custom nodes print non-cp1251 characters; with a cp1251 log file that kills ComfyUI's worker thread   # CREATE_NO_WINDOW
        for _ in range(START_TIMEOUT * 2):
            if comfy_up():
                comfy["last"] = time.time()
                return
            if p.poll() is not None:
                raise OSError("ComfyUI закрылся при запуске, смотри logs\\comfyui.out.log")
            time.sleep(0.5)
        raise OSError("ComfyUI не успел запуститься")


def busy():
    try:
        q = json.load(urllib.request.urlopen(COMFY + "/queue", timeout=2))
        return bool(q.get("queue_running") or q.get("queue_pending"))
    except (OSError, ValueError):
        return False


def idle_watch():
    """Stops the ComfyUI we started once nothing has been generating for IDLE seconds; also drops the translation model."""
    while True:
        time.sleep(5)
        now = time.time()
        p = comfy["proc"]
        if p is not None and p.poll() is None and not comfy_lock.locked():
            if busy():
                comfy["last"] = now
            elif now - comfy["last"] > (IDLE_VIDEO if LTX["pid"] else IDLE):
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True, creationflags=0x08000000)
                comfy["proc"] = None
        with tr_lock:
            if _tr and now - tr_last[0] > IDLE:
                _tr.clear()
                gc.collect()


def translate(texts):
    """Translates only strings that contain Cyrillic; everything else passes through unchanged."""
    if sum(len(t) for t in texts) > 4000:
        raise ValueError("слишком длинный текст для перевода")
    with tr_lock:
        tr_last[0] = time.time()
        if not _tr:   # first call loads ~300 MB; later calls take well under a second
            import torch
            from transformers import MarianMTModel, MarianTokenizer
            has_st = (TR_DIR / "model.safetensors").exists()
            _tr["tok"] = MarianTokenizer.from_pretrained(str(TR_DIR))
            _tr["model"] = MarianMTModel.from_pretrained(str(TR_DIR), use_safetensors=has_st).eval()
            _tr["torch"] = torch
        pieces, owners, terms = [], [], {}   # sentences of all texts go through the model as one batch
        for i, t in enumerate(texts):
            if CYR.search(t):
                terms[i] = []

                def hold(e, i=i):   # the model garbles English inside Russian ("dept of Field"), so glossary terms travel as opaque tokens
                    terms[i].append(e)
                    return "qz%dzq" % (len(terms[i]) - 1)
                for g, e in GLOSSARY:
                    t = g.sub(lambda m, e=e: hold(e), t)
                for s in SENT.split(t):
                    if s.strip():
                        pieces.append(s.strip())
                        owners.append(i)
        out = list(texts)
        if pieces:
            tok, model, torch = _tr["tok"], _tr["model"], _tr["torch"]
            with torch.no_grad():
                ids = model.generate(**tok(pieces, return_tensors="pt", padding=True, truncation=True, max_length=256), num_beams=4, max_new_tokens=256)
            done = {}
            for i, s in zip(owners, tok.batch_decode(ids, skip_special_tokens=True)):
                done.setdefault(i, []).append(s)
            for i, parts in done.items():
                text = " ".join(parts)
                for n, e in enumerate(terms[i]):
                    text, hit = re.subn(r"qz%dzq" % n, e, text, flags=re.I)
                    if not hit:   # the model dropped the token: keep the term anyway
                        text += ", " + e
                out[i] = text
        return out


def read_registry():
    try:
        return json.loads(REGISTRY.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def set_registry(name, family):
    with lock:
        reg = read_registry()
        if family is None:
            reg.pop(name, None)
        else:
            reg[name] = family
        REGISTRY.write_text(json.dumps(reg, ensure_ascii=False, indent=1), encoding="utf-8")


def guess_family(folder, name, reg):
    if name in reg:
        return reg[name]
    low = name.lower()
    if "klein" in low:
        return "klein"
    if "noct" in low or "qwen" in low:
        return "qwen21"
    return "sdxl" if folder == "checkpoints" else "zimage"


def list_models():
    reg, out = read_registry(), []
    for folder in ("diffusion_models", "checkpoints"):
        for p in sorted((MODELS / folder).glob("*.safetensors"), key=lambda p: -p.stat().st_mtime):   # newest first = default pick
            out.append({"name": p.name, "folder": folder, "family": guess_family(folder, p.name, reg), "size": p.stat().st_size})
    return out


def dest(name, family):
    if family not in FOLDER or not NAME_RE.match(name or ""):
        raise ValueError("недопустимое имя файла или тип модели (нужен .safetensors)")
    return MODELS / FOLDER[family] / name


def valid_safetensors(path):
    """A real safetensors file starts with an 8-byte header length and a JSON header; this rejects HTML error pages and renamed junk."""
    try:
        with open(path, "rb") as f:
            n = int.from_bytes(f.read(8), "little")
            if not 2 <= n <= 50_000_000:
                return False
            json.loads(f.read(n))
        return True
    except (OSError, ValueError):
        return False


def finish(part, final, name, family):
    if not valid_safetensors(part):
        part.unlink(missing_ok=True)
        raise ValueError("файл не похож на .safetensors (повреждён или это не модель)")
    part.replace(final)
    set_registry(name, family)


def download_job(jid, url, name, family):
    job = jobs[jid]
    final = dest(name, family)
    part = final.with_suffix(".part")
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60) as r, open(part, "wb") as f:
            job["total"] = int(r.headers.get("Content-Length") or 0)
            while chunk := r.read(1 << 20):
                f.write(chunk)
                job["done"] += len(chunk)
        finish(part, final, name, family)
        job["state"] = "done"
    except Exception as e:  # report any failure to the page instead of dying silently in the thread
        part.unlink(missing_ok=True)
        job.update(state="error", error=str(e))


def list_images(limit=100):
    if not OUTPUT_DIR.is_dir():
        return []
    items = []
    try:
        for p in OUTPUT_DIR.iterdir():
            if p.is_file() and p.suffix.lower() in IMAGE_EXTS:
                items.append({
                    "filename": p.name,
                    "subfolder": "",
                    "type": "output",
                    "mtime": p.stat().st_mtime
                })
            elif p.is_dir() and not p.name.startswith("."):
                for sub in p.iterdir():
                    if sub.is_file() and sub.suffix.lower() in IMAGE_EXTS:
                        items.append({
                            "filename": sub.name,
                            "subfolder": p.name,
                            "type": "output",
                            "mtime": sub.stat().st_mtime
                        })
    except Exception:
        pass
    items.sort(key=lambda x: x["mtime"], reverse=True)
    return items[:limit]


def delete_image_file(filename, subfolder="", folder_type="output"):
    if not filename or not isinstance(filename, str):
        raise ValueError("не указано имя файла")
    if "/" in filename or "\\" in filename:
        raise ValueError("недопустимое имя файла")
    base = TEMP_DIR if folder_type == "temp" else (INPUT_DIR if folder_type == "input" else OUTPUT_DIR)
    target = (base / (subfolder or "") / filename).resolve()
    base_resolved = base.resolve()
    if not target.is_relative_to(base_resolved):
        raise ValueError("недопустимый путь к файлу")
    if not target.is_file():
        raise ValueError("файл не найден на диске")
    try:
        target.unlink()
    except PermissionError:
        raise ValueError("файл занят другим процессом")
    return True


# --- LTX-2.5 video: file discovery, ComfyUI websocket progress, history next to the files ---
LTX_DIR = HERE / "ltx"
VIDEO_DIR = OUTPUT_DIR / "video"
LTX = {"cid": uuid.uuid4().hex, "pid": None, "node": None, "nodes": {}, "state": "idle", "error": None}   # progress of the last job, filled by the websocket thread
STATIC_EXT = {".js": "text/javascript", ".css": "text/css", ".json": "application/json"}


def ltx_event(m):
    t, d = m.get("type"), m.get("data") or {}
    if t == "execution_start":
        LTX.update(pid=d.get("prompt_id"), state="running", error=None, node=None, nodes={})
    elif t == "progress_state":   # ComfyUI >= 0.3x reports sampler steps this way
        if d.get("prompt_id") and d["prompt_id"] != LTX["pid"]:
            LTX.update(pid=d["prompt_id"], state="running", error=None, nodes={})
        for nid, n in (d.get("nodes") or {}).items():
            LTX["nodes"][str(nid)] = [n.get("value", 0), n.get("max", 0)]
            if n.get("state") == "running":
                LTX["node"] = str(nid)
    elif t in ("executing", "progress"):
        if d.get("prompt_id") and d["prompt_id"] != LTX["pid"]:   # the socket may connect after execution_start (ComfyUI was just started)
            LTX.update(pid=d["prompt_id"], state="running", error=None, nodes={})
        if t == "executing":
            LTX["node"] = d.get("node")
        else:
            LTX["nodes"][str(d.get("node"))] = [d.get("value", 0), d.get("max", 0)]
    elif t == "execution_success":
        LTX["state"] = "done"
    elif t == "execution_error":
        LTX.update(state="error", error={"type": d.get("exception_type", ""), "message": d.get("exception_message", ""), "node": d.get("node_type", "")})
    elif t == "execution_interrupted":
        LTX["state"] = "interrupted"


def ltx_ws():
    """Listens to ComfyUI's websocket (reconnects whenever ComfyUI is started or stopped); the page polls /api/ltx/progress."""
    import asyncio, aiohttp

    async def run():
        while True:
            if comfy_up():
                try:
                    async with aiohttp.ClientSession() as s, s.ws_connect(COMFY.replace("http", "ws") + "/ws?clientId=" + LTX["cid"], heartbeat=30) as ws:
                        async for m in ws:
                            if m.type == aiohttp.WSMsgType.TEXT:
                                ltx_event(json.loads(m.data))
                except Exception:
                    pass
            await asyncio.sleep(0.5)
    asyncio.run(run())


def ltx_files():
    def names(folder, *parts):
        d = MODELS / folder
        return sorted(p.name for p in d.glob("*") if p.is_file() and all(x in p.name.lower() for x in parts) and p.suffix in (".gguf", ".safetensors")) if d.is_dir() else []
    return {
        "unet": sorted(set(names("unet", "ltx") + names("diffusion_models", "ltx")), key=str.lower),
        "clip": names("text_encoders", "gemma4"),
        "vae": names("vae", "ltx", "video"),
        "audio_vae": names("vae", "ltx", "audio"),
        "upscaler": names("latent_upscale_models", "ltx", "spatial"),
    }


def ltx_history(limit=200, sub="video", prefix="ltx"):
    d = OUTPUT_DIR / sub
    out = []
    for p in d.glob(prefix + "*.mp4") if d.is_dir() else []:
        meta = {}
        try:
            meta = json.loads(p.with_suffix(".json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            pass
        out.append({"filename": p.name, "subfolder": sub, "type": "output", "mtime": p.stat().st_mtime, "size": p.stat().st_size, "meta": meta})
    out.sort(key=lambda x: -x["mtime"])
    return out[:limit]


def ltx_file(name):
    if not re.fullmatch(r"(ltx|ls)[\w\-. ]{1,100}\.mp4", name or ""):
        raise ValueError("недопустимое имя файла")
    d = (OUTPUT_DIR / ("lipsync" if name.startswith("ls") else "video")).resolve()
    f = (d / name).resolve()
    if f.parent != d or not f.is_file():
        raise ValueError("файл не найден")
    return f


def serve_file(h, f, ctype):
    """Whole file or one Range slice: <video> needs ranges to seek."""
    size, start, end = f.stat().st_size, 0, f.stat().st_size - 1
    m = re.fullmatch(r"bytes=(\d*)-(\d*)", h.headers.get("Range", ""))
    if m and (m[1] or m[2]):
        start = int(m[1]) if m[1] else max(0, size - int(m[2]))
        end = min(int(m[2]), size - 1) if m[1] and m[2] else end
    h.send_response(206 if m else 200)
    h.send_header("Content-Type", ctype)
    h.send_header("Accept-Ranges", "bytes")
    h.send_header("Content-Length", str(end - start + 1))
    if m:
        h.send_header("Content-Range", "bytes %d-%d/%d" % (start, end, size))
    h.end_headers()
    with open(f, "rb") as fh:
        fh.seek(start)
        left = end - start + 1
        while left > 0 and (chunk := fh.read(min(1 << 20, left))):
            h.wfile.write(chunk)
            left -= len(chunk)


# --- lip sync (LatentSync): media prepared with ffmpeg (the model assumes 25 fps) and progress read from ComfyUI's log ---
LIPSYNC_DIR = HERE / "lipsync"
STEP_RE = re.compile(r"Doing inference\.\.\.:\s*\d+%\|[^|]*\|\s*(\d+)/(\d+)")
FFMPEG = {
    "video": (["-an", "-vf", "fps=25,scale=min(1920\\,iw):-2", "-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p"], ".mp4", 4 << 30),
    "audio": (["-vn", "-ar", "44100", "-c:a", "pcm_s16le"], ".wav", 1 << 29),
}


def lipsync_progress(since=0):
    """Latest 'Doing inference n/N' written to ComfyUI's log after byte offset `since` (so a finished earlier job is not mistaken for this one)."""
    try:
        with open(HERE.parent / "logs" / "comfyui.out.log", "rb") as f:
            size = f.seek(0, 2)
            f.seek(max(0, since if since <= size else 0))   # the log restarts when ComfyUI does
            tail = f.read().decode("utf-8", "replace")[-16384:]
    except OSError:
        return {"step": 0, "total": 0, "size": 0}
    m = STEP_RE.findall(tail)
    return {"step": int(m[-1][0]) if m else 0, "total": int(m[-1][1]) if m else 0, "size": size}


def hard_cancel():
    """LatentSync is one long node that ComfyUI's /interrupt cannot stop, so the ComfyUI we started is restarted instead (it comes back on the next request)."""
    p = comfy["proc"]
    if p is None or p.poll() is not None:
        return False
    subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True, creationflags=0x08000000)
    comfy["proc"] = None
    return True


def prepare_media(h, kind):
    """Saves the upload, converts it with ffmpeg into ComfyUI's input folder and returns the new name and its duration."""
    if kind not in FFMPEG or not shutil.which("ffmpeg"):
        raise ValueError("ffmpeg не найден: установите его (winget install Gyan.FFmpeg) и перезапустите Студию" if kind in FFMPEG else "неизвестный тип")
    args, ext, limit = FFMPEG[kind]
    left = int(h.headers.get("Content-Length") or 0)
    if not 0 < left <= limit:
        raise ValueError("файл пустой или слишком большой")
    TEMP_DIR.mkdir(parents=True, exist_ok=True)
    src = TEMP_DIR / ("up_" + uuid.uuid4().hex)
    dst = INPUT_DIR / ("ls_" + uuid.uuid4().hex[:10] + ext)
    try:
        with open(src, "wb") as f:
            while left > 0:
                chunk = h.rfile.read(min(1 << 20, left))
                if not chunk:
                    raise ValueError("загрузка прервана")
                f.write(chunk)
                left -= len(chunk)
        r = subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(src), *args, str(dst)], capture_output=True, text=True, timeout=900, creationflags=0x08000000)
        if r.returncode or not dst.is_file():
            dst.unlink(missing_ok=True)
            raise ValueError("не удалось прочитать файл: " + (r.stderr.strip().splitlines() or ["неизвестный формат"])[-1][:200])
        sec = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(dst)], capture_output=True, text=True, creationflags=0x08000000).stdout.strip() or 0)
        return {"name": dst.name, "seconds": sec}
    finally:
        src.unlink(missing_ok=True)


class H(http.server.BaseHTTPRequestHandler):
    def _send(self, code, body, ctype="application/json"):
        if isinstance(body, (dict, list)):
            body = json.dumps(body, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _json(self):
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}")

    def _proxy(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else None
        req = urllib.request.Request(COMFY + self.path, data=body, method=self.command)
        req.add_header("Content-Type", self.headers.get("Content-Type", "application/json"))
        try:
            if self.path.startswith(ON_DEMAND):
                ensure_comfy()
            comfy["last"] = time.time()   # polling /history during a generation keeps ComfyUI alive
            with urllib.request.urlopen(req, timeout=600) as r:
                self._send(r.status, r.read(), r.headers.get("Content-Type", "application/octet-stream"))
        except urllib.error.HTTPError as e:
            self._send(e.code, e.read())
        except OSError as e:
            self._send(502, {"error": "ComfyUI not reachable: %s" % e})

    def _upload(self, q):
        name, family = q.get("name", [""])[0], q.get("family", [""])[0]
        final = dest(name, family)
        if final.exists():
            return self._send(409, {"error": "файл с таким именем уже есть"})
        part, left = final.with_suffix(".part"), int(self.headers.get("Content-Length") or 0)
        try:
            with open(part, "wb") as f:
                while left > 0:
                    chunk = self.rfile.read(min(1 << 20, left))
                    if not chunk:
                        raise ValueError("загрузка прервана")
                    f.write(chunk)
                    left -= len(chunk)
            finish(part, final, name, family)
        except Exception:
            part.unlink(missing_ok=True)
            raise
        self._send(200, {"ok": True})

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        if u.path == "/api/models":
            return self._send(200, list_models())
        if u.path == "/api/models/jobs":
            return self._send(200, jobs)
        if u.path == "/api/images":
            return self._send(200, list_images())
        if u.path == "/api/ltx/models":
            return self._send(200, {"files": ltx_files(), "comfy": comfy_up()})
        if u.path == "/api/ltx/progress":
            return self._send(200, {k: v for k, v in LTX.items()} | {"comfy": comfy_up()})
        if u.path == "/api/ltx/history":
            return self._send(200, ltx_history())
        if u.path == "/api/lipsync/history":
            return self._send(200, ltx_history(sub="lipsync", prefix="ls"))
        if u.path == "/api/lipsync/progress":
            return self._send(200, lipsync_progress(int(urllib.parse.parse_qs(u.query).get("since", ["0"])[0] or 0)))
        if u.path.startswith(("/ltx/", "/lipsync/")):   # the lazily loaded tabs: scripts, styles and workflow templates
            base = LTX_DIR if u.path.startswith("/ltx/") else LIPSYNC_DIR
            f = (base / urllib.parse.unquote(u.path.split("/", 2)[2])).resolve()
            if f.parent == base.resolve() and f.suffix in STATIC_EXT and f.is_file():
                return self._send(200, f.read_bytes(), STATIC_EXT[f.suffix] + "; charset=utf-8")
            return self._send(404, {"error": "not found"})
        if u.path == "/view":   # straight from disk, so browsing the gallery never wakes ComfyUI
            q = urllib.parse.parse_qs(u.query)
            name, sub, kind = (q.get(k, [""])[0] for k in ("filename", "subfolder", "type"))
            base = {"temp": TEMP_DIR, "input": INPUT_DIR}.get(kind, OUTPUT_DIR).resolve()
            f = (base / sub / name).resolve()
            if name and "/" not in name and "\\" not in name and f.is_relative_to(base) and f.is_file():
                if f.suffix.lower() == ".mp4":
                    return serve_file(self, f, "video/mp4")
                return self._send(200, f.read_bytes(), mimetypes.guess_type(name)[0] or "application/octet-stream")
            return self._send(404, {"error": "not found"})
        if u.path.startswith("/art/"):   # design illustrations; resolve() blocks ../ escapes
            art = (HERE / "art").resolve()
            f = (art / urllib.parse.unquote(u.path[5:])).resolve()
            if f.parent == art and f.is_file():
                return self._send(200, f.read_bytes(), "image/jpeg" if f.suffix == ".jpg" else "image/png")
            return self._send(404, {"error": "not found"})
        if self.path.startswith(PROXIED):
            return self._proxy()
        self._send(200, (HERE / "index.html").read_bytes(), "text/html; charset=utf-8")

    def do_POST(self):
        u = urllib.parse.urlparse(self.path)
        try:
            if u.path == "/api/models/upload":
                return self._upload(urllib.parse.parse_qs(u.query))
            if u.path == "/api/models/download":
                d = self._json()
                if not str(d.get("url", "")).startswith("https://"):
                    raise ValueError("нужна ссылка https://")
                dest(d.get("name"), d.get("family"))   # validate before starting the thread
                jid = uuid.uuid4().hex[:8]
                jobs[jid] = {"name": d["name"], "done": 0, "total": 0, "state": "running", "error": ""}
                threading.Thread(target=download_job, args=(jid, d["url"], d["name"], d["family"]), daemon=True).start()
                return self._send(200, {"id": jid})
            if u.path == "/api/translate":
                texts = self._json().get("texts")
                if not isinstance(texts, list) or not all(isinstance(t, str) for t in texts):
                    raise ValueError("нужен список строк texts")
                return self._send(200, {"texts": translate(texts)})
            if u.path == "/api/models/delete":
                name = self._json().get("name", "")
                matches = [p for f in FOLDER.values() for p in [MODELS / f / name] if NAME_RE.match(name) and p.is_file()]
                if not matches:
                    raise ValueError("такого файла нет")
                matches[0].unlink()
                set_registry(name, None)
                return self._send(200, {"ok": True})
            if u.path == "/api/lipsync/cancel":
                return self._send(200, {"killed": hard_cancel()})
            if u.path == "/api/lipsync/prepare":
                return self._send(200, prepare_media(self, urllib.parse.parse_qs(u.query).get("kind", [""])[0]))
            if u.path == "/api/ltx/meta":   # settings of a finished video, stored next to it
                d = self._json()
                ltx_file(d.get("filename")).with_suffix(".json").write_text(json.dumps(d.get("meta") or {}, ensure_ascii=False, indent=1), encoding="utf-8")
                return self._send(200, {"ok": True})
            if u.path == "/api/ltx/open-folder":
                f = ltx_file(self._json().get("filename"))
                subprocess.Popen(["explorer", "/select,", str(f)], creationflags=0x08000000)
                return self._send(200, {"ok": True})
            if u.path == "/api/ltx/delete":
                f = ltx_file(self._json().get("filename"))
                f.unlink()
                f.with_suffix(".json").unlink(missing_ok=True)
                return self._send(200, {"ok": True})
            if u.path == "/api/image/delete":
                d = self._json()
                filename = str(d.get("filename", "")).strip()
                subfolder = str(d.get("subfolder", "")).strip()
                folder_type = str(d.get("type", "output")).strip()
                delete_image_file(filename, subfolder, folder_type)
                return self._send(200, {"ok": True})
        except ValueError as e:
            return self._send(400, {"error": str(e)})
        except Exception as e:   # includes a missing translation model / library: the page shows the message
            return self._send(500, {"error": str(e)})
        if self.path.startswith(PROXIED):
            return self._proxy()
        self._send(404, {"error": "not found"})

    def log_message(self, *a):
        pass


if __name__ == "__main__":
    print("UI: http://127.0.0.1:8189")
    threading.Thread(target=idle_watch, daemon=True).start()
    threading.Thread(target=ltx_ws, daemon=True).start()
    http.server.ThreadingHTTPServer(("127.0.0.1", 8189), H).serve_forever()
