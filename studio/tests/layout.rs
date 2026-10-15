//! Renders the real window offscreen at several sizes, tabs and languages into logs\ui\*.png so the layout can be
//! checked by eye. Needs a GPU and the Windows fonts, so it is opt-in: cargo test --release --test layout -- --ignored
use eframe::egui;
use egui_kittest::Harness;
use studio::ui::{i18n, App, Options, Tab};

/// Made-up results in target/ui_fixture, so the screenshots do not depend on what is in the real output folder.
fn fixture() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("ui_fixture").join("output");
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(out.join("video")).unwrap();
        std::fs::create_dir_all(out.join("lipsync")).unwrap();
        for i in 0..6u32 {
            let img = image::RgbImage::from_fn(512, 512, |x, y| image::Rgb([(40 + i * 35 + x / 6) as u8, (200 - i * 20 - y / 5) as u8, (90 + i * 25 + (x + y) / 12) as u8]));
            img.save(out.join(format!("studio_{:05}_.png", i + 1))).unwrap();
        }
        let clip = |path: &std::path::Path, color: &str| {
            let _ = std::process::Command::new("ffmpeg").args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", &format!("gradients=s=640x360:d=1:r=12:c0={color}:c1=0x203040"), "-pix_fmt", "yuv420p"]).arg(path).status();
        };
        for (i, c) in ["0x5fd4c8", "0xd4f24a"].iter().enumerate() {
            let p = out.join("video").join(format!("ltx25_{:05}_.mp4", i + 1));
            clip(&p, c);
            std::fs::write(p.with_extension("json"), r#"{"mode":"t2v","prompt":"A red fox runs through a snowy forest at dawn","width":768,"height":448,"seconds":2.0,"fps":24,"seed":1}"#).unwrap();
        }
        for (i, c) in ["0xf2a65a", "0x5fd4c8"].iter().enumerate() {
            let p = out.join("lipsync").join(format!("ls_{:05}_.mp4", i + 1));
            clip(&p, c);
            std::fs::write(p.with_extension("json"), r#"{"mode":"p","q":"q","audioSec":3.0,"seed":1}"#).unwrap();
        }
        // SAFETY: runs once before anything reads the variable
        unsafe { std::env::set_var("STUDIO_OUTPUT", &out) };
    });
}

fn shot(name: &str, size: (f32, f32), setup: impl FnOnce(&mut App)) {
    fixture();
    let mut h = Harness::builder().with_size(egui::vec2(size.0, size.1)).wgpu()
        .build_eframe(|cc| App::new(&cc.egui_ctx, None, Options { api: false }));
    setup(h.state_mut());
    for _ in 0..25 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(120)); // thumbnails decode on background threads
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("logs").join("ui");
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("render").save(dir.join(format!("{name}.png"))).unwrap();
    i18n::set(i18n::Lang::Ru);
}

const TABS: [(&str, Tab); 5] = [("create", Tab::Create), ("edit", Tab::Edit), ("upscale", Tab::Upscale), ("video", Tab::Video), ("lipsync", Tab::Lipsync)];

#[test]
#[ignore]
fn screenshots() {
    // the default window (1416x907 client area) for every tab, then the extremes
    for (n, t) in TABS { shot(&format!("w_{n}"), (1416.0, 907.0), move |a| a.tab = t); }
    for (n, t) in TABS { shot(&format!("big_{n}"), (2540.0, 1380.0), move |a| a.tab = t); }
    for (n, t) in TABS { shot(&format!("mid_{n}"), (1000.0, 700.0), move |a| a.tab = t); }
    for (n, t) in TABS { shot(&format!("phone_{n}"), (560.0, 900.0), move |a| a.tab = t); }
    shot("en_edit", (1416.0, 907.0), |a| { a.tab = Tab::Edit; a.prefs.lang = i18n::Lang::En; i18n::set(i18n::Lang::En); });
    shot("uk_lipsync", (1416.0, 907.0), |a| { a.tab = Tab::Lipsync; a.prefs.lang = i18n::Lang::Uk; i18n::set(i18n::Lang::Uk); });
    shot("settings", (1416.0, 907.0), |a| { a.tab = Tab::Video; a.settings_open = true; });
}

