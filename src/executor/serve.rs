use super::access::probe_dir;
use super::builtin::{content_type_for, decode_block_file, decode_body, encode_body};
use super::executor::{ExecutionError, Executor};
use super::net::{header_object, listen_addr};
use super::paths::{safe_path, walk_files};
use super::select::pick;
use crate::dsl::ast::Value;
use crate::dsl::ir::RouteIr;
use notify::Watcher;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
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

const QUEUE: usize = 32;

#[derive(Clone)]
pub(super) struct ServeUrl {
    pub address: String,
    pub cert: Option<String>,
    pub key: Option<String>,
}

#[derive(Clone)]
pub(super) struct DirSource {
    pub path: String,
    pub suffix: Option<String>,
    pub deep: bool,
    pub exclude: Vec<String>,
}

struct Live<'a> {
    dirs: HashMap<String, DirSource>,
    routes: &'a [RouteIr],
    watch_rx: Receiver<notify::Result<notify::Event>>,
    _watcher: notify::RecommendedWatcher,
    pending: HashMap<PathBuf, (u64, SystemTime, Instant)>,
    queue: VecDeque<Job>,
    incoming: Receiver<Incoming>,
    accept_stop: Arc<AtomicBool>,
    acceptors: Vec<JoinHandle<()>>,
}

enum Job {
    File(String, PathBuf),
}

struct Incoming {
    source: String,
    stream: TcpStream,
    tls: Option<Arc<ServerConfig>>,
}

