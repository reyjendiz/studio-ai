//! The left-hand composer of each tab (what the old web page had in its form), Settings and the model manager.
use super::i18n::{self, t, tf, Lang};
use super::theme::{self, MUTE, RED, WARN};
use super::{App, Tab};
use crate::jobs::{EditReq, ImageReq, LipsyncReq, Request, UpscaleReq, VideoReq};
use crate::{media, models};
use eframe::egui::{self, Color32, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const SIZES: &[((u32, u32), &str)] = &[((1024, 1024), "1:1"), ((832, 1216), "2:3"), ((1216, 832), "3:2"), ((768, 1344), "9:16"), ((1344, 768), "16:9")];
const ASPECTS: &[(&str, [(u32, u32); 3])] = &[("16:9", [(768, 448), (1024, 576), (1280, 704)]), ("9:16", [(448, 768), (576, 1024), (704, 1280)]), ("1:1", [(512, 512), (768, 768), (960, 960)])];
pub const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];
pub const VIDEO_EXT: &[&str] = &["mp4", "mov", "webm", "mkv", "avi", "m4v"];
pub const AUDIO_EXT: &[&str] = &["wav", "mp3", "m4a", "aac", "ogg", "flac", "opus"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageForm {
    pub prompt: String,
    pub neg: String,
    pub translate: bool,
    pub model_gen: Option<String>,
    pub model_edit: Option<String>,
    pub size: (u32, u32),
    pub count: u32,
    pub seed: u64,
    pub auto_seed: bool,
    pub strength: f64,
    pub anime: bool,
    #[serde(skip)] pub photo1: Option<PathBuf>,
    #[serde(skip)] pub photo2: Option<PathBuf>,
}

impl Default for ImageForm {
    fn default() -> Self {
        Self { prompt: String::new(), neg: String::new(), translate: true, model_gen: None, model_edit: None, size: (1024, 1024), count: 1,
            seed: 1, auto_seed: true, strength: 0.9, anime: false, photo1: None, photo2: None }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoForm {
    pub i2v: bool,
    pub prompt: String,
    pub neg: String,
    pub translate: bool,
    pub unet: Option<String>,
    pub clip: Option<String>,
    pub aspect: usize,
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
    pub fps: u32,
    pub seed: u64,
    pub auto_seed: bool,
    pub cfg: f64,
    pub audio: bool,
    #[serde(skip)] pub image: Option<PathBuf>,
}

impl Default for VideoForm {
    fn default() -> Self {
        Self { i2v: false, prompt: String::new(), neg: String::new(), translate: true, unet: None, clip: None, aspect: 0, width: 1024, height: 576,
            seconds: 5.0, fps: 24, seed: 1, auto_seed: true, cfg: 1.0, audio: true, image: None }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LipForm {
    pub photo: bool,
    pub fast: bool,
    pub loop_video: bool,
    pub seed: u64,
    pub auto_seed: bool,
    #[serde(skip)] pub media: Option<PathBuf>,
    #[serde(skip)] pub audio: Option<PathBuf>,
    #[serde(skip)] pub media_secs: f64,
    #[serde(skip)] pub audio_secs: f64,
}

impl Default for LipForm {
    fn default() -> Self { Self { photo: false, fast: false, loop_video: false, seed: 1247, auto_seed: true, media: None, audio: None, media_secs: 0.0, audio_secs: 0.0 } }
}

/// Window-wide preferences (Settings).
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub lang: Lang,
    /// card opacity in percent: lower = more of the background glow shows through
    pub opacity: u8,
    pub glow: bool,
    /// the background gradient drifts (off = still), and how fast (1.0 = normal)
    /// frosting of the background behind tiles, in percent
    pub blur: u32,
    pub motion: bool,
    pub speed: f32,
    /// background brightness in percent (100 = as designed), grain amount in percent, grain size in px
    pub brightness: u32,
    pub noise: u32,
    pub grain: u32,
    /// interface scale in percent; 0 = automatic (grows on big windows)
    pub scale: u32,
}

impl Default for Prefs { fn default() -> Self { Self { lang: windows_lang(), opacity: 82, glow: true, blur: 60, motion: true, speed: 1.0, brightness: 100, noise: 20, grain: 1, scale: 0 } } }

/// First launch follows the Windows display language: Ukrainian or Russian, otherwise English.
fn windows_lang() -> Lang {
    unsafe extern "system" { fn GetUserDefaultUILanguage() -> u16; }
    match unsafe { GetUserDefaultUILanguage() } & 0x3ff { 0x22 => Lang::Uk, 0x19 => Lang::Ru, _ => Lang::En }
}

fn has_ext(p: &std::path::Path, list: &[&str]) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| list.contains(&e.to_lowercase().as_str()))
}

fn pick(title: &str, exts: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).add_filter(t("файлы"), exts).pick_file()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot { Photo1, Photo2, VideoImage, LipMedia, LipAudio }

/// A photo/video/audio slot: thumbnail or file name, "Выбрать…" and "✕".
fn file_slot(app: &mut App, ui: &mut egui::Ui, slot: Slot, label: &str, exts: &[&str]) {
    let mut changed = false;
    let cur = app.slot(slot).clone();
    egui::Frame::new().fill(theme::field()).stroke(egui::Stroke::new(1.0, if cur.is_some() { theme::LINE } else { theme::DIM }))
        .corner_radius(theme::PILL).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), if cur.is_some() { 54.0 } else { 30.0 }), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if cur.is_some() && ui.small_button("✕").on_hover_text(t("Убрать")).clicked() {
                    *app.slot(slot) = None;
                    changed = true;
                }
                if theme::ghost(ui, t("Выбрать…")).clicked() {
                    if let Some(p) = pick(label, exts) {
                        *app.slot(slot) = Some(p);
                        changed = true;
                    }
                }
                ui.allocate_ui_with_layout(ui.available_size(), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                if let Some(p) = &cur {
                    let tex = if has_ext(p, IMAGE_EXT) || has_ext(p, VIDEO_EXT) { app.thumbs.get(ui.ctx(), p, 256) } else { None };
                    match tex {
                        Some(tx) => { let s = tx.size_vec2(); let h = 52.0; ui.add(egui::Image::new(&tx).fit_to_exact_size(Vec2::new(h * s.x / s.y.max(1.0), h)).corner_radius(theme::PILL)); }
                        None => { ui.label(RichText::new(if has_ext(p, AUDIO_EXT) { "♪" } else { "▣" }).size(26.0).color(MUTE)); }
                    }
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(p.file_name().unwrap_or_default().to_string_lossy()).size(13.0)).truncate());
                        let secs = match slot { Slot::LipMedia => app.lip.media_secs, Slot::LipAudio => app.lip.audio_secs, _ => 0.0 };
                        if secs > 0.0 { ui.label(theme::mute(format!("{secs:.1} {}", t("с")))); }
                    });
                } else {
                    ui.add(egui::Label::new(RichText::new(label).color(MUTE)).truncate());
                }
                });
            });
        });
    if changed {
        app.slot_changed(slot);
    }
}

