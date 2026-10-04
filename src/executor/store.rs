use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;

use super::executor::ExecutionError;
use super::paths::hash_chunks;

const CHUNK: usize = 1024 * 1024;

pub(super) fn admit(lock: &Mutex<()>, root: &str, source: &str, capacity: Option<u64>, max_pkgs: Option<u64>) -> Result<String, ExecutionError> {
    let _guard = lock.lock().expect("仓库锁");
    let (name, version) = read_identity(source)?;
    let staging = format!("{root}/.staging/{name}/{version}");
    if Path::new(&staging).exists() {
        fs::remove_dir_all(&staging).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    }
    copy_tree(source, &staging)?;
    match commit(root, &staging, &name, &version, capacity, max_pkgs) {
        Ok(dest) => Ok(dest),
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            Err(err)
        }
    }
}

pub(super) fn get(root: &str, name: &str, version: &str) -> Result<String, ExecutionError> {
    check_segment(name)?;
    check_segment(version)?;
    let dest = format!("{root}/{name}/{version}");
    if !Path::new(&format!("{dest}/manifest.json")).is_file() {
        return Err(ExecutionError::new(3002, format!("仓库没有 {name} {version}")));
    }
    Ok(dest)
}

pub(super) fn versions(root: &str, name: &str) -> Result<Vec<String>, ExecutionError> {
    check_segment(name)?;
    let dir = format!("{root}/{name}");
    if !Path::new(&dir).is_dir() {
        return Err(ExecutionError::new(3002, format!("仓库没有 {name}")));
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| ExecutionError::new(3002, err.to_string()))? {
        let entry = entry.map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        let version = entry.file_name().to_string_lossy().into_owned();
        if Path::new(&format!("{dir}/{version}/manifest.json")).is_file() {
            if let Ok(parsed) = semver::Version::parse(&version) {
                found.push(parsed);
            }
        }
    }
    found.sort();
    Ok(found.into_iter().map(|version| version.to_string()).collect())
}

pub(super) fn put_manifest(lock: &Mutex<()>, root: &str, name: &str, version: &str, stream: &mut impl Read, length: u64) -> Result<(), ExecutionError> {
    let _guard = lock.lock().expect("仓库锁");
    check_segment(name)?;
    check_segment(version)?;
    let staging = format!("{root}/.staging/{name}/{version}");
    if Path::new(&staging).exists() {
        fs::remove_dir_all(&staging).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    }
    fs::create_dir_all(&staging).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    write_stream(&format!("{staging}/manifest.json"), stream, length)?;
    let (got_name, got_version) = read_identity(&staging).map_err(|err| {
        let _ = fs::remove_dir_all(&staging);
        err
    })?;
    if got_name != name || got_version != version {
        let _ = fs::remove_dir_all(&staging);
        return Err(ExecutionError::new(4012, format!("清单是 {got_name} {got_version}，路径是 {name} {version}")));
    }
    Ok(())
}

pub(super) fn put_file(lock: &Mutex<()>, root: &str, name: &str, version: &str, rel: &str, stream: &mut impl Read, length: u64) -> Result<(), ExecutionError> {
    let _guard = lock.lock().expect("仓库锁");
    if rel == "dake.key" {
        return Err(ExecutionError::new(3001, "包内名字不安全: dake.key".into()));
    }
    crate::dsl::pathcheck::tighten(rel).map_err(|message| ExecutionError::new(3001, message))?;
    let staging = format!("{root}/.staging/{name}/{version}");
    if !Path::new(&format!("{staging}/manifest.json")).is_file() {
        return Err(ExecutionError::new(4011, "先提交清单".into()));
    }
    let (_, _, files) = read_manifest(&staging)?;
    let listed = files.iter().any(|(file, _, _)| file == rel);
    if !listed && rel != "dake.pub" && rel != "dake.seal" {
        return Err(ExecutionError::new(3001, format!("包内名字不安全: {rel}")));
    }
    write_stream(&format!("{staging}/{rel}"), stream, length)
}

pub(super) fn commit_staging(lock: &Mutex<()>, root: &str, name: &str, version: &str, capacity: Option<u64>, max_pkgs: Option<u64>) -> Result<String, ExecutionError> {
    let _guard = lock.lock().expect("仓库锁");
    let staging = format!("{root}/.staging/{name}/{version}");
    match commit(root, &staging, name, version, capacity, max_pkgs) {
        Ok(dest) => Ok(dest),
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            Err(err)
        }
    }
}

