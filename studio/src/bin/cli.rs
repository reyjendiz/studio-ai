//! studio-cli: everything the Studio window does, from a terminal or Claude Code. Prints JSON on stdout,
//! progress on stderr. Uses the open Studio window if there is one, otherwise starts a hidden server
//! (`studio-cli serve`) that exits by itself once idle, stopping ComfyUI with it.
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use studio::{api, engine::Engine, jobs::Jobs, API_PORT};

const HELP: &str = r#"studio-cli: local AI studio (ComfyUI) from the command line. Output is JSON.

  status | models
  gen      --prompt TEXT [--model NAME] [--size 1344x768] [--count 1-4] [--seed N] [--neg TEXT]
           [--photo IN.png --strength 0.9] [--no-translate] [--out FILE] [--no-wait]
  edit     --prompt TEXT --photo IN.png [--photo SECOND.png] [--model NAME] [--count N] [--seed N] [--strength 0.85] [--out FILE]
  upscale  --image IN.png [--kind photo|anime] [--out FILE]
  video    --prompt TEXT [--image FIRST_FRAME.png] [--size 1024x576] [--seconds 5] [--fps 24] [--seed N]
           [--cfg 1] [--no-audio] [--unet NAME] [--clip NAME] [--out FILE.mp4]
  lipsync  --media FACE.mp4|FACE.png --audio SPEECH.wav [--fast] [--loop] [--seed N] [--out FILE.mp4]
  jobs | job ID | wait ID | cancel ID | cancel-all
  list photos|videos|lipsync [--limit N]      open FILE      delete FILE...
  delete-all photos|videos|lipsync --yes       translate TEXT
  engine-stop          stop ComfyUI if Studio started it
  serve [--idle-exit 120]   headless server (started automatically when needed)
  shutdown             stop the headless server

--out: one result is saved as FILE, several as FILE_1, FILE_2...; images are converted by extension (.jpg .png .webp).
"#;

struct Args { pos: Vec<String>, flags: Vec<(String, Option<String>)> }

impl Args {
    fn parse() -> Self {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let (mut pos, mut flags, mut i) = (Vec::new(), Vec::new(), 0);
        const SWITCHES: &[&str] = &["--no-translate", "--no-wait", "--no-audio", "--fast", "--loop", "--yes", "--help"];
        while i < raw.len() {
            let a = &raw[i];
            if a.starts_with("--") {
                if SWITCHES.contains(&a.as_str()) || i + 1 >= raw.len() { flags.push((a.clone(), None)); } else { flags.push((a.clone(), Some(raw[i + 1].clone()))); i += 1; }
            } else { pos.push(a.clone()); }
            i += 1;
        }
        Self { pos, flags }
    }
    fn get(&self, k: &str) -> Option<&str> { self.flags.iter().find(|(f, _)| f == k).and_then(|(_, v)| v.as_deref()) }
    fn all(&self, k: &str) -> Vec<&str> { self.flags.iter().filter(|(f, _)| f == k).filter_map(|(_, v)| v.as_deref()).collect() }
    fn has(&self, k: &str) -> bool { self.flags.iter().any(|(f, _)| f == k) }
    fn num<T: std::str::FromStr>(&self, k: &str) -> Result<Option<T>> {
        self.get(k).map(|v| v.parse().map_err(|_| anyhow!("{k}: неверное число «{v}»"))).transpose()
    }
}

fn abs(p: &str) -> Result<PathBuf> {
    let p = Path::new(p);
    let a = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir()?.join(p) };
    Ok(a)
}

fn size(s: Option<&str>, default: (u32, u32)) -> Result<(u32, u32)> {
    match s {
        None => Ok(default),
        Some(s) => {
            let (w, h) = s.split_once(['x', 'X', '×']).ok_or_else(|| anyhow!("--size: нужно ШИРИНАxВЫСОТА, например 1344x768"))?;
            Ok((w.trim().parse()?, h.trim().parse()?))
        }
    }
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(120))).http_status_as_error(false).build().into()
}

fn call(method: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let url = format!("http://127.0.0.1:{API_PORT}{path}");
    let mut r = if method == "GET" { client().get(&url).call()? } else {
        client().post(&url).header("Content-Type", "application/json").send(body.unwrap_or(json!({})).to_string())?
    };
    let status = r.status();
    let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
    if !status.is_success() { bail!("{}", v["error"].as_str().unwrap_or("ошибка Студии")); }
    Ok(v)
}

fn server_up() -> bool { call("GET", "/api/status", None).map(|v| v["app"] == "studio").unwrap_or(false) }

