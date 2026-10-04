use super::access::probe_dir;
use super::builtin::{content_type_for, decode_body, encode_body};
use super::executor::{ExecutionError, Executor};
use super::net::{listen_addr, read_raw};
use super::paths::{safe_path, walk_files};
use super::select::pick;
use crate::dsl::ast::Value;
use crate::dsl::ir::RouteIr;
use notify::Watcher;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant, SystemTime};

static SERVE_STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn serve_stop() -> &'static Arc<AtomicBool> {
    SERVE_STOP.get_or_init(|| {
        let flag = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&flag));
        flag
    })
}

#[derive(Clone)]
pub(super) struct DirSource {
    pub path: String,
    pub suffix: Option<String>,
    pub deep: bool,
    pub exclude: Vec<String>,
}

struct Live<'a> {
    urls: Vec<(String, TcpListener)>,
    dirs: HashMap<String, DirSource>,
    routes: &'a [RouteIr],
    watch_rx: Receiver<notify::Result<notify::Event>>,
    _watcher: notify::RecommendedWatcher,
    pending: HashMap<PathBuf, (u64, SystemTime, Instant)>,
    queue: VecDeque<Job>,
}

enum Job {
    File(String, PathBuf),
    Conn(String, TcpStream),
}

pub(super) fn run(exec: &mut Executor, routes: &[RouteIr]) -> Result<(), ExecutionError> {
    let stop = serve_stop();
    stop.store(false, Ordering::Relaxed);
    let mut live = Live::open(exec, routes)?;
    let scanned = live.scan()?;
    for (source, path) in scanned {
        if stop.load(Ordering::Relaxed) || exec.halt {
            break;
        }
        live.handle_file(exec, &source, path);
        if exec.halt {
            break;
        }
    }
    while !stop.load(Ordering::Relaxed) && !exec.halt {
        live.pump()?;
        if let Some(job) = live.queue.pop_front() {
            match job {
                Job::File(source, path) => live.handle_file(exec, &source, path),
                Job::Conn(source, stream) => live.handle_conn(exec, &source, stream),
            }
        } else {
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    exec.halt = false;
    Ok(())
}

impl<'a> Live<'a> {
    fn open(exec: &Executor, routes: &'a [RouteIr]) -> Result<Self, ExecutionError> {
        if routes.is_empty() {
            return Err(ExecutionError::new(4035, "serve 没有路由".into()));
        }
        let mut urls = Vec::new();
        for (name, address) in &exec.serve_urls {
            let (host, port) = listen_addr(address)?;
            let listener = TcpListener::bind(format!("{host}:{port}")).map_err(|err| {
                ExecutionError::new(4035, format!("监听 {address} 失败: {err}"))
            })?;
            listener.set_nonblocking(true).map_err(|err| ExecutionError::new(4035, err.to_string()))?;
            urls.push((name.clone(), listener));
        }
        for (name, dir) in &exec.serve_dirs {
            let path = safe_path(&dir.path)?;
            probe_dir(&path).map_err(|err| ExecutionError::new(4035, format!("{name}: {err}")))?;
        }
        let (tx, watch_rx) = channel();
        let mut watcher = notify::recommended_watcher(tx).map_err(|err| ExecutionError::new(4035, err.to_string()))?;
        for dir in exec.serve_dirs.values() {
            let mode = if dir.deep { notify::RecursiveMode::Recursive } else { notify::RecursiveMode::NonRecursive };
            watcher.watch(std::path::Path::new(&safe_path(&dir.path)?), mode).map_err(|err| ExecutionError::new(4035, err.to_string()))?;
        }
        Ok(Self {
            urls,
            dirs: exec.serve_dirs.clone(),
            routes,
            watch_rx,
            _watcher: watcher,
            pending: HashMap::new(),
            queue: VecDeque::new(),
        })
    }

    fn scan(&self) -> Result<Vec<(String, PathBuf)>, ExecutionError> {
        let mut out = Vec::new();
        for (name, dir) in &self.dirs {
            let mut found = Vec::new();
            walk_files(std::path::Path::new(&safe_path(&dir.path)?), dir.deep, dir.suffix.as_deref(), &dir.exclude, &mut found)?;
            found.sort();
            for path in found {
                out.push((name.clone(), PathBuf::from(path)));
            }
        }
        Ok(out)
    }

    fn pump(&mut self) -> Result<(), ExecutionError> {
        while let Ok(event) = self.watch_rx.try_recv() {
            let Ok(event) = event else { continue };
            if matches!(event.kind, notify::EventKind::Remove(_)) {
                continue;
            }
            for path in event.paths {
                if path.is_file() {
                    self.note(path);
                }
            }
        }
        let mut ready = Vec::new();
        let mut changed = Vec::new();
        for (path, (len, modified, seen)) in &self.pending {
            let Ok(meta) = std::fs::metadata(path) else { continue };
            let same = meta.len() == *len && meta.modified().ok() == Some(*modified);
            if same && seen.elapsed() > Duration::from_millis(200) {
                ready.push(path.clone());
            } else if !same {
                if let Ok(modified) = meta.modified() {
                    changed.push((path.clone(), meta.len(), modified));
                }
            }
        }
        for (path, len, modified) in changed {
            self.pending.insert(path, (len, modified, Instant::now()));
        }
        for path in ready {
            self.pending.remove(&path);
            if let Some(source) = self.owner(&path) {
                self.queue.push_back(Job::File(source, path));
            }
        }
        for (name, listener) in &self.urls {
            match listener.accept() {
                Ok((stream, _)) => self.queue.push_back(Job::Conn(name.clone(), stream)),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => return Err(ExecutionError::new(4031, err.to_string())),
            }
        }
        Ok(())
    }

    fn note(&mut self, path: PathBuf) {
        let Ok(meta) = std::fs::metadata(&path) else { return };
        let Ok(modified) = meta.modified() else { return };
        self.pending.insert(path, (meta.len(), modified, Instant::now()));
    }

    fn owner(&self, path: &PathBuf) -> Option<String> {
        self.dirs.iter().find_map(|(name, dir)| {
            let root = PathBuf::from(safe_path(&dir.path).ok()?);
            path.starts_with(&root).then(|| name.clone())
        })
    }

    fn handle_file(&self, exec: &mut Executor, source: &str, path: PathBuf) {
        let Some(dir) = self.dirs.get(source) else { return };
        let rel = relative(dir, &path);
        let Some(rel) = rel else { return };
        if !allowed(dir, &rel) {
            return;
        }
        let Some((route, caps)) = pick(&self.routes, source, false, &rel) else { return };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("{}", ExecutionError::new(4037, format!("读取 {} 失败: {err}", path.display())));
                return;
            }
        };
        if let Err(err) = dispatch(exec, route, bytes, caps) {
            eprintln!("{err}");
        }
    }

    fn handle_conn(&self, exec: &mut Executor, source: &str, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let raw = match read_raw(&mut stream) {
            Ok(raw) => raw,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };
        let head_end = raw.windows(4).position(|window| window == b"\r\n\r\n").unwrap_or(0);
        let head = String::from_utf8_lossy(&raw[..head_end]);
        let target = head.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("/");
        let bare = target.split('?').next().unwrap_or("/");
        let path = match percent_decode(bare) {
            Ok(path) => path,
            Err(()) => {
                let _ = empty(&mut stream, 400);
                return;
            }
        };
        let rel = path.trim_start_matches('/');
        let Some((route, caps)) = pick(&self.routes, source, true, rel) else {
            let _ = empty(&mut stream, 404);
            return;
        };
        let body = raw.get(head_end + 4..).unwrap_or_default().to_vec();
        match dispatch(exec, route, body, caps) {
            Ok(Value::Bytes(bytes)) => { let _ = reply(&mut stream, 200, bytes.as_slice(), "application/octet-stream"); }
            Ok(Value::Record { path: id, fields }) => {
                match encode_body(exec, &id, &fields).and_then(|bytes| {
                    let kind = content_type_for(exec, &id)?;
                    reply(&mut stream, 200, &bytes, kind)
                }) {
                    Ok(()) => {}
                    Err(err) => {
                        eprintln!("{err}");
                        let _ = empty(&mut stream, 500);
                    }
                }
            }
            Ok(_) => { let _ = empty(&mut stream, 500); }
            Err(err) => {
                eprintln!("{err}");
                let _ = empty(&mut stream, 500);
            }
        }
    }
}

