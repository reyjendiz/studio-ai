//! ffmpeg helpers (lip sync needs 25 fps input; video thumbnails) and handing files to Windows.
use crate::engine::NO_WINDOW;
use crate::paths;
use anyhow::{bail, Result};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").creation_flags(NO_WINDOW).output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn duration(file: &Path) -> f64 {
    Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(file)
        .creation_flags(NO_WINDOW).output().ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0.0)
}

/// Converts a user's video (to 25 fps, at most 1920 wide, no sound) or audio (to WAV) into ComfyUI's input folder.
/// Returns the new file name there and its length in seconds.
pub fn prepare(src: &Path, kind: &str) -> Result<(String, f64)> {
    if !have_ffmpeg() {
        bail!("ffmpeg не найден: установите его (winget install Gyan.FFmpeg) и перезапустите Студию");
    }
    let (args, ext): (&[&str], &str) = match kind {
        "video" => (&["-an", "-vf", r"fps=25,scale=min(1920\,iw):-2", "-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p"], "mp4"),
        _ => (&["-vn", "-ar", "44100", "-c:a", "pcm_s16le"], "wav"),
    };
    let name = format!("ls_{:010x}.{ext}", rand_u64() & 0xff_ffff_ffff);
    let dst = paths::input().join(&name);
    let o = Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-i"]).arg(src).args(args).arg(&dst).creation_flags(NO_WINDOW).output()?;
    if !o.status.success() || !dst.is_file() {
        let _ = std::fs::remove_file(&dst);
        let msg = String::from_utf8_lossy(&o.stderr);
        bail!("не удалось прочитать {}: {}", src.display(), msg.lines().last().unwrap_or("неизвестный формат"));
    }
    Ok((name, duration(&dst)))
}

/// First frame of a video as a small JPEG in the cache (made once).
pub fn thumbnail(video: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(video).ok()?;
    let key = format!("{}_{}_{}.jpg", video.file_stem()?.to_string_lossy(), meta.len(), meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs());
    let out = paths::cache().join("thumbs").join(key);
    if out.is_file() {
        return Some(out);
    }
    std::fs::create_dir_all(out.parent()?).ok()?;
    let ok = Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-ss", "0.1", "-i"]).arg(video)
        .args(["-frames:v", "1", "-vf", "scale=480:-2"]).arg(&out).creation_flags(NO_WINDOW).status().ok()?.success();
    (ok && out.is_file()).then_some(out)
}

/// Opens a file with the program Windows associates with it (photo viewer, video player).
pub fn open_default(file: &Path) {
    let _ = Command::new("explorer").arg(file).spawn();
}

/// Opens Explorer with the file selected.
pub fn reveal(file: &Path) {
    let _ = Command::new("explorer").arg(format!("/select,{}", file.display())).spawn();
}

pub fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish()
}

pub fn random_seed() -> u64 { rand_u64() % (1 << 31) }
