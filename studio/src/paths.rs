use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Install root (the folder with `ComfyUI\` and `python_embeded\`): $STUDIO_ROOT, else the first parent of the
/// executable that contains ComfyUI\main.py (works for app\Studio.exe and for target\release builds), else E:\ComfyUI.
pub fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        if let Some(r) = std::env::var_os("STUDIO_ROOT") {
            return PathBuf::from(r);
        }
        if let Ok(exe) = std::env::current_exe() {
            for dir in exe.ancestors().skip(1).take(6) {
                if dir.join("ComfyUI").join("main.py").is_file() {
                    return dir.to_path_buf();
                }
            }
        }
        PathBuf::from(r"E:\ComfyUI")
    })
}

pub fn comfy() -> PathBuf { root().join("ComfyUI") }
pub fn models() -> PathBuf { comfy().join("models") }
/// STUDIO_OUTPUT redirects it (the layout screenshots use made-up results).
pub fn output() -> PathBuf { std::env::var_os("STUDIO_OUTPUT").map(PathBuf::from).unwrap_or_else(|| comfy().join("output")) }
pub fn input() -> PathBuf { comfy().join("input") }
pub fn python() -> PathBuf { root().join("python_embeded").join("python.exe") }
pub fn logs() -> PathBuf { root().join("logs") }
pub fn translator() -> PathBuf { root().join("translate").join("opus-mt-ru-en") }
pub fn registry() -> PathBuf { root().join("ui").join("models.json") }
/// Thumbnails of videos and other throwaway files.
pub fn cache() -> PathBuf { root().join("studio_cache") }
