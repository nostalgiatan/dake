use super::executor::ExecutionError;
use std::path::Path;

use crate::dsl::pathcheck;

pub(super) fn safe_path(path: &str) -> Result<String, ExecutionError> {
    let lexical = pathcheck::tighten(path).map_err(|message| ExecutionError::new(3001, message))?;
    follow_links(&lexical).map_err(|message| ExecutionError::new(3001, message))
}

fn follow_links(start: &str) -> Result<String, String> {
    let mut current = start.to_string();
    for _ in 0..16 {
        let Some(at) = first_symlink(&current) else {
            return Ok(current);
        };
        let target = std::fs::read_link(&at).map_err(|err| format!("路径不安全: {start}: {err}"))?;
        if target.is_absolute() {
            return Err(format!("路径不安全: {start}"));
        }
        let target_text = target.to_string_lossy();
        pathcheck::tighten(&target_text)?;
        let at_text = at.to_string_lossy();
        let rest = current.strip_prefix(at_text.as_ref()).unwrap_or("").trim_start_matches('/');
        let parent = at.parent().unwrap_or(std::path::Path::new(""));
        let joined = if rest.is_empty() { parent.join(target) } else { parent.join(target).join(rest) };
        current = pathcheck::tighten(&joined.to_string_lossy())?;
    }
    Err(format!("路径不安全: {start}"))
}

fn first_symlink(path: &str) -> Option<std::path::PathBuf> {
    let mut acc = std::path::PathBuf::new();
    if std::path::Path::new(path).is_absolute() {
        acc.push("/");
    }
    for part in path.split('/') {
        if part.is_empty() {
            continue;
        }
        acc.push(part);
        let meta = std::fs::symlink_metadata(&acc).ok()?;
        if meta.file_type().is_symlink() {
            return Some(acc);
        }
    }
    None
}

pub(super) fn walk_files(dir: &Path, deep: bool, suffix: Option<&str>, exclude: &[String], found: &mut Vec<String>) -> Result<(), ExecutionError> {
    let entries = std::fs::read_dir(dir).map_err(|err| ExecutionError::new(3002, format!("{}: {err}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();
        if path.is_dir() {
            if exclude.iter().any(|item| item == name.as_ref()) {
                continue;
            }
            if deep {
                walk_files(&path, true, suffix, exclude, found)?;
            }
        } else if suffix.is_none_or(|suffix| name.ends_with(suffix)) {
            found.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

const CHUNK: usize = 1024 * 1024;

pub(super) fn hash_chunks(path: &str) -> std::io::Result<String> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(CHUNK, file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub(super) fn read_chunks(path: &str) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(CHUNK, file);
    let mut out = Vec::new();
    if let Ok(len) = reader.get_ref().metadata().map(|meta| meta.len()) {
        out.try_reserve(len as usize).ok();
    }
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_chunks_matches_a_file_larger_than_one_block() {
        let dir = std::env::temp_dir().join(format!("dake_hash_chunks_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.bin");
        let mut bytes = vec![7u8; 1024 * 1024 + 3];
        bytes[1024 * 1024 + 2] = 9;
        std::fs::write(&path, &bytes).unwrap();
        let got = super::hash_chunks(&path.to_string_lossy()).unwrap();
        assert_eq!(got, blake3::hash(&bytes).to_hex().to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub(super) fn write_chunks(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::with_capacity(CHUNK, file);
    for piece in bytes.chunks(CHUNK) {
        writer.write_all(piece)?;
    }
    writer.flush()
}