/// Hover bar on a gallery tile (opt-in like the rest): logs\ui\hover_<tab>.png
#[test]
#[ignore]
fn hover() {
    for (n, t, pos) in [("create", Tab::Create, egui::pos2(800.0, 250.0)), ("video", Tab::Video, egui::pos2(560.0, 250.0))] {
        fixture();
        let mut h = Harness::builder().with_size(egui::vec2(1416.0, 907.0)).wgpu()
            .build_eframe(|cc| App::new(&cc.egui_ctx, None, Options { api: false }));
        h.state_mut().tab = t;
        for _ in 0..20 { h.step(); std::thread::sleep(std::time::Duration::from_millis(120)); }
        h.hover_at(pos);
        for _ in 0..6 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("logs").join("ui");
        h.render().expect("render").save(dir.join(format!("hover_{n}.png"))).unwrap();
        if n == "create" {   // pointer on the delete icon: its red disc must stay inside the bar
            h.hover_at(egui::pos2(876.0, 371.0));
            for _ in 0..8 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
            h.render().expect("render").save(dir.join("hover_trash.png")).unwrap();
        }
    }
}

#[test]
#[ignore]
fn one() {
    shot("one_create", (1000.0, 700.0), |a| a.tab = Tab::Create);
}

/// Switching tabs must settle in one frame: the first frame after the switch and a later one may only differ
/// in the (animated) glow and thumbnails, not in the form on the left.
#[test]
#[ignore]
fn tab_switch_settles() {
    fixture();
    for (from, to) in [(Tab::Create, Tab::Video), (Tab::Video, Tab::Lipsync), (Tab::Lipsync, Tab::Edit), (Tab::Edit, Tab::Upscale)] {
        let mut h = Harness::builder().with_size(egui::vec2(1416.0, 907.0)).wgpu().build_eframe(|cc| App::new(&cc.egui_ctx, None, Options { api: false }));
        h.state_mut().tab = from;
        for _ in 0..20 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
        h.state_mut().tab = to;
        h.step();
        let a = h.render().expect("render");
        for _ in 0..25 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
        let b = h.render().expect("render");
        let (mut diff, mut total) = (0u32, 0u32);
        for y in 130..820 { for x in 30..400 {
            total += 1;
            let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
            if p.0.iter().zip(q.0.iter()).any(|(m, n)| m.abs_diff(*n) > 24) { diff += 1; }
        } }
        eprintln!("{from:?} -> {to:?}: {diff}/{total} pixels of the form differ");
        assert!(diff * 100 < total, "{from:?} -> {to:?}: the form changed after the first frame ({diff}/{total})");
        i18n::set(i18n::Lang::Ru);
    }
}

/// The model drop-down opened: complete at once, no scroll bar for a short list.
#[test]
#[ignore]
fn combo_open() {
    fixture();
    let mut h = Harness::builder().with_size(egui::vec2(1416.0, 907.0)).wgpu().build_eframe(|cc| App::new(&cc.egui_ctx, None, Options { api: false }));
    for _ in 0..20 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
    let pos = egui::pos2(200.0, 169.0);   // the model drop-down of the Create tab
    h.hover_at(pos);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        h.step();
    }
    h.step();   // the very first frame of the open list must already be complete
    h.render().expect("render").save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("logs").join("ui").join("combo_first.png")).unwrap();
    for _ in 0..6 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
    h.render().expect("render").save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("logs").join("ui").join("combo.png")).unwrap();
    // the pointer over the last entry must not make a scroll bar appear, and clicking it must pick it
    let third = egui::pos2(170.0, 292.0);
    h.hover_at(third);
    for _ in 0..6 { h.step(); std::thread::sleep(std::time::Duration::from_millis(60)); }
    h.render().expect("render").save(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("logs").join("ui").join("combo_hover.png")).unwrap();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: third, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        h.step();
    }
    for _ in 0..4 { h.step(); }
    assert!(h.state().img.model_gen.as_deref().is_some_and(|m| m.starts_with("NoctQ")), "clicking the third entry selects it: {:?}", h.state().img.model_gen);
}

