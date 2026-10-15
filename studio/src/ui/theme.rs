//! Orbit-style look: charcoal surfaces with hairline borders, light two-tone headings, tiny monospace labels,
//! translucent glass cards over a soft glow. The accent follows the tab (photo chartreuse, video teal, lip sync amber)
//! and card opacity comes from Settings.
use super::icons::{self, Icon};
use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

/// Controls are fully rounded: the maximum radius, which the painter clamps to half the control's height (a pill).
pub const PILL: u8 = u8::MAX;
/// Big surfaces (cards, windows, tiles): as round as still looks like a card.
pub const SURFACE_R: u8 = 32;
/// Height shared by everything on the header line.
pub const HEADER_H: f32 = 40.0;

pub const BG: Color32 = Color32::from_rgb(11, 11, 12);
pub const SURFACE: Color32 = Color32::from_rgb(22, 22, 24);
pub const FIELD: Color32 = Color32::from_rgb(15, 15, 17);
pub const LINE: Color32 = Color32::from_rgb(40, 40, 43);
pub const TEXT: Color32 = Color32::from_rgb(237, 237, 237);
pub const MUTE: Color32 = Color32::from_rgb(138, 138, 142);
pub const DIM: Color32 = Color32::from_rgb(92, 92, 96);
pub const RED: Color32 = Color32::from_rgb(240, 110, 100);
pub const WARN: Color32 = Color32::from_rgb(242, 184, 75);

pub const PHOTO: Color32 = Color32::from_rgb(212, 242, 74); // chartreuse
pub const VIDEO: Color32 = Color32::from_rgb(95, 212, 200); // teal
pub const LIPSYNC: Color32 = Color32::from_rgb(242, 166, 90); // amber

static ACCENT_RGB: AtomicU32 = AtomicU32::new(0xD4F24A);
static OPACITY: AtomicU8 = AtomicU8::new(82);
static BLUR: AtomicU8 = AtomicU8::new(60);
static APPLIED: AtomicU32 = AtomicU32::new(u32::MAX);

/// Accent of the open tab.
pub fn accent() -> Color32 {
    let v = ACCENT_RGB.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Tile opacity in percent (Settings → transparency).
pub fn opacity() -> u8 { OPACITY.load(Ordering::Relaxed) }

/// Frosting of the background behind tiles, 0..1 (Settings → blur).
pub fn blur() -> f32 { BLUR.load(Ordering::Relaxed) as f32 / 100.0 }
pub fn set_blur(pct: u8) { BLUR.store(pct.min(100), Ordering::Relaxed); }

/// `base` with the tile opacity as its alpha: every glass surface (cards, sheets, the toast) goes through this.
pub fn tint(base: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (opacity() as f32 / 100.0 * 255.0) as u8)
}

/// Dark inset fields (text boxes, option tracks): they follow the tile opacity too, but never get see-through enough to lose contrast.
pub fn field() -> Color32 {
    let a = 0.4 + 0.6 * opacity() as f32 / 100.0;
    Color32::from_rgba_unmultiplied(FIELD.r(), FIELD.g(), FIELD.b(), (a * 255.0) as u8)
}

/// Applies the tab accent and the opacity setting to egui's widget colours.
pub fn apply(ctx: &egui::Context, accent: Color32, opacity_pct: u8) {
    OPACITY.store(opacity_pct, Ordering::Relaxed);
    let packed = (accent.r() as u32) << 16 | (accent.g() as u32) << 8 | accent.b() as u32;
    ACCENT_RGB.store(packed, Ordering::Relaxed);
    let key = packed | (opacity_pct as u32) << 24;
    if APPLIED.swap(key, Ordering::Relaxed) == key {
        return;
    }
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        v.selection.bg_fill = accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, accent);
        v.hyperlink_color = accent;
        v.extreme_bg_color = field();
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.6));
        v.slider_trailing_fill = true;
    });
}

pub fn surface() -> Color32 { tint(SURFACE) }

