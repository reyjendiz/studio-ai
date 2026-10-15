//! ComfyUI API graphs. A straight port of the graphs the old web UI built, so results stay identical.
use serde_json::{json, Value};

pub const QWEN_NEG: &str = "artifacts, gpt-image, washed-out colors, low quality, low resolution, AI slop, deviantart, sloppy lines, rough sketch, blurry, indistinct, missing fingers, badly drawn hands, wrong number of fingers";
pub const LTX_NEG: &str = "pc game, console game, video game, cartoon, childish, ugly";

pub const LTX_T2V: &str = include_str!("../../ui/ltx/ltx_t2v_api.json");
pub const LTX_I2V: &str = include_str!("../../ui/ltx/ltx_i2v_api.json");
pub const LTX_CONFIG: &str = include_str!("../../ui/ltx/config.json");
pub const LIPSYNC_V2V: &str = include_str!("../../ui/lipsync/lipsync_v2v_api.json");
pub const LIPSYNC_P2V: &str = include_str!("../../ui/lipsync/lipsync_p2v_api.json");

fn zimage(p: &str, w: u32, h: u32, n: u32, seed: u64, model: &str) -> Value {
    json!({
      "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
      "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen_3_4b_fp8_mixed.safetensors","type":"lumina2","device":"default"}},
      "3": {"class_type":"VAELoader","inputs":{"vae_name":"ae.safetensors"}},
      "4": {"class_type":"ModelSamplingAuraFlow","inputs":{"model":["1",0],"shift":3}},
      "5": {"class_type":"CLIPTextEncode","inputs":{"clip":["2",0],"text":p}},
      "6": {"class_type":"ConditioningZeroOut","inputs":{"conditioning":["5",0]}},
      "7": {"class_type":"EmptySD3LatentImage","inputs":{"width":w,"height":h,"batch_size":n}},
      "8": {"class_type":"KSampler","inputs":{"model":["4",0],"positive":["5",0],"negative":["6",0],"latent_image":["7",0],
            "seed":seed,"steps":8,"cfg":1,"sampler_name":"res_multistep","scheduler":"simple","denoise":1}},
      "9": {"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["3",0]}},
      "10": {"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"studio"}}
    })
}

/// SDXL / SD1.5 single-file checkpoint: model, CLIP and VAE all come from node 1; supports a real negative prompt.
fn sdxl(p: &str, w: u32, h: u32, n: u32, seed: u64, model: &str, neg: &str) -> Value {
    json!({
      "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":model}},
      "5": {"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":p}},
      "6": {"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":neg}},
      "7": {"class_type":"EmptyLatentImage","inputs":{"width":w,"height":h,"batch_size":n}},
      "8": {"class_type":"KSampler","inputs":{"model":["1",0],"positive":["5",0],"negative":["6",0],"latent_image":["7",0],
            "seed":seed,"steps":28,"cfg":6,"sampler_name":"dpmpp_2m","scheduler":"karras","denoise":1}},
      "9": {"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["1",2]}},
      "10": {"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"studio"}}
    })
}

/// Qwen-Image 2.1 (e.g. NoctQ int8): one text-encode node gives positive + negative; 25 steps, cfg 3.
fn qwen21(p: &str, w: u32, h: u32, n: u32, seed: u64, model: &str, neg: &str) -> Value {
    json!({
      "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
      "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen3vl_8b_int8_convrot.safetensors","type":"qwen_image","device":"default"}},
      "3": {"class_type":"VAELoader","inputs":{"vae_name":"qwen_image_2.1_vae_bf16.safetensors"}},
      "5": {"class_type":"TextEncodeQwenImage21","inputs":{"clip":["2",0],"prompt":p,"negative_prompt":or(neg, QWEN_NEG),"resolution":1024}},
      "7": {"class_type":"EmptyLatentImage","inputs":{"width":w,"height":h,"batch_size":n}},
      "8": {"class_type":"KSampler","inputs":{"model":["1",0],"positive":["5",0],"negative":["5",1],"latent_image":["7",0],
            "seed":seed,"steps":25,"cfg":3,"sampler_name":"euler","scheduler":"simple","denoise":1}},
      "9": {"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["3",0]}},
      "10": {"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"studio"}}
    })
}

