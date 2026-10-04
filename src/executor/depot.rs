use std::io::{Read, Write};

use super::executor::{ExecutionError, Executor};
use super::serve::reply;
use super::store;

pub(super) fn fetch(exec: &Executor, base: &str, name: &str, version: &str, dest: &str) -> Result<String, ExecutionError> {
    if std::path::Path::new(dest).exists() {
        return Err(ExecutionError::new(3003, "目标目录已存在".into()));
    }
    std::fs::create_dir_all(dest).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    let result = fetch_into(exec, base, name, version, dest);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(dest);
    }
    result?;
    Ok(dest.to_string())
}

fn fetch_into(exec: &Executor, base: &str, name: &str, version: &str, dest: &str) -> Result<(), ExecutionError> {
    let manifest = format!("{dest}/manifest.json");
    expect(exec, "GET", &url(base, &format!("/dake/v1/{name}/{version}/manifest")), None, Some(&manifest), &[200])?;
    let (_, _, files) = store::read_manifest(dest)?;
    for (file, _, _) in files {
        let path = format!("{dest}/{file}");
        expect(exec, "GET", &url(base, &format!("/dake/v1/{name}/{version}/file/{}", encode(&file))), None, Some(&path), &[200])?;
    }
    for extra in ["dake.pub", "dake.seal"] {
        let path = format!("{dest}/{extra}");
        let status = expect(exec, "GET", &url(base, &format!("/dake/v1/{name}/{version}/file/{extra}")), None, Some(&path), &[200, 404])?;
        if status == 404 {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

pub(super) fn push(exec: &Executor, base: &str, dir: &str) -> Result<(), ExecutionError> {
    let (name, version, files) = store::read_manifest(dir)?;
    let manifest = format!("{dir}/manifest.json");
    expect(exec, "PUT", &url(base, &format!("/dake/v1/{name}/{version}/manifest")), Some(&manifest), None, &[201])?;
    for (file, _, _) in &files {
        let path = format!("{dir}/{file}");
        expect(exec, "PUT", &url(base, &format!("/dake/v1/{name}/{version}/file/{}", encode(file))), Some(&path), None, &[201])?;
    }
    for extra in ["dake.pub", "dake.seal"] {
        if files.iter().any(|(name, _, _)| name == extra) {
            continue;
        }
        let path = format!("{dir}/{extra}");
        if std::path::Path::new(&path).is_file() {
            expect(exec, "PUT", &url(base, &format!("/dake/v1/{name}/{version}/file/{extra}")), Some(&path), None, &[201])?;
        }
    }
    expect(exec, "POST", &url(base, &format!("/dake/v1/{name}/{version}")), None, None, &[201])?;
    Ok(())
}

fn expect(exec: &Executor, method: &str, url: &str, upload: Option<&str>, save: Option<&str>, ok: &[u16]) -> Result<u16, ExecutionError> {
    let (status, body) = super::net::http_call(exec, method, url, upload, save)?;
    if ok.contains(&status) {
        Ok(status)
    } else {
        Err(ExecutionError::new(4033, format!("协议状态 {status} {body}")))
    }
}

fn url(base: &str, path: &str) -> String {
    format!("{}{path}", base.trim_end_matches('/'))
}

fn encode(text: &str) -> String {
    text.bytes().map(|byte| match byte {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (byte as char).to_string(),
        _ => format!("%{byte:02X}"),
    }).collect()
}

pub(super) fn handle(exec: &mut Executor, method: &str, path: &str, _head: &str, length: Option<u64>, rest: &[u8], stream: &mut (impl Read + Write)) -> Result<(), ExecutionError> {
    let repo = exec.repo.clone().ok_or_else(|| ExecutionError::new(4011, "没有 repo".into()))?;
    let route = parse(path)?;
    match (method, route) {
        ("GET", Route::Versions(name)) => {
            let versions = store::versions(&repo.dir, &name)?;
            let body = format!("{{\"versions\":[{}]}}\n", versions.iter().map(|v| format!("\"{v}\"")).collect::<Vec<_>>().join(","));
            reply(stream, 200, body.as_bytes(), "application/json", &[])
        }
        ("GET", Route::Manifest(name, version)) => {
            let dir = store::get(&repo.dir, &name, &version)?;
            stream_file(stream, &format!("{dir}/manifest.json"), "application/json")
        }
        ("GET", Route::File(name, version, rel)) => {
            let dir = store::get(&repo.dir, &name, &version)?;
            let allowed = file_allowed(&dir, &rel)?;
            if !allowed {
                return Err(ExecutionError::new(3002, format!("仓库没有 {rel}")));
            }
            stream_file(stream, &format!("{dir}/{rel}"), "application/octet-stream")
        }
        ("PUT", Route::Manifest(name, version)) => {
            let length = length.ok_or_else(|| ExecutionError::new(4011, "PUT 需要 Content-Length".into()))?;
            let mut body = Body { rest, stream, rest_at: 0 };
            store::put_manifest(&exec.repo_lock, &repo.dir, &name, &version, &mut body, length)?;
            reply(stream, 201, b"ok", "text/plain", &[])
        }
        ("PUT", Route::File(name, version, rel)) => {
            let length = length.ok_or_else(|| ExecutionError::new(4011, "PUT 需要 Content-Length".into()))?;
            let mut body = Body { rest, stream, rest_at: 0 };
            store::put_file(&exec.repo_lock, &repo.dir, &name, &version, &rel, &mut body, length)?;
            reply(stream, 201, b"ok", "text/plain", &[])
        }
        ("POST", Route::Commit(name, version)) => {
            if let Some(length) = length {
                let mut body = Body { rest, stream, rest_at: 0 };
                let mut left = length;
                let mut buf = vec![0u8; 1024 * 1024];
                while left > 0 {
                    let take = std::cmp::min(left as usize, buf.len());
                    let n = body.read(&mut buf[..take]).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
                    if n == 0 {
                        break;
                    }
                    left -= n as u64;
                }
            }
            store::commit_staging(&exec.repo_lock, &repo.dir, &name, &version, repo.capacity, repo.max_pkgs)?;
            reply(stream, 201, b"ok", "text/plain", &[])
        }
        _ => Err(ExecutionError::new(4011, "协议方法不对".into())),
    }
}

enum Route {
    Versions(String),
    Manifest(String, String),
    File(String, String, String),
    Commit(String, String),
}

fn parse(path: &str) -> Result<Route, ExecutionError> {
    let rest = path.trim_start_matches("/dake/v1/");
    if rest.is_empty() || rest.ends_with('/') {
        return Err(ExecutionError::new(3001, "协议路径不合法".into()));
    }
    let mut parts = rest.splitn(3, '/');
    let name = parts.next().unwrap_or("");
    let version = parts.next();
    let tail = parts.next();
    match (version, tail) {
        (None, None) => Ok(Route::Versions(name.to_string())),
        (Some(version), None) => Ok(Route::Commit(name.to_string(), version.to_string())),
        (None, Some(_)) => Err(ExecutionError::new(3001, "协议路径不合法".into())),
        (Some(version), Some(tail)) => {
            if tail == "manifest" {
                Ok(Route::Manifest(name.to_string(), version.to_string()))
            } else if let Some(rel) = tail.strip_prefix("file/") {
                if rel.is_empty() || rel.ends_with('/') {
                    return Err(ExecutionError::new(3001, "协议路径不合法".into()));
                }
                Ok(Route::File(name.to_string(), version.to_string(), rel.to_string()))
            } else {
                Err(ExecutionError::new(3001, "协议路径不合法".into()))
            }
        }
    }
}

fn file_allowed(dir: &str, rel: &str) -> Result<bool, ExecutionError> {
    if rel == "dake.key" {
        return Ok(false);
    }
    let (_, _, files) = store::read_manifest(dir)?;
    let listed = files.iter().any(|(name, _, _)| name == rel);
    if listed || ((rel == "dake.pub" || rel == "dake.seal") && std::path::Path::new(&format!("{dir}/{rel}")).is_file()) {
        return Ok(std::path::Path::new(&format!("{dir}/{rel}")).is_file());
    }
    Ok(false)
}

fn stream_file(stream: &mut impl Write, path: &str, content_type: &str) -> Result<(), ExecutionError> {
    let file = std::fs::File::open(path).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
    let len = file.metadata().map_err(|err| ExecutionError::new(3002, err.to_string()))?.len();
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        if n == 0 {
            break;
        }
        stream.write_all(&buf[..n]).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    }
    Ok(())
}

struct Body<'a, S> {
    rest: &'a [u8],
    stream: &'a mut S,
    rest_at: usize,
}

impl<S: Read> Read for Body<'_, S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.rest_at < self.rest.len() {
            let n = std::cmp::min(buf.len(), self.rest.len() - self.rest_at);
            buf[..n].copy_from_slice(&self.rest[self.rest_at..self.rest_at + n]);
            self.rest_at += n;
            return Ok(n);
        }
        self.stream.read(buf)
    }
}
