//! The Studio window (egui). Same tabs and functions as the old web UI; results open in the Windows default app on click.
pub mod forms;
mod aurora;
pub mod i18n;
mod icons;
mod theme;
mod thumbs;

use crate::api;
use crate::engine::Engine;
use crate::jobs::{Jobs, LipsyncReq, Request, State, VideoReq};
use crate::models::{self, LtxFiles, Model};
use crate::results::{self, Item, Kind};
use crate::{media, API_PORT};
use eframe::egui::{self, Color32, Margin, RichText, Sense, Vec2};
use forms::{ImageForm, LipForm, Prefs, VideoForm};
use icons::Icon;
use i18n::{t, tf};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::{MUTE, RED, TEXT};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Default)]
pub enum Tab { #[default] Create, Edit, Upscale, Video, Lipsync }

impl Tab {
    fn kind(self) -> Kind { match self { Tab::Video => Kind::Videos, Tab::Lipsync => Kind::Lipsync, _ => Kind::Photos } }
    fn of(r: &Request) -> Tab {
        match r { Request::Image(_) => Tab::Create, Request::Edit(_) => Tab::Edit, Request::Upscale(_) => Tab::Upscale, Request::Video(_) => Tab::Video, Request::Lipsync(_) => Tab::Lipsync }
    }
    fn accent(self) -> Color32 { match self { Tab::Video => theme::VIDEO, Tab::Lipsync => theme::LIPSYNC, _ => theme::PHOTO } }
    /// (section number, grey heading part, white heading part)
    fn heading(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Tab::Create => ("01", t("Создание"), t("фото")),
            Tab::Edit => ("02", t("Правка"), t("фото")),
            Tab::Upscale => ("03", t("Апскейл"), "×2"),
            Tab::Video => ("04", t("Видео"), "LTX-2.5"),
            Tab::Lipsync => ("05", t("Липсинк"), "LatentSync"),
        }
    }
}

pub enum Confirm { One(PathBuf), All(Kind, usize), Model(String) }

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved { tab: Tab, img: ImageForm, vid: VideoForm, lip: LipForm, prefs: Prefs }

pub struct Options { pub api: bool }

pub struct App {
    pub jobs: Arc<Jobs>,
    thumbs: thumbs::Thumbs,
    pub tab: Tab,
    pub img: ImageForm,
    pub vid: VideoForm,
    pub lip: LipForm,
    pub prefs: Prefs,
    pub settings_open: bool,
    pub models: Vec<Model>,
    pub ltx: LtxFiles,
    results: HashMap<Kind, Vec<Item>>,
    scanned: Instant,
    seen_finished: usize,
    pub confirm: Option<Confirm>,
    toast: Option<(String, Instant, bool)>,
    pub sheet: Option<forms::ModelSheet>,
    api_note: Option<String>,
    api_quit: Arc<AtomicBool>,
    engine_up: Arc<AtomicBool>,
    pub last: HashMap<Tab, u64>,
    prompts: HashMap<PathBuf, Option<String>>,
    pub want_submit: bool,
    title_lang: Option<i18n::Lang>,
    footer_h: f32,
    // motion: the accent colour gliding between tabs
    acc: (Color32, Color32, f64),
    bg: (Color32, Color32, f64),   // the background glow changes colour far more slowly than the controls
}