/// Manrope (SIL OFL, bundled) as three static weights cut from the variable font by tools/make_fonts.py. Static instances
/// have the right advance widths per weight; variable coordinates set at run time only changed the outlines, which spaced
/// bold Cyrillic unevenly.
static MANROPE_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Manrope-Regular.ttf");
static MANROPE_LIGHT: &[u8] = include_bytes!("../../assets/fonts/Manrope-Light.ttf");
static MANROPE_BOLD: &[u8] = include_bytes!("../../assets/fonts/Manrope-Bold.ttf");

pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [("manrope", MANROPE_REGULAR), ("manrope-light", MANROPE_LIGHT), ("manrope-bold", MANROPE_BOLD)] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // Windows symbol/emoji fonts only supply icons Manrope lacks (⚙ ▶ 📂); egui's built-in fonts stay as the last fallback
    let win = std::path::Path::new(r"C:\Windows\Fonts");
    for (name, file) in [("segoe-sym", "seguisym.ttf"), ("segoe-emoji", "seguiemj.ttf")] {
        if let Ok(bytes) = std::fs::read(win.join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        }
    }
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    let build = |first: &str| -> Vec<String> {
        let mut v = vec![first.to_string()];
        v.extend(["segoe-sym", "segoe-emoji"].iter().filter(|n| fonts.font_data.contains_key(**n)).map(|n| n.to_string()));
        v.extend(fallback.iter().filter(|f| !f.starts_with("manrope")).cloned());
        v
    };
    let (regular, light, bold) = (build("manrope"), build("manrope-light"), build("manrope-bold"));
    fonts.families.insert(FontFamily::Proportional, regular.clone());
    fonts.families.insert(FontFamily::Monospace, regular); // small labels use Manrope too
    fonts.families.insert(FontFamily::Name("light".into()), light);
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = Color32::from_rgb(18, 18, 20);
        v.extreme_bg_color = FIELD;
        v.faint_bg_color = SURFACE;
        v.code_bg_color = FIELD;
        v.override_text_color = Some(TEXT);
        v.window_corner_radius = CornerRadius::same(SURFACE_R);
        v.menu_corner_radius = CornerRadius::same(24);
        v.window_stroke = Stroke::new(1.0, LINE);
        v.window_shadow = egui::Shadow { offset: [0, 18], blur: 40, spread: 0, color: Color32::from_black_alpha(160) };
        v.popup_shadow = egui::Shadow { offset: [0, 8], blur: 20, spread: 0, color: Color32::from_black_alpha(140) };
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTE);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = CornerRadius::same(PILL);
        }
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(26, 26, 29);
        v.widgets.inactive.bg_fill = Color32::from_rgb(34, 34, 37);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(34, 34, 38);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(44, 44, 48);
        // text must stay clear of the rounded corners of windows, menus and tooltips
        s.spacing.window_margin = Margin::same(22);
        s.spacing.menu_margin = Margin::symmetric(16, 12);
        s.animation_time = 0.0;   // menus and tooltips appear complete at once; own animations pass their durations
        s.spacing.item_spacing = Vec2::new(8.0, 9.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.slider_width = 200.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(11.5));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::new(26.0, FontFamily::Name("light".into())));
    });
}

/// Glass card: translucent charcoal with a hairline border.
/// A glass tile: frosted background behind, translucent fill on top.
pub fn glass<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(egui::Shape::Noop);
    let out = card().show(ui, add);
    ui.painter().set(slot, super::aurora::frost(ui.ctx(), out.response.rect, SURFACE_R as f32, blur()));
    out
}

pub fn card() -> egui::Frame {
    egui::Frame::new().fill(surface()).stroke(Stroke::new(1.0, Color32::from_white_alpha(18))).corner_radius(SURFACE_R).inner_margin(Margin::same(18))
}

pub fn bold(t: impl Into<String>) -> RichText { RichText::new(t).family(FontFamily::Name("bold".into())) }
pub fn light(t: impl Into<String>) -> RichText { RichText::new(t).family(FontFamily::Name("light".into())) }
pub fn mute(t: impl Into<String>) -> RichText { RichText::new(t).color(MUTE).size(12.5) }
/// Tiny monospace section label, e.g. "01 · ФОТО" (a leading "▪ " in the text is dropped).
pub fn tag(t: impl Into<String>) -> RichText {
    let t: String = t.into();
    RichText::new(t.trim_start_matches("▪ ")).monospace().color(DIM).size(11.0)
}