impl App {
    pub fn slot(&mut self, s: Slot) -> &mut Option<PathBuf> {
        match s {
            Slot::Photo1 => &mut self.img.photo1,
            Slot::Photo2 => &mut self.img.photo2,
            Slot::VideoImage => &mut self.vid.image,
            Slot::LipMedia => &mut self.lip.media,
            Slot::LipAudio => &mut self.lip.audio,
        }
    }

    /// Lengths for the lip-sync estimate are read with ffprobe when a file is chosen.
    pub fn slot_changed(&mut self, s: Slot) {
        match s {
            Slot::LipMedia => self.lip.media_secs = self.lip.media.as_deref().filter(|p| has_ext(p, VIDEO_EXT)).map(media::duration).unwrap_or(0.0),
            Slot::LipAudio => self.lip.audio_secs = self.lip.audio.as_deref().map(media::duration).unwrap_or(0.0),
            _ => {}
        }
    }

    /// Files dropped on the window go to the fitting slot of the open tab.
    pub fn drop_files(&mut self, files: Vec<PathBuf>) {
        for f in files {
            let (img, vid, aud) = (has_ext(&f, IMAGE_EXT), has_ext(&f, VIDEO_EXT), has_ext(&f, AUDIO_EXT));
            let slot = match self.tab {
                Tab::Create | Tab::Upscale if img => Some(Slot::Photo1),
                Tab::Edit if img => Some(if self.img.photo1.is_none() { Slot::Photo1 } else { Slot::Photo2 }),
                Tab::Video if img => { self.vid.i2v = true; Some(Slot::VideoImage) }
                Tab::Lipsync if aud => Some(Slot::LipAudio),
                Tab::Lipsync if vid || img => { self.lip.photo = img; Some(Slot::LipMedia) }
                _ => None,
            };
            match slot {
                Some(s) => { *self.slot(s) = Some(f); self.slot_changed(s); }
                None => self.toast(t("Этот файл сюда не подходит"), true),
            }
        }
    }

    fn model_combo(&mut self, ui: &mut egui::Ui, edit: bool) -> Option<models::Model> {
        let list: Vec<models::Model> = self.models.iter().filter(|m| !edit || models::is_edit_family(&m.family)).cloned().collect();
        let chosen = if edit { &mut self.img.model_edit } else { &mut self.img.model_gen };
        if chosen.as_ref().is_none_or(|c| !list.iter().any(|m| &m.name == c)) {
            *chosen = list.iter().find(|m| m.family == "zimage").or(list.first()).map(|m| m.name.clone());
        }
        let cur = list.iter().find(|m| Some(&m.name) == chosen.as_ref()).cloned();
        let name = |m: &models::Model| format!("{} · {}", m.name.trim_end_matches(".safetensors"), theme::fmt_size(m.size));
        ui.horizontal(|ui| {
            let w = ui.available_width() - 34.0 - ui.spacing().item_spacing.x;
            combo_box(ui, ("model", edit), w, cur.as_ref().map(name).unwrap_or(t("нет моделей: добавьте через ＋").into()), |ui| {
                for m in &list { ui.selectable_value(chosen, Some(m.name.clone()), name(m)); }
            });
            if ui.add(egui::Button::new(RichText::new("+").size(18.0).color(theme::accent())).corner_radius(theme::PILL).min_size(Vec2::splat(34.0))).on_hover_text(t("Модели: добавить или удалить")).clicked() {
                self.sheet = Some(ModelSheet::default());
            }
        });
        if let Some(m) = &cur { ui.label(theme::mute(t(models::family_note(&m.family, edit)))); }
        cur
    }

    fn seed_row(ui: &mut egui::Ui, seed: &mut u64, auto: &mut bool) {
        ui.horizontal(|ui| {
            ui.label(theme::mute("Seed"));
            ui.add_enabled(!*auto, egui::DragValue::new(seed).range(0..=u32::MAX as u64));
            check(ui, auto, t("авто"));
        });
    }