impl App {
    pub fn new(ctx: &egui::Context, storage: Option<&dyn eframe::Storage>, opts: Options) -> Self {
        theme::setup(ctx);
        let engine = Engine::new();
        let jobs = Jobs::new(engine.clone());
        let api_quit = Arc::new(AtomicBool::new(false));
        let api_note = if opts.api { start_api(&jobs, &api_quit) } else { None };
        let engine_up = Arc::new(AtomicBool::new(false));
        {
            let (e, up, c) = (engine, engine_up.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let now = e.up();
                if up.swap(now, Ordering::Relaxed) != now { c.request_repaint(); }
                std::thread::sleep(Duration::from_secs(2));
            });
        }
        let saved: Saved = storage.and_then(|s| eframe::get_value(s, "studio")).unwrap_or_default();
        i18n::set(saved.prefs.lang);
        let mut app = Self {
            thumbs: thumbs::Thumbs::new(ctx), jobs, tab: saved.tab, img: saved.img, vid: saved.vid, lip: saved.lip, prefs: saved.prefs, settings_open: false,
            models: vec![], ltx: LtxFiles::default(), results: HashMap::new(), scanned: Instant::now() - Duration::from_secs(60), seen_finished: 0,
            confirm: None, toast: None, sheet: None, api_note, api_quit, engine_up, last: HashMap::new(), prompts: HashMap::new(), want_submit: false, title_lang: None, footer_h: 60.0,
            acc: (saved.tab.accent(), saved.tab.accent(), -10.0), bg: (saved.tab.accent(), saved.tab.accent(), -10.0),
        };
        app.refresh_models();
        app
    }

    pub fn refresh_models(&mut self) {
        self.models = models::image_models();
        self.ltx = models::ltx_files();
    }

    /// Only problems get a message; a success ("Удалено") is already visible, the tile or model is simply gone.
    pub fn toast(&mut self, text: &str, err: bool) {
        if err { self.toast = Some((text.to_string(), Instant::now(), err)); }
    }

    pub fn submit(&mut self, tab: Tab, req: Request) {
        let id = self.jobs.submit(req);
        self.last.insert(tab, id);
    }

    pub fn tab_of(&self, id: u64) -> Option<Tab> { self.jobs.get(id).map(|j| Tab::of(&j.request)) }

    /// Rescans the output folders every few seconds and right after a job finishes.
    fn refresh_results(&mut self) {
        let finished = self.jobs.all().iter().filter(|j| !j.active()).count();
        if finished != self.seen_finished || self.scanned.elapsed() > Duration::from_secs(4) {
            self.seen_finished = finished;
            self.scanned = Instant::now();
            for k in [Kind::Photos, Kind::Videos, Kind::Lipsync] { self.results.insert(k, results::list(k)); }
        }
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        let tabs = [(Tab::Create, t("Создать")), (Tab::Edit, t("Править")), (Tab::Upscale, t("Апскейл")), (Tab::Video, t("Видео")), (Tab::Lipsync, t("Липсинк"))];
        let mut cur = self.tab;
        theme::segmented_h(ui, &mut cur, &tabs, theme::HEADER_H - 8.0); // frame padding + border add 8 px: the whole control is HEADER_H high
        self.tab = cur;
    }

    /// One line, one height: logo, title, tabs, status chip and settings button are all HEADER_H tall and centred together.
    fn header(&mut self, ui: &mut egui::Ui, narrow: bool) {
        let h = theme::HEADER_H;
        let (up, busy) = (self.engine_up.load(Ordering::Relaxed), self.jobs.busy());
        let (dot, status) = if busy { (theme::accent(), t("работает")) } else if up { (theme::accent(), t("ComfyUI готов")) } else { (theme::DIM, t("ComfyUI спит")) };
        let api_note = self.api_note.clone();
        let mut gear = false;
        let mut cur = self.tab;
        let ctx = ui.ctx().clone();
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        // the whole header line drags the window (the controls added later sit on top of it); double click maximises
        let drag = ui.interact(ui.max_rect(), egui::Id::new("title_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
        if drag.double_clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized)); }
        egui::containers::Sides::new().height(h).spacing(8.0).shrink_left().show(ui,
            |ui| {
                theme::logo(ui);
                ui.label(theme::light(t("Студия")).size(21.0).color(TEXT));
                if !narrow && !ui.is_sizing_pass() { ui.add(egui::Label::new(theme::tag(t("▪ ЛОКАЛЬНАЯ AI-СТУДИЯ"))).truncate()); }
            },
            |ui| {
                // the window has no system title bar: its own buttons, one round icon each
                for (icon, tip, close) in [(Icon::Close, t("Закрыть"), true), (if maximized { Icon::Restore } else { Icon::Maximize }, if maximized { t("Восстановить") } else { t("Развернуть") }, false), (Icon::Minimize, t("Свернуть"), false)] {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(h), Sense::hover());
                    if theme::window_button(ui, r, icon, tip, close) {
                        ctx.send_viewport_cmd(match (icon, maximized) {
                            (Icon::Close, _) => egui::ViewportCommand::Close,
                            (Icon::Minimize, _) => egui::ViewportCommand::Minimized(true),
                            (_, m) => egui::ViewportCommand::Maximized(!m),
                        });
                    }
                }
                ui.add_space(8.0);
                let (r, _) = ui.allocate_exact_size(Vec2::splat(h), Sense::hover());
                ui.painter().circle(r.center(), h / 2.0 - 0.5, theme::surface(), egui::Stroke::new(1.0, theme::LINE));
                gear = theme::icon_button(ui, r, Icon::Gear, t("Настройки"), false);
                // drawn by hand at exactly HEADER_H so its centre line matches the tabs and the gear
                let galley = ui.painter().layout_no_wrap(status.to_string(), egui::FontId::monospace(11.0), MUTE);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(16.0 + 10.0 + 8.0 + galley.size().x + 16.0, h), Sense::hover());
                ui.painter().rect(rect, theme::PILL, theme::surface(), egui::Stroke::new(1.0, theme::LINE), egui::StrokeKind::Inside);
                ui.painter().circle_filled(egui::pos2(rect.left() + 21.0, rect.center().y), 5.0, dot);
                ui.painter().galley(egui::pos2(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0), galley, MUTE);
                resp.on_hover_text(tf("Локально, без лимитов. API для Claude Code: 127.0.0.1:{}", &[&API_PORT]));
                if let Some(n) = &api_note { ui.label(RichText::new("⚠ API").color(theme::WARN).size(12.5)).on_hover_text(n.clone()); }
                if !narrow {
                    ui.allocate_ui_with_layout(Vec2::new(ui.available_width().min(560.0), h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let tabs = [(Tab::Create, t("Создать")), (Tab::Edit, t("Править")), (Tab::Upscale, t("Апскейл")), (Tab::Video, t("Видео")), (Tab::Lipsync, t("Липсинк"))];
                        theme::segmented_h(ui, &mut cur, &tabs, h - 8.0);
                    });
                }
            });
        self.tab = cur;
        if gear { self.settings_open = !self.settings_open; }
        if narrow { ui.add_space(8.0); self.tabs(ui); }
    }

    /// `pinned`: wide layout, fields scroll and the main button stays at the bottom of the card.
    fn composer(&mut self, ui: &mut egui::Ui, pinned: bool) {
        let w = ui.available_width();
        ui.set_max_width(w);
        let (n, grey, white) = self.tab.heading();
        ui.label(theme::tag(format!("▪ {n} · {}", match self.tab.kind() { Kind::Photos => t("ФОТО"), Kind::Videos => t("ВИДЕО"), Kind::Lipsync => t("ЛИПСИНК") })));
        theme::heading(ui, grey, white);
        ui.add_space(4.0);
        if pinned {
            // the fields scroll in whatever height is left after the footer, whose height is measured each frame
            let room = (ui.available_height() - self.footer_h - 10.0).max(120.0);
            let out = egui::ScrollArea::vertical().id_salt(("fields", self.tab)).auto_shrink(false).max_height(room).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.fields(ui);
                ui.add_space(8.0);
            });
            if out.content_size.y > out.inner_rect.height() + 1.0 && out.state.offset.y + out.inner_rect.height() < out.content_size.y - 1.0 {
                theme::fade_bottom(ui, out.inner_rect, 28.0);   // content continues below: soften the cut edge
            }
            ui.add_space(6.0);
            let measured = ui.vertical(|ui| self.footer(ui)).response.rect.height();
            if (measured - self.footer_h).abs() > 0.5 { self.footer_h = measured; ui.ctx().request_discard("footer height changed"); }
        } else {
            self.fields(ui);
            ui.add_space(8.0);
            self.footer(ui);
        }
    }

    fn gallery(&mut self, ui: &mut egui::Ui, scroll: bool) {
        let kind = self.tab.kind();
        let items = self.results.get(&kind).cloned().unwrap_or_default();
        let running: Vec<crate::jobs::Job> = self.jobs.all().into_iter().filter(|j| j.active() && j.request.result_kind() == kind).collect();
        let narrow = ui.available_width() < 700.0;
        // same two rows as the composer (tag, then a light heading), so both cards' titles sit on one line
        ui.label(theme::tag(format!("▪ {}", t("ГАЛЕРЕЯ"))));
        // `rtl`: laid out right to left, so the order is reversed to read "Папка, Удалить всё" either way
        let buttons = |ui: &mut egui::Ui, this: &mut App, rtl: bool| {
            let del = |ui: &mut egui::Ui, this: &mut App| {
                if !items.is_empty() && theme::danger(ui, t("Удалить всё")).on_hover_text(t("Удалить все файлы этой вкладки с диска")).clicked() {
                    this.confirm = Some(Confirm::All(kind, items.len()));
                }
            };
            let folder = |ui: &mut egui::Ui| {
                if theme::ghost_icon(ui, t("Папка"), Some(Icon::Folder)).on_hover_text(t("Открыть папку с результатами")).clicked() {
                    let dir = crate::paths::output().join(match kind { Kind::Photos => "", Kind::Videos => "video", Kind::Lipsync => "lipsync" });
                    let _ = std::fs::create_dir_all(&dir);
                    media::open_default(&dir);
                }
            };
            if rtl { del(ui, this); folder(ui); } else { folder(ui); del(ui, this); }
        };
        let (grey, white) = (t("Результаты"), items.len().to_string());
        if narrow {
            theme::heading(ui, grey, &white);
            ui.horizontal(|ui| buttons(ui, self, false));
        } else {
            ui.horizontal(|ui| {
                theme::heading_inline(ui, grey, &white);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| buttons(ui, self, true));
            });
        }
        ui.label(theme::mute(t("клик открывает файл в программе Windows по умолчанию")));
        ui.add_space(6.0);
        let mut body = |ui: &mut egui::Ui| {
            let avail = ui.available_width();
            if items.is_empty() && running.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::light(t("Здесь появятся результаты")).size(20.0).color(MUTE));
                    ui.label(theme::mute(tf("Файлы сохраняются в {}", &[&crate::paths::output().display()])));
                });
                return;
            }
            let gap = 12.0;
            let min = if avail < 560.0 { 140.0 } else { 210.0 };
            let cols = (((avail + gap) / (min + gap)).floor() as usize).max(1);
            let tw = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
            let th = if kind == Kind::Photos { tw } else { (tw * 0.625).floor() };
            let tiles: Vec<Option<&Item>> = running.iter().map(|_| None).chain(items.iter().map(Some)).collect();
            for (r, row) in tiles.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (c, tile) in row.iter().enumerate() {
                        // a running job's tile holds a title, percentage, stage, bar and cancel button: taller than a short video tile
                        let rect = ui.allocate_space(Vec2::new(tw, if tile.is_none() { th.max(196.0) } else { th })).1;
                        match tile {
                            None => self.job_tile(ui, rect, &running[r * cols + c]),
                            Some(it) => self.item_tile(ui, rect, it, kind),
                        }
                    }
                });
                ui.add_space(gap - ui.spacing().item_spacing.y);
            }
        };
        if scroll { egui::ScrollArea::vertical().id_salt(("gallery", kind)).auto_shrink(false).show(ui, |ui| body(ui)); } else { body(ui); }
    }

    fn job_tile(&mut self, ui: &mut egui::Ui, rect: egui::Rect, j: &crate::jobs::Job) {
        let a = theme::accent();
        let p = ui.painter();
        p.rect_filled(rect, theme::SURFACE_R, theme::field());
        p.rect_stroke(rect, theme::SURFACE_R, egui::Stroke::new(1.0, a.gamma_multiply(0.4)), egui::StrokeKind::Inside);
        let inner = rect.shrink(14.0);
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));   // not scope_builder: that moves the row cursor and shifts the next tile
        let ui = &mut ui;
        {
            ui.vertical_centered(|ui| {
                ui.add_space(((inner.height() - 140.0) / 2.0).max(0.0));
                ui.label(theme::tag(format!("▪ {}", t(j.request.title()))));
                ui.label(theme::light(format!("{:.0}%", j.pct)).size(30.0).color(TEXT));
                ui.add(egui::Label::new(RichText::new(i18n::t_dyn(&j.label)).color(MUTE).size(12.0)).truncate());
                ui.add(egui::ProgressBar::new(j.pct / 100.0).desired_width(inner.width()).desired_height(4.0).fill(a));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if j.state == State::Queued { t("в очереди").into() } else { theme::fmt_secs(j.elapsed_now()) }).monospace().color(MUTE).size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if theme::danger(ui, t("Отменить")).clicked() { self.jobs.cancel(j.id); } });
                });
            });
        }
        ui.ctx().request_repaint_after(Duration::from_millis(400));
    }

    fn item_tile(&mut self, ui: &mut egui::Ui, rect: egui::Rect, it: &Item, kind: Kind) {
        let ctx = ui.ctx().clone();
        let hov = ctx.animate_bool_with_time_and_easing(egui::Id::new(("tile", &it.path)), ui.rect_contains_pointer(rect), 0.28, egui::emath::easing::cubic_out);
        let loaded = self.thumbs.get(&ctx, &it.path, 512);
        // a thumbnail that arrives late fades in over the empty tile instead of popping in
        let shown = ctx.animate_bool_with_time(egui::Id::new(("thumb", &it.path)), loaded.is_some(), 0.3);
        ui.painter().rect_filled(rect, theme::SURFACE_R, theme::field());
        let resp = match loaded {
            Some(tex) => {
                // cover: crop the texture to the tile's aspect ratio
                let (ts, r) = (tex.size_vec2(), rect.size());
                let (sx, sy) = if ts.x / ts.y > r.x / r.y { (r.x / r.y * ts.y / ts.x, 1.0) } else { (1.0, r.y / r.x * ts.x / ts.y) };
                let z = 1.0 - 0.06 * hov;   // slow zoom-in under the pointer
                let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), Vec2::new(sx * z, sy * z));
                ui.put(rect, egui::Image::new(&tex).uv(uv).fit_to_exact_size(r).maintain_aspect_ratio(false).corner_radius(theme::SURFACE_R).tint(Color32::WHITE.gamma_multiply(shown)).sense(Sense::click()))
            }
            None => {
                let r = ui.allocate_rect(rect, Sense::click());
                if self.thumbs.failed(&it.path) {
                    let s = if kind == Kind::Photos { "▣" } else { "▶" };
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, s, egui::FontId::proportional(26.0), MUTE);
                }
                r
            }
        };
        ui.painter().rect_stroke(rect, theme::SURFACE_R, egui::Stroke::new(1.0, Color32::from_white_alpha(16)), egui::StrokeKind::Inside);
        if kind != Kind::Photos {
            let c = rect.center();
            ui.painter().circle_filled(c, 19.0, Color32::from_black_alpha(150));
            ui.painter().circle_stroke(c, 19.0, egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.8)));
            ui.painter().text(c + Vec2::new(2.0, 0.0), egui::Align2::CENTER_CENTER, "▶", egui::FontId::proportional(16.0), Color32::WHITE);
        }
        let chip = match kind {
            Kind::Videos => it.meta["prompt"].as_str().map(|_| format!("{}×{} · {} {}", it.meta["width"], it.meta["height"], it.meta["seconds"], t("с"))),
            Kind::Lipsync => it.meta["mode"].as_str().map(|m| format!("{} · {:.1} {}", if m == "p" { t("Фото + аудио") } else { t("Видео + аудио") }, it.meta["audioSec"].as_f64().unwrap_or(0.0), t("с"))),
            Kind::Photos => None,
        };
        if let Some(c) = chip {
            let g = ui.painter().layout_no_wrap(c, egui::FontId::proportional(11.5), Color32::WHITE);
            let r = egui::Rect::from_min_size(rect.left_top() + Vec2::new(10.0, 10.0), g.size() + Vec2::new(16.0, 8.0));
            ui.painter().rect_filled(r, theme::PILL, Color32::from_black_alpha(150));
            ui.painter().galley(r.min + Vec2::new(8.0, 4.0), g, Color32::WHITE);
        }
        let cap = match kind {
            Kind::Videos => it.meta["prompt"].as_str().map(|p| format!("{}×{} · {} {} · {p}", it.meta["width"], it.meta["height"], it.meta["seconds"], t("с"))),
            Kind::Lipsync => it.meta["mode"].as_str().map(|m| format!("{} · {:.1} {}", if m == "p" { t("Фото + аудио") } else { t("Видео + аудио") }, it.meta["audioSec"].as_f64().unwrap_or(0.0), t("с"))),
            Kind::Photos => None,
        };
        let resp = resp.on_hover_text(format!("{}\n{}{}\n{}", it.name, theme::fmt_size(it.size), cap.as_deref().map(|c| format!("\n{c}")).unwrap_or_default(), t("Клик — открыть")));
        if resp.clicked() { media::open_default(&it.path); }
        // action bar: fades and rises in on hover; the icons are spread evenly across it
        let a = ctx.animate_bool_with_time_and_easing(egui::Id::new(("bar", &it.path)), ui.rect_contains_pointer(rect), 0.22, egui::emath::easing::cubic_out);
        if a < 0.01 { return; }
        #[derive(Clone, Copy, PartialEq)]
        enum Act { Reveal, Upscale, Edit, Prompt, Same, Again, Delete }
        let mut acts: Vec<(Icon, &str, Act)> = vec![(Icon::Folder, t("Показать в папке"), Act::Reveal)];
        let mut prompt = None;
        match kind {
            Kind::Photos => {
                acts.push((Icon::Expand, t("Увеличить ×2"), Act::Upscale));
                acts.push((Icon::Pencil, t("Править это фото"), Act::Edit));
                prompt = self.prompts.entry(it.path.clone()).or_insert_with(|| results::png_prompt(&it.path)).clone();
                if prompt.is_some() { acts.push((Icon::Chat, t("Вставить промпт этого фото"), Act::Prompt)); }
            }
            _ if it.meta.is_object() => {
                acts.push((Icon::Sliders, t("Те же настройки"), Act::Same));
                acts.push((Icon::Refresh, t("Ещё раз, новый seed"), Act::Again));
            }
            _ => {}
        }
        acts.push((Icon::Trash, t("Удалить с диска"), Act::Delete));
        let bar_h = 46.0;
        let lift = (1.0 - a) * 8.0;
        let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0 - bar_h + lift), egui::pos2(rect.right() - 10.0, rect.bottom() - 10.0 + lift));
        let mut c = ui.new_child(egui::UiBuilder::new().id_salt(("bar", &it.path)).max_rect(bar));
        c.set_opacity(a);
        c.painter().rect(bar, theme::PILL, Color32::from_rgba_unmultiplied(16, 16, 18, 222), egui::Stroke::new(1.0, Color32::from_white_alpha(24)), egui::StrokeKind::Inside);
        // the first and last icon sit one bar radius from the ends, so their hover discs stay inside the rounded bar
        let end = bar.height() / 2.0;
        let span = (bar.width() - 2.0 * end).max(0.0);
        let step = span / (acts.len() as f32 - 1.0).max(1.0);
        let size = step.clamp(22.0, 34.0);
        let mut chosen = None;
        for (i, (icon, tip, act)) in acts.iter().enumerate() {
            let r = egui::Rect::from_center_size(egui::pos2(bar.left() + end + step * i as f32, bar.center().y), Vec2::splat(size));
            if theme::icon_button(&mut c, r, *icon, tip, *act == Act::Delete) { chosen = Some(*act); }
        }
        match chosen {
            Some(Act::Reveal) => media::reveal(&it.path),
            Some(Act::Upscale) => { self.tab = Tab::Upscale; self.img.photo1 = Some(it.path.clone()); }
            Some(Act::Edit) => { self.tab = Tab::Edit; self.img.photo1 = Some(it.path.clone()); }
            Some(Act::Prompt) => { self.tab = Tab::Create; self.img.prompt = prompt.unwrap_or_default(); }
            Some(Act::Same) => self.reuse(kind, &it.meta),
            Some(Act::Again) => self.regen(kind, &it.meta),
            Some(Act::Delete) => self.confirm = Some(Confirm::One(it.path.clone())),
            None => {}
        }
    }

    fn reuse(&mut self, kind: Kind, m: &serde_json::Value) {
        let s = |k: &str| m[k].as_str().map(str::to_string);
        if kind == Kind::Videos {
            let v = &mut self.vid;
            v.i2v = m["mode"] == "i2v";
            v.prompt = s("prompt").unwrap_or_default();
            v.neg = s("negative").unwrap_or_default();
            v.width = m["width"].as_u64().unwrap_or(1024) as u32;
            v.height = m["height"].as_u64().unwrap_or(576) as u32;
            v.seconds = m["seconds"].as_f64().unwrap_or(5.0);
            v.fps = m["fps"].as_u64().unwrap_or(24) as u32;
            v.seed = m["seed"].as_u64().unwrap_or(1);
            v.auto_seed = false;
            v.cfg = m["cfg"].as_f64().unwrap_or(1.0);
            v.audio = m["audio"].as_bool().unwrap_or(true);
            v.unet = s("unet");
            v.clip = s("clip");
            v.image = s("image").map(PathBuf::from).filter(|p| p.is_file());
            self.tab = Tab::Video;
        } else {
            let l = &mut self.lip;
            l.photo = m["mode"] == "p";
            l.fast = m["q"] == "f";
            l.loop_video = m["len"] == "pingpong";
            l.seed = m["seed"].as_u64().unwrap_or(1247);
            l.auto_seed = false;
            l.media = s("media").map(PathBuf::from).filter(|p| p.is_file());
            l.audio = s("audio").map(PathBuf::from).filter(|p| p.is_file());
            self.slot_changed(forms::Slot::LipMedia);
            self.slot_changed(forms::Slot::LipAudio);
            self.tab = Tab::Lipsync;
        }
    }

    fn regen(&mut self, kind: Kind, m: &serde_json::Value) {
        let seed = Some(media::random_seed());
        let s = |k: &str| m[k].as_str().map(str::to_string);
        let req = if kind == Kind::Videos {
            if s("prompt").is_none() { return self.toast(t("У этого видео нет сохранённых настроек"), true); }
            Request::Video(VideoReq { prompt: s("prompt").unwrap(), mode: s("mode").unwrap_or("t2v".into()), neg: s("negative").unwrap_or_default(),
                image: s("image").map(PathBuf::from), width: m["width"].as_u64().unwrap_or(1024) as u32, height: m["height"].as_u64().unwrap_or(576) as u32,
                seconds: m["seconds"].as_f64().unwrap_or(5.0), fps: m["fps"].as_u64().unwrap_or(24) as u32, seed, cfg: m["cfg"].as_f64().unwrap_or(1.0),
                audio: m["audio"].as_bool().unwrap_or(true), unet: s("unet"), clip: s("clip"), translate: true })
        } else {
            // the converted inputs are still in ComfyUI's input folder; the originals may have moved
            let input = |k: &str, orig: &str| s(orig).map(PathBuf::from).filter(|p| p.is_file()).or_else(|| s(k).map(|n| crate::paths::input().join(n)).filter(|p| p.is_file()));
            let (Some(media), Some(audio)) = (input("mediaName", "media"), input("audioName", "audio")) else { return self.toast(t("Исходные файлы этого видео не найдены"), true) };
            Request::Lipsync(LipsyncReq { media, audio, quality: if m["q"] == "f" { "fast" } else { "quality" }.into(), fit: if m["len"] == "pingpong" { "loop" } else { "trim" }.into(), seed })
        };
        let tab = Tab::of(&req);
        self.submit(tab, req);
        self.tab = tab;
    }

    fn confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let text = match c {
            Confirm::One(p) => tf("Удалить {} с диска?", &[&p.file_name().unwrap_or_default().to_string_lossy()]),
            Confirm::All(Kind::Photos, n) => tf("Удалить все фото ({} шт.) с диска?", &[n]),
            Confirm::All(Kind::Videos, n) => tf("Удалить все видео ({} шт.) с диска?", &[n]),
            Confirm::All(Kind::Lipsync, n) => tf("Удалить все видео липсинка ({} шт.) с диска?", &[n]),
            Confirm::Model(n) => tf("Удалить модель {} с диска?", &[n]),
        };
        let mut decision = None;
        let r = egui::Modal::new(egui::Id::new("confirm")).frame(theme::card()).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(theme::light(text).size(20.0).color(TEXT));
            ui.label(theme::mute(t("Это нельзя отменить.")));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::danger(ui, t("Удалить")).clicked() { decision = Some(true); }
                if theme::ghost(ui, t("Отмена")).clicked() { decision = Some(false); }
            });
        });
        if r.should_close() && decision.is_none() { decision = Some(false); }
        if let Some(yes) = decision {
            let c = self.confirm.take().unwrap();
            if yes {
                match c {
                    Confirm::One(p) => match results::delete(&p) { Ok(()) => self.toast(t("Удалено"), false), Err(e) => self.toast(&e.to_string(), true) },
                    Confirm::All(k, _) => {
                        let (n, failed) = results::delete_all(k);
                        if failed.is_empty() { self.toast(&tf("Удалено файлов: {}", &[&n]), false) } else { self.toast(&tf("Удалено {}, не удалось {}: {}", &[&n, &failed.len(), &failed[0]]), true) }
                    }
                    Confirm::Model(n) => { match models::delete(&n) { Ok(()) => self.toast(t("Модель удалена"), false), Err(e) => self.toast(&e.to_string(), true) }; self.refresh_models(); }
                }
                self.scanned = Instant::now() - Duration::from_secs(60);
            }
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((text, at, err)) = &self.toast else { return };
        if at.elapsed() > Duration::from_secs(5) { self.toast = None; return; }
        egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0)).show(ctx, |ui| {
            let secs = at.elapsed().as_secs_f32();
            let a = theme::ease_out((secs / 0.22).min(1.0)) * ((5.0 - secs) / 0.3).clamp(0.0, 1.0);   // eases in, fades out
            ui.set_opacity(a);
            ui.set_max_width(480.0);
            theme::adaptive_frame(ui, theme::tint(Color32::from_rgb(20, 20, 22)), egui::Stroke::new(1.0, if *err { RED } else { theme::accent() }), Margin::symmetric(22, 12), 20,
                |ui| { ui.label(RichText::new(text).color(if *err { RED } else { TEXT })); });
        });
        ctx.request_repaint_after(Duration::from_millis(30));
    }

    /// Lays the window out for its current width: side by side when wide, one scrolling column when narrow.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.title_lang != Some(self.prefs.lang) {
            self.title_lang = Some(self.prefs.lang);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t("Студия").into()));
        }
        // the accent glides to the new tab's colour (photo tabs share one, so switching between them changes nothing)
        let now = ctx.input(|i| i.time);
        // drop-down lists show every entry at once and scroll only when they would take over the window
        let combo_h = (ctx.content_rect().height() * 0.5).clamp(160.0, 420.0);
        if (ctx.global_style().spacing.combo_height - combo_h).abs() > 1.0 { ctx.all_styles_mut(|s| s.spacing.combo_height = combo_h); }
        let ease = |t0: f64, secs: f64| theme::ease_in_out(((now - t0) / secs).clamp(0.0, 1.0) as f32);
        if self.tab.accent() != self.acc.1 { self.acc = (self.acc.0.lerp_to_gamma(self.acc.1, ease(self.acc.2, 0.4)), self.tab.accent(), now); }
        let accent = self.acc.0.lerp_to_gamma(self.acc.1, ease(self.acc.2, 0.4));
        if now - self.acc.2 < 0.4 { ctx.request_repaint(); }
        let bg_secs = 3.0;
        if self.tab.accent() != self.bg.1 { self.bg = (self.bg.0.lerp_to_gamma(self.bg.1, ease(self.bg.2, bg_secs)), self.tab.accent(), now); }
        let bg_color = self.bg.0.lerp_to_gamma(self.bg.1, ease(self.bg.2, bg_secs));
        if now - self.bg.2 < bg_secs { ctx.request_repaint(); }
        theme::set_blur(self.prefs.blur as u8);
        theme::apply(&ctx, accent, self.prefs.opacity);
        let phys = ui.ctx().content_rect().size() * ctx.pixels_per_point();
        // grows with the window, but not so much that the longest form (video) needs scrolling
        let zoom = if self.prefs.scale == 0 { ((phys.x / 1600.0).min(phys.y / 1100.0).clamp(1.0, 1.5) * 20.0).round() / 20.0 } else { self.prefs.scale as f32 / 100.0 };
        if (ctx.zoom_factor() - zoom).abs() > 0.001 { ctx.set_zoom_factor(zoom); }
        // the glow swells in at start-up and when switched on, and fades away when switched off
        let glow = ctx.animate_bool_with_time_and_easing(egui::Id::new("glow"), self.prefs.glow, 1.0, egui::emath::easing::cubic_out) * theme::appear(&ctx, egui::Id::new("bg_start"), 1.6);
        let phase = aurora::advance(&ctx, self.prefs.motion, self.prefs.speed);
        let look = aurora::Look { brightness: self.prefs.brightness as f32 / 100.0, noise: self.prefs.noise as f32 / 100.0, grain: self.prefs.grain as f32 };
        aurora::paint(ui, ui.max_rect(), bg_color, glow, phase, look);
        if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            ui.painter().rect_stroke(ui.max_rect(), 0, egui::Stroke::new(1.0, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
        }
        let w = ui.available_width();
        let wide = w >= 980.0;
        let pad = if w < 600.0 { 10 } else { 18 };
        let clear = egui::Frame::new().fill(Color32::TRANSPARENT);
        egui::Panel::top("header").show_separator_line(false).frame(clear.inner_margin(Margin { left: pad, right: pad, top: 12, bottom: 10 })).show(ui, |ui| self.header(ui, w < 1100.0));
        if wide {
            egui::Panel::left("composer").resizable(false).show_separator_line(false).exact_size((w * 0.30).clamp(340.0, 440.0))
                .frame(clear.inner_margin(Margin { left: pad, right: 9, top: 0, bottom: pad })).show(ui, |ui| {
                    theme::glass(ui, |ui| {
                        ui.set_min_height(ui.available_height());
                        self.composer(ui, true);
                    });
                });
            egui::CentralPanel::default().frame(clear.inner_margin(Margin { left: 9, right: pad, top: 0, bottom: pad })).show(ui, |ui| {
                theme::glass(ui, |ui| { ui.set_min_height(ui.available_height()); self.gallery(ui, true); });
            });
        } else {
            egui::CentralPanel::default().frame(clear.inner_margin(Margin { left: pad, right: pad, top: 0, bottom: pad })).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    theme::glass(ui, |ui| { ui.set_width(ui.available_width()); self.composer(ui, false); });
                    ui.add_space(12.0);
                    theme::glass(ui, |ui| { ui.set_width(ui.available_width()); self.gallery(ui, false); });
                });
            });
        }
        self.settings_window(&ctx);
        self.model_sheet(&ctx);
        self.confirm_modal(&ctx);
        self.toast_ui(&ctx);
    }
}