fn commit(root: &str, staging: &str, name: &str, version: &str, capacity: Option<u64>, max_pkgs: Option<u64>) -> Result<String, ExecutionError> {
    let (got_name, got_version, files) = read_manifest(staging)?;
    if got_name != name || got_version != version {
        return Err(ExecutionError::new(4012, format!("清单是 {got_name} {got_version}，路径是 {name} {version}")));
    }
    let mut names = std::collections::HashSet::new();
    for (file_name, expect, bytes) in &files {
        if !names.insert(file_name.clone()) {
            return Err(ExecutionError::new(4012, format!("清单文件名重复: {file_name}")));
        }
        if file_name == "dake.key" || file_name == "manifest.json" {
            return Err(ExecutionError::new(3001, format!("包内名字不安全: {file_name}")));
        }
        crate::dsl::pathcheck::tighten(file_name).map_err(|message| ExecutionError::new(3001, message))?;
        let path = format!("{staging}/{file_name}");
        let meta = fs::metadata(&path).map_err(|_| ExecutionError::new(4012, format!("缺少文件 {file_name}")))?;
        if meta.len() != *bytes {
            return Err(ExecutionError::new(4012, format!("{file_name} 的字节数不符")));
        }
        let got = hash_chunks(&path).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        if &got != expect {
            return Err(ExecutionError::new(4012, format!("{file_name} 的哈希不符")));
        }
    }
    let present = walk(staging)?;
    for rel in &present {
        if rel == "manifest.json" || names.contains(rel) {
            continue;
        }
        if (rel == "dake.pub" || rel == "dake.seal") && !names.contains(rel) {
            continue;
        }
        return Err(ExecutionError::new(3001, format!("包内多出文件: {rel}")));
    }
    let dest = format!("{root}/{name}/{version}");
    if Path::new(&dest).exists() {
        return Err(ExecutionError::new(3003, format!("{name} {version} 已存在")));
    }
    let adding = dir_bytes(staging)?;
    let (count, used) = stored(root)?;
    if let Some(max) = max_pkgs {
        if count + 1 > max {
            return Err(ExecutionError::new(3010, format!("包数 {} 超过上限 {max}", count + 1)));
        }
    }
    if let Some(limit) = capacity {
        let cap = limit.saturating_mul(1024 * 1024);
        if used + adding > cap {
            return Err(ExecutionError::new(3010, format!("数据 {} 字节超过容量 {limit} MiB", used + adding)));
        }
    }
    fs::create_dir_all(format!("{root}/{name}")).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    fs::rename(staging, &dest).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    Ok(dest)
}

fn read_identity(dir: &str) -> Result<(String, String), ExecutionError> {
    let (name, version, _) = read_manifest(dir)?;
    check_segment(&name)?;
    check_segment(&version)?;
    Ok((name, version))
}

pub(super) fn read_manifest(dir: &str) -> Result<(String, String, Vec<(String, String, u64)>), ExecutionError> {
    let text = fs::read_to_string(format!("{dir}/manifest.json")).map_err(|err| ExecutionError::new(3002, format!("manifest.json: {err}")))?;
    let value = crate::data::SerializableValue::from_json(&text).map_err(|err| ExecutionError::new(err.code(), err.message().to_string()))?;
    let crate::data::SerializableValue::Object(map) = value else {
        return Err(ExecutionError::new(4012, "清单不是对象".into()));
    };
    let name = match map.get("name") {
        Some(crate::data::SerializableValue::String(text)) => text.clone(),
        _ => return Err(ExecutionError::new(4012, "清单字段需要字符串".into())),
    };
    let version = match map.get("version") {
        Some(crate::data::SerializableValue::String(text)) => text.clone(),
        _ => return Err(ExecutionError::new(4012, "清单字段需要字符串".into())),
    };
    let files = match map.get("files") {
        Some(crate::data::SerializableValue::Array(items)) => items,
        _ => return Err(ExecutionError::new(4012, "清单没有 files".into())),
    };
    let mut listed = Vec::new();
    for item in files {
        let crate::data::SerializableValue::Object(file) = item else {
            return Err(ExecutionError::new(4012, "清单文件项不是对象".into()));
        };
        let file_name = match file.get("name") {
            Some(crate::data::SerializableValue::String(text)) => text.clone(),
            _ => return Err(ExecutionError::new(4012, "清单字段需要字符串".into())),
        };
        let hash = match file.get("blake3") {
            Some(crate::data::SerializableValue::String(text)) => text.clone(),
            _ => return Err(ExecutionError::new(4012, "清单字段需要字符串".into())),
        };
        let bytes = match file.get("bytes") {
            Some(crate::data::SerializableValue::Int(n)) if *n >= 0 => *n as u64,
            _ => return Err(ExecutionError::new(4012, "清单字节数需要整数".into())),
        };
        listed.push((file_name, hash, bytes));
    }
    Ok((name, version, listed))
}