fn or<'a>(s: &'a str, d: &'a str) -> &'a str { if s.trim().is_empty() { d } else { s } }

/// Text to image; with `photo` it becomes img2img where `strength` is the denoise.
#[allow(clippy::too_many_arguments)]
pub fn generate(family: &str, model: &str, p: &str, w: u32, h: u32, n: u32, seed: u64, neg: &str, photo: Option<&str>, strength: f64) -> Value {
    if family == "klein" {
        return klein_gen(p, w, h, n, seed, model);
    }
    let is_sdxl = family == "sdxl";
    let mut wf = match family {
        "sdxl" => sdxl(p, w, h, n, seed, model, neg),
        "qwen21" => qwen21(p, w, h, n, seed, model, neg),
        _ => zimage(p, w, h, n, seed, model),
    };
    if let Some(photo) = photo {
        // img2img: the photo replaces the empty latent; denoise sets how far it may change
        wf["11"] = json!({"class_type":"LoadImage","inputs":{"image":photo}});
        wf["12"] = json!({"class_type":"ImageScaleToTotalPixels","inputs":{"image":["11",0],"upscale_method":"lanczos","megapixels":(w * h) as f64 / 1e6,"resolution_steps":16}});
        wf["13"] = json!({"class_type":"VAEEncode","inputs":{"pixels":["12",0],"vae": if is_sdxl { json!(["1",2]) } else { json!(["3",0]) }}});
        wf["14"] = json!({"class_type":"RepeatLatentBatch","inputs":{"samples":["13",0],"amount":n}});
        wf["8"]["inputs"]["latent_image"] = json!(["14",0]);
        wf["8"]["inputs"]["denoise"] = json!(strength);
    }
    wf
}

/// FLUX.2 klein text-to-image: the edit graph without reference latents.
fn klein_gen(p: &str, w: u32, h: u32, n: u32, seed: u64, model: &str) -> Value {
    json!({
      "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
      "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen_3_4b_fp4_flux2.safetensors","type":"flux2","device":"default"}},
      "3": {"class_type":"VAELoader","inputs":{"vae_name":"flux2-vae.safetensors"}},
      "7": {"class_type":"CLIPTextEncode","inputs":{"clip":["2",0],"text":p}},
      "8": {"class_type":"ConditioningZeroOut","inputs":{"conditioning":["7",0]}},
      "12": {"class_type":"EmptyFlux2LatentImage","inputs":{"width":w,"height":h,"batch_size":n}},
      "13": {"class_type":"Flux2Scheduler","inputs":{"steps":4,"width":w,"height":h}},
      "14": {"class_type":"KSamplerSelect","inputs":{"sampler_name":"euler"}},
      "15": {"class_type":"RandomNoise","inputs":{"noise_seed":seed}},
      "16": {"class_type":"CFGGuider","inputs":{"model":["1",0],"positive":["7",0],"negative":["8",0],"cfg":1}},
      "17": {"class_type":"SamplerCustomAdvanced","inputs":{"noise":["15",0],"guider":["16",0],"sampler":["14",0],"sigmas":["13",0],"latent_image":["12",0]}},
      "18": {"class_type":"VAEDecode","inputs":{"samples":["17",0],"vae":["3",0]}},
      "19": {"class_type":"SaveImage","inputs":{"images":["18",0],"filename_prefix":"studio"}}
    })
}