pub(super) fn run(exec: &mut Executor, routes: &[RouteIr], workers: u32, repo: Option<String>) -> Result<(), ExecutionError> {
    exec.depot_url = repo;
    let stop = serve_stop();
    stop.store(false, Ordering::Relaxed);
    let mut live = Live::open(exec, routes)?;
    let scanned = live.scan()?;
    if workers <= 1 {
        for (source, path) in scanned {
            if stop.load(Ordering::Relaxed) || exec.halt {
                break;
            }
            live.handle_file(exec, &source, path);
            if exec.halt {
                break;
            }
        }
    } else {
        for chunk in scanned.chunks(workers as usize) {
            if stop.load(Ordering::Relaxed) || exec.halt {
                break;
            }
            live.handle_files(exec, chunk);
        }
    }
    while !stop.load(Ordering::Relaxed) && !exec.halt {
        live.pump()?;
        if workers > 1 && !live.queue.is_empty() {
            let mut jobs = Vec::new();
            while jobs.len() < workers as usize {
                let Some(Job::File(source, path)) = live.queue.pop_front() else { break };
                jobs.push((source, path));
            }
            live.handle_files(exec, &jobs);
        } else if let Some(Job::File(source, path)) = live.queue.pop_front() {
            live.handle_file(exec, &source, path);
        } else if workers <= 1 {
            if let Some(incoming) = live.next_conn()? {
                live.handle_conn(exec, incoming);
            }
        } else {
            let batch = live.take_batch(workers as usize)?;
            live.handle_many(exec, batch);
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
        let (tx, incoming) = std::sync::mpsc::sync_channel(QUEUE);
        let accept_stop = Arc::new(AtomicBool::new(false));
        let mut acceptors = Vec::new();
        for (name, url) in &exec.serve_urls {
            let (host, port) = listen_addr(&url.address)?;
            let listener = TcpListener::bind(format!("{host}:{port}")).map_err(|err| {
                ExecutionError::new(4035, format!("监听 {} 失败: {err}", url.address))
            })?;
            listener.set_nonblocking(true).map_err(|err| ExecutionError::new(4035, err.to_string()))?;
            let tls = match (&url.cert, &url.key) {
                (Some(cert), Some(key)) => Some(server_config(cert, key)?),
                (None, None) => None,
                _ => return Err(ExecutionError::new(4032, "cert 与 key 必须同时给出".into())),
            };
            let source = name.clone();
            let stop = Arc::clone(&accept_stop);
            let sender = tx.clone();
            acceptors.push(std::thread::spawn(move || accept_loop(source, listener, tls, sender, stop)));
        }
        drop(tx);
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
            dirs: exec.serve_dirs.clone(),
            routes,
            watch_rx,
            _watcher: watcher,
            pending: HashMap::new(),
            queue: VecDeque::new(),
            incoming,
            accept_stop,
            acceptors,
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
        Ok(())
    }

    fn take_batch(&self, n: usize) -> Result<Vec<Incoming>, ExecutionError> {
        let mut batch = Vec::new();
        if let Some(first) = self.next_conn()? {
            batch.push(first);
        }
        while batch.len() < n {
            match self.incoming.try_recv() {
                Ok(item) => batch.push(item),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) if self.acceptors.is_empty() => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(ExecutionError::new(4031, "监听已断开".into()));
                }
            }
        }
        Ok(batch)
    }

    fn handle_many(&self, exec: &mut Executor, jobs: Vec<Incoming>) {
        if jobs.is_empty() {
            return;
        }
        if jobs.len() == 1 {
            self.handle_conn(exec, jobs.into_iter().next().unwrap());
            return;
        }
        let routes = self.routes.to_vec();
        let workers: Vec<Executor> = smol::block_on(async {
            let mut tasks = Vec::new();
            for job in jobs {
                let mut worker = exec.fork();
                let routes = routes.clone();
                tasks.push(smol::unblock(move || {
                    respond(&mut worker, &routes, job);
                    worker
                }));
            }
            let mut done = Vec::new();
            for task in tasks {
                done.push(task.await);
            }
            done
        });
        for worker in workers {
            exec.absorb(worker);
        }
    }

    fn next_conn(&self) -> Result<Option<Incoming>, ExecutionError> {
        if !self.queue.is_empty() {
            return Ok(None);
        }
        match self.incoming.recv_timeout(Duration::from_millis(30)) {
            Ok(incoming) => Ok(Some(incoming)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) if self.acceptors.is_empty() => {
                std::thread::sleep(Duration::from_millis(30));
                Ok(None)
            },
            Err(RecvTimeoutError::Disconnected) => Err(ExecutionError::new(4031, "监听已断开".into())),
        }
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

    fn handle_files(&self, exec: &mut Executor, jobs: &[(String, PathBuf)]) {
        if jobs.is_empty() {
            return;
        }
        if jobs.len() == 1 {
            self.handle_file(exec, &jobs[0].0, jobs[0].1.clone());
            return;
        }
        let routes = self.routes.to_vec();
        let dirs = self.dirs.clone();
        let done = smol::block_on(async {
            let mut tasks = Vec::new();
            for (source, path) in jobs.iter().cloned() {
                let mut worker = exec.fork();
                let routes = routes.clone();
                let dirs = dirs.clone();
                tasks.push(smol::unblock(move || {
                    respond_file(&mut worker, &routes, &dirs, &source, path);
                    worker
                }));
            }
            let mut out = Vec::new();
            for task in tasks {
                out.push(task.await);
            }
            out
        });
        for worker in done {
            exec.absorb(worker);
        }
    }

    fn handle_file(&self, exec: &mut Executor, source: &str, path: PathBuf) {
        respond_file(exec, self.routes, &self.dirs, source, path);
    }

    fn handle_conn(&self, exec: &mut Executor, incoming: Incoming) {
        respond(exec, self.routes, incoming);
    }
}

fn respond_file(exec: &mut Executor, routes: &[RouteIr], dirs: &HashMap<String, DirSource>, source: &str, path: PathBuf) {
    let Some(dir) = dirs.get(source) else { return };
    let rel = relative(dir, &path);
    let Some(rel) = rel else { return };
    if !allowed(dir, &rel) {
        return;
    }
    let Some((route, caps)) = pick(routes, source, false, &rel) else { return };
    let record = match decode_block_file(exec, &route.struct_id, &path.to_string_lossy()) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };
    if let Err(err) = dispatch_value(exec, route, record, caps, Value::Object(std::sync::Arc::new(Vec::new()))) {
        eprintln!("{err}");
    }
}