    fn status_line(&self, ui: &mut egui::Ui, tab: Tab) {
        let Some(j) = self.last.get(&tab).and_then(|id| self.jobs.get(*id)) else { return };
        use crate::jobs::State::*;
        let (text, color) = match j.state {
            Queued | Running => (format!("{} · {:.0}% · {}", i18n::t_dyn(&j.label), j.pct, theme::fmt_secs(j.elapsed_now())), MUTE),
            Done => (tf("Готово за {}", &[&theme::fmt_secs(j.elapsed)]), theme::accent()),
            Error => (i18n::t_dyn(&j.error.clone().unwrap_or_default()), RED),
            Cancelled => (t("Остановлено").into(), MUTE),
        };
        ui.add(egui::Label::new(RichText::new(text).color(color).size(13.0)).wrap());
        if let Some(tr) = &j.translated { ui.add(egui::Label::new(theme::mute(format!("EN: {tr}"))).truncate()); }
    }

    /// Main button + cancel for this tab's running jobs.
    fn go_row(&mut self, ui: &mut egui::Ui, tab: Tab, text: &str, ok: bool) -> bool {
        let running: Vec<u64> = self.jobs.all().iter().filter(|j| j.active() && self.tab_of(j.id) == Some(tab)).map(|j| j.id).collect();
        let mut go = false;
        ui.horizontal(|ui| {
            let cancel_w = if running.is_empty() { 0.0 } else { 128.0 };
            ui.allocate_ui(Vec2::new(ui.available_width() - cancel_w, 42.0), |ui| {
                go = theme::primary(ui, if running.is_empty() { text } else { t("В очередь") }, ok).clicked();
            });
            if !running.is_empty() && theme::danger(ui, t("■ Отменить")).clicked() {
                running.iter().for_each(|id| self.jobs.cancel(*id));
            }
        });
        go
    }