/// Photo edit by instruction, dispatched by model family (Qwen-Image 2.1, Z-Image img2img, FLUX.2 klein).
pub fn edit(family: &str, model: &str, p: &str, photos: &[String], seed: u64, neg: &str, strength: f64) -> Value {
    match family {
        "qwen21" => {
            let mut wf = json!({
              "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
              "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen3vl_8b_int8_convrot.safetensors","type":"qwen_image","device":"default"}},
              "3": {"class_type":"VAELoader","inputs":{"vae_name":"qwen_image_2.1_vae_bf16.safetensors"}},
              "5": {"class_type":"TextEncodeQwenImage21","inputs":{"clip":["2",0],"prompt":p,"negative_prompt":or(neg, QWEN_NEG),"resolution":1024,"vae":["3",0]}},
              "6": {"class_type":"KSampler","inputs":{"model":["1",0],"positive":["5",0],"negative":["5",1],"latent_image":["5",2],
                    "seed":seed,"steps":25,"cfg":3,"sampler_name":"euler","scheduler":"simple","denoise":1}},
              "7": {"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["3",0]}},
              "8": {"class_type":"SaveImage","inputs":{"images":["7",0],"filename_prefix":"edit"}}
            });
            for (i, name) in photos.iter().enumerate() {
                let l = (20 + i).to_string();
                wf[&l] = json!({"class_type":"LoadImage","inputs":{"image":name}});
                wf["5"]["inputs"][format!("images.image_{}", i + 1)] = json!([l, 0]);
            }
            wf
        }
        "zimage" => json!({
          "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
          "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen_3_4b_fp8_mixed.safetensors","type":"lumina2","device":"default"}},
          "3": {"class_type":"VAELoader","inputs":{"vae_name":"ae.safetensors"}},
          "4": {"class_type":"ModelSamplingAuraFlow","inputs":{"model":["1",0],"shift":3}},
          "5": {"class_type":"CLIPTextEncode","inputs":{"clip":["2",0],"text":p}},
          "6": {"class_type":"ConditioningZeroOut","inputs":{"conditioning":["5",0]}},
          "11": {"class_type":"LoadImage","inputs":{"image":photos[0]}},
          "12": {"class_type":"ImageScaleToTotalPixels","inputs":{"image":["11",0],"upscale_method":"lanczos","megapixels":1,"resolution_steps":16}},
          "13": {"class_type":"VAEEncode","inputs":{"pixels":["12",0],"vae":["3",0]}},
          "8": {"class_type":"KSampler","inputs":{"model":["4",0],"positive":["5",0],"negative":["6",0],"latent_image":["13",0],
                "seed":seed,"steps":8,"cfg":1,"sampler_name":"res_multistep","scheduler":"simple","denoise":strength}},
          "9": {"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["3",0]}},
          "10": {"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"edit"}}
        }),
        _ => {
            // FLUX.2 klein: every photo becomes a reference latent on both conditionings; the first sets the output size
            let mut wf = json!({
              "1": {"class_type":"UNETLoader","inputs":{"unet_name":model,"weight_dtype":"default"}},
              "2": {"class_type":"CLIPLoader","inputs":{"clip_name":"qwen_3_4b_fp4_flux2.safetensors","type":"flux2","device":"default"}},
              "3": {"class_type":"VAELoader","inputs":{"vae_name":"flux2-vae.safetensors"}},
              "7": {"class_type":"CLIPTextEncode","inputs":{"clip":["2",0],"text":p}},
              "8": {"class_type":"ConditioningZeroOut","inputs":{"conditioning":["7",0]}},
              "14": {"class_type":"KSamplerSelect","inputs":{"sampler_name":"euler"}},
              "15": {"class_type":"RandomNoise","inputs":{"noise_seed":seed}}
            });
            let (mut pos, mut negc) = (json!(["7", 0]), json!(["8", 0]));
            for (i, name) in photos.iter().enumerate() {
                let l = 20 + i * 10;
                let s = |k: usize| (l + k).to_string();
                wf[&s(0)] = json!({"class_type":"LoadImage","inputs":{"image":name}});
                wf[&s(1)] = json!({"class_type":"ImageScaleToTotalPixels","inputs":{"image":[s(0),0],"upscale_method":"lanczos","megapixels":1,"resolution_steps":16}});
                wf[&s(2)] = json!({"class_type":"VAEEncode","inputs":{"pixels":[s(1),0],"vae":["3",0]}});
                wf[&s(3)] = json!({"class_type":"ReferenceLatent","inputs":{"conditioning":pos,"latent":[s(2),0]}});
                wf[&s(4)] = json!({"class_type":"ReferenceLatent","inputs":{"conditioning":negc,"latent":[s(2),0]}});
                pos = json!([s(3), 0]);
                negc = json!([s(4), 0]);
            }
            wf["6"] = json!({"class_type":"GetImageSize","inputs":{"image":["21",0]}});
            wf["12"] = json!({"class_type":"EmptyFlux2LatentImage","inputs":{"width":["6",0],"height":["6",1],"batch_size":1}});
            wf["13"] = json!({"class_type":"Flux2Scheduler","inputs":{"steps":4,"width":["6",0],"height":["6",1]}});
            wf["16"] = json!({"class_type":"CFGGuider","inputs":{"model":["1",0],"positive":pos,"negative":negc,"cfg":1}});
            wf["17"] = json!({"class_type":"SamplerCustomAdvanced","inputs":{"noise":["15",0],"guider":["16",0],"sampler":["14",0],"sigmas":["13",0],"latent_image":["12",0]}});
            wf["18"] = json!({"class_type":"VAEDecode","inputs":{"samples":["17",0],"vae":["3",0]}});
            wf["19"] = json!({"class_type":"SaveImage","inputs":{"images":["18",0],"filename_prefix":"edit"}});
            wf
        }
    }
}