/// Two-tone light heading (grey part, white part), as in Orbit's titles.
pub fn heading(ui: &mut egui::Ui, grey: &str, white: &str) {
    ui.horizontal_wrapped(|ui| heading_inline(ui, grey, white));
}

/// The heading pieces without a wrapping row, for putting buttons on the same line.
pub fn heading_inline(ui: &mut egui::Ui, grey: &str, white: &str) {
    ui.spacing_mut().item_spacing.x = 6.0;
    ui.label(light(grey).size(26.0).color(MUTE));
    ui.label(light(white).size(26.0).color(TEXT));
}

// ---- motion ------------------------------------------------------------------------------------------------------

pub fn ease_out(t: f32) -> f32 { 1.0 - (1.0 - t).powi(3) }
pub fn ease_in_out(t: f32) -> f32 { if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 } }

/// 0..1 (eased) since `id` was first asked for; `forget` it to play again.
pub fn appear(ctx: &egui::Context, id: egui::Id, secs: f32) -> f32 {
    let now = ctx.input(|i| i.time);
    let start = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now));
    let t = ((now - start) as f32 / secs).clamp(0.0, 1.0);
    if t < 1.0 { ctx.request_repaint(); }
    ease_out(t)
}

pub fn forget(ctx: &egui::Context, id: egui::Id) { ctx.data_mut(|d| d.remove::<f64>(id)); }

/// A value that glides to `target` (ease in-out) whenever the target changes; a retarget continues from where it is shown.
pub fn morph(ctx: &egui::Context, id: egui::Id, target: f32, secs: f32) -> f32 {
    let now = ctx.input(|i| i.time);
    let (mut from, mut to, mut start) = ctx.data(|d| d.get_temp::<(f32, f32, f64)>(id)).unwrap_or((target, target, now - 10.0));
    let at = |from: f32, to: f32, start: f64| from + (to - from) * ease_in_out(((now - start) as f32 / secs).clamp(0.0, 1.0));
    if (to - target).abs() > 0.01 {
        from = at(from, to, start);
        (to, start) = (target, now);
    }
    ctx.data_mut(|d| d.insert_temp(id, (from, to, start)));
    if ((now - start) as f32) < secs { ctx.request_repaint(); }
    at(from, to, start)
}

// ---- controls -----------------------------------------------------------------------------------------------------

/// Pill button with an eased hover and press state. `icon` goes left of the text.
#[allow(clippy::too_many_arguments)]
fn pill(ui: &mut egui::Ui, text: &str, font: FontId, color: Color32, fill: Color32, hover_fill: Color32, stroke: Stroke, hover_stroke: Stroke, icon: Option<Icon>, min: Vec2, enabled: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let icon_w = if icon.is_some() { 16.0 + 7.0 } else { 0.0 };
    let size = Vec2::new(min.x.max(galley.size().x + icon_w + 32.0), min.y.max(32.0));
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let on = enabled && resp.hovered();
    let (h, d) = (ui.ctx().animate_bool_with_time(resp.id.with("h"), on, 0.16), ui.ctx().animate_bool_with_time(resp.id.with("d"), on && resp.is_pointer_button_down_on(), 0.08));
    let r = rect.shrink(1.5 * d);
    ui.painter().rect(r, PILL, fill.lerp_to_gamma(hover_fill, h), Stroke::new(stroke.width, stroke.color.lerp_to_gamma(hover_stroke.color, h)), egui::StrokeKind::Inside);
    let mut x = r.center().x - (galley.size().x + icon_w) / 2.0;
    if let Some(i) = icon {
        icons::draw(ui.painter(), Pos2::new(x + 8.0, r.center().y), 16.0, i, color, fill);
        x += icon_w;
    }
    ui.painter().galley(Pos2::new(x, r.center().y - galley.size().y / 2.0), galley, color);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Main action: accent fill, dark text.
pub fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let a = accent();
    let fill = if enabled { a } else { a.gamma_multiply(0.35) };
    let none = Stroke::NONE;
    pill(ui, text, FontId::new(14.5, FontFamily::Name("bold".into())), Color32::from_rgb(12, 14, 6), fill, a.lerp_to_gamma(Color32::WHITE, 0.2), none, none, None, Vec2::new(ui.available_width(), 42.0), enabled)
}

