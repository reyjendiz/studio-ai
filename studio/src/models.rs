//! Installed model files: image models (+ the family registry ui\models.json that Setup.exe also writes),
//! LTX-2.5 files, and adding/removing image models.
use crate::paths;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Model {
    pub name: String,
    pub folder: String,
    pub family: String,
    pub size: u64,
}

pub const FAMILIES: &[(&str, &str)] = &[
    ("zimage", "Z-Image: Turbo и его дообучения"),
    ("qwen21", "Qwen-Image 2.1: создание и редактирование (NoctQ и др.)"),
    ("sdxl", "SDXL / SD 1.5: чекпойнт одним файлом"),
    ("klein", "FLUX.2 klein 4B: редактирование фото"),
];

pub fn family_note(family: &str, edit: bool) -> &'static str {
    match (family, edit) {
        ("qwen21", true) => "Qwen-Image 2.1 · правка фото по инструкции, 25 шагов",
        ("zimage", true) => "Z-Image · быстрая правка фото (img2img), 8 шагов",
        ("zimage", _) => "Z-Image · создание по тексту и правка фото (img2img), 8 шагов, очень быстро",
        ("sdxl", _) => "SDXL · картинка по тексту, 28 шагов, есть негативный промпт",
        ("qwen21", _) => "Qwen-Image 2.1 · правки по фото и по тексту, 25 шагов",
        _ => "FLUX.2 klein · правки по фото и картинки по тексту, 4 шага",
    }
}

pub fn is_edit_family(f: &str) -> bool { matches!(f, "klein" | "qwen21" | "zimage") }

fn folder_for(family: &str) -> &'static str { if family == "sdxl" { "checkpoints" } else { "diffusion_models" } }

fn registry() -> Map<String, Value> {
    std::fs::read(paths::registry()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn set_registry(name: &str, family: Option<&str>) -> Result<()> {
    let mut r = registry();
    match family {
        Some(f) => { r.insert(name.into(), Value::from(f)); }
        None => { r.remove(name); }
    }
    std::fs::write(paths::registry(), serde_json::to_string_pretty(&r)?)?;
    Ok(())
}

fn guess_family(folder: &str, name: &str, reg: &Map<String, Value>) -> String {
    if let Some(f) = reg.get(name).and_then(|v| v.as_str()) {
        return f.into();
    }
    let low = name.to_lowercase();
    if low.contains("klein") { "klein" } else if low.contains("noct") || low.contains("qwen") { "qwen21" } else if folder == "checkpoints" { "sdxl" } else { "zimage" }.into()
}

/// Image models, newest first (the newest one is the default pick). LTX files live elsewhere and are not listed.
pub fn image_models() -> Vec<Model> {
    let reg = registry();
    let mut out = Vec::new();
    for folder in ["diffusion_models", "checkpoints"] {
        let mut files: Vec<_> = std::fs::read_dir(paths::models().join(folder)).into_iter().flatten().flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "safetensors"))
            .filter_map(|e| Some((e.metadata().ok()?, e.file_name().to_string_lossy().into_owned())))
            .collect();
        files.sort_by_key(|(m, _)| std::cmp::Reverse(m.modified().ok()));
        for (m, name) in files {
            out.push(Model { family: guess_family(folder, &name, &reg), name, folder: folder.into(), size: m.len() });
        }
    }
    out
}

/// LTX-2.5 files by role: unet, clip, vae, audio_vae, upscaler.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LtxFiles {
    pub unet: Vec<String>,
    pub clip: Vec<String>,
    pub vae: Vec<String>,
    pub audio_vae: Vec<String>,
    pub upscaler: Vec<String>,
}

pub fn ltx_files() -> LtxFiles {
    let names = |folder: &str, parts: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(paths::models().join(folder)).into_iter().flatten().flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| (n.ends_with(".gguf") || n.ends_with(".safetensors")) && parts.iter().all(|p| n.to_lowercase().contains(p)))
            .collect();
        v.sort_by_key(|n| n.to_lowercase());
        v
    };
    let mut unet = names("unet", &["ltx"]);
    unet.extend(names("diffusion_models", &["ltx"]));
    unet.dedup();
    let mut clip = names("text_encoders", &["gemma4"]);
    clip.sort_by_key(|n| n.ends_with(".gguf")); // the GGUF encoder follows prompts poorly here, so int8 comes first
    LtxFiles { unet, clip, vae: names("vae", &["ltx", "video"]), audio_vae: names("vae", &["ltx", "audio"]), upscaler: names("latent_upscale_models", &["ltx", "spatial"]) }
}