    /// The form fields of the open tab: this part scrolls.
    pub fn fields(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Video => self.video_fields(ui),
            Tab::Lipsync => self.lipsync_fields(ui),
            _ => self.image_fields(ui),
        }
    }

    /// Job status and the main button: pinned to the bottom of the card, so it never scrolls out of sight.
    pub fn footer(&mut self, ui: &mut egui::Ui) {
        let tab = self.tab;
        self.status_line(ui, tab);
        match tab {
            Tab::Video => {
                let ok = self.video_ok();
                if self.go_row(ui, tab, t("Создать видео"), ok) || std::mem::take(&mut self.want_submit) { self.submit_video(); }
            }
            Tab::Lipsync => {
                let ok = self.lip_ok();
                if self.go_row(ui, tab, t("Синхронизировать губы"), ok) { self.submit_lip(); }
            }
            _ => {
                let (edit, up) = (tab == Tab::Edit, tab == Tab::Upscale);
                let ok = up || if edit { self.img.model_edit.is_some() } else { self.img.model_gen.is_some() };
                let text = if up { t("Увеличить ×2") } else if edit { t("Править фото") } else { t("Сгенерировать") };
                if self.go_row(ui, tab, text, ok) || std::mem::take(&mut self.want_submit) {
                    if let Err(e) = self.submit_image() { self.toast(e, true); }
                }
            }
        }
    }

    fn image_fields(&mut self, ui: &mut egui::Ui) {
        let tab = self.tab;
        let (edit, up) = (tab == Tab::Edit, tab == Tab::Upscale);
        let model = if up { None } else { self.model_combo(ui, edit) };
        let family = model.as_ref().map(|m| m.family.clone()).unwrap_or_default();
        if up {
            let mut anime = self.img.anime;
            theme::segmented(ui, &mut anime, &[(false, t("Фото и люди")), (true, t("Аниме и арт"))]);
            self.img.anime = anime;
            ui.label(theme::mute(t("Увеличение в 2 раза с восстановлением деталей. Результат сохраняется новым файлом, исходник не меняется. До 3500 px.")));
        } else {
            let hint = if edit { t("Что изменить: «Замени куртку на красную. Лицо и фон не меняй»") } else { t("Опишите картинку: «Фото… мягкий свет из окна, 35 мм, естественная кожа»") };
            if text_box(ui, &mut self.img.prompt, hint, 5) {
                self.want_submit = ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter));
            }
            check(ui, &mut self.img.translate, t("переводить с русского на английский (локально)"));
            if family == "sdxl" || family == "qwen21" {
                text_box(ui, &mut self.img.neg, t("Негативный промпт: чего не должно быть (пусто = стандартный)"), 2);
            }
        }
        let label = if up { t("Фото для увеличения (обязательно)") } else if edit { t("Ваше фото (обязательно)") } else { t("Фото для img2img (необязательно)") };
        file_slot(self, ui, Slot::Photo1, label, IMAGE_EXT);
        if edit && family != "zimage" {
            file_slot(self, ui, Slot::Photo2, t("Второе фото для совмещения (необязательно)"), IMAGE_EXT);
        }
        let show_strength = (!edit && !up && self.img.photo1.is_some() && family != "klein") || (edit && family == "zimage");
        if show_strength {
            ui.label(theme::mute(t("Насколько менять фото")));
            slider_row(ui, &mut self.img.strength, 0.6..=1.0, 0.01, "");
            ui.label(theme::mute(t("0.6–0.8 почти как оригинал · 0.85–0.9 меняет детали и свет · 0.95+ меняет стиль")));
        }
        if !edit && !up {
            let mut size = self.img.size;
            theme::segmented(ui, &mut size, SIZES);
            self.img.size = size;
        }
        if !up {
            ui.label(theme::mute(if edit { t("Вариантов") } else { t("Картинок") }));
            theme::segmented(ui, &mut self.img.count, &[(1, "1"), (2, "2"), (4, "4")]);
            Self::seed_row(ui, &mut self.img.seed, &mut self.img.auto_seed);
        }
    }

    fn submit_image(&mut self) -> Result<(), &'static str> {
        let f = &mut self.img;
        let tab = self.tab;
        if tab != Tab::Upscale && f.prompt.trim().is_empty() { return Err(t("Введите промпт")); }
        if tab != Tab::Create && f.photo1.is_none() { return Err(t("Выберите фото")); }
        if f.auto_seed { f.seed = media::random_seed(); }
        let req = match tab {
            Tab::Upscale => Request::Upscale(UpscaleReq { image: f.photo1.clone().unwrap(), mode: if f.anime { "anime" } else { "photo" }.into() }),
            Tab::Edit => Request::Edit(EditReq { prompt: f.prompt.clone(), photos: [f.photo1.clone(), f.photo2.clone()].into_iter().flatten().collect(),
                neg: f.neg.clone(), model: f.model_edit.clone(), count: f.count, seed: Some(f.seed), strength: if f.strength > 0.95 { 0.85 } else { f.strength }, translate: f.translate }),
            _ => Request::Image(ImageReq { prompt: f.prompt.clone(), neg: f.neg.clone(), model: f.model_gen.clone(), width: f.size.0, height: f.size.1,
                count: f.count, seed: Some(f.seed), photo: f.photo1.clone(), strength: f.strength, translate: f.translate }),
        };
        self.submit(tab, req);
        Ok(())
    }

    fn size_ok(&self) -> bool {
        let ok = |n: u32| (256..=1920).contains(&n) && n % 64 == 0;
        ok(self.vid.width) && ok(self.vid.height)
    }

    fn video_ok(&self) -> bool { self.size_ok() && self.ltx.missing().is_empty() && (!self.vid.i2v || self.vid.image.is_some()) }

    fn video_fields(&mut self, ui: &mut egui::Ui) {
        let mut i2v = self.vid.i2v;
        theme::segmented(ui, &mut i2v, &[(false, t("Текст → видео")), (true, t("Фото → видео"))]);
        self.vid.i2v = i2v;
        if i2v { file_slot(self, ui, Slot::VideoImage, t("Стартовое фото (первый кадр)"), IMAGE_EXT); }
        let f = &mut self.vid;
        let hint = if f.i2v { t("Что происходит в кадре и как движется камера. Фото станет первым кадром.") }
            else { t("Кто/что в кадре, что делает, как движется камера, какой свет. Пример: «Рыжая лиса идёт по заснеженному лесу на рассвете, камера плавно едет следом, мягкий золотой свет».") };
        if text_box(ui, &mut f.prompt, hint, 5) {
            self.want_submit = ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter));
        }
        let f = &mut self.vid;
        check(ui, &mut f.translate, t("переводить с русского на английский (локально)"));
        text_box(ui, &mut f.neg, t("Негативный промпт (пусто = стандартный)"), 1);
        let files = &self.ltx;
        ui.label(theme::mute(t("Видео-модель")));
        combo(ui, "unet", &mut f.unet, &files.unet, |n| n.trim_end_matches(".gguf").to_string());
        ui.label(theme::mute(t("Текстовый энкодер")));
        combo(ui, "clip", &mut f.clip, &files.clip, |n| {
            let s = n.trim_start_matches("gemma4-12b-with-proj-ltx-2.5-").trim_end_matches(".gguf").trim_end_matches(".safetensors").to_string();
            if n.ends_with(".gguf") { format!("{s} ({})", t("хуже следует промпту")) } else { s }
        });
        let mut asp = f.aspect;
        if theme::segmented(ui, &mut asp, &[(0, "16:9"), (1, "9:16"), (2, "1:1")]) {
            f.aspect = asp;
            (f.width, f.height) = ASPECTS[asp].1[1];
        }
        let presets = ASPECTS[f.aspect].1;
        let mut cur = presets.iter().position(|p| *p == (f.width, f.height)).unwrap_or(9);
        if theme::segmented(ui, &mut cur, &[(0, t("Малый")), (1, t("Средний")), (2, t("Большой"))]) { (f.width, f.height) = presets[cur]; }
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::mute(t("Ширина")));
            ui.add(egui::DragValue::new(&mut f.width).range(256..=1920).speed(64.0));
            ui.label(theme::mute(t("Высота")));
            ui.add(egui::DragValue::new(&mut f.height).range(256..=1920).speed(64.0));
        });
        let size_ok = self.size_ok();
        let f = &mut self.vid;
        ui.label(if size_ok { theme::mute(tf("{}×{}: черновик считается вдвое меньше и увеличивается ×2, поэтому размер кратен 64", &[&f.width, &f.height])) }
            else { RichText::new(t("Размер должен быть кратен 64, от 256 до 1920")).color(RED).size(12.5) });
        let cfg: serde_json::Value = serde_json::from_str(crate::workflows::LTX_CONFIG).unwrap_or_default();
        let max_sec = cfg["max_seconds"].as_f64().unwrap_or(10.0);
        ui.label(theme::mute(t("Длительность")));
        slider_row(ui, &mut f.seconds, 1.0..=max_sec, 0.5, &format!(" {}", t("с")));
        ui.label(theme::mute(t("Кадров в секунду")));
        theme::segmented(ui, &mut f.fps, &[(24, "24"), (25, "25"), (30, "30")]);
        let frames = ((f.seconds * f.fps as f64 / 8.0).round() as u32) * 8 + 1;
        ui.label(theme::mute(tf("{} кадров · {} с при {} к/с", &[&frames, &format!("{:.1}", frames as f64 / f.fps as f64), &f.fps])));
        Self::seed_row(ui, &mut f.seed, &mut f.auto_seed);
        egui::CollapsingHeader::new(t("Дополнительно")).default_open(false).show(ui, |ui| {
            ui.label(theme::mute(t("Шаги: 8 на черновик + 3 на детали. У дистиллированной модели расписание зашито.")));
            ui.horizontal(|ui| { ui.label(theme::mute(t("CFG (оставьте 1.0)"))); ui.add(egui::DragValue::new(&mut f.cfg).range(1.0..=4.0).speed(0.1)); });
            if f.cfg > 1.0 { ui.label(RichText::new(t("CFG выше 1.0 сильно увеличивает расход видеопамяти и время.")).color(WARN).size(12.5)); }
            check(ui, &mut f.audio, t("звук (модель делает его вместе с видео)"));
        });
        // fit estimate from the smoke tests (ui\ltx\config.json): cost grows with width*height*frames
        if size_ok {
            let cost = (f.width * f.height * frames) as f64 / 1e6 * f.cfg.max(1.0).powf(1.5);
            let (safe, max) = (cfg["safe"].as_f64().unwrap_or(253.0), cfg["max"].as_f64().unwrap_or(400.0));
            let mins = (cfg["sec_base"].as_f64().unwrap_or(125.0) + cost * cfg["sec_per_mpxf"].as_f64().unwrap_or(0.6)) / 60.0;
            let (c, s) = (format!("{cost:.0}"), format!("{safe:.0}"));
            let (color, text) = if cost <= safe { (theme::accent(), tf("Поместится: {} из проверенных {} Мпикс·кадров · около {} мин, не считая первой загрузки моделей", &[&c, &s, &format!("{mins:.1}")])) }
                else if cost <= max { (WARN, tf("Выше проверенного ({} из {}): может не хватить памяти. Если упадёт, уменьшите размер или длительность.", &[&c, &s])) }
                else { (RED, tf("Скорее всего не хватит памяти: {} Мпикс·кадров при проверенных {}.", &[&c, &s])) };
            theme::info_box(ui, color, |ui| { ui.label(RichText::new(text).size(12.5)); });
        }
        let missing = self.ltx.missing();
        if !missing.is_empty() {
            theme::info_box(ui, RED, |ui| {
                ui.label(theme::bold(t("Не хватает файлов:")));
                for (what, wh) in &missing { ui.label(RichText::new(format!("• {} → {wh}", t(what))).size(12.5)); }
            });
        }
    }

    fn submit_video(&mut self) {
        let f = &mut self.vid;
        if f.prompt.trim().is_empty() { return self.toast(t("Введите промпт"), true); }
        if f.auto_seed { f.seed = media::random_seed(); }
        let req = Request::Video(VideoReq { prompt: f.prompt.clone(), mode: if f.i2v { "i2v" } else { "t2v" }.into(), neg: f.neg.clone(), image: if f.i2v { f.image.clone() } else { None },
            width: f.width, height: f.height, seconds: f.seconds, fps: f.fps, seed: Some(f.seed), cfg: f.cfg, audio: f.audio, unet: f.unet.clone(), clip: f.clip.clone(), translate: f.translate });
        self.submit(Tab::Video, req);
    }

    /// Length of the lip-sync result in seconds (from the audio and, for videos, the picture).
    fn lip_secs(&self) -> f64 {
        let f = &self.lip;
        if f.photo || f.media_secs == 0.0 { f.audio_secs } else if f.loop_video { f.audio_secs.max(f.media_secs) } else { f.audio_secs.min(f.media_secs) }
    }

    fn lip_ok(&self) -> bool { models::lipsync_installed() && self.lip.media.is_some() && self.lip.audio.is_some() && self.lip_secs() <= 60.0 }

    fn lipsync_fields(&mut self, ui: &mut egui::Ui) {
        let mut photo = self.lip.photo;
        if theme::segmented(ui, &mut photo, &[(false, t("Видео + аудио")), (true, t("Фото + аудио"))]) {
            self.lip.photo = photo;
            self.lip.media = None;
            self.lip.media_secs = 0.0;
        }
        ui.label(theme::mute(if photo { t("Фото держится кадром всю речь, двигаются только губы. Лицо анфас.") } else { t("Остальной кадр остаётся вашим, меняется только область рта. Лицо должно быть видно в каждом кадре.") }));
        if photo { file_slot(self, ui, Slot::LipMedia, t("Фото с лицом"), IMAGE_EXT); } else { file_slot(self, ui, Slot::LipMedia, t("Видео с лицом (mp4, mov, webm…)"), VIDEO_EXT); }
        file_slot(self, ui, Slot::LipAudio, t("Аудио с речью (wav, mp3, m4a…)"), AUDIO_EXT);
        let f = &mut self.lip;
        theme::segmented(ui, &mut f.fast, &[(false, t("Качество")), (true, t("Быстро"))]);
        ui.label(theme::mute(if f.fast { t("Без усиления губ: быстрее, синхронизация чуть слабее.") } else { t("Полное управление губами: точнее синхронизация, дольше в 2-3 раза.") }));
        if !f.photo {
            ui.label(theme::mute(t("Если длины разные")));
            theme::segmented(ui, &mut f.loop_video, &[(false, t("Обрезать")), (true, t("Зациклить"))]);
            ui.label(theme::mute(t("Обрезать: по более короткому из двух. Зациклить: видео повторяется до конца аудио.")));
        }
        Self::seed_row(ui, &mut f.seed, &mut f.auto_seed);
        // measured on an RTX 4080 SUPER 16 GB: seconds of work per second of result, +60 s for loading
        let (secs, fast, audio_secs) = (self.lip_secs(), self.lip.fast, self.lip.audio_secs);
        if audio_secs > 0.0 {
            let eta = 60.0 + secs * if fast { 45.0 } else { 120.0 };
            let (c, text) = if secs > 60.0 { (RED, tf("Слишком длинно: результат {} с, максимум 60 с. Обрежьте аудио.", &[&format!("{secs:.0}")])) }
                else { (if eta > 1800.0 { WARN } else { theme::accent() }, tf("Результат {} с, это займёт около {} (видеокарта будет загружена полностью).", &[&format!("{secs:.1}"), &theme::fmt_secs(eta as u64)])) };
            theme::info_box(ui, c, |ui| { ui.label(RichText::new(text).size(12.5)); });
        }
        if !models::lipsync_installed() { theme::info_box(ui, RED, |ui| { ui.label(t("LatentSync не установлен (ComfyUI-LatentSyncWrapper и его модели).")); }); }
        ui.label(theme::mute(t("«Отменить» перезапускает ComfyUI: модель нельзя остановить на ходу.")));
    }

    fn submit_lip(&mut self) {
        let f = &mut self.lip;
        if f.auto_seed { f.seed = media::random_seed(); }
        let req = Request::Lipsync(LipsyncReq { media: f.media.clone().unwrap(), audio: f.audio.clone().unwrap(), quality: if f.fast { "fast" } else { "quality" }.into(),
            fit: if f.loop_video { "loop" } else { "trim" }.into(), seed: Some(f.seed) });
        self.submit(Tab::Lipsync, req);
    }

    /// Settings: language, transparency of the glass cards, background glow.
    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open { theme::forget_sheet(ctx, "settings"); return; }
        let mut open = true;
        let width = 340.0f32.min(ctx.content_rect().width() - 100.0);
        theme::sheet(ctx, "settings", egui::Align2::RIGHT_TOP, Vec2::new(-18.0, 64.0), t("Настройки"), width, &mut open, |ui| {
            ui.label(theme::tag(t("▪ ЯЗЫК")));
            let mut l = self.prefs.lang;
            if theme::segmented(ui, &mut l, &[(Lang::Ru, "Русский"), (Lang::Uk, "Українська"), (Lang::En, "English")]) {
                self.prefs.lang = l;
                i18n::set(l);
            }
            ui.add_space(8.0);
            ui.label(theme::tag(t("▪ ПРОЗРАЧНОСТЬ ЭЛЕМЕНТОВ")));
            let mut pct = self.prefs.opacity as f64;
            slider_row(ui, &mut pct, 35.0..=100.0, 1.0, " %");
            self.prefs.opacity = pct.round() as u8;
            ui.label(theme::mute(t("Меньше — сильнее просвечивает цветное свечение фона.")));
            ui.add_space(6.0);
            ui.label(theme::tag(t("▪ РАЗМЫТИЕ ПОДЛОЖКИ")));
            let mut bl = self.prefs.blur as f64;
            slider_row(ui, &mut bl, 0.0..=100.0, 1.0, " %");
            self.prefs.blur = bl.round() as u32;
            ui.label(theme::mute(t("Матовое стекло: скрывает зерно и резкость фона за плитками.")));
            ui.add_space(6.0);
            check(ui, &mut self.prefs.glow, t("Цветное свечение фона"));
            ui.add_space(4.0);
            check(ui, &mut self.prefs.motion, t("Плавное движение фона"));
            if self.prefs.motion {
                ui.label(theme::mute(t("Скорость движения")));
                let mut sp = self.prefs.speed as f64;
                slider_row(ui, &mut sp, 0.25..=2.0, 0.05, " ×");
                self.prefs.speed = sp as f32;
            }
            ui.add_space(4.0);
            ui.label(theme::mute(t("Яркость фона")));
            let mut b = self.prefs.brightness as f64;
            slider_row(ui, &mut b, 40.0..=200.0, 1.0, " %");
            self.prefs.brightness = b.round() as u32;
            ui.label(theme::mute(t("Шум (зерно) на фоне")));
            let mut n = self.prefs.noise as f64;
            slider_row(ui, &mut n, 0.0..=100.0, 1.0, " %");
            self.prefs.noise = n.round() as u32;
            if self.prefs.noise > 0 {
                ui.label(theme::mute(t("Размер зерна")));
                theme::segmented(ui, &mut self.prefs.grain, &[(1, "1 px"), (2, "2 px"), (3, "3 px"), (4, "4 px")]);
            }
            ui.add_space(8.0);
            ui.label(theme::tag(t("▪ МАСШТАБ")));
            theme::segmented(ui, &mut self.prefs.scale, &[(0, t("Авто")), (100, "100%"), (125, "125%"), (150, "150%")]);
            ui.add_space(8.0);
            ui.label(theme::tag(t("▪ ЦВЕТА ВКЛАДОК")));
            for (c, l) in [(theme::PHOTO, t("Фото: создание, правка, апскейл")), (theme::VIDEO, t("Видео")), (theme::LIPSYNC, t("Липсинк"))] {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, theme::PILL, c);
                    ui.label(l);
                });
            }
            ui.add_space(8.0);
            ui.label(theme::tag(t("▪ АВТОМАТИЗАЦИЯ")));
            ui.label(theme::mute(tf("Claude Code и скрипты управляют Студией через studio-cli или API 127.0.0.1:{} (см. CLAUDE.md).", &[&crate::API_PORT])));
        });
        if !open { self.settings_open = false; }
    }
}

