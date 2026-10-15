//! One queue for everything the app does (window, local API and studio-cli all submit here): prepares inputs,
//! sends graphs to ComfyUI, follows progress, collects the files and keeps settings next to videos.
use crate::engine::Engine;
use crate::translate::{has_cyrillic, Translator};
use crate::{media, models, paths, results, workflows};
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

fn t() -> bool { true }
fn one() -> u32 { 1 }
fn w1024() -> u32 { 1024 }
fn h576() -> u32 { 576 }
fn s09() -> f64 { 0.9 }
fn sec5() -> f64 { 5.0 }
fn fps24() -> u32 { 24 }
fn cfg1() -> f64 { 1.0 }
fn photo_kind() -> String { "photo".into() }
fn t2v() -> String { "t2v".into() }
fn quality() -> String { "quality".into() }
fn trim() -> String { "trim".into() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageReq {
    pub prompt: String,
    #[serde(default)] pub neg: String,
    #[serde(default)] pub model: Option<String>,
    #[serde(default = "w1024")] pub width: u32,
    #[serde(default = "w1024")] pub height: u32,
    #[serde(default = "one")] pub count: u32,
    #[serde(default)] pub seed: Option<u64>,
    /// optional photo for img2img (with FLUX.2 klein it becomes an instruction edit)
    #[serde(default)] pub photo: Option<PathBuf>,
    #[serde(default = "s09")] pub strength: f64,
    #[serde(default = "t")] pub translate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditReq {
    pub prompt: String,
    pub photos: Vec<PathBuf>,
    #[serde(default)] pub neg: String,
    #[serde(default)] pub model: Option<String>,
    #[serde(default = "one")] pub count: u32,
    #[serde(default)] pub seed: Option<u64>,
    #[serde(default = "s09")] pub strength: f64,
    #[serde(default = "t")] pub translate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpscaleReq {
    pub image: PathBuf,
    /// "photo" (people, objects) or "anime" (drawings, art); not called `kind`, which tags the request type
    #[serde(default = "photo_kind")] pub mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoReq {
    pub prompt: String,
    /// "t2v" or "i2v" (needs `image`, it becomes the first frame)
    #[serde(default = "t2v")] pub mode: String,
    #[serde(default)] pub neg: String,
    #[serde(default)] pub image: Option<PathBuf>,
    #[serde(default = "w1024")] pub width: u32,
    #[serde(default = "h576")] pub height: u32,
    #[serde(default = "sec5")] pub seconds: f64,
    #[serde(default = "fps24")] pub fps: u32,
    #[serde(default)] pub seed: Option<u64>,
    #[serde(default = "cfg1")] pub cfg: f64,
    #[serde(default = "t")] pub audio: bool,
    #[serde(default)] pub unet: Option<String>,
    #[serde(default)] pub clip: Option<String>,
    #[serde(default = "t")] pub translate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LipsyncReq {
    /// a video or a photo with a face
    pub media: PathBuf,
    /// speech to sync the lips to
    pub audio: PathBuf,
    /// "quality" (stronger lip guidance) or "fast"
    #[serde(default = "quality")] pub quality: String,
    /// video and audio of different length: "trim" to the shorter or "loop" the video
    #[serde(default = "trim")] pub fit: String,
    #[serde(default)] pub seed: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Request {
    Image(ImageReq),
    Edit(EditReq),
    Upscale(UpscaleReq),
    Video(VideoReq),
    Lipsync(LipsyncReq),
}

impl Request {
    pub fn result_kind(&self) -> results::Kind {
        match self {
            Request::Video(_) => results::Kind::Videos,
            Request::Lipsync(_) => results::Kind::Lipsync,
            _ => results::Kind::Photos,
        }
    }
    pub fn title(&self) -> &'static str {
        match self {
            Request::Image(_) => "Создание фото",
            Request::Edit(_) => "Правка фото",
            Request::Upscale(_) => "Увеличение ×2",
            Request::Video(_) => "Видео",
            Request::Lipsync(_) => "Липсинк",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Queued,
    Running,
    Done,
    Error,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: u64,
    pub request: Request,
    pub state: State,
    pub label: String,
    pub pct: f32,
    pub elapsed: u64,
    pub files: Vec<PathBuf>,
    pub error: Option<String>,
    /// English text actually sent to the model, when the prompt was translated
    pub translated: Option<String>,
    pub seed: Option<u64>,
    #[serde(skip)] started: Option<Instant>,
    #[serde(skip)] pub created: SystemTime,
}

impl Job {
    pub fn active(&self) -> bool { matches!(self.state, State::Queued | State::Running) }
    pub fn elapsed_now(&self) -> u64 { self.started.map(|s| s.elapsed().as_secs()).unwrap_or(self.elapsed) }
}

pub struct Jobs {
    pub engine: Arc<Engine>,
    list: Mutex<Vec<Job>>,
    wake: Condvar,
    cancel: Mutex<Option<u64>>,
    translator: Mutex<(Option<Translator>, Instant)>,
    next: Mutex<u64>,
}

struct Cancelled;
impl std::fmt::Display for Cancelled { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("отменено") } }
impl std::fmt::Debug for Cancelled { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("отменено") } }
impl std::error::Error for Cancelled {}