fn copy_tree(source: &str, dest: &str) -> Result<(), ExecutionError> {
    fs::create_dir_all(dest).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    for rel in walk(source)? {
        if rel.rsplit('/').next() == Some("dake.key") {
            continue;
        }
        let from = format!("{source}/{rel}");
        let to = format!("{dest}/{rel}");
        if let Some(parent) = Path::new(&to).parent() {
            fs::create_dir_all(parent).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
        }
        copy_file(&from, &to)?;
    }
    Ok(())
}

fn copy_file(from: &str, to: &str) -> Result<(), ExecutionError> {
    let mut input = fs::File::open(from).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
    let mut output = fs::File::create(to).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = input.read(&mut buf).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n]).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    }
    Ok(())
}

fn walk(dir: &str) -> Result<Vec<String>, ExecutionError> {
    let mut found = Vec::new();
    walk_into(Path::new(dir), "", &mut found)?;
    Ok(found)
}

fn walk_into(dir: &Path, prefix: &str, found: &mut Vec<String>) -> Result<(), ExecutionError> {
    for entry in fs::read_dir(dir).map_err(|err| ExecutionError::new(3002, err.to_string()))? {
        let entry = entry.map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let path = entry.path();
        if path.is_dir() {
            walk_into(&path, &rel, found)?;
        } else {
            found.push(rel);
        }
    }
    Ok(())
}

fn stored(root: &str) -> Result<(u64, u64), ExecutionError> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    let root_path = Path::new(root);
    if !root_path.exists() {
        return Ok((0, 0));
    }
    for name in fs::read_dir(root_path).map_err(|err| ExecutionError::new(3002, err.to_string()))? {
        let name = name.map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        if name.file_name() == ".staging" || !name.path().is_dir() {
            continue;
        }
        for version in fs::read_dir(name.path()).map_err(|err| ExecutionError::new(3002, err.to_string()))? {
            let version = version.map_err(|err| ExecutionError::new(3002, err.to_string()))?;
            if version.path().join("manifest.json").is_file() {
                count += 1;
                bytes += dir_bytes(&version.path().to_string_lossy())?;
            }
        }
    }
    Ok((count, bytes))
}

fn dir_bytes(dir: &str) -> Result<u64, ExecutionError> {
    let mut total = 0u64;
    for rel in walk(dir)? {
        let meta = fs::metadata(format!("{dir}/{rel}")).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        total += meta.len();
    }
    Ok(total)
}

fn check_segment(text: &str) -> Result<(), ExecutionError> {
    if text.is_empty() || text == "." || text == ".." || text.contains('/') || text.contains('\\') || text.contains('\0') {
        return Err(ExecutionError::new(3001, format!("不能作为路径段: {text}")));
    }
    Ok(())
}

pub(super) fn write_stream(path: &str, stream: &mut impl Read, length: u64) -> Result<(), ExecutionError> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    }
    let mut output = fs::File::create(path).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
    let mut left = length;
    let mut buf = vec![0u8; CHUNK];
    while left > 0 {
        let take = std::cmp::min(left as usize, buf.len());
        let n = stream.read(&mut buf[..take]).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
        if n == 0 {
            return Err(ExecutionError::new(4031, "正文短于 Content-Length".into()));
        }
        output.write_all(&buf[..n]).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
        left -= n as u64;
    }
    Ok(())
}