/// Switch with a text label.
fn check(ui: &mut egui::Ui, value: &mut bool, label: &str) { theme::toggle(ui, value, label); }

/// Text box: pill-shaped when one line, 22 px round when tall; the padding keeps the text clear of the curve.
/// Returns true while it has keyboard focus.
fn text_box(ui: &mut egui::Ui, text: &mut String, hint: &str, rows: usize) -> bool {
    let out = theme::adaptive_frame(ui, theme::field(), egui::Stroke::new(1.0, theme::LINE), egui::Margin::symmetric(18, 11), 22, |ui| {
        let edit = if rows <= 1 { egui::TextEdit::singleline(text) } else { egui::TextEdit::multiline(text).desired_rows(rows) };
        ui.add(edit.hint_text(hint).frame(egui::Frame::NONE).desired_width(f32::INFINITY)).has_focus()
    });
    if out.inner {
        let r = if out.response.rect.height() <= 46.0 { theme::PILL } else { 22 };
        ui.painter().rect_stroke(out.response.rect, r, egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.7)), egui::StrokeKind::Inside);
    }
    out.inner
}

/// Slider with its value box on the right edge: a thin rail, accent fill up to a round handle, drag or click to set.
fn slider_row(ui: &mut egui::Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>, step: f64, suffix: &str) {
    ui.horizontal(|ui| {
        let (lo, hi) = (*range.start(), *range.end());
        let box_w = 70.0;
        let w = (ui.available_width() - box_w - ui.spacing().item_spacing.x).max(80.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), egui::Sense::click_and_drag());
        if let (true, Some(p)) = (resp.is_pointer_button_down_on() || resp.dragged(), resp.interact_pointer_pos()) {
            let f = ((p.x - rect.left() - 9.0) / (rect.width() - 18.0)).clamp(0.0, 1.0) as f64;
            *value = ((lo + f * (hi - lo)) / step).round() * step;
        }
        *value = value.clamp(lo, hi);
        let f = ((*value - lo) / (hi - lo)) as f32;
        let x = rect.left() + 9.0 + f * (rect.width() - 18.0);
        let cy = rect.center().y;
        let a = theme::accent();
        let p = ui.painter();
        p.line_segment([egui::pos2(rect.left() + 9.0, cy), egui::pos2(rect.right() - 9.0, cy)], egui::Stroke::new(4.0, Color32::from_white_alpha(22)));
        p.line_segment([egui::pos2(rect.left() + 9.0, cy), egui::pos2(x, cy)], egui::Stroke::new(4.0, a.gamma_multiply(0.75)));
        p.circle_filled(egui::pos2(x, cy), 9.0, Color32::from_rgb(240, 240, 238));
        p.circle_stroke(egui::pos2(x, cy), 9.0, egui::Stroke::new(2.0, a));
        ui.add_sized([box_w, 28.0], egui::DragValue::new(value).range(range).speed(step).fixed_decimals(if step < 0.1 { 2 } else { 1 }).suffix(suffix));
    });
}