/// The open window serves the API; without it a hidden `serve` process is started.
fn ensure_server() -> Result<()> {
    if server_up() { return Ok(()); }
    const DETACHED: u32 = 0x0000_0008;
    std::process::Command::new(std::env::current_exe()?).arg("serve")
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .creation_flags(DETACHED | studio::engine::NO_WINDOW | 0x0000_0200).spawn().context("не удалось запустить сервер Студии")?;
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(20) {
        if server_up() { return Ok(()); }
        std::thread::sleep(Duration::from_millis(300));
    }
    bail!("сервер Студии не запустился на 127.0.0.1:{API_PORT} (порт занят другой программой?)")
}

fn wait(id: u64) -> Result<Value> {
    let mut last = String::new();
    loop {
        let j = call("GET", &format!("/api/jobs/{id}"), None)?;
        let line = format!("[{:>3.0}%] {}", j["pct"].as_f64().unwrap_or(0.0), j["label"].as_str().unwrap_or(""));
        if line != last { eprintln!("{line}"); last = line; }
        match j["state"].as_str() {
            Some("done") | Some("error") | Some("cancelled") => return Ok(j),
            _ => std::thread::sleep(Duration::from_millis(800)),
        }
    }
}

/// Copies results to --out (FILE, or FILE_1, FILE_2...), converting images to the extension's format.
fn save(files: &[PathBuf], out: &Path) -> Result<Vec<PathBuf>> {
    let mut saved = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let dst = if files.len() == 1 { out.to_path_buf() } else {
            let stem = out.file_stem().unwrap_or_default().to_string_lossy();
            out.with_file_name(format!("{stem}_{}.{}", i + 1, out.extension().unwrap_or_default().to_string_lossy()))
        };
        if let Some(d) = dst.parent() { std::fs::create_dir_all(d)?; }
        let same_ext = f.extension().map(|e| e.to_ascii_lowercase()) == dst.extension().map(|e| e.to_ascii_lowercase());
        if same_ext || f.extension().is_some_and(|e| e == "mp4") {
            std::fs::copy(f, &dst)?;
        } else {
            let img = image::open(f)?;
            match dst.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
                Some("jpg") | Some("jpeg") => {
                    let mut w = std::io::BufWriter::new(std::fs::File::create(&dst)?);
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 92).encode_image(&img.to_rgb8())?;
                }
                _ => img.save(&dst)?,
            }
        }
        saved.push(dst);
    }
    Ok(saved)
}

fn run_job(a: &Args, req: Value) -> Result<Value> {
    ensure_server()?;
    let id = call("POST", "/api/jobs", Some(req))?["id"].as_u64().context("сервер не вернул номер задачи")?;
    if a.has("--no-wait") { return Ok(json!({"id": id, "state": "queued"})); }
    let mut j = wait(id)?;
    if j["state"] == "error" { bail!("{}", j["error"].as_str().unwrap_or("ошибка")); }
    if let (Some(out), Some(files)) = (a.get("--out"), j["files"].as_array()) {
        let files: Vec<PathBuf> = files.iter().filter_map(|f| f.as_str().map(PathBuf::from)).collect();
        j["saved"] = json!(save(&files, &abs(out)?)?);
    }
    Ok(j)
}

fn serve(a: &Args) -> Result<Value> {
    let idle_exit = a.num::<u64>("--idle-exit")?.unwrap_or(120);
    let engine = Engine::new();
    let jobs = Jobs::new(engine.clone());
    let quit = Arc::new(AtomicBool::new(false));
    let h = api::start(jobs.clone(), api::Api { quit: quit.clone(), headless: true }).map_err(|e| anyhow!("порт {API_PORT} занят: {e}"))?;
    let mut idle_since = Instant::now();
    while !quit.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_secs(2));
        if jobs.busy() || engine.started_by_us() { idle_since = Instant::now(); }
        if idle_exit > 0 && idle_since.elapsed() > Duration::from_secs(idle_exit) { break; }
    }
    quit.store(true, Ordering::Relaxed);
    engine.shutdown();
    let _ = h.join();
    Ok(json!({"stopped": true}))
}