/// 2x upscale: the 4x ESRGAN-style model, then lanczos halves it back, which keeps detail and removes noise.
pub fn upscale(photo: &str, kind: &str) -> Value {
    let model = if kind == "anime" { "4xHFA2k.safetensors" } else { "4xNomos2_hq_dat2.safetensors" };
    json!({
      "1": {"class_type":"LoadImage","inputs":{"image":photo}},
      "2": {"class_type":"UpscaleModelLoader","inputs":{"model_name":model}},
      "3": {"class_type":"ImageUpscaleWithModel","inputs":{"upscale_model":["2",0],"image":["1",0]}},
      "4": {"class_type":"ImageScaleBy","inputs":{"image":["3",0],"upscale_method":"lanczos","scale_by":0.5}},
      "5": {"class_type":"SaveImage","inputs":{"images":["4",0],"filename_prefix":"upscale"}}
    })
}

/// Replaces every "{{name}}" string in a template with vars[name], keeping the value's JSON type.
pub fn fill(template: &str, vars: &Value) -> Value {
    fn walk(v: &mut Value, vars: &Value) {
        match v {
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, vars)),
            Value::Object(o) => o.values_mut().for_each(|x| walk(x, vars)),
            Value::String(s) if s.starts_with("{{") && s.ends_with("}}") => {
                if let Some(x) = vars.get(&s[2..s.len() - 2]) {
                    *v = x.clone();
                }
            }
            _ => {}
        }
    }
    let mut g: Value = serde_json::from_str(template).expect("built-in template is valid JSON");
    walk(&mut g, vars);
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_without_leftover_placeholders() {
        let vars = json!({"unet":"u.gguf","clip":"c","vae":"v","audio_vae":"a","upscaler":"x","prompt":"p","negative":"n","fps":24,
            "half_width":512,"half_height":288,"length":121,"seed":1,"cfg":1.0,"image":"i.png","video":"v.mp4","audio":"a.wav",
            "frames":75,"mode":"normal","lips":1.5,"steps":12});
        for t in [LTX_T2V, LTX_I2V, LIPSYNC_V2V, LIPSYNC_P2V] {
            let s = fill(t, &vars).to_string();
            assert!(!s.contains("{{"), "placeholder left in {s}");
        }
        assert_eq!(fill(LTX_T2V, &vars)["9"]["inputs"]["width"], 512); // numbers stay numbers
    }

    #[test]
    fn img2img_and_edits_have_expected_nodes() {
        let g = generate("zimage", "m", "p", 1024, 1024, 2, 7, "", Some("x.png"), 0.8);
        assert_eq!(g["8"]["inputs"]["latent_image"], json!(["14", 0]));
        assert_eq!(g["8"]["inputs"]["denoise"], 0.8);
        let k = edit("klein", "m", "p", &["a.png".into(), "b.png".into()], 1, "", 1.0);
        assert_eq!(k["16"]["inputs"]["positive"], json!(["33", 0])); // second photo's positive reference
        let q = edit("qwen21", "m", "p", &["a.png".into()], 1, "", 1.0);
        assert_eq!(q["5"]["inputs"]["images.image_1"], json!(["20", 0]));
    }
}
