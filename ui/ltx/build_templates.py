"""Regenerates ltx_t2v_api.json / ltx_i2v_api.json (ComfyUI API format, {{placeholders}} are filled in by the page).
Graph = ComfyUI's LTX-2.5 two-stage distilled template: 8 steps at half size, x2 latent upscale, 3 refine steps."""
import json, pathlib

OUT = pathlib.Path(__file__).parent
S1 = "1.0, 0.99375, 0.9875, 0.98125, 0.975, 0.909375, 0.725, 0.421875, 0.0"
S2 = "0.85, 0.7250, 0.4219, 0.0"


def build(i2v):
    g = {
        "1": {"class_type": "UnetLoaderGGUF", "inputs": {"unet_name": "{{unet}}"}},
        "2": {"class_type": "CLIPLoader", "inputs": {"clip_name": "{{clip}}", "type": "ltxv", "device": "default"}},
        "3": {"class_type": "VAELoader", "inputs": {"vae_name": "{{vae}}"}},
        "4": {"class_type": "VAELoader", "inputs": {"vae_name": "{{audio_vae}}"}},
        "5": {"class_type": "LatentUpscaleModelLoader", "inputs": {"model_name": "{{upscaler}}"}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": "{{prompt}}"}},
        "7": {"class_type": "CLIPTextEncode", "inputs": {"clip": ["2", 0], "text": "{{negative}}"}},
        "8": {"class_type": "LTXVConditioning", "inputs": {"positive": ["6", 0], "negative": ["7", 0], "frame_rate": "{{fps}}"}},
        "9": {"class_type": "EmptyLTXVLatentVideo", "inputs": {"width": "{{half_width}}", "height": "{{half_height}}", "length": "{{length}}", "batch_size": 1}},
        "10": {"class_type": "LTXVEmptyLatentAudio", "inputs": {"frames_number": "{{length}}", "frame_rate": "{{fps}}", "batch_size": 1, "audio_vae": ["4", 0]}},
        "11": {"class_type": "LTXVConcatAVLatent", "inputs": {"video_latent": ["9", 0], "audio_latent": ["10", 0]}},
        "12": {"class_type": "RandomNoise", "inputs": {"noise_seed": "{{seed}}"}},
        "13": {"class_type": "KSamplerSelect", "inputs": {"sampler_name": "euler_ancestral"}},
        "14": {"class_type": "ManualSigmas", "inputs": {"sigmas": "{{sigmas1}}"}},
        "15": {"class_type": "LTXVDualCFGGuider", "inputs": {"model": ["1", 0], "positive": ["8", 0], "negative": ["8", 1], "video_cfg": "{{cfg}}", "audio_cfg": "{{cfg}}"}},
        "16": {"class_type": "SamplerCustomAdvanced", "inputs": {"noise": ["12", 0], "guider": ["15", 0], "sampler": ["13", 0], "sigmas": ["14", 0], "latent_image": ["11", 0]}},
        "17": {"class_type": "LTXVSeparateAVLatent", "inputs": {"av_latent": ["16", 0]}},
        "18": {"class_type": "LTXVLatentUpsampler", "inputs": {"samples": ["17", 0], "upscale_model": ["5", 0], "vae": ["3", 0]}},
        "19": {"class_type": "LTXVConcatAVLatent", "inputs": {"video_latent": ["18", 0], "audio_latent": ["17", 1]}},
        "30": {"class_type": "RandomNoise", "inputs": {"noise_seed": 42}},
        "31": {"class_type": "ManualSigmas", "inputs": {"sigmas": S2}},
        "32": {"class_type": "LTXVDualCFGGuider", "inputs": {"model": ["1", 0], "positive": ["8", 0], "negative": ["8", 1], "video_cfg": "{{cfg}}", "audio_cfg": "{{cfg}}"}},
        "33": {"class_type": "SamplerCustomAdvanced", "inputs": {"noise": ["30", 0], "guider": ["32", 0], "sampler": ["13", 0], "sigmas": ["31", 0], "latent_image": ["19", 0]}},
        "34": {"class_type": "LTXVSeparateAVLatent", "inputs": {"av_latent": ["33", 0]}},
        "35": {"class_type": "VAEDecodeTiled", "inputs": {"samples": ["34", 0], "vae": ["3", 0], "tile_size": 512, "overlap": 64, "temporal_size": 64, "temporal_overlap": 16}},
        "36": {"class_type": "LTXVAudioVAEDecode", "inputs": {"samples": ["34", 1], "audio_vae": ["4", 0]}},
        "37": {"class_type": "CreateVideo", "inputs": {"images": ["35", 0], "audio": ["36", 0], "fps": "{{fps}}"}},
        "38": {"class_type": "SaveVideo", "inputs": {"video": ["37", 0], "filename_prefix": "video/ltx25", "format": "auto", "format.codec": "auto"}},
    }
    g["14"]["inputs"]["sigmas"] = S1
    if i2v:
        g["20"] = {"class_type": "LoadImage", "inputs": {"image": "{{image}}"}}
        g["21"] = {"class_type": "ResizeImageMaskNode", "inputs": {"input": ["20", 0], "resize_type": "scale longer dimension", "resize_type.longer_size": 1536, "scale_method": "lanczos"}}
        g["22"] = {"class_type": "LTXVPreprocess", "inputs": {"image": ["21", 0], "img_compression": 18}}
        g["23"] = {"class_type": "LTXVImgToVideoInplace", "inputs": {"vae": ["3", 0], "image": ["22", 0], "latent": ["9", 0], "strength": 0.7, "bypass": False}}
        g["24"] = {"class_type": "LTXVImgToVideoInplace", "inputs": {"vae": ["3", 0], "image": ["22", 0], "latent": ["18", 0], "strength": 1.0, "bypass": False}}
        g["11"]["inputs"]["video_latent"] = ["23", 0]
        g["19"]["inputs"]["video_latent"] = ["24", 0]
    return g


if __name__ == "__main__":
    for name, i2v in (("t2v", False), ("i2v", True)):
        (OUT / f"ltx_{name}_api.json").write_text(json.dumps(build(i2v), indent=1), encoding="utf-8")
