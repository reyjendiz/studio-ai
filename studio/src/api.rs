//! Local control API on 127.0.0.1:8189 (JSON). The window and `studio-cli serve` both host it, so Claude Code or
//! scripts can do everything the window can. Browsers are refused (any Origin header, or a POST that is not
//! application/json), so a web page cannot quietly start jobs or delete files.
use crate::jobs::{Jobs, Request};
use crate::results::Kind;
use crate::{media, models, results, API_PORT};
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Api {
    pub quit: Arc<AtomicBool>,
    /// the window refuses /api/shutdown; the headless server obeys it
    pub headless: bool,
}

fn reply(req: tiny_http::Request, code: u16, v: Value) {
    let ct = tiny_http::Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap();
    let _ = req.respond(tiny_http::Response::from_string(v.to_string()).with_status_code(code).with_header(ct));
}

fn header(r: &tiny_http::Request, n: &'static str) -> Option<String> {
    r.headers().iter().find(|h| h.field.equiv(n)).map(|h| h.value.as_str().to_string())
}

fn body(req: &mut tiny_http::Request) -> Result<Value, String> {
    let mut s = String::new();
    req.as_reader().take(1 << 20).read_to_string(&mut s).map_err(|e| e.to_string())?;
    if s.trim().is_empty() { Ok(json!({})) } else { serde_json::from_str(&s).map_err(|e| format!("неверный JSON: {e}")) }
}

/// Binds the port (fails if another Studio already has it) and serves on a background thread.
pub fn start(jobs: Arc<Jobs>, api: Api) -> std::io::Result<std::thread::JoinHandle<()>> {
    let server = tiny_http::Server::http(("127.0.0.1", API_PORT)).map_err(|e| std::io::Error::new(std::io::ErrorKind::AddrInUse, e.to_string()))?;
    Ok(std::thread::spawn(move || {
        while !api.quit.load(Ordering::Relaxed) {
            let Ok(Some(mut req)) = server.recv_timeout(Duration::from_millis(500)) else { continue };
            let jobs = jobs.clone();
            let method = req.method().to_string();
            let url = req.url().split('?').next().unwrap_or("").to_string();
            let host_ok = header(&req, "Host").is_none_or(|h| h == format!("127.0.0.1:{API_PORT}") || h == format!("localhost:{API_PORT}"));
            let json_ct = header(&req, "Content-Type").is_some_and(|c| c.starts_with("application/json"));
            if header(&req, "Origin").is_some() || !host_ok || (method == "POST" && !json_ct) {
                reply(req, 403, json!({"error": "только для локальных программ: POST с Content-Type: application/json, без Origin"}));
                continue;
            }
            let parts: Vec<&str> = url.trim_matches('/').split('/').collect();
            let b = if method == "POST" { match body(&mut req) { Ok(b) => b, Err(e) => { reply(req, 400, json!({"error": e})); continue; } } } else { Value::Null };
            let (code, v) = route(&jobs, &api, &method, &parts, b);
            reply(req, code, v);
        }
    }))
}

fn route(jobs: &Arc<Jobs>, api: &Api, method: &str, parts: &[&str], b: Value) -> (u16, Value) {
    let path_arg = || b["path"].as_str().map(PathBuf::from).ok_or("нужно поле path");
    let kind_arg = |s: &str| Kind::parse(s).ok_or(format!("неизвестный вид «{s}»: photos, videos или lipsync"));
    let r: Result<Value, String> = match (method, parts) {
        ("GET", ["api", "status"]) => Ok(json!({
            "app": "studio", "version": env!("CARGO_PKG_VERSION"), "root": crate::paths::root(), "headless": api.headless,
            "engine_up": jobs.engine.up(), "engine_started_by_studio": jobs.engine.started_by_us(), "busy": jobs.busy(),
            "jobs": jobs.all().iter().filter(|j| j.active()).map(|j| json!({"id": j.id, "kind": j.request.title(), "label": j.label, "pct": j.pct})).collect::<Vec<_>>()})),
        ("GET", ["api", "models"]) => Ok(json!({"image": models::image_models(), "ltx": models::ltx_files(), "ltx_missing": models::ltx_files().missing(),
            "lipsync": models::lipsync_installed(), "ffmpeg": media::have_ffmpeg()})),
        ("GET", ["api", "jobs"]) => Ok(json!(jobs.all())),
        ("GET", ["api", "jobs", id]) => id.parse().ok().and_then(|id| jobs.get(id)).map(|j| json!(j)).ok_or("нет такой задачи".into()),
        ("POST", ["api", "jobs"]) => serde_json::from_value::<Request>(b).map(|r| json!({"id": jobs.submit(r)})).map_err(|e| format!("неверная задача: {e}")),
        ("POST", ["api", "jobs", "cancel-all"]) => { jobs.cancel_all(); Ok(json!({"ok": true})) }
        ("POST", ["api", "jobs", id, "cancel"]) => id.parse().map(|id| { jobs.cancel(id); json!({"ok": true}) }).map_err(|_| "неверный номер".into()),
        ("GET", ["api", "results", k]) => kind_arg(k).map(|k| json!(results::list(k))),
        ("POST", ["api", "results", "delete"]) => path_arg().map_err(String::from).and_then(|p| results::delete(&p).map(|_| json!({"ok": true})).map_err(|e| e.to_string())),
        ("POST", ["api", "results", "delete-all"]) => kind_arg(b["kind"].as_str().unwrap_or("")).map(|k| { let (n, failed) = results::delete_all(k); json!({"deleted": n, "failed": failed}) }),
        ("POST", ["api", "open"]) => path_arg().map_err(String::from).and_then(|p| {
            // only results may be opened, so the API cannot be used to launch arbitrary files
            if p.canonicalize().ok().zip(crate::paths::output().canonicalize().ok()).is_some_and(|(f, o)| f.starts_with(o)) { media::open_default(&p); Ok(json!({"ok": true})) }
            else { Err("открывать можно только результаты в ComfyUI\\output".into()) }
        }),
        ("POST", ["api", "translate"]) => jobs.translate(b["text"].as_str().unwrap_or("")).map(|t| json!({"text": t})).map_err(|e| e.to_string()),
        ("POST", ["api", "engine", "stop"]) => { jobs.cancel_all(); Ok(json!({"stopped": jobs.engine.hard_stop()})) }
        ("POST", ["api", "shutdown"]) if api.headless => { api.quit.store(true, Ordering::Relaxed); Ok(json!({"ok": true})) }
        ("POST", ["api", "shutdown"]) => Err("окно Студии закрывается только пользователем".into()),
        _ => return (404, json!({"error": "нет такого адреса; см. CLAUDE.md"})),
    };
    match r {
        Ok(v) => (200, v),
        Err(e) => (400, json!({"error": e})),
    }
}
