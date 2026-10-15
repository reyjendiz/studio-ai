//! ComfyUI as a managed background process: started on the first job, stopped after IDLE seconds without work,
//! progress read from its websocket and (for LatentSync, which reports nothing there) from its console output.
use crate::paths;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const COMFY: &str = "http://127.0.0.1:8188";
pub const NO_WINDOW: u32 = 0x0800_0000;
const IDLE: u64 = 60; // seconds without work before the ComfyUI we started is stopped
const IDLE_VIDEO: u64 = 300; // after video/lip sync: ~30 GB of weights take minutes to reload
const START_TIMEOUT: u64 = 240;

/// Live state of the prompt ComfyUI is executing, filled from its websocket.
#[derive(Default, Clone, Debug)]
pub struct Progress {
    pub pid: Option<String>,
    pub node: Option<String>,
    pub nodes: HashMap<String, (f64, f64)>,
}

pub struct Engine {
    proc: Mutex<Option<Child>>,
    start_lock: Mutex<()>,
    last: Mutex<Instant>,
    long_idle: AtomicBool,
    /// jobs being prepared or run by this app; ComfyUI is never stopped while this is non-zero
    pub active: AtomicUsize,
    pub progress: Mutex<Progress>,
    /// last "Doing inference...: n/N" line in ComfyUI's output (LatentSync progress), with a counter so a job can
    /// ignore lines written before it started
    pub infer_step: Mutex<(u64, u32, u32)>,
    pub cid: String,
    http: ureq::Agent,
    quick: ureq::Agent,
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .http_status_as_error(false)
        .build()
        .into()
}

pub fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).creation_flags(NO_WINDOW).output();
}

impl Engine {
    pub fn new() -> Arc<Self> {
        let e = Arc::new(Self {
            proc: Mutex::new(None),
            start_lock: Mutex::new(()),
            last: Mutex::new(Instant::now()),
            long_idle: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            progress: Mutex::new(Progress::default()),
            infer_step: Mutex::new((0, 0, 0)),
            cid: format!("studio-{:x}", std::process::id() as u64 ^ Instant::now().elapsed().as_nanos() as u64 ^ 0x5eed),
            http: agent(600),
            quick: agent(2),
        });
        let (a, b) = (e.clone(), e.clone());
        std::thread::spawn(move || a.websocket_loop());
        std::thread::spawn(move || b.idle_loop());
        e
    }

    pub fn up(&self) -> bool {
        self.quick.get(format!("{COMFY}/system_stats")).call().map(|r| r.status().is_success()).unwrap_or(false)
    }

    pub fn touch(&self) { *self.last.lock().unwrap() = Instant::now(); }
    pub fn keep_longer(&self) { self.long_idle.store(true, Ordering::Relaxed); }
    pub fn started_by_us(&self) -> bool {
        self.proc.lock().unwrap().as_mut().map(|p| p.try_wait().ok().flatten().is_none()).unwrap_or(false)
    }