/// The window hosts the control API. A hidden `studio-cli serve` started earlier hands the port over.
fn start_api(jobs: &Arc<Jobs>, quit: &Arc<AtomicBool>) -> Option<String> {
    let start = || api::start(jobs.clone(), api::Api { quit: quit.clone(), headless: false });
    if start().is_ok() { return None; }
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(2))).build().into();
    let _ = agent.post(format!("http://127.0.0.1:{API_PORT}/api/shutdown")).header("Content-Type", "application/json").send("{}");
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(300));
        if start().is_ok() { return None; }
    }
    Some(tf("Порт {} занят другой программой: автоматизация через Claude Code из этого окна недоступна", &[&API_PORT]))
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() { self.drop_files(dropped); }
        self.refresh_results();
        self.draw(ui);
        ctx.request_repaint_after(if self.jobs.busy() { Duration::from_millis(400) } else { Duration::from_secs(2) });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "studio", &Saved { tab: self.tab, img: self.img.clone(), vid: self.vid.clone(), lip: self.lip.clone(), prefs: self.prefs.clone() });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // nothing outlives the window: the API stops and so does the ComfyUI we started
        self.api_quit.store(true, Ordering::Relaxed);
        self.jobs.cancel_all();
        self.jobs.engine.shutdown();
    }
}