pub fn danger(ui: &mut egui::Ui, text: &str) -> egui::Response {
    pill(ui, text, FontId::proportional(13.5), RED, Color32::TRANSPARENT, RED.gamma_multiply(0.16), Stroke::new(1.0, RED.gamma_multiply(0.45)), Stroke::new(1.0, RED.gamma_multiply(0.8)), None, Vec2::ZERO, true)
}

pub fn ghost(ui: &mut egui::Ui, text: &str) -> egui::Response { ghost_icon(ui, text, None) }

pub fn ghost_icon(ui: &mut egui::Ui, text: &str, icon: Option<Icon>) -> egui::Response {
    pill(ui, text, FontId::proportional(13.5), TEXT, Color32::TRANSPARENT, Color32::from_white_alpha(14), Stroke::new(1.0, LINE), Stroke::new(1.0, Color32::from_white_alpha(60)), icon, Vec2::ZERO, true)
}

/// Round icon button at `rect` (the hover disc fades in). Does not move the layout cursor.
pub fn icon_button(ui: &mut egui::Ui, rect: Rect, icon: Icon, tip: &str, danger: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("icon", tip)), Sense::click()).on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.14);
    let press = ui.ctx().animate_bool_with_time(resp.id.with("d"), resp.is_pointer_button_down_on(), 0.07);
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0 * (1.0 - 0.06 * press);
    ui.painter().circle_filled(c, r, if danger { RED.gamma_multiply(0.22 * h) } else { Color32::from_white_alpha((34.0 * h) as u8) });
    let base = if danger { RED } else { Color32::from_gray(200) };
    let col = base.lerp_to_gamma(if danger { Color32::from_rgb(255, 150, 140) } else { Color32::WHITE }, h);
    icons::draw(ui.painter(), c, (rect.width() * 0.56).min(20.0), icon, col, Color32::TRANSPARENT);
    resp.clicked()
}

/// Round window button of the custom title bar: same disc as the settings gear; the close one turns red under the pointer.
pub fn window_button(ui: &mut egui::Ui, rect: Rect, icon: Icon, tip: &str, close: bool) -> bool {
    let resp = ui.interact(rect, ui.id().with(("window", tip)), Sense::click()).on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
    let h = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.14);
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0 - 0.5;
    ui.painter().circle(c, r, surface(), Stroke::new(1.0, LINE));
    if close { ui.painter().circle_filled(c, r, RED.gamma_multiply(0.85 * h)); } else { ui.painter().circle_filled(c, r, Color32::from_white_alpha((26.0 * h) as u8)); }
    let col = Color32::from_gray(200).lerp_to_gamma(Color32::WHITE, h);
    icons::draw(ui.painter(), c, 18.0, icon, col, Color32::TRANSPARENT);
    resp.clicked()
}

/// Segmented control: equal-width options on a pill track; the lifted thumb glides to the chosen one.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    segmented_h(ui, value, options, 28.0)
}

