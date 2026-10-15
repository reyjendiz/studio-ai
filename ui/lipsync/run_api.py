"""Run an API-format template ({{name}} placeholders) on a running ComfyUI (:8188); prints time, peak VRAM, outputs.
usage: python run_api.py template.json key=value ...   (numbers are sent as numbers)"""
import json, sys, time, subprocess, threading, urllib.request, urllib.error, uuid

COMFY = "http://127.0.0.1:8188"
v = {}
for kv in sys.argv[2:]:
    k, _, x = kv.partition("=")
    try: v[k] = int(x)
    except ValueError:
        try: v[k] = float(x)
        except ValueError: v[k] = x

def fill(n):
    if isinstance(n, list): return [fill(x) for x in n]
    if isinstance(n, dict): return {k: fill(x) for k, x in n.items()}
    return v[n[2:-2]] if isinstance(n, str) and n.startswith("{{") else n

graph = fill(json.load(open(sys.argv[1], encoding="utf-8")))
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
try: pid = call("/prompt", {"prompt": graph, "client_id": uuid.uuid4().hex})["prompt_id"]
except urllib.error.HTTPError as e: print("REJECTED", e.read().decode()[:2000]); sys.exit(2)
while True:
    time.sleep(1)
    item = call("/history/" + pid).get(pid)
    if item and (item["status"].get("completed") or item["status"]["status_str"] == "error"): break
stop.set()
st = item["status"]
err = next((m[1] for m in st["messages"] if m[0] == "execution_error"), {})
print(json.dumps({"status": st["status_str"], "seconds": round(time.time() - t0, 1), "peak_vram_mib": peak[0],
                  "files": [i["filename"] for o in item["outputs"].values() for i in o.get("images", [])],
                  "error": (err.get("exception_type", "") + ": " + err.get("exception_message", "")[:600] + " @ " + err.get("node_type", "")) if err else None}, ensure_ascii=False))
