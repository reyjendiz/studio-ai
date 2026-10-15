//! Animated "aurora" background: a full-window mesh whose vertex colours come from a domain-warped sine field; the GPU
//! interpolates between the vertices. The palette is derived from the app theme (`palette`), nothing here is hard-coded.
use super::theme;
use eframe::egui::{self, ecolor::Hsva, epaint::{Mesh, Shape, Vertex}, Color32, Pos2, Rect, TextureHandle};
use std::time::Duration;

/// Mesh resolution (vertices); the field is smooth, so this is plenty.
const COLS: usize = 32;
const ROWS: usize = 18;
/// How much of the aurora is mixed over the dark base colour. Low keeps text and widgets readable.
const INTENSITY: f32 = 0.30;
/// Switch off to ship a still gradient everywhere (the Settings switch only matters while this is true).
pub const ANIMATED: bool = true;
/// Frame pace of the animation; the motion is slow, so 30 fps looks smooth and costs little.
const FRAME: Duration = Duration::from_millis(33);

/// The four gradient colours, all from the theme: A the darkest base, B the accent of the open tab,
/// C the accent turned 45 degrees in hue, D a lighter, less saturated neighbour on the other side.
/// The app only has a dark theme; a light theme would pass a light base colour as A here.
pub fn palette(accent: Color32) -> [Color32; 4] {
    let a = Hsva::from(accent);
    let turned = |dh: f32, s: f32, v: f32| Color32::from(Hsva::new((a.h + dh).rem_euclid(1.0), (a.s * s).min(1.0), v, 1.0));
    [theme::BG, accent, turned(0.125, 0.9, a.v * 0.9), turned(-0.06, 0.8, 1.0)]
}

/// Aurora colour at `uv` (0..1) for the window's `aspect` and animation time `t` (seconds).
pub fn color_at(p: &[Color32; 4], uv: (f32, f32), aspect: f32, t: f32) -> Color32 {
    let (mut qx, mut qy) = (uv.0 * aspect * 3.0, uv.1 * 3.0);
    for i in 0..4 {
        let i = i as f32;
        qx += 0.5 * (qy * 1.5 + t * 0.4 + i).sin();
        qy += 0.5 * (qx * 1.5 - t * 0.3 + i).cos();
    }
    let s = 0.5 + 0.5 * (qx + qy + t * 0.2).sin();
    let s2 = 0.5 + 0.5 * (qx - qy - t * 0.15).cos();
    let mixed = p[0].lerp_to_gamma(p[1], s).lerp_to_gamma(p[2].lerp_to_gamma(p[3], s2), 0.45);
    p[0].lerp_to_gamma(mixed, INTENSITY)
}

/// Animation time in seconds. It only runs (and only asks for repaints) while the window is visible and focused and
/// `animate` is on; otherwise it holds still, which is the static-gradient fallback. `speed` scales it (1.0 = normal).
pub fn advance(ctx: &egui::Context, animate: bool, speed: f32) -> f32 {
    let id = egui::Id::new("aurora_phase");
    let mut phase: f32 = ctx.data(|d| d.get_temp(id)).unwrap_or(12.0);
    let (focused, minimized) = ctx.input(|i| (i.viewport().focused.unwrap_or(true), i.viewport().minimized.unwrap_or(false)));
    if animate && ANIMATED && focused && !minimized {
        phase += ctx.input(|i| i.stable_dt).min(0.1) * speed.clamp(0.1, 3.0);
        ctx.request_repaint_after(FRAME);
    }
    ctx.data_mut(|d| d.insert_temp(id, phase));
    phase
}

/// Adjustable look of the background (Settings): brightness (1.0 = as designed), grain amount (0..1) and grain size in px.
#[derive(Clone, Copy)]
pub struct Look {
    pub brightness: f32,
    pub noise: f32,
    pub grain: f32,
}

/// Every channel times `k`, clamped; the colour stays opaque.
fn scaled(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Film grain tile: each pixel is white or black with a random opacity, so it lightens and darkens evenly and the
/// average brightness of the background does not move. Deterministic, 256 x 256, tiles without a visible seam.
fn grain_image() -> egui::ColorImage {
    const N: usize = 256;
    let mut x: u32 = 0x9E37_79B9;
    let pixels = (0..N * N).map(|_| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let v = (x >> 8) as u8;
        if v >= 128 { Color32::from_white_alpha((v - 128) * 2) } else { Color32::from_black_alpha((127 - v) * 2) }
    }).collect();
    egui::ColorImage { size: [N, N], pixels, source_size: egui::vec2(N as f32, N as f32) }
}

fn grain_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::new("aurora_grain");
    if let Some(t) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) { return t; }
    let opts = egui::TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Nearest, wrap_mode: egui::TextureWrapMode::Repeat, mipmap_mode: None };
    let t = ctx.load_texture("grain", grain_image(), opts);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// Paints the aurora over `rect` on `ui`'s painter. `strength` (0..1) fades it in and out; 0 leaves the plain base colour.