    /// Starts ComfyUI if it is not answering and waits until it does.
    pub fn ensure(self: &Arc<Self>) -> Result<()> {
        let _g = self.start_lock.lock().unwrap();
        self.touch();
        if self.up() {
            return Ok(());
        }
        let py = paths::python();
        if !py.is_file() {
            bail!("не найден {}: Студия должна лежать рядом с ComfyUI и python_embeded", py.display());
        }
        let mut guard = self.proc.lock().unwrap();
        let running = guard.as_mut().map(|p| p.try_wait().ok().flatten().is_none()).unwrap_or(false);
        if !running {
            std::fs::create_dir_all(paths::logs())?;
            let log = Arc::new(Mutex::new(File::create(paths::logs().join("comfyui.out.log"))?));
            let mut child = Command::new(&py)
                .args(["-s", "ComfyUI\\main.py", "--windows-standalone-build", "--disable-auto-launch"])
                .current_dir(paths::root())
                // custom nodes print characters cp1251 cannot encode, which kills ComfyUI's worker thread
                .env("PYTHONIOENCODING", "utf-8")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(NO_WINDOW)
                .spawn()?;
            let out: Box<dyn Read + Send> = Box::new(child.stdout.take().unwrap());
            let err: Box<dyn Read + Send> = Box::new(child.stderr.take().unwrap());
            for stream in [out, err] {
                let (log, me) = (log.clone(), self.clone());
                std::thread::spawn(move || me.pump(stream, log));
            }
            *guard = Some(child);
        }
        drop(guard);
        for _ in 0..START_TIMEOUT * 2 {
            if self.up() {
                self.touch();
                return Ok(());
            }
            if let Some(p) = self.proc.lock().unwrap().as_mut() {
                if p.try_wait().ok().flatten().is_some() {
                    bail!("ComfyUI закрылся при запуске, смотри logs\\comfyui.out.log");
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        bail!("ComfyUI не успел запуститься за {START_TIMEOUT} с")
    }

    /// Copies ComfyUI's console output into logs\comfyui.out.log and picks out LatentSync's progress lines.
    fn pump(&self, stream: Box<dyn Read + Send>, log: Arc<Mutex<File>>) {
        let re = regex::Regex::new(r"Doing inference\.\.\.:\s*\d+%\|[^|]*\|\s*(\d+)/(\d+)").unwrap();
        let mut r = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            // tqdm redraws with \r, so a "line" ends at either \r or \n
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let _ = log.lock().unwrap().write_all(&buf);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(c) = re.captures_iter(&text).last() {
                        let mut s = self.infer_step.lock().unwrap();
                        *s = (s.0 + 1, c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0));
                    }
                }
            }
        }
    }

    fn websocket_loop(&self) {
        loop {
            if self.up() {
                if let Ok((mut ws, _)) = tungstenite::connect(format!("ws://127.0.0.1:8188/ws?clientId={}", self.cid)) {
                    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_mut() {
                        let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                    }
                    loop {
                        match ws.read() {
                            Ok(tungstenite::Message::Text(t)) => {
                                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                                    self.event(&v);
                                }
                            }
                            Ok(_) => {}
                            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                                if !self.up() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(700));
        }
    }

    fn event(&self, v: &Value) {
        let d = &v["data"];
        let pid = d["prompt_id"].as_str().map(str::to_string);
        let mut p = self.progress.lock().unwrap();
        if pid.is_some() && pid != p.pid {
            *p = Progress { pid: pid.clone(), ..Default::default() };
        }
        match v["type"].as_str().unwrap_or("") {
            "executing" => p.node = d["node"].as_str().map(str::to_string),
            "progress" => {
                let n = d["node"].as_str().unwrap_or("").to_string();
                p.nodes.insert(n, (d["value"].as_f64().unwrap_or(0.0), d["max"].as_f64().unwrap_or(0.0)));
            }
            "progress_state" => {
                if let Some(nodes) = d["nodes"].as_object() {
                    for (id, n) in nodes {
                        p.nodes.insert(id.clone(), (n["value"].as_f64().unwrap_or(0.0), n["max"].as_f64().unwrap_or(0.0)));
                        if n["state"] == "running" {
                            p.node = Some(id.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn idle_loop(&self) {
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if !self.started_by_us() {
                continue;
            }
            let (run, pend) = self.queue().unwrap_or_default();
            if self.active.load(Ordering::Relaxed) > 0 || !run.is_empty() || !pend.is_empty() {
                self.touch();
                continue;
            }
            let limit = if self.long_idle.load(Ordering::Relaxed) { IDLE_VIDEO } else { IDLE };
            if self.last.lock().unwrap().elapsed().as_secs() > limit {
                self.hard_stop();
                self.long_idle.store(false, Ordering::Relaxed);
            }
        }
    }

    /// Stops the ComfyUI this app started (LatentSync cannot be interrupted, so cancelling it means this).
    pub fn hard_stop(&self) -> bool {
        let Some(mut p) = self.proc.lock().unwrap().take() else { return false };
        if p.try_wait().ok().flatten().is_some() {
            return false;
        }
        kill_tree(p.id());
        let _ = p.wait();
        true
    }

    pub fn get(&self, path: &str) -> Result<Value> {
        let mut r = self.http.get(format!("{COMFY}{path}")).call()?;
        let text = r.body_mut().read_to_string()?;
        if !r.status().is_success() {
            bail!("ComfyUI {}: {}", r.status(), text.chars().take(300).collect::<String>());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let mut r = self.http.post(format!("{COMFY}{path}")).send_json(body)?;
        let status = r.status();
        let text = r.body_mut().read_to_string().unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("{}", describe_rejection(&v).unwrap_or_else(|| format!("ComfyUI {status}: {}", text.chars().take(300).collect::<String>())));
        }
        Ok(v)
    }

    /// Queues a graph and returns its prompt id. Starts ComfyUI first if needed.
    pub fn submit(self: &Arc<Self>, graph: &Value) -> Result<String> {
        self.ensure()?;
        let r = self.post("/prompt", &json!({"prompt": graph, "client_id": self.cid}))?;
        r["prompt_id"].as_str().map(str::to_string).ok_or_else(|| anyhow!("ComfyUI не вернул номер задачи"))
    }

    pub fn history(&self, pid: &str) -> Result<Option<Value>> {
        Ok(self.get(&format!("/history/{pid}"))?.get(pid).cloned())
    }

    /// (running, pending) prompt ids.
    pub fn queue(&self) -> Result<(Vec<String>, Vec<String>)> {
        let mut r = self.quick.get(format!("{COMFY}/queue")).call()?;
        let q: Value = r.body_mut().read_json()?;
        let ids = |k: &str| q[k].as_array().map(|a| a.iter().filter_map(|x| x[1].as_str().map(str::to_string)).collect()).unwrap_or_default();
        Ok((ids("queue_running"), ids("queue_pending")))
    }

    pub fn interrupt(&self) { let _ = self.post("/interrupt", &json!({})); }
    pub fn free(&self) { let _ = self.post("/free", &json!({"unload_models": true})); }
    pub fn delete_pending(&self, ids: &[String]) { let _ = self.post("/queue", &json!({"delete": ids})); }

    /// Uploads a picture into ComfyUI's input folder (overwriting a same-named file) and returns its name there.
    pub fn upload_image(self: &Arc<Self>, file: &Path) -> Result<String> {
        self.ensure()?;
        let data = std::fs::read(file)?;
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("image.png").replace('"', "");
        let boundary = format!("----studio{:x}", data.len() ^ 0x9e3779b9);
        let mut body = Vec::with_capacity(data.len() + 512);
        write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n")?;
        write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n")?;
        body.extend_from_slice(&data);
        write!(body, "\r\n--{boundary}--\r\n")?;
        let mut r = self.http.post(format!("{COMFY}/upload/image"))
            .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
            .send(&body[..])?;
        let v: Value = r.body_mut().read_json().map_err(|_| anyhow!("не удалось загрузить фото в ComfyUI"))?;
        let n = v["name"].as_str().ok_or_else(|| anyhow!("не удалось загрузить фото в ComfyUI"))?;
        Ok(match v["subfolder"].as_str() { Some(s) if !s.is_empty() => format!("{s}/{n}"), _ => n.to_string() })
    }

    /// Stops what this app started (called when the window closes / the headless server exits).
    pub fn shutdown(&self) { self.hard_stop(); }
}

/// Turns ComfyUI's 400 answer to /prompt into one readable line.
fn describe_rejection(v: &Value) -> Option<String> {
    if let Some(errs) = v["node_errors"].as_object() {
        if let Some((_, ne)) = errs.iter().next() {
            let e = &ne["errors"][0];
            return Some(format!("ComfyUI отклонил задачу ({}): {} {}", ne["class_type"].as_str().unwrap_or("?"),
                e["message"].as_str().unwrap_or(""), e["details"].as_str().unwrap_or("")).trim().to_string());
        }
    }
    v["error"]["message"].as_str().map(|m| format!("ComfyUI отклонил задачу: {m}"))
}