fn respond(exec: &mut Executor, routes: &[RouteIr], incoming: Incoming) {
        let Incoming { source, mut stream, tls } = incoming;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
        if let Some(config) = tls {
            let mut session = match rustls::ServerConnection::new(config) {
                Ok(session) => session,
                Err(err) => {
                    eprintln!("{}", ExecutionError::new(4032, err.to_string()));
                    return;
                }
            };
            let mut tls_stream = rustls::Stream::new(&mut session, &mut stream);
            finish_conn(exec, routes, &source, &mut tls_stream);
            return;
        }
        finish_conn(exec, routes, &source, &mut stream);
}

fn finish_conn(exec: &mut Executor, routes: &[RouteIr], source: &str, stream: &mut (impl std::io::Read + Write)) {
        let (head, rest) = match read_head(stream) {
            Ok(parts) => parts,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };
        let request = head.lines().next().unwrap_or("");
        let mut parts = request.split_whitespace();
        let method = parts.next().unwrap_or("");
        let target = parts.next().unwrap_or("/");
        let bare = target.split('?').next().unwrap_or("/");
        let path = match percent_decode(bare) {
            Ok(path) => path,
            Err(()) => {
                let _ = empty(stream, 400);
                return;
            }
        };
        if exec.depot_url.as_deref() == Some(source) && path.starts_with("/dake/v1/") {
            let length = content_length(&head);
            if let Err(err) = super::depot::handle(exec, method, &path, &head, length, &rest, stream) {
                let status = http_status(err.code());
                let _ = reply(stream, status, err.explain().as_bytes(), "text/plain", &[]);
            }
            return;
        }
        let raw = match read_body(stream, &head, &rest) {
            Ok(raw) => raw,
            Err(err) => {
                eprintln!("{err}");
                return;
            }
        };
        let rel = path.trim_start_matches('/');
        let Some((route, caps)) = pick(routes, source, true, rel) else {
            let _ = empty(stream, 404);
            return;
        };
        let body = raw;
        let headers = header_object(&head);
        match dispatch(exec, route, body, caps, headers) {
            Ok(Value::Bytes(bytes)) => {
                match exec.take_http_meta() {
                    Ok((status, extra)) => { let _ = reply(stream, status, bytes.as_slice(), "application/octet-stream", &extra); }
                    Err(err) => { eprintln!("{err}"); let _ = empty(stream, 500); }
                }
            }
            Ok(Value::Record { path: id, fields }) => {
                match exec.take_http_meta().and_then(|(status, extra)| {
                    let bytes = encode_body(exec, &id, &fields)?;
                    let kind = content_type_for(exec, &id)?;
                    reply(stream, status, &bytes, kind, &extra)
                }) {
                    Ok(()) => {}
                    Err(err) => {
                        eprintln!("{err}");
                        let _ = empty(stream, 500);
                    }
                }
            }
            Ok(_) => { let _ = empty(stream, 500); }
            Err(err) => {
                eprintln!("{err}");
                let _ = empty(stream, 500);
            }
        }
}

fn dispatch(exec: &mut Executor, route: &RouteIr, body: Vec<u8>, caps: Vec<String>, headers: Value) -> Result<Value, ExecutionError> {
    let record = decode_body(exec, &route.struct_id, body)?;
    dispatch_value(exec, route, record, caps, headers)
}

fn dispatch_value(exec: &mut Executor, route: &RouteIr, record: Value, caps: Vec<String>, headers: Value) -> Result<Value, ExecutionError> {
    let mut args = vec![record];
    args.extend(caps.into_iter().map(Value::String));
    if exec.param_count(&route.action)? == args.len() + 1 {
        args.push(headers);
    }
    exec.serve_depth += 1;
    let result = exec.call_action(&route.action, args);
    exec.serve_depth -= 1;
    result
}