pub fn paint(ui: &egui::Ui, rect: Rect, accent: Color32, strength: f32, phase: f32, look: Look) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0, scaled(theme::BG, look.brightness));
    if rect.width() < 1.0 || rect.height() < 1.0 { return; }
    ui.ctx().data_mut(|d| d.insert_temp(snapshot_id(), Snapshot { rect, accent, phase, strength, brightness: look.brightness }));
    if strength > 0.001 { paint_mesh(painter, rect, accent, strength, phase, look.brightness); }
    if look.noise > 0.001 {
        // a tile is `grain` physical pixels per texel, so the grain keeps its size when the interface is zoomed.
        // Tiled by hand (one quad per tile) instead of relying on the sampler's repeat mode, which not every backend honours.
        let tile = 256.0 * look.grain.max(1.0) / ui.ctx().pixels_per_point();
        let tint = Color32::from_white_alpha((look.noise.clamp(0.0, 1.0) * 0.45 * 255.0) as u8);
        let mut m = Mesh::with_texture(grain_texture(ui.ctx()).id());
        let (nx, ny) = ((rect.width() / tile).ceil() as u32, (rect.height() / tile).ceil() as u32);
        for ty in 0..ny {
            for tx in 0..nx {
                let min = rect.min + egui::vec2(tx as f32 * tile, ty as f32 * tile);
                let max = Pos2::new((min.x + tile).min(rect.right()), (min.y + tile).min(rect.bottom()));
                let (u, v) = ((max.x - min.x) / tile, (max.y - min.y) / tile);
                let base = m.vertices.len() as u32;
                for (p, uv) in [(min, (0.0, 0.0)), (Pos2::new(max.x, min.y), (u, 0.0)), (max, (u, v)), (Pos2::new(min.x, max.y), (0.0, v))] {
                    m.vertices.push(Vertex { pos: p, uv: Pos2::new(uv.0, uv.1), color: tint });
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        painter.add(Shape::mesh(m));
    }
}

/// What the background looked like this frame, so tiles can frost the part of it that lies behind them.
#[derive(Clone, Copy)]
struct Snapshot {
    rect: Rect,
    accent: Color32,
    phase: f32,
    strength: f32,
    brightness: f32,
}

fn snapshot_id() -> egui::Id { egui::Id::new("aurora_snapshot") }

/// Background colour at a screen position (no grain), as painted by `paint`.
fn background_at(s: &Snapshot, p: Pos2) -> Color32 {
    let uv = (((p.x - s.rect.left()) / s.rect.width()).clamp(0.0, 1.0), ((p.y - s.rect.top()) / s.rect.height()).clamp(0.0, 1.0));
    scaled(theme::BG.lerp_to_gamma(color_at(&palette(s.accent), uv, s.rect.width() / s.rect.height(), s.phase), s.strength), s.brightness)
}

/// Frosted-glass layer for a tile: the smooth background colour repainted over the grain behind the tile, drawn under the
/// tile's own fill. `blur` (0..1) sets how opaque the frost is and how far it flattens the aurora towards the tile's average
/// colour, which is what a real backdrop blur would do to this soft background. Returns `Shape::Noop` when there is nothing to do.
pub fn frost(ctx: &egui::Context, rect: Rect, radius: f32, blur: f32) -> Shape {
    let Some(snap) = ctx.data(|d| d.get_temp::<Snapshot>(snapshot_id())) else { return Shape::Noop };
    if blur <= 0.01 || rect.width() < 2.0 || rect.height() < 2.0 { return Shape::Noop; }
    let (w, h) = (rect.width(), rect.height());
    let mut avg = [0.0f32; 3];
    for iy in 0..4 {
        for ix in 0..4 {
            let c = background_at(&snap, Pos2::new(rect.left() + w * (ix as f32 + 0.5) / 4.0, rect.top() + h * (iy as f32 + 0.5) / 4.0));
            for (a, v) in avg.iter_mut().zip([c.r(), c.g(), c.b()]) { *a += v as f32 / 16.0; }
        }
    }
    let avg = Color32::from_rgb(avg[0] as u8, avg[1] as u8, avg[2] as u8);
    let color = |p: Pos2| {
        let c = background_at(&snap, p).lerp_to_gamma(avg, blur * 0.7);
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (blur * 255.0) as u8)
    };
    // outline of the rounded rectangle, then a second ring and the centre so the colour can vary across big tiles
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut ring: Vec<Pos2> = Vec::new();
    for (cx, cy, a0) in [(rect.right() - r, rect.top() + r, -90.0f32), (rect.right() - r, rect.bottom() - r, 0.0), (rect.left() + r, rect.bottom() - r, 90.0), (rect.left() + r, rect.top() + r, 180.0)] {
        for k in 0..=8 {
            let a = (a0 + 90.0 * k as f32 / 8.0).to_radians();
            ring.push(Pos2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    let n = ring.len() as u32;
    let centre = rect.center();
    let mut m = Mesh::default();
    m.vertices.push(Vertex { pos: centre, uv: egui::epaint::WHITE_UV, color: color(centre) });
    for p in &ring { let q = centre + (*p - centre) * 0.5; m.vertices.push(Vertex { pos: q, uv: egui::epaint::WHITE_UV, color: color(q) }); }
    for p in &ring { m.vertices.push(Vertex { pos: *p, uv: egui::epaint::WHITE_UV, color: color(*p) }); }
    for k in 0..n {
        let (a, b) = (1 + k, 1 + (k + 1) % n);
        let (c, d) = (1 + n + k, 1 + n + (k + 1) % n);
        m.indices.extend([0, a, b, a, c, b, b, c, d]);
    }
    Shape::mesh(m)
}

fn paint_mesh(painter: &egui::Painter, rect: Rect, accent: Color32, strength: f32, phase: f32, brightness: f32) {
    let pal = palette(accent);
    let aspect = rect.width() / rect.height();
    let mut mesh = Mesh::default();
    for r in 0..ROWS {
        for c in 0..COLS {
            let uv = (c as f32 / (COLS - 1) as f32, r as f32 / (ROWS - 1) as f32);
            let col = color_at(&pal, uv, aspect, phase);
            let col = scaled(theme::BG.lerp_to_gamma(col, strength), brightness);
            mesh.vertices.push(Vertex { pos: Pos2::new(rect.left() + uv.0 * rect.width(), rect.top() + uv.1 * rect.height()), uv: egui::epaint::WHITE_UV, color: col });
        }
    }
    for r in 0..ROWS - 1 {
        for c in 0..COLS - 1 {
            let i = (r * COLS + c) as u32;
            let w = COLS as u32;
            mesh.indices.extend([i, i + 1, i + w, i + 1, i + w + 1, i + w]);
        }
    }
    painter.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_follows_the_theme() {
        let p = palette(theme::VIDEO);
        assert_eq!(p[0], theme::BG);
        assert_eq!(p[1], theme::VIDEO);
        assert_ne!(palette(theme::PHOTO), p, "another accent must give another gradient");
    }

    #[test]
    fn frost_is_a_valid_mesh() {
        let ctx = egui::Context::default();
        assert!(matches!(frost(&ctx, Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 100.0)), 32.0, 0.5), Shape::Noop), "nothing to frost before a background was painted");
        ctx.data_mut(|d| d.insert_temp(snapshot_id(), Snapshot { rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)), accent: theme::PHOTO, phase: 3.0, strength: 1.0, brightness: 1.0 }));
        let Shape::Mesh(m) = frost(&ctx, Rect::from_min_size(Pos2::new(20.0, 20.0), egui::vec2(300.0, 400.0)), 32.0, 0.6) else { panic!("expected a mesh") };
        assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
        assert!(m.vertices.iter().all(|v| v.color.a() == (0.6f32 * 255.0) as u8), "alpha is the blur amount");
        assert!(matches!(frost(&ctx, Rect::from_min_size(Pos2::ZERO, egui::vec2(50.0, 50.0)), 32.0, 0.0), Shape::Noop));
    }

    #[test]
    fn grain_is_even_and_brightness_scales() {
        let img = grain_image();
        let (mut light, mut dark) = (0u64, 0u64);
        for p in &img.pixels { if p.r() > 0 { light += p.a() as u64 } else { dark += p.a() as u64 } }
        assert!((light as f64 / dark as f64 - 1.0).abs() < 0.05, "grain must lighten and darken equally ({light} vs {dark})");
        assert_eq!(scaled(Color32::from_rgb(100, 200, 50), 0.5), Color32::from_rgb(50, 100, 25));
        assert_eq!(scaled(Color32::from_rgb(200, 10, 0), 2.0), Color32::from_rgb(255, 20, 0));
    }

    #[test]
    fn aurora_stays_dark_and_changes_slowly() {
        let p = palette(theme::PHOTO);
        let bright = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        let base = bright(theme::BG);
        let limit = bright(theme::BG.lerp_to_gamma(p[1], 0.5)); // never anywhere near the accent itself
        for i in 0..50 {
            let t = i as f32 * 0.9;
            let c = color_at(&p, ((i % 7) as f32 / 6.0, (i % 5) as f32 / 4.0), 1.7, t);
            assert!(bright(c) >= base.saturating_sub(3) && bright(c) <= limit, "{c:?} at t={t}");
            // one frame later (1/30 s) the colour barely moves
            let n = color_at(&p, ((i % 7) as f32 / 6.0, (i % 5) as f32 / 4.0), 1.7, t + 0.033);
            assert!((bright(c) as i32 - bright(n) as i32).abs() <= 4, "jump between frames at t={t}");
        }
    }
}