/// Segmented control with the given option height (the frame adds 3 px of padding on each side).
pub fn segmented_h<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)], h: f32) -> bool {
    let mut changed = false;
    let (gap, n) = (3.0, options.len());
    let w = ui.available_width();
    let font = FontId::proportional(13.0);
    let widest = options.iter().map(|(_, l)| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), TEXT).size().x)).fold(0.0, f32::max);
    // labels that do not fit side by side are stacked instead of pushing the panel wider
    let stacked = widest + 14.0 > ((w - 8.0 - gap * (n as f32 - 1.0)) / n as f32).max(36.0);
    let id = ui.id().with(("segmented", options.iter().map(|(_, l)| *l).collect::<Vec<_>>().join("|")));
    egui::Frame::new().fill(field()).stroke(Stroke::new(1.0, LINE)).corner_radius(if stacked { 22 } else { PILL }).inner_margin(Margin::same(3)).show(ui, |ui| {
        let iw = ui.available_width();
        let bw = if stacked { iw } else { (iw - gap * (n as f32 - 1.0)) / n as f32 };
        let total = if stacked { Vec2::new(iw, n as f32 * h + gap * (n as f32 - 1.0)) } else { Vec2::new(iw, h) };
        let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
        let step = if stacked { Vec2::new(0.0, h + gap) } else { Vec2::new(bw + gap, 0.0) };
        let slot = |i: f32| Rect::from_min_size(rect.min + step * i, Vec2::new(bw, h));
        let cur = options.iter().position(|(v, _)| *v == *value).unwrap_or(0);
        let pos = morph(ui.ctx(), id, cur as f32, 0.30);
        let thumb = slot(pos);
        let rad = if stacked { 19 } else { PILL };
        ui.painter().rect(thumb.translate(Vec2::new(0.0, 1.0)), rad, Color32::from_black_alpha(70), Stroke::NONE, egui::StrokeKind::Inside);
        ui.painter().rect(thumb, rad, Color32::from_rgb(50, 50, 55), Stroke::new(1.0, accent().gamma_multiply(0.4)), egui::StrokeKind::Inside);
        for (i, (v, label)) in options.iter().enumerate() {
            let r = slot(i as f32);
            let resp = ui.interact(r, id.with(i), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            let near = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            let hov = ui.ctx().animate_bool_with_time(id.with(("h", i)), resp.hovered(), 0.14) * (1.0 - near);
            if hov > 0.0 { ui.painter().rect_filled(r, rad, Color32::from_white_alpha((14.0 * hov) as u8)); }
            let col = MUTE.lerp_to_gamma(TEXT, near.max(hov * 0.5));
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, font.clone(), col);
            if resp.clicked() && *value != *v { *value = *v; changed = true; }
        }
    });
    changed
}

/// Frame that is a full pill while its content is one line tall and an ordinary round rectangle (radius `multi_r`) once the
/// text wraps: a stretched capsule around several lines looks wrong and eats the text.
pub fn adaptive_frame<R>(ui: &mut egui::Ui, fill: Color32, stroke: Stroke, margin: Margin, multi_r: u8, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(egui::Shape::Noop);
    let out = egui::Frame::new().inner_margin(margin).show(ui, add);
    let r = if out.response.rect.height() <= 46.0 { PILL } else { multi_r };
    ui.painter().set(slot, egui::epaint::RectShape::new(out.response.rect, r, fill, stroke, egui::StrokeKind::Inside));
    out
}

pub fn info_box(ui: &mut egui::Ui, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    adaptive_frame(ui, color.gamma_multiply(0.05), Stroke::new(1.0, color.gamma_multiply(0.4)), Margin::symmetric(18, 11), 20, |ui| {
        ui.set_width(ui.available_width());
        ui.style_mut().visuals.override_text_color = Some(color);
        add(ui)
    });
}

/// iOS-style switch with a label; the knob slides and the track takes the accent colour.
pub fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let (tw, th, gap): (f32, f32, f32) = (36.0, 22.0, 10.0);
    let galley = ui.painter().layout(label.to_string(), FontId::proportional(13.0), Color32::from_rgb(196, 196, 200), (ui.available_width() - tw - gap).max(40.0));
    let size = Vec2::new(tw + gap + galley.size().x, th.max(galley.size().y + 4.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let mut changed = false;
    if resp.clicked() { *value = !*value; changed = true; }
    let t = ui.ctx().animate_bool_with_time_and_easing(resp.id, *value, 0.22, egui::emath::easing::cubic_out);
    let h = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.14);
    let track = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - th / 2.0), Vec2::new(tw, th));
    let off = Color32::from_rgb(50, 50, 55).lerp_to_gamma(Color32::from_rgb(64, 64, 70), h);
    ui.painter().rect(track, PILL, off.lerp_to_gamma(accent(), t), Stroke::NONE, egui::StrokeKind::Inside);
    let knob_x = egui::lerp((track.left() + th / 2.0)..=(track.right() - th / 2.0), t);
    ui.painter().circle_filled(Pos2::new(knob_x, track.center().y + 0.8), th / 2.0 - 1.5, Color32::from_black_alpha(60));
    ui.painter().circle_filled(Pos2::new(knob_x, track.center().y), th / 2.0 - 2.0, Color32::from_rgb(244, 244, 242));
    ui.painter().galley(Pos2::new(rect.left() + tw + gap, rect.center().y - galley.size().y / 2.0), galley, Color32::from_rgb(196, 196, 200));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    changed
}

