//! Finished files in ComfyUI's output folder: photos (any image in output\ and its subfolders), LTX videos
//! (output\video\ltx*.mp4) and lip-sync videos (output\lipsync\ls*.mp4), each video with its settings in a .json beside it.
use crate::paths;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Photos,
    Videos,
    Lipsync,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s.to_lowercase().as_str() {
            "photos" | "photo" | "images" | "image" => Some(Kind::Photos),
            "videos" | "video" => Some(Kind::Videos),
            "lipsync" | "lip-sync" => Some(Kind::Lipsync),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub mtime: u64,
    pub size: u64,
    /// settings saved next to a video (empty for photos)
    pub meta: Value,
}

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp"];

fn ext_is(p: &Path, list: &[&str]) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| list.contains(&e.to_lowercase().as_str()))
}

fn item(p: PathBuf) -> Option<Item> {
    let m = std::fs::metadata(&p).ok()?;
    let mtime = m.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
    let meta = std::fs::read(p.with_extension("json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    Some(Item { name: p.file_name()?.to_string_lossy().into_owned(), path: p, mtime, size: m.len(), meta })
}

/// Newest first.
pub fn list(kind: Kind) -> Vec<Item> {
    let out = paths::output();
    let mut files: Vec<PathBuf> = match kind {
        Kind::Photos => {
            let mut v = Vec::new();
            for e in std::fs::read_dir(&out).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() && !e.file_name().to_string_lossy().starts_with('.') {
                    v.extend(std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && ext_is(p, IMAGE_EXT)));
                } else if p.is_file() && ext_is(&p, IMAGE_EXT) {
                    v.push(p);
                }
            }
            v
        }
        Kind::Videos | Kind::Lipsync => {
            let (dir, prefix) = if kind == Kind::Videos { ("video", "ltx") } else { ("lipsync", "ls") };
            std::fs::read_dir(out.join(dir)).into_iter().flatten().flatten().map(|e| e.path())
                .filter(|p| ext_is(p, &["mp4"]) && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(prefix)))
                .collect()
        }
    };
    files.sort();
    let mut items: Vec<Item> = files.into_iter().filter_map(item).collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.mtime));
    items
}

/// Only files inside ComfyUI\output with a result extension may be deleted.
fn check(p: &Path) -> Result<PathBuf> {
    let out = paths::output().canonicalize()?;
    let f = p.canonicalize().map_err(|_| anyhow::anyhow!("файл не найден: {}", p.display()))?;
    if !f.starts_with(&out) || !f.is_file() || !ext_is(&f, &["png", "jpg", "jpeg", "webp", "mp4"]) {
        bail!("можно удалять только результаты в ComfyUI\\output");
    }
    Ok(f)
}

pub fn delete(p: &Path) -> Result<()> {
    let f = check(p)?;
    std::fs::remove_file(&f).map_err(|e| anyhow::anyhow!("не удалось удалить {}: {e}", f.display()))?;
    if ext_is(&f, &["mp4"]) {
        let _ = std::fs::remove_file(f.with_extension("json"));
    }
    Ok(())
}

/// Deletes every result of a kind; returns how many files went and the ones that could not be removed.
pub fn delete_all(kind: Kind) -> (usize, Vec<String>) {
    let (mut n, mut failed) = (0, Vec::new());
    for it in list(kind) {
        match delete(&it.path) {
            Ok(()) => n += 1,
            Err(e) => failed.push(e.to_string()),
        }
    }
    (n, failed)
}

/// The English prompt ComfyUI stored inside a PNG it saved (tEXt chunk "prompt" holds the API graph).
pub fn png_prompt(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    let mut i = 8;
    while i + 8 <= b.len() {
        let len = u32::from_be_bytes(b[i..i + 4].try_into().ok()?) as usize;
        let kind = &b[i + 4..i + 8];
        let data = b.get(i + 8..i + 8 + len)?;
        if kind == b"tEXt" && data.starts_with(b"prompt\0") {
            let g: Value = serde_json::from_slice(&data[7..]).ok()?;
            for id in ["5", "7"] {
                let inp = &g[id]["inputs"];
                if let Some(t) = inp["text"].as_str().or(inp["prompt"].as_str()) {
                    return Some(t.to_string());
                }
            }
            return None;
        }
        if kind == b"IEND" {
            break;
        }
        i += 12 + len;
    }
    None
}