/// ComfyUI node id -> (label, start %, share %) for the LTX two-stage graph (ids from ui\ltx\build_templates.py).
fn ltx_phase(node: &str) -> Option<(&'static str, f32, f32)> {
    Some(match node {
        "2" => ("Загрузка текстового энкодера", 0.0, 5.0),
        "6" | "7" | "8" => ("Кодирование промпта", 5.0, 3.0),
        "1" => ("Загрузка видео-модели", 8.0, 7.0),
        "3" | "4" | "5" => ("Загрузка VAE", 8.0, 2.0),
        "20" | "21" | "22" | "23" => ("Подготовка фото", 8.0, 2.0),
        "16" => ("Этап 1: черновик", 15.0, 35.0),
        "17" | "18" | "24" | "19" => ("Увеличение ×2", 50.0, 5.0),
        "33" => ("Этап 2: детали", 55.0, 20.0),
        "34" | "35" => ("Декодирование видео", 75.0, 18.0),
        "36" => ("Декодирование звука", 93.0, 2.0),
        "37" | "38" => ("Сохранение", 95.0, 5.0),
        _ => return None,
    })
}

pub fn friendly_error(msg: &str) -> String {
    let m = msg.to_lowercase();
    if m.contains("out of memory") || m.contains("outofmemory") || m.contains("allocation on device") {
        "Не хватило видеопамяти: уменьшите размер или длительность, выберите модель или режим полегче, закройте игры и другие программы на видеокарте".into()
    } else if m.contains("face not detected") || m.contains("no face") {
        "Лицо не найдено: нужно чёткое лицо анфас, видное в каждом кадре".into()
    } else {
        msg.lines().next().unwrap_or(msg).to_string()
    }
}

impl Jobs {
    pub fn new(engine: Arc<Engine>) -> Arc<Self> {
        let j = Arc::new(Self {
            engine,
            list: Mutex::new(Vec::new()),
            wake: Condvar::new(),
            cancel: Mutex::new(None),
            translator: Mutex::new((None, Instant::now())),
            next: Mutex::new(1),
        });
        let w = j.clone();
        std::thread::spawn(move || w.worker());
        j
    }

