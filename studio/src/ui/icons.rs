//! Line icons in the SF Symbols manner: one stroke weight, rounded corners and ends, drawn on a 20 x 20 grid
//! (centre = origin) and scaled to the requested size. Emoji glyphs differ per font, so nothing here uses text.
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon { Folder, Expand, Pencil, Chat, Sliders, Refresh, Trash, Gear, Close, Minimize, Maximize, Restore }

type P = (f32, f32);

/// Polyline whose corners are rounded with radius `r` (quadratic curves), optionally closed.
fn smooth(pts: &[P], r: f32, closed: bool) -> Vec<P> {
    let n = pts.len();
    let mut out = Vec::new();
    for i in 0..n {
        let p = pts[i];
        if !closed && (i == 0 || i == n - 1) { out.push(p); continue; }
        let (a, b) = (pts[(i + n - 1) % n], pts[(i + 1) % n]);
        let unit = |q: P| { let (dx, dy) = (q.0 - p.0, q.1 - p.1); let l = (dx * dx + dy * dy).sqrt().max(1e-3); (dx / l, dy / l, l) };
        let (ua, ub) = (unit(a), unit(b));
        let (ra, rb) = (r.min(ua.2 / 2.0), r.min(ub.2 / 2.0));
        let (q1, q2) = ((p.0 + ua.0 * ra, p.1 + ua.1 * ra), (p.0 + ub.0 * rb, p.1 + ub.1 * rb));
        for s in 0..=5 {
            let t = s as f32 / 5.0;
            let (u, v) = ((1.0 - t) * (1.0 - t), t * t);
            let w = 2.0 * t * (1.0 - t);
            out.push((u * q1.0 + w * p.0 + v * q2.0, u * q1.1 + w * p.1 + v * q2.1));
        }
    }
    out
}

/// Draws `icon` centred at `c`, `size` px wide, in colour `col`. `bg` is the colour behind it (knobs are filled with it).
pub fn draw(p: &egui::Painter, c: Pos2, size: f32, icon: Icon, col: Color32, bg: Color32) {
    let k = size / 20.0;
    let w = (1.5 * (size / 18.0)).clamp(1.3, 2.0);
    let st = Stroke::new(w, col);
    let pt = |q: P| c + Vec2::new(q.0, q.1) * k;
    let pts = |v: &[P]| -> Vec<Pos2> { v.iter().map(|&q| pt(q)).collect() };
    // open line with round ends
    let line = |v: &[P]| {
        let v = pts(v);
        p.circle_filled(v[0], w / 2.0, col);
        p.circle_filled(*v.last().unwrap(), w / 2.0, col);
        p.add(egui::Shape::line(v, st));
    };
    let closed = |v: Vec<P>| { p.add(egui::Shape::closed_line(pts(&v), st)); };
    match icon {
        Icon::Folder => {
            closed(smooth(&[(-8.5, -5.0), (-3.0, -5.0), (-1.2, -2.8), (8.5, -2.8), (8.5, 6.0), (-8.5, 6.0)], 2.4, true));
        }
        Icon::Expand => {
            line(&[(1.5, -1.5), (7.5, -7.5)]);
            line(&[(2.5, -7.5), (7.5, -7.5), (7.5, -2.5)]);
            line(&[(-1.5, 1.5), (-7.5, 7.5)]);
            line(&[(-2.5, 7.5), (-7.5, 7.5), (-7.5, 2.5)]);
        }
        Icon::Pencil => {
            closed(smooth(&[(-7.5, 7.5), (-6.4, 3.2), (3.4, -6.6), (6.6, -3.4), (-3.2, 6.4)], 1.2, true));
            line(&[(1.0, -4.2), (4.2, -1.0)]);
        }
        Icon::Chat => {
            closed(smooth(&[(-8.5, -7.0), (8.5, -7.0), (8.5, 3.5), (0.0, 3.5), (-4.5, 7.5), (-4.5, 3.5), (-8.5, 3.5)], 3.2, true));
            line(&[(-4.0, -3.0), (4.0, -3.0)]);
            line(&[(-4.0, 0.0), (1.0, 0.0)]);
        }
        Icon::Sliders => {
            for (y, x) in [(-5.5, -3.0), (0.0, 3.5), (5.5, -1.0)] {
                line(&[(-8.5, y), (x - 2.8, y)]);
                line(&[(x + 2.8, y), (8.5, y)]);
                p.circle_filled(pt((x, y)), 2.4 * k, bg);
                p.circle_stroke(pt((x, y)), 2.4 * k, st);
            }
        }
        Icon::Refresh => {
            let arc: Vec<P> = (0..=24).map(|i| { let a = (35.0 + 285.0 * i as f32 / 24.0).to_radians(); (7.2 * a.cos(), -7.2 * a.sin()) }).collect();
            line(&arc);
            // arrow head at the end of the arc, pointing along the tangent
            let (e, b) = (arc[24], arc[22]);
            let (dx, dy) = (e.0 - b.0, e.1 - b.1);
            let l = (dx * dx + dy * dy).sqrt();
            let (dx, dy) = (dx / l, dy / l);
            let (nx, ny) = (-dy, dx);
            line(&[(e.0 - dx * 3.4 + nx * 3.0, e.1 - dy * 3.4 + ny * 3.0), e, (e.0 - dx * 3.4 - nx * 3.0, e.1 - dy * 3.4 - ny * 3.0)]);
        }
        Icon::Trash => {
            line(&[(-8.0, -4.8), (8.0, -4.8)]);
            line(&[(-2.6, -4.8), (-2.6, -7.5), (2.6, -7.5), (2.6, -4.8)]);
            p.add(egui::Shape::line(pts(&smooth(&[(-6.0, -4.8), (-5.0, 7.2), (5.0, 7.2), (6.0, -4.8)], 2.0, false)), st));
            line(&[(-2.2, -0.8), (-2.2, 3.6)]);
            line(&[(2.2, -0.8), (2.2, 3.6)]);
        }
        Icon::Gear => {
            let mut v: Vec<P> = Vec::new();
            for i in 0..8 {
                let base = i as f32 * 45.0;
                for (r, d) in [(6.2, -17.0), (8.4, -9.0), (8.4, 9.0), (6.2, 17.0)] {
                    let a = (base + d).to_radians();
                    v.push((r * a.cos(), r * a.sin()));
                }
            }
            closed(smooth(&v, 1.1, true));
            p.circle_stroke(pt((0.0, 0.0)), 2.8 * k, st);
        }
        Icon::Close => {
            line(&[(-5.0, -5.0), (5.0, 5.0)]);
            line(&[(-5.0, 5.0), (5.0, -5.0)]);
        }
        Icon::Minimize => line(&[(-6.0, 0.0), (6.0, 0.0)]),
        Icon::Maximize => closed(smooth(&[(-6.0, -6.0), (6.0, -6.0), (6.0, 6.0), (-6.0, 6.0)], 2.6, true)),
        Icon::Restore => {
            closed(smooth(&[(-7.0, -3.0), (3.0, -3.0), (3.0, 7.0), (-7.0, 7.0)], 2.2, true));
            p.add(egui::Shape::line(pts(&smooth(&[(-3.0, -3.0), (-3.0, -7.0), (7.0, -7.0), (7.0, 3.0), (3.0, 3.0)], 2.2, false)), st));
        }
    }
}