impl LtxFiles {
    /// (what, where) for every role with no file installed.
    pub fn missing(&self) -> Vec<(&'static str, &'static str)> {
        let mut m = Vec::new();
        for (v, what, wh) in [
            (&self.unet, "видео-модель (.gguf)", r"ComfyUI\models\unet"),
            (&self.clip, "текстовый энкодер Gemma 4", r"ComfyUI\models\text_encoders"),
            (&self.vae, "видео-VAE", r"ComfyUI\models\vae"),
            (&self.audio_vae, "аудио-VAE", r"ComfyUI\models\vae"),
            (&self.upscaler, "апскейлер латентов ×2", r"ComfyUI\models\latent_upscale_models"),
        ] {
            if v.is_empty() {
                m.push((what, wh));
            }
        }
        m
    }
}

pub fn lipsync_installed() -> bool {
    let n = paths::comfy().join("custom_nodes").join("ComfyUI-LatentSyncWrapper");
    n.join("checkpoints").join("latentsync_unet.pt").is_file() && n.join("checkpoints").join("vae").join("config.json").is_file()
}

/// .safetensors only: pickle formats (.ckpt/.pt) can run code on load.
pub fn valid_name(name: &str) -> bool {
    regex::Regex::new(r"^[\w\-. ()\[\]]{1,120}\.safetensors$").unwrap().is_match(name)
}

fn dest(name: &str, family: &str) -> Result<PathBuf> {
    if !valid_name(name) || !FAMILIES.iter().any(|(f, _)| *f == family) {
        bail!("недопустимое имя файла или тип модели (нужен .safetensors)");
    }
    let d = paths::models().join(folder_for(family)).join(name);
    if d.exists() {
        bail!("файл с таким именем уже есть");
    }
    Ok(d)
}

/// A real safetensors file starts with an 8-byte header length and a JSON header; rejects HTML error pages.
pub fn valid_safetensors(p: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(p) else { return false };
    let mut len = [0u8; 8];
    if f.read_exact(&mut len).is_err() {
        return false;
    }
    let n = u64::from_le_bytes(len);
    if !(2..=50_000_000).contains(&n) {
        return false;
    }
    let mut head = vec![0u8; n as usize];
    f.read_exact(&mut head).is_ok() && serde_json::from_slice::<Value>(&head).is_ok()
}

fn copy_stream(mut src: impl Read, part: &Path, total: u64, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<()> {
    let mut out = std::fs::File::create(part)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0;
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        if !progress(done, total) {
            bail!("отменено");
        }
    }
    Ok(())
}

fn finish(part: &Path, final_: &Path, name: &str, family: &str) -> Result<()> {
    if !valid_safetensors(part) {
        let _ = std::fs::remove_file(part);
        bail!("файл не похож на .safetensors (повреждён или это не модель)");
    }
    std::fs::rename(part, final_)?;
    set_registry(name, Some(family))
}

/// Adds a model by copying a local file; `progress(done, total)` returns false to cancel.
pub fn add_file(src: &Path, name: &str, family: &str, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<()> {
    let final_ = dest(name, family)?;
    let part = final_.with_extension("part");
    let total = std::fs::metadata(src)?.len();
    let r = copy_stream(std::fs::File::open(src)?, &part, total, progress).and_then(|_| finish(&part, &final_, name, family));
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// Adds a model by downloading an https:// link.
pub fn add_url(url: &str, name: &str, family: &str, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<()> {
    if !url.starts_with("https://") {
        bail!("нужна ссылка, начинающаяся с https://");
    }
    let final_ = dest(name, family)?;
    let part = final_.with_extension("part");
    let r = (|| {
        let resp = ureq::get(url).header("User-Agent", "Mozilla/5.0").call().context("не удалось скачать")?;
        let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()?.parse().ok()).unwrap_or(0);
        copy_stream(resp.into_body().into_reader(), &part, total, progress)?;
        finish(&part, &final_, name, family)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

pub fn delete(name: &str) -> Result<()> {
    if !valid_name(name) {
        bail!("такого файла нет");
    }
    for f in ["diffusion_models", "checkpoints"] {
        let p = paths::models().join(f).join(name);
        if p.is_file() {
            std::fs::remove_file(&p)?;
            return set_registry(name, None);
        }
    }
    bail!("такого файла нет")
}

/// Picks an image model by exact name or case-insensitive substring; None = the default (newest Z-Image, else newest).
pub fn pick(want: Option<&str>, edit: bool) -> Result<Model> {
    let all: Vec<Model> = image_models().into_iter().filter(|m| !edit || is_edit_family(&m.family)).collect();
    match want.map(str::to_lowercase) {
        Some(w) => all.iter().find(|m| m.name.to_lowercase() == w).or_else(|| all.iter().find(|m| m.name.to_lowercase().contains(&w))).cloned()
            .with_context(|| format!("нет модели «{w}»; список: studio-cli models")),
        None => all.iter().find(|m| m.family == "zimage").or(all.first()).cloned().context("нет ни одной модели: добавьте её в Студии (＋)"),
    }
}