fn main() {
    let a = Args::parse();
    let cmd = a.pos.first().map(String::as_str).unwrap_or("help");
    let r = (|| -> Result<Value> {
        let translate = !a.has("--no-translate");
        let req_path = |k: &str| a.get(k).map(abs).transpose();
        Ok(match cmd {
            "help" | "--help" | "-h" => { print!("{HELP}"); return Ok(Value::Null); }
            "serve" => serve(&a)?,
            "status" => if server_up() { call("GET", "/api/status", None)? } else { json!({"server": false, "note": "Студия не запущена; любая команда запустит её сервер"}) },
            "models" => { ensure_server()?; call("GET", "/api/models", None)? }
            "gen" => {
                let (w, h) = size(a.get("--size"), (1344, 768))?;
                run_job(&a, json!({"kind": "image", "prompt": a.get("--prompt").context("нужен --prompt")?, "neg": a.get("--neg").unwrap_or(""),
                    "model": a.get("--model"), "width": w, "height": h, "count": a.num::<u32>("--count")?.unwrap_or(1), "seed": a.num::<u64>("--seed")?,
                    "photo": req_path("--photo")?, "strength": a.num::<f64>("--strength")?.unwrap_or(0.9), "translate": translate}))?
            }
            "edit" => {
                let photos = a.all("--photo").into_iter().map(abs).collect::<Result<Vec<_>>>()?;
                run_job(&a, json!({"kind": "edit", "prompt": a.get("--prompt").context("нужен --prompt")?, "photos": photos, "neg": a.get("--neg").unwrap_or(""),
                    "model": a.get("--model"), "count": a.num::<u32>("--count")?.unwrap_or(1), "seed": a.num::<u64>("--seed")?,
                    "strength": a.num::<f64>("--strength")?.unwrap_or(0.85), "translate": translate}))?
            }
            "upscale" => run_job(&a, json!({"kind": "upscale", "image": req_path("--image")?.context("нужен --image")?, "mode": a.get("--kind").unwrap_or("photo")}))?,
            "video" => {
                let (w, h) = size(a.get("--size"), (1024, 576))?;
                let image = req_path("--image")?;
                run_job(&a, json!({"kind": "video", "mode": if image.is_some() { "i2v" } else { "t2v" }, "prompt": a.get("--prompt").context("нужен --prompt")?,
                    "neg": a.get("--neg").unwrap_or(""), "image": image, "width": w, "height": h, "seconds": a.num::<f64>("--seconds")?.unwrap_or(5.0),
                    "fps": a.num::<u32>("--fps")?.unwrap_or(24), "seed": a.num::<u64>("--seed")?, "cfg": a.num::<f64>("--cfg")?.unwrap_or(1.0),
                    "audio": !a.has("--no-audio"), "unet": a.get("--unet"), "clip": a.get("--clip"), "translate": translate}))?
            }
            "lipsync" => run_job(&a, json!({"kind": "lipsync", "media": req_path("--media")?.context("нужен --media")?, "audio": req_path("--audio")?.context("нужен --audio")?,
                "quality": if a.has("--fast") { "fast" } else { "quality" }, "fit": if a.has("--loop") { "loop" } else { "trim" }, "seed": a.num::<u64>("--seed")?}))?,
            "jobs" => { ensure_server()?; call("GET", "/api/jobs", None)? }
            "job" | "wait" | "cancel" => {
                let id: u64 = a.pos.get(1).context("нужен номер задачи")?.parse().context("неверный номер задачи")?;
                ensure_server()?;
                match cmd { "job" => call("GET", &format!("/api/jobs/{id}"), None)?, "wait" => wait(id)?, _ => call("POST", &format!("/api/jobs/{id}/cancel"), None)? }
            }
            "cancel-all" => { ensure_server()?; call("POST", "/api/jobs/cancel-all", None)? }
            "list" => {
                ensure_server()?;
                let mut v = call("GET", &format!("/api/results/{}", a.pos.get(1).map(String::as_str).unwrap_or("photos")), None)?;
                if let (Some(n), Some(arr)) = (a.num::<usize>("--limit")?, v.as_array_mut()) { arr.truncate(n); }
                v
            }
            "open" => { ensure_server()?; call("POST", "/api/open", Some(json!({"path": abs(a.pos.get(1).context("нужен путь")?)?})))? }
            "delete" => {
                ensure_server()?;
                let mut done = Vec::new();
                for p in a.pos.iter().skip(1) { call("POST", "/api/results/delete", Some(json!({"path": abs(p)?})))?; done.push(p.clone()); }
                json!({"deleted": done})
            }
            "delete-all" => {
                let k = a.pos.get(1).context("что удалить: photos, videos или lipsync")?;
                if !a.has("--yes") { bail!("удаление всех файлов нельзя отменить: добавьте --yes"); }
                ensure_server()?;
                call("POST", "/api/results/delete-all", Some(json!({"kind": k})))?
            }
            "translate" => { ensure_server()?; call("POST", "/api/translate", Some(json!({"text": a.pos[1..].join(" ")})))? }
            "engine-stop" => if server_up() { call("POST", "/api/engine/stop", None)? } else { json!({"stopped": false}) },
            "shutdown" => if server_up() { call("POST", "/api/shutdown", None)? } else { json!({"ok": true}) },
            other => bail!("неизвестная команда «{other}»; studio-cli help"),
        })
    })();
    match r {
        Ok(Value::Null) => {}
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => { eprintln!("ошибка: {e}"); std::process::exit(1); }
    }
}