    pub fn submit(&self, request: Request) -> u64 {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            *n - 1
        };
        self.list.lock().unwrap().push(Job { id, request, state: State::Queued, label: "В очереди".into(), pct: 0.0, elapsed: 0, files: vec![],
            error: None, translated: None, seed: None, started: None, created: SystemTime::now() });
        self.wake.notify_all();
        id
    }

    pub fn get(&self, id: u64) -> Option<Job> { self.list.lock().unwrap().iter().find(|j| j.id == id).cloned() }
    pub fn all(&self) -> Vec<Job> { self.list.lock().unwrap().clone() }
    pub fn busy(&self) -> bool { self.list.lock().unwrap().iter().any(Job::active) }

    pub fn cancel(&self, id: u64) {
        let mut l = self.list.lock().unwrap();
        if let Some(j) = l.iter_mut().find(|j| j.id == id) {
            match j.state {
                State::Queued => { j.state = State::Cancelled; j.label = "Отменено".into(); }
                State::Running => *self.cancel.lock().unwrap() = Some(id),
                _ => {}
            }
        }
    }

    pub fn cancel_all(&self) {
        let ids: Vec<u64> = self.list.lock().unwrap().iter().filter(|j| j.active()).map(|j| j.id).collect();
        ids.into_iter().for_each(|id| self.cancel(id));
    }

    /// Forgets finished jobs (the files stay on disk).
    pub fn clear_finished(&self) { self.list.lock().unwrap().retain(Job::active); }

    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) {
        if let Some(j) = self.list.lock().unwrap().iter_mut().find(|j| j.id == id) {
            f(j);
        }
    }

    fn set(&self, id: u64, label: &str, pct: f32) {
        self.update(id, |j| { j.label = label.into(); j.pct = j.pct.max(pct); });
    }

    fn cancelled(&self, id: u64) -> bool { *self.cancel.lock().unwrap() == Some(id) }

    /// RU->EN for prompts (loaded on first use, about 1 s; dropped after a minute without use).
    pub fn translate(&self, text: &str) -> Result<String> {
        if !has_cyrillic(text) {
            return Ok(text.to_string());
        }
        let mut g = self.translator.lock().unwrap();
        if g.0.is_none() {
            g.0 = Some(Translator::load(&paths::translator()).map_err(|e| anyhow!("переводчик не загрузился ({e}); установите его через Setup.exe"))?);
        }
        g.1 = Instant::now();
        g.0.as_mut().unwrap().translate(text)
    }

    fn worker(self: Arc<Self>) {
        loop {
            let job = {
                let mut l = self.list.lock().unwrap();
                loop {
                    if let Some(j) = l.iter_mut().find(|j| j.state == State::Queued) {
                        j.state = State::Running;
                        j.started = Some(Instant::now());
                        j.label = "Подготовка…".into();
                        break j.clone();
                    }
                    let (g, _) = self.wake.wait_timeout(l, Duration::from_secs(10)).unwrap();
                    l = g;
                    let mut tr = self.translator.lock().unwrap();
                    if tr.0.is_some() && tr.1.elapsed() > Duration::from_secs(60) {
                        tr.0 = None;
                    }
                }
            };
            self.engine.active.fetch_add(1, Ordering::SeqCst);
            let r = self.run(&job);
            self.engine.active.fetch_sub(1, Ordering::SeqCst);
            self.engine.touch();
            *self.cancel.lock().unwrap() = None;
            self.update(job.id, |j| {
                j.elapsed = j.started.take().map(|s| s.elapsed().as_secs()).unwrap_or(0);
                match r {
                    Ok(files) => { j.state = State::Done; j.label = "Готово".into(); j.pct = 100.0; j.files = files; }
                    Err(e) if e.is::<Cancelled>() => { j.state = State::Cancelled; j.label = "Отменено".into(); }
                    Err(e) => { j.state = State::Error; j.label = "Ошибка".into(); j.error = Some(friendly_error(&e.to_string())); }
                }
            });
        }
    }

    fn run(&self, job: &Job) -> Result<Vec<PathBuf>> {
        match &job.request {
            Request::Image(r) => self.run_image(job.id, r),
            Request::Edit(r) => self.run_edit(job.id, r),
            Request::Upscale(r) => self.run_upscale(job.id, r),
            Request::Video(r) => self.run_video(job.id, r),
            Request::Lipsync(r) => self.run_lipsync(job.id, r),
        }
    }

    fn en(&self, id: u64, text: &str, translate: bool) -> Result<String> {
        if !translate || !has_cyrillic(text) {
            return Ok(text.to_string());
        }
        self.set(id, "Перевожу…", 0.0);
        self.translate(text)
    }

    fn upload(&self, id: u64, p: &Path) -> Result<String> {
        if !p.is_file() {
            bail!("файл не найден: {}", p.display());
        }
        self.set(id, "Загружаю фото…", 1.0);
        self.engine.upload_image(p)
    }

    /// Waits for one queued prompt; `share` maps ComfyUI progress into this job's percent range.
    fn wait(&self, id: u64, pid: &str, lipsync: bool, (from, span): (f32, f32), video: bool) -> Result<Vec<PathBuf>> {
        let step0 = self.engine.infer_step.lock().unwrap().0;
        loop {
            if self.cancelled(id) {
                self.engine.delete_pending(&[pid.to_string()]);
                self.engine.interrupt();
                if lipsync {
                    self.engine.hard_stop(); // LatentSync runs as one long node that /interrupt cannot stop
                }
                return Err(Cancelled.into());
            }
            if let Some(h) = self.engine.history(pid).ok().flatten() {
                let st = &h["status"];
                if st["status_str"] == "error" {
                    let msgs = st["messages"].as_array().cloned().unwrap_or_default();
                    if msgs.iter().any(|m| m[0] == "execution_interrupted") {
                        return Err(Cancelled.into());
                    }
                    let e = msgs.iter().find(|m| m[0] == "execution_error").map(|m| m[1].clone()).unwrap_or(Value::Null);
                    bail!("{}", e["exception_message"].as_str().unwrap_or("ComfyUI вернул ошибку (смотри logs\\comfyui.out.log)"));
                }
                if st["completed"] == true {
                    let mut files = Vec::new();
                    for o in h["outputs"].as_object().into_iter().flatten().map(|(_, o)| o) {
                        for i in o["images"].as_array().into_iter().flatten() {
                            if i["type"] != "output" { continue; }
                            files.push(paths::output().join(i["subfolder"].as_str().unwrap_or("")).join(i["filename"].as_str().unwrap_or("")));
                        }
                    }
                    return Ok(files);
                }
            }
            if !self.engine.up() && !self.engine.started_by_us() {
                bail!("ComfyUI закрылся во время работы, смотри logs\\comfyui.out.log");
            }
            // progress
            let p = self.engine.progress.lock().unwrap().clone();
            if p.pid.as_deref() == Some(pid) {
                if lipsync {
                    let (n, s, total) = *self.engine.infer_step.lock().unwrap();
                    if n > step0 && total > 0 {
                        let label = if s >= total { "Сборка видео…".to_string() } else { format!("Синхронизация губ · часть {} из {total}", s + 1) };
                        self.set(id, &label, 5.0 + 90.0 * s as f32 / total as f32);
                    } else {
                        self.set(id, "Подготовка (поиск лица)…", 2.0);
                    }
                } else if video {
                    if let Some((name, base, share)) = p.node.as_deref().and_then(ltx_phase) {
                        let (v, m) = p.nodes.get(p.node.as_deref().unwrap()).copied().unwrap_or((0.0, 0.0));
                        let label = if m > 1.0 { format!("{name} · шаг {v}/{m}") } else { name.to_string() };
                        self.set(id, &label, base + if m > 1.0 { share * (v / m) as f32 } else { 0.0 });
                    }
                } else {
                    let (v, m) = p.node.as_ref().and_then(|n| p.nodes.get(n)).copied().unwrap_or((0.0, 0.0));
                    if m > 1.0 {
                        self.set(id, &format!("Генерация · шаг {v}/{m}"), from + span * (v / m) as f32);
                    } else {
                        self.set(id, "Генерация…", from);
                    }
                }
            } else if !lipsync && !video {
                self.set(id, "Ждёт ComfyUI…", from);
            }
            std::thread::sleep(Duration::from_millis(600));
        }
    }

    fn seed(&self, id: u64, s: Option<u64>) -> u64 {
        let s = s.unwrap_or_else(media::random_seed);
        self.update(id, |j| j.seed = Some(s));
        s
    }

    fn run_image(&self, id: u64, r: &ImageReq) -> Result<Vec<PathBuf>> {
        if r.prompt.trim().is_empty() { bail!("введите промпт"); }
        let m = models::pick(r.model.as_deref(), false)?;
        let seed = self.seed(id, r.seed);
        let (p, neg) = (self.en(id, &r.prompt, r.translate)?, self.en(id, &r.neg, r.translate)?);
        if p != r.prompt { self.update(id, |j| j.translated = Some(p.clone())); }
        let photo = match &r.photo { Some(f) => Some(self.upload(id, f)?), None => None };
        let n = r.count.clamp(1, 4);
        self.set(id, "Запуск ComfyUI…", 2.0);
        let res = if m.family == "klein" && photo.is_some() {
            // klein with a photo = instruction edit, one request per picture
            let mut files = Vec::new();
            for i in 0..n {
                let pid = self.engine.submit(&workflows::edit("klein", &m.name, &p, &[photo.clone().unwrap()], seed + i as u64, &neg, 1.0))?;
                files.extend(self.wait(id, &pid, false, (5.0 + 90.0 * i as f32 / n as f32, 90.0 / n as f32), false)?);
            }
            Ok(files)
        } else {
            let (w, h) = (r.width / 16 * 16, r.height / 16 * 16);
            let pid = self.engine.submit(&workflows::generate(&m.family, &m.name, &p, w, h, n, seed, &neg, photo.as_deref(), r.strength))?;
            self.wait(id, &pid, false, (5.0, 90.0), false)
        };
        self.engine.free();
        res
    }

    fn run_edit(&self, id: u64, r: &EditReq) -> Result<Vec<PathBuf>> {
        if r.prompt.trim().is_empty() { bail!("опишите, что изменить"); }
        if r.photos.is_empty() || r.photos.len() > 2 { bail!("нужно одно или два фото"); }
        let m = models::pick(r.model.as_deref(), true)?;
        let seed = self.seed(id, r.seed);
        let (p, neg) = (self.en(id, &r.prompt, r.translate)?, self.en(id, &r.neg, r.translate)?);
        if p != r.prompt { self.update(id, |j| j.translated = Some(p.clone())); }
        let names = r.photos.iter().map(|f| self.upload(id, f)).collect::<Result<Vec<_>>>()?;
        let n = r.count.clamp(1, 4);
        let mut files = Vec::new();
        for i in 0..n {
            if self.cancelled(id) { return Err(Cancelled.into()); }
            let pid = self.engine.submit(&workflows::edit(&m.family, &m.name, &p, &names, seed + i as u64, &neg, r.strength))?;
            files.extend(self.wait(id, &pid, false, (5.0 + 90.0 * i as f32 / n as f32, 90.0 / n as f32), false)?);
        }
        self.engine.free();
        Ok(files)
    }

    fn run_upscale(&self, id: u64, r: &UpscaleReq) -> Result<Vec<PathBuf>> {
        let (w, h) = image::image_dimensions(&r.image).map_err(|_| anyhow!("не удалось прочитать фото {}", r.image.display()))?;
        if w.max(h) > 3500 { bail!("фото слишком большое: до 3500 px по длинной стороне"); } // the x4 step lives in RAM
        let name = self.upload(id, &r.image)?;
        let pid = self.engine.submit(&workflows::upscale(&name, &r.mode))?;
        let res = self.wait(id, &pid, false, (5.0, 90.0), false);
        self.engine.free();
        res
    }

    fn run_video(&self, id: u64, r: &VideoReq) -> Result<Vec<PathBuf>> {
        if r.prompt.trim().is_empty() { bail!("введите промпт"); }
        let f = models::ltx_files();
        if let Some((what, wh)) = f.missing().first() { bail!("не хватает файла: {what} → {wh}"); }
        let ok = |n: u32| (256..=1920).contains(&n) && n % 64 == 0;
        if !ok(r.width) || !ok(r.height) { bail!("размер должен быть кратен 64, от 256 до 1920"); }
        let cfg: Value = serde_json::from_str(workflows::LTX_CONFIG).unwrap_or_default();
        let max_sec = cfg["max_seconds"].as_f64().unwrap_or(10.0);
        if !(1.0..=max_sec).contains(&r.seconds) { bail!("длительность от 1 до {max_sec} с"); }
        let i2v = r.mode == "i2v";
        let pick = |want: &Option<String>, list: &[String], what: &str| -> Result<String> {
            match want {
                Some(w) => list.iter().find(|n| n.to_lowercase().contains(&w.to_lowercase())).cloned().ok_or_else(|| anyhow!("нет {what} «{w}»")),
                None => Ok(list[0].clone()),
            }
        };
        let unet = pick(&r.unet, &f.unet, "видео-модели")?;
        let clip = pick(&r.clip, &f.clip, "энкодера")?;
        let seed = self.seed(id, r.seed);
        let (p, neg) = (self.en(id, &r.prompt, r.translate)?, self.en(id, &r.neg, r.translate)?);
        if p != r.prompt { self.update(id, |j| j.translated = Some(p.clone())); }
        let image = if i2v {
            Some(self.upload(id, r.image.as_ref().ok_or_else(|| anyhow!("для «фото → видео» нужно стартовое фото"))?)?)
        } else { None };
        let frames = ((r.seconds * r.fps as f64 / 8.0).round() as u32) * 8 + 1; // LTX wants 8k+1 frames
        let mut g = workflows::fill(if i2v { workflows::LTX_I2V } else { workflows::LTX_T2V }, &json!({
            "unet": unet, "clip": clip, "vae": f.vae[0], "audio_vae": f.audio_vae[0], "upscaler": f.upscaler[0],
            "prompt": p, "negative": if neg.trim().is_empty() { workflows::LTX_NEG.to_string() } else { neg.clone() },
            "fps": r.fps, "half_width": r.width / 2, "half_height": r.height / 2, "length": frames, "seed": seed, "cfg": r.cfg, "image": image}));
        if !unet.ends_with(".gguf") { g["1"] = json!({"class_type":"UNETLoader","inputs":{"unet_name":unet,"weight_dtype":"default"}}); }
        if clip.ends_with(".gguf") { g["2"] = json!({"class_type":"CLIPLoaderGGUF","inputs":{"clip_name":clip,"type":"ltxv"}}); }
        if !r.audio { g["37"]["inputs"].as_object_mut().unwrap().remove("audio"); }
        self.set(id, "Запуск ComfyUI…", 0.0);
        self.engine.keep_longer();
        let pid = self.engine.submit(&g)?;
        let files = self.wait(id, &pid, false, (0.0, 100.0), true)?;
        let elapsed = self.get(id).map(|j| j.elapsed_now());
        let meta = json!({"mode": r.mode, "prompt": r.prompt, "negative": r.neg, "width": r.width, "height": r.height, "seconds": r.seconds,
            "fps": r.fps, "frames": frames, "seed": seed, "cfg": r.cfg, "audio": r.audio, "unet": unet, "clip": clip, "enPrompt": p,
            "image": r.image, "elapsed": elapsed, "created": now_iso()});
        for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "mp4")) {
            let _ = std::fs::write(f.with_extension("json"), serde_json::to_vec_pretty(&meta)?);
        }
        Ok(files)
    }

    fn run_lipsync(&self, id: u64, r: &LipsyncReq) -> Result<Vec<PathBuf>> {
        if !models::lipsync_installed() { bail!("LatentSync не установлен (ComfyUI-LatentSyncWrapper и его модели)"); }
        let photo = r.media.extension().and_then(|e| e.to_str()).is_some_and(|e| ["png", "jpg", "jpeg", "webp", "bmp"].contains(&e.to_lowercase().as_str()));
        if !r.media.is_file() { bail!("файл не найден: {}", r.media.display()); }
        let seed = self.seed(id, r.seed);
        let media_name = if photo { self.upload(id, &r.media)? } else {
            self.set(id, "Готовлю видео (25 к/с)…", 1.0);
            media::prepare(&r.media, "video")?.0
        };
        self.set(id, "Готовлю аудио…", 2.0);
        let (audio_name, sec) = media::prepare(&r.audio, "audio")?;
        if sec > 61.0 { bail!("аудио длиннее 60 с: обрежьте его"); }
        let fast = r.quality == "fast";
        let g = workflows::fill(if photo { workflows::LIPSYNC_P2V } else { workflows::LIPSYNC_V2V }, &json!({
            "video": media_name, "image": media_name, "audio": audio_name, "frames": (sec * 25.0).ceil() as u32,
            "mode": if r.fit == "loop" { "pingpong" } else { "normal" }, "seed": seed, "lips": if fast { 1.0 } else { 1.5 }, "steps": 12}));
        self.set(id, "Запуск ComfyUI…", 2.0);
        self.engine.keep_longer();
        self.engine.ensure()?;
        let pid = self.engine.submit(&g)?;
        let files = self.wait(id, &pid, true, (0.0, 100.0), false)?;
        let elapsed = self.get(id).map(|j| j.elapsed_now());
        let meta = json!({"mode": if photo { "p" } else { "v" }, "q": if fast { "f" } else { "q" }, "len": if r.fit == "loop" { "pingpong" } else { "normal" },
            "seed": seed, "audioSec": sec, "mediaName": media_name, "audioName": audio_name, "media": r.media, "audio": r.audio,
            "elapsed": elapsed, "created": now_iso()});
        for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "mp4")) {
            let _ = std::fs::write(f.with_extension("json"), serde_json::to_vec_pretty(&meta)?);
        }
        Ok(files)
    }
}