/// Drop-down of exactly `width`, long names cut with an ellipsis. Drawn by hand (not `egui::ComboBox`) so the list can be
/// complete at once and show a scroll bar only when it really is longer than the window allows: ComboBox's own scroll area
/// counted a pixel or two of hover outline as overflow and flashed a bar over short lists.
fn combo_box(ui: &mut egui::Ui, _id: impl std::hash::Hash + std::fmt::Debug, width: f32, selected: String, add: impl FnOnce(&mut egui::Ui)) {
    let font = egui::FontId::proportional(14.0);
    let fits = |s: &str| ui.fonts_mut(|f| f.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE).size().x) <= width - 50.0;
    let mut shown = selected;
    if !fits(&shown) {
        let mut chars: Vec<char> = shown.chars().collect();
        while chars.len() > 1 && !fits(&format!("{}…", chars.iter().collect::<String>())) { chars.pop(); }
        shown = format!("{}…", chars.iter().collect::<String>());
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 32.0), egui::Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let hov = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered() || open, 0.14);
    let fill = Color32::from_rgb(26, 26, 29).lerp_to_gamma(Color32::from_rgb(34, 34, 38), hov);
    let line = theme::LINE.lerp_to_gamma(theme::accent().gamma_multiply(0.6), hov);
    ui.painter().rect(rect, theme::PILL, fill, egui::Stroke::new(1.0, line), egui::StrokeKind::Inside);
    ui.painter().text(egui::pos2(rect.left() + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, shown, font, theme::TEXT);
    let (cx, cy) = (rect.right() - 18.0, rect.center().y);
    ui.painter().add(egui::Shape::convex_polygon(vec![egui::pos2(cx - 4.5, cy - 2.5), egui::pos2(cx + 4.5, cy - 2.5), egui::pos2(cx, cy + 3.0)], theme::TEXT, egui::Stroke::NONE));
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let max_h = ui.spacing().combo_height;
    let needs_bar_id = resp.id.with("needs_bar");
    let needs_bar = ui.ctx().data(|d| d.get_temp::<bool>(needs_bar_id)).unwrap_or(false);
    egui::Popup::menu(&resp).width(width).show(|ui| {
        ui.set_min_width(width - 2.0 * ui.spacing().menu_margin.left as f32);
        ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);   // entry text stays clear of the rounded highlight
        let bars = if needs_bar { egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded } else { egui::scroll_area::ScrollBarVisibility::AlwaysHidden };
        let out = egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).scroll_bar_visibility(bars).show(ui, |ui| add(ui));
        // a real overflow (more than the odd pixel of hover outline) turns the bar on from the next frame
        let over = out.content_size.y > out.inner_rect.height() + 6.0;
        ui.ctx().data_mut(|d| d.insert_temp(needs_bar_id, over));
    });
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut Option<String>, list: &[String], label: impl Fn(&str) -> String) {
    if value.as_ref().is_none_or(|v| !list.contains(v)) { *value = list.first().cloned(); }
    let w = ui.available_width();
    combo_box(ui, id, w, value.as_deref().map(&label).unwrap_or(t("нет файлов").into()), |ui| {
        for n in list { ui.selectable_value(value, Some(n.clone()), label(n)); }
    });
}