/// Release screenshots for the README: real sample results copied from the output folder (never the lip-sync results, which
/// come from private test files), English and Russian, written to docs/screenshots. Opt-in like the rest.
#[test]
#[ignore]
fn readme_shots() {
    use std::path::Path;
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let real = studio::paths::output();
    let stage = manifest.join("target").join("readme_fixture").join("output");
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(stage.join("video")).unwrap();
    std::fs::create_dir_all(stage.join("lipsync")).unwrap();
    let copy = |from: &Path, to: &Path| for e in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let p = e.path();
        let n = e.file_name().to_string_lossy().into_owned();
        if p.is_file() && (n.starts_with("studio_") || n.starts_with("upscale_") || n.starts_with("ltx")) { std::fs::copy(&p, to.join(&n)).unwrap(); }
    };
    copy(&real, &stage);
    copy(&real.join("video"), &stage.join("video"));
    // SAFETY: set before the first Harness of this test reads it
    unsafe { std::env::set_var("STUDIO_OUTPUT", &stage) };
    let photo = |n: usize| stage.join(format!("studio_{n:05}_.png"));
    let out = manifest.join("..").join("docs").join("screenshots");
    std::fs::create_dir_all(&out).unwrap();
    let langs = [("", i18n::Lang::En), ("ru-", i18n::Lang::Ru)];
    for (prefix, lang) in langs {
        let (p1, p2) = if lang == i18n::Lang::Ru {
            ("Рыжая лиса сидит на свежем снегу на рассвете, мягкий золотой свет, малая глубина резкости, фото", "Сделай шерсть лисы серебристой")
        } else {
            ("A red fox sitting in fresh snow at sunrise, soft golden light, shallow depth of field, photo", "Make the fox's fur silver")
        };
        let video_prompt = if lang == i18n::Lang::Ru { "Рыжая лиса идёт по заснеженному лесу на рассвете, падают снежинки, камера плавно едет следом" } else { "A red fox walks through a snowy forest at dawn, snowflakes falling, the camera follows smoothly" };
        let shots: Vec<(&str, Box<dyn Fn(&mut App)>)> = vec![
            ("01-create", Box::new(move |a: &mut App| { a.tab = Tab::Create; a.img.prompt = p1.into(); })),
            ("02-edit", Box::new({ let f = photo(1); move |a: &mut App| { a.tab = Tab::Edit; a.img.prompt = p2.into(); a.img.photo1 = Some(f.clone()); } })),
            ("03-upscale", Box::new({ let f = photo(1); move |a: &mut App| { a.tab = Tab::Upscale; a.img.photo1 = Some(f.clone()); } })),
            ("04-video", Box::new(move |a: &mut App| { a.tab = Tab::Video; a.vid.prompt = video_prompt.into(); })),
            ("05-lipsync", Box::new(|a: &mut App| { a.tab = Tab::Lipsync; })),
            ("06-settings", Box::new(|a: &mut App| { a.tab = Tab::Create; a.settings_open = true; })),
        ];
        for (name, setup) in shots {
            let height = if name == "06-settings" { 1180.0 } else { 900.0 };   // the settings card is taller than the window otherwise
            let mut h = Harness::builder().with_size(egui::vec2(1400.0, height)).wgpu().build_eframe(|cc| App::new(&cc.egui_ctx, None, Options { api: false }));
            h.state_mut().prefs.lang = lang;
            i18n::set(lang);
            setup(h.state_mut());
            for _ in 0..30 { h.step(); std::thread::sleep(std::time::Duration::from_millis(120)); }
            // JPEG: these are photo-heavy, and a quantised PNG bands the photos
            let img = image::DynamicImage::ImageRgba8(h.render().expect("render")).to_rgb8();
            let file = std::fs::File::create(out.join(format!("{prefix}{name}.jpg"))).unwrap();
            image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 90).encode_image(&img).unwrap();
        }
    }
    i18n::set(i18n::Lang::Ru);
}