/// The round icon is baked into the executable (assets/icon.png, built by tools/make_icon.py).
fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn SetCurrentProcessExplicitAppUserModelID(id: *const u16) -> i32;
}

/// Gives the process its own taskbar identity: the shell keeps a per-identity icon cache, and the one tied to the old
/// executable still held the previous icon, so the taskbar ignored the icon of the window. A fresh id makes it use ours.
fn set_app_id() {
    #[cfg(windows)]
    {
        let id: Vec<u16> = "LocalAIStudio.Studio.2".encode_utf16().chain(Some(0)).collect();
        // SAFETY: `id` is a NUL-terminated UTF-16 string that outlives the call
        unsafe { SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
    }
}

pub fn run() -> anyhow::Result<()> {
    set_app_id();
    let mut viewport = egui::ViewportBuilder::default().with_title("Студия").with_inner_size([1400.0, 900.0]).with_min_inner_size([560.0, 520.0]).with_drag_and_drop(true).with_decorations(false);
    if let Some(i) = icon() { viewport = viewport.with_icon(Arc::new(i)); }
    let options = eframe::NativeOptions { viewport, persistence_path: Some(crate::paths::cache().join("window.ron")), ..Default::default() };
    eframe::run_native("Студия", options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx, cc.storage, Options { api: true })))))
        .map_err(|e| anyhow::anyhow!("не удалось открыть окно: {e}"))
}