fn accept_loop(source: String, listener: TcpListener, tls: Option<Arc<ServerConfig>>, tx: SyncSender<Incoming>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                if tx.send(Incoming { source: source.clone(), stream, tls: tls.clone() }).is_err() {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break,
        }
    }
}

pub(super) fn server_config(cert_path: &str, key_path: &str) -> Result<Arc<ServerConfig>, ExecutionError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert_bytes = std::fs::read(cert_path).map_err(|err| ExecutionError::new(4032, format!("{cert_path}: {err}")))?;
    let key_bytes = std::fs::read(key_path).map_err(|err| ExecutionError::new(4032, format!("{key_path}: {err}")))?;
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExecutionError::new(4032, format!("cert 不是合法的 PEM: {err}")))?;
    if certs.is_empty() {
        return Err(ExecutionError::new(4032, "cert 里没有证书".into()));
    }
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_bytes))
        .map_err(|err| ExecutionError::new(4032, format!("key 不是合法的 PEM: {err}")))?
        .ok_or_else(|| ExecutionError::new(4032, "key 里没有私钥".into()))?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| ExecutionError::new(4032, err.to_string()))?;
    Ok(Arc::new(config))
}

impl Drop for Live<'_> {
    fn drop(&mut self) {
        self.accept_stop.store(true, Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let _ = std::mem::replace(&mut self.incoming, rx);
        drop(tx);
        for handle in self.acceptors.drain(..) {
            let _ = handle.join();
        }
    }
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

fn read_head(stream: &mut impl std::io::Read) -> Result<(String, Vec<u8>), ExecutionError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..index]).into_owned();
            return Ok((head, raw[index + 4..].to_vec()));
        }
        if raw.len() > 65536 {
            return Err(ExecutionError::new(4034, "请求头过长".into()));
        }
        match stream.read(&mut buf) {
            Ok(0) => return Err(ExecutionError::new(4031, "请求不是 HTTP".into())),
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut => {
                return Err(ExecutionError::new(4031, "读取请求超时".into()));
            }
            Err(err) => return Err(ExecutionError::new(4031, err.to_string())),
        }
    }
}

fn content_length(head: &str) -> Option<u64> {
    head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().ok())?
    })
}

fn read_body(stream: &mut impl std::io::Read, head: &str, rest: &[u8]) -> Result<Vec<u8>, ExecutionError> {
    let Some(length) = content_length(head) else {
        return Ok(rest.to_vec());
    };
    let mut body = rest.to_vec();
    let mut buf = vec![0u8; 1024 * 1024];
    while (body.len() as u64) < length {
        let n = stream.read(&mut buf).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    if (body.len() as u64) < length {
        return Err(ExecutionError::new(4031, "正文短于 Content-Length".into()));
    }
    body.truncate(length as usize);
    Ok(body)
}

fn http_status(code: u32) -> u16 {
    match code {
        3002 => 404,
        3003 => 409,
        3010 => 413,
        _ => 400,
    }
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

fn empty(stream: &mut impl Write, status: u16) -> Result<(), ExecutionError> {
    reply(stream, status, b"", "text/plain", &[])
}

pub(super) fn reply(stream: &mut impl Write, status: u16, body: &[u8], content_type: &str, extra: &[(String, String)]) -> Result<(), ExecutionError> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        500 => "Error",
        _ => "Status",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\n");
    let mut content_type = content_type.to_string();
    for (name, value) in extra {
        if name.eq_ignore_ascii_case("Content-Length") || name.eq_ignore_ascii_case("Connection") {
            continue;
        }
        if name.eq_ignore_ascii_case("Content-Type") {
            content_type = value.clone();
            continue;
        }
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!("Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()));
    stream.write_all(head.as_bytes()).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    stream.write_all(body).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    stream.flush().map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    Ok(())
}