/// Popover card (settings, model manager): slides in and fades, closes with its X or Esc. `body` fills the card.
#[allow(clippy::too_many_arguments)]
pub fn sheet(ctx: &egui::Context, id: &str, anchor: egui::Align2, offset: Vec2, title: &str, width: f32, open: &mut bool, body: impl FnOnce(&mut egui::Ui)) {
    let aid = egui::Id::new(("sheet", id));
    let a = appear(ctx, aid.with("t"), 0.26);
    egui::Area::new(aid).order(egui::Order::Foreground).anchor(anchor, offset + Vec2::new(0.0, (1.0 - a) * 14.0)).show(ctx, |ui| {
        ui.set_opacity(a);
        let slot = ui.painter().add(egui::Shape::Noop);
        let out = egui::Frame::new().fill(Color32::from_rgba_unmultiplied(20, 20, 22, ((0.86 + 0.14 * opacity() as f32 / 100.0) * 255.0) as u8)).stroke(Stroke::new(1.0, Color32::from_white_alpha(24))).corner_radius(SURFACE_R)
            .shadow(egui::Shadow { offset: [0, 18], blur: 44, spread: 0, color: Color32::from_black_alpha(150) }).inner_margin(Margin::same(24)).show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.label(bold(title).size(18.0).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                        if icon_button(ui, r, Icon::Close, "Esc", false) { *open = false; }
                    });
                });
                ui.add_space(8.0);
                // tall content scrolls inside the card instead of running off a small window
                let max_h = (ctx.content_rect().height() - offset.y.abs() - 120.0).max(160.0);
                // the Area hands out little height, so the scroll area gets an explicit rectangle
                let room = Rect::from_min_size(ui.cursor().min, Vec2::new(width, max_h));
                ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| {
                    egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| body(ui));
                });
            });
        ui.painter().set(slot, super::aurora::frost(ctx, out.response.rect, SURFACE_R as f32, blur()));
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *open = false; }
}

pub fn forget_sheet(ctx: &egui::Context, id: &str) { forget(ctx, egui::Id::new(("sheet", id)).with("t")); }

/// Vertical fade from transparent to the card colour along the bottom edge of `r` (hides a hard scroll cut).
pub fn fade_bottom(ui: &egui::Ui, r: egui::Rect, h: f32) {
    use egui::epaint::{Mesh, Shape, Vertex, WHITE_UV};
    let base = Color32::from_rgb(22, 22, 24);
    let (top, bottom) = (Color32::TRANSPARENT, base.gamma_multiply((opacity() as f32 / 100.0).max(0.6)));
    let y0 = r.bottom() - h;
    let mut m = Mesh::default();
    for (x, y, c) in [(r.left(), y0, top), (r.right(), y0, top), (r.right(), r.bottom(), bottom), (r.left(), r.bottom(), bottom)] {
        m.vertices.push(Vertex { pos: egui::pos2(x, y), uv: WHITE_UV, color: c });
    }
    m.indices.extend([0, 1, 2, 0, 2, 3]);
    ui.painter().add(Shape::mesh(m));
}

/// Orbit-like logo: a ring with an accent dot.
pub fn logo(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
    let c = rect.center();
    ui.painter().circle_stroke(c, 10.0, Stroke::new(2.0, TEXT));
    ui.painter().circle_filled(c, 3.2, TEXT);
    ui.painter().circle_filled(c + Vec2::new(7.5, -7.5), 4.2, accent());
}

pub fn fmt_size(b: u64) -> String {
    if b >= 1 << 30 { format!("{:.1} {}", b as f64 / (1u64 << 30) as f64, super::i18n::t("ГБ")) } else { format!("{:.0} {}", b as f64 / (1u64 << 20) as f64, super::i18n::t("МБ")) }
}

pub fn fmt_secs(s: u64) -> String {
    if s < 90 { format!("{s} {}", super::i18n::t("с")) } else { format!("{:.1} {}", s as f64 / 60.0, super::i18n::t("мин")) }
}
