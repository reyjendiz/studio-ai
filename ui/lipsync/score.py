"""SyncNet confidence + AV offset for a video (LatentSync's eval code). usage: python score.py video.mp4 [...]
Higher confidence = better lip sync (real footage is typically ~6-9); offset 0 = in sync. Needs the GPU, so run it when ComfyUI is idle."""
import os, sys, shutil, tempfile
NODE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "ComfyUI", "custom_nodes", "ComfyUI-LatentSyncWrapper"))
VIDEOS = [os.path.abspath(v) for v in sys.argv[1:]]   # before chdir
os.chdir(NODE); sys.path.insert(0, NODE)
import torch
from statistics import fmean
from eval.syncnet import SyncNetEval
from eval.syncnet_detect import SyncNetDetector

dev = "cuda" if torch.cuda.is_available() else "cpu"
net = SyncNetEval(device=dev); net.loadParameters("checkpoints/auxiliary/syncnet_v2.model")
det = SyncNetDetector(device=dev, detect_results_dir=os.path.join(tempfile.gettempdir(), "ls_detect"))
for v in VIDEOS:
    shutil.rmtree(det.detect_results_dir, ignore_errors=True) if hasattr(det, "detect_results_dir") else None
    det(video_path=v, min_track=50)
    crops = os.listdir(os.path.join(det.detect_results_dir, "crop"))
    r = [net.evaluate(video_path=os.path.join(det.detect_results_dir, "crop", c), temp_dir=tempfile.mkdtemp()) for c in crops]
    print(f"{os.path.basename(v)}: confidence {fmean(x[2] for x in r):.2f}  offset {int(fmean(x[0] for x in r))} frames", flush=True)
