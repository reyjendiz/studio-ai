//! Thumbnails decoded on background threads (photos with `image`, videos via an ffmpeg frame grab) and turned into
//! textures on the UI thread. Keyed by path + modification time: ComfyUI reuses a deleted file's name.
use crate::media;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

enum Slot {
    Loading,
    Ready(TextureHandle),
    Failed,
}

pub struct Thumbs {
    map: HashMap<String, Slot>,
    req: Sender<(String, PathBuf, u32)>,
    done: Receiver<(String, Option<ColorImage>)>,
}

fn key(p: &Path) -> String {
    let m = std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
    format!("{}|{m}", p.display())
}

fn decode(p: &Path, max: u32) -> Option<ColorImage> {
    let src = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) { media::thumbnail(p)? } else { p.to_path_buf() };
    let img = image::open(&src).ok()?.thumbnail(max, max).to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

impl Thumbs {
    pub fn new(ctx: &egui::Context) -> Self {
        let (req, rx) = channel::<(String, PathBuf, u32)>();
        let (tx, done) = channel();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..4 {
            let (rx, tx, ctx) = (rx.clone(), tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let Ok((k, p, max)) = rx.lock().unwrap().recv() else { return };
                let _ = tx.send((k, decode(&p, max)));
                ctx.request_repaint();
            });
        }
        Self { map: HashMap::new(), req, done }
    }

    /// The thumbnail if ready; otherwise queues it and returns None (the caller draws a placeholder).
    pub fn get(&mut self, ctx: &egui::Context, p: &Path, max: u32) -> Option<TextureHandle> {
        while let Ok((k, img)) = self.done.try_recv() {
            let slot = match img {
                Some(img) => Slot::Ready(ctx.load_texture(&k, img, TextureOptions::LINEAR)),
                None => Slot::Failed,
            };
            self.map.insert(k, slot);
        }
        let k = key(p);
        match self.map.get(&k) {
            Some(Slot::Ready(t)) => Some(t.clone()),
            Some(_) => None,
            None => {
                self.map.insert(k.clone(), Slot::Loading);
                let _ = self.req.send((k, p.to_path_buf(), max));
                None
            }
        }
    }

    pub fn failed(&self, p: &Path) -> bool { matches!(self.map.get(&key(p)), Some(Slot::Failed)) }
}