pub fn now_iso() -> String {
    let s = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (s / 86400, s % 86400);
    // civil-from-days (Howard Hinnant), UTC
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_with_defaults() {
        let r: Request = serde_json::from_str(r#"{"kind":"image","prompt":"a cat"}"#).unwrap();
        match r { Request::Image(i) => { assert_eq!((i.width, i.height, i.count), (1024, 1024, 1)); assert!(i.translate); } _ => panic!() }
        let v: Request = serde_json::from_str(r#"{"kind":"video","prompt":"x","seconds":3}"#).unwrap();
        match v { Request::Video(v) => assert_eq!((v.width, v.height, v.fps, v.mode.as_str()), (1024, 576, 24, "t2v")), _ => panic!() }
        assert!(serde_json::from_str::<Request>(r#"{"kind":"lipsync","media":"a.mp4"}"#).is_err()); // audio is required
        let u: Request = serde_json::from_str(r#"{"kind":"upscale","image":"a.png","mode":"anime"}"#).unwrap();
        match u { Request::Upscale(u) => assert_eq!(u.mode, "anime"), _ => panic!() }
    }

    #[test]
    fn iso_date_is_well_formed() {
        assert!(regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$").unwrap().is_match(&now_iso()));
    }

    #[test]
    fn oom_is_explained() {
        assert!(friendly_error("CUDA out of memory. Tried to allocate").starts_with("Не хватило видеопамяти"));
    }
}