fn dispatch(exec: &mut Executor, route: &RouteIr, body: Vec<u8>, caps: Vec<String>) -> Result<Value, ExecutionError> {
    let record = decode_body(exec, &route.struct_id, body)?;
    let mut args = vec![record];
    args.extend(caps.into_iter().map(Value::String));
    exec.serve_depth += 1;
    let result = exec.call_action(&route.action, args);
    exec.serve_depth -= 1;
    result
}

fn relative(dir: &DirSource, path: &PathBuf) -> Option<String> {
    let root = PathBuf::from(safe_path(&dir.path).ok()?);
    let rel = path.strip_prefix(&root).ok()?;
    let mut text = String::new();
    for part in rel.components() {
        if !text.is_empty() {
            text.push('/');
        }
        text.push_str(&part.as_os_str().to_string_lossy());
    }
    Some(text)
}

fn allowed(dir: &DirSource, rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if dir.suffix.as_ref().is_some_and(|suffix| !name.ends_with(suffix)) {
        return false;
    }
    if !dir.deep && rel.contains('/') {
        return false;
    }
    !rel.split('/').any(|part| dir.exclude.iter().any(|item| item == part))
}

fn percent_decode(text: &str) -> Result<String, ()> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|_| ())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| ())?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn empty(stream: &mut TcpStream, status: u16) -> Result<(), ExecutionError> {
    reply(stream, status, b"", "text/plain")
}

fn reply(stream: &mut TcpStream, status: u16, body: &[u8], content_type: &str) -> Result<(), ExecutionError> {
    let reason = match status { 200 => "OK", 400 => "Bad Request", 404 => "Not Found", 500 => "Error", _ => "Error" };
    let head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(head.as_bytes()).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    stream.write_all(body).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    Ok(())
}