/// "Модели" window: add an image model from a file or an https link, or delete one.
#[derive(Default)]
pub struct ModelSheet {
    by_url: bool,
    file: Option<PathBuf>,
    url: String,
    name: String,
    family: usize,
    task: Option<Arc<Mutex<(u64, u64, Option<Result<(), String>>)>>>,
    status: Option<(String, bool)>,
}

impl App {
    pub fn model_sheet(&mut self, ctx: &egui::Context) {
        let Some(mut s) = self.sheet.take() else { theme::forget_sheet(ctx, "models"); return };
        let mut open = true;
        let width = 500.0f32.min(ctx.content_rect().width() - 100.0);
        theme::sheet(ctx, "models", egui::Align2::CENTER_CENTER, Vec2::ZERO, t("Модели"), width, &mut open, |ui| {
                theme::segmented(ui, &mut s.by_url, &[(false, t("Файл с компьютера")), (true, t("По ссылке"))]);
                if s.by_url {
                    if ui.add(egui::TextEdit::singleline(&mut s.url).margin(egui::Margin::symmetric(18, 10)).hint_text(t("https://… прямая ссылка на .safetensors")).desired_width(f32::INFINITY)).changed() {
                        if let Some(n) = s.url.rsplit('/').next().map(|n| n.split('?').next().unwrap_or(n)).filter(|n| n.ends_with(".safetensors")) { s.name = n.to_string(); }
                    }
                } else {
                    ui.horizontal(|ui| {
                        if theme::ghost(ui, t("Выбрать файл…")).clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("safetensors", &["safetensors"]).pick_file() {
                                s.name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                s.file = Some(p);
                            }
                        }
                        if let Some(p) = &s.file { ui.add(egui::Label::new(theme::mute(p.display().to_string())).truncate()); }
                    });
                }
                ui.label(theme::mute(t("Имя файла")));
                ui.add(egui::TextEdit::singleline(&mut s.name).margin(egui::Margin::symmetric(18, 10)).desired_width(f32::INFINITY));
                ui.label(theme::mute(t("Тип модели (от него зависит, как она запускается)")));
                egui::ComboBox::from_id_salt("family").width(ui.available_width()).selected_text(t(models::FAMILIES[s.family].1)).show_ui(ui, |ui| {
                    for (i, (_, l)) in models::FAMILIES.iter().enumerate() { ui.selectable_value(&mut s.family, i, t(*l)); }
                });
                ui.label(theme::mute(t("Только .safetensors: форматы .ckpt и .pt могут запускать код.")));
                let busy = s.task.as_ref().is_some_and(|tk| tk.lock().unwrap().2.is_none());
                if theme::primary(ui, t("Добавить"), !busy).clicked() {
                    let (name, family, by_url, file, url) = (s.name.trim().to_string(), models::FAMILIES[s.family].0.to_string(), s.by_url, s.file.clone(), s.url.trim().to_string());
                    if !models::valid_name(&name) { s.status = Some((t("Имя должно заканчиваться на .safetensors").into(), true)); }
                    else if !by_url && file.is_none() { s.status = Some((t("Выберите файл").into(), true)); }
                    else {
                        let task = Arc::new(Mutex::new((0, 0, None)));
                        let (tk, c) = (task.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let mut prog = |d: u64, total: u64| { let mut g = tk.lock().unwrap(); g.0 = d; g.1 = total; c.request_repaint(); true };
                            let r = if by_url { models::add_url(&url, &name, &family, &mut prog) } else { models::add_file(file.as_deref().unwrap(), &name, &family, &mut prog) };
                            tk.lock().unwrap().2 = Some(r.map_err(|e| e.to_string()));
                            c.request_repaint();
                        });
                        s.task = Some(task);
                    }
                }
                if let Some(tk) = &s.task {
                    let g = tk.lock().unwrap();
                    match &g.2 {
                        None => { ui.add(egui::ProgressBar::new(if g.1 > 0 { g.0 as f32 / g.1 as f32 } else { 0.0 }).text(tf("{} из {}", &[&theme::fmt_size(g.0), &if g.1 > 0 { theme::fmt_size(g.1) } else { "?".into() }]))); }
                        Some(Ok(())) => { s.status = Some((t("Готово, модель добавлена").into(), false)); }
                        Some(Err(e)) => { s.status = Some((e.clone(), true)); }
                    }
                }
                if s.task.as_ref().is_some_and(|tk| tk.lock().unwrap().2.is_some()) { s.task = None; self.refresh_models(); }
                if let Some((m, err)) = &s.status { ui.label(RichText::new(m).color(if *err { RED } else { theme::accent() })); }
                ui.separator();
                ui.label(theme::tag(t("▪ УСТАНОВЛЕННЫЕ МОДЕЛИ")));
                for m in self.models.clone() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| { ui.label(&m.name); ui.label(theme::mute(format!("{} · {}", m.family, theme::fmt_size(m.size)))); });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::danger(ui, t("Удалить")).clicked() { self.confirm = Some(super::Confirm::Model(m.name.clone())); }
                        });
                    });
                }
            });
        if open { self.sheet = Some(s); }
    }
}
