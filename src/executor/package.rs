/*
 * 把一次执行收集到的 lib、文件和加密设置写成目录里的数据包。
 */

use crate::dsl::ast::{LibDefinition, RepoDefinition};
use crate::executor::crypto::CryptoOperations;
use crate::executor::file_ops::FileOperations;
use crate::dsl::pathcheck;
use std::fs;
use std::path::Path;

/// 写入失败
#[derive(Debug)]
pub struct PackageError {
    pub message: String,
}

#[derive(Clone)]
pub struct PackedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub struct_id: String,
    pub encrypted: bool,
}

pub struct PackageBuild<'a> {
    pub lib: &'a LibDefinition,
    pub repo: Option<&'a RepoDefinition>,
    pub files: &'a [String],
    pub packed: &'a [PackedFile],
    pub crypto: Option<&'a CryptoOperations>,
}

/// 在 lib.out_dir 下写入清单和文件副本。
pub fn materialize(build: PackageBuild<'_>) -> Result<String, PackageError> {
    let out_dir = pathcheck::tighten(&build.lib.out_dir).map_err(|message| PackageError { message })?;

    let mut total_bytes: u64 = 0;
    let mut staged: Vec<Staged> = Vec::new();

    for source in build.files {
        FileOperations::validate_path(source).map_err(|e| PackageError {
            message: e.to_string(),
        })?;
        let len = fs::metadata(source).map_err(|e| PackageError {
            message: format!("读取文件失败 {source}: {e}"),
        })?.len();
        total_bytes = total_bytes.saturating_add(len);
        let name = Path::new(source)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        if staged.iter().any(|item| item.name == name) {
            return Err(PackageError {
                message: format!("数据包内文件名冲突: {}", name),
            });
        }
        staged.push(Staged { name, bytes: Vec::new(), source: Some(source.clone()), struct_id: String::new(), already: false });
    }

    for packed in build.packed {
        if packed.name.contains("..") || packed.name.contains('/') || packed.name.contains('\\') {
            return Err(PackageError {
                message: format!("包内名字不安全: {}", packed.name),
            });
        }
        total_bytes = total_bytes.saturating_add(packed.bytes.len() as u64);
        if staged.iter().any(|item| item.name == packed.name) {
            return Err(PackageError {
                message: format!("数据包内文件名冲突: {}", packed.name),
            });
        }
        staged.push(Staged {
            name: packed.name.clone(),
            bytes: packed.bytes.clone(),
            source: None,
            struct_id: packed.struct_id.clone(),
            already: packed.encrypted,
        });
    }

    let _ = (build.repo, total_bytes);
    fs::create_dir_all(&out_dir).map_err(|e| PackageError {
        message: format!("创建输出目录失败 {}: {}", out_dir, e),
    })?;

    let mut written = Vec::new();
    for item in &staged {
        let dest_name = if item.already {
            item.name.clone()
        } else if build.crypto.is_some() {
            format!("{}.enc", item.name)
        } else {
            item.name.clone()
        };
        let dest = format!("{out_dir}/{dest_name}");
        let (len, hash) = write_staged(build.crypto, item, &dest)?;
        written.push((dest_name, len, hash, item.struct_id.clone(), item.already));
    }
    let mut file_entries = String::new();
    for (index, (dest_name, len, hash, struct_id, already)) in written.iter().enumerate() {
        if index > 0 {
            file_entries.push_str(",\n");
        }
        file_entries.push_str(&format!(
            "    {{\"name\": \"{}\", \"bytes\": {}, \"blake3\": \"{}\", \"struct\": \"{}\", \"encrypted\": {}}}",
            json_escape(dest_name),
            len,
            hash,
            json_escape(struct_id),
            if *already { "true" } else { "false" }
        ));
    }
    let signature = sign_manifest(&build, &written)?;
    let depends = build.lib.depends.iter().map(|item| format!("\"{}\"", json_escape(item))).collect::<Vec<_>>().join(", ");

    if let Some(crypto) = build.crypto {
        let key_path = format!("{out_dir}/dake.key");
        let hex: String = crypto
            .key_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        FileOperations::write_file(&key_path, hex.as_bytes()).map_err(|e| PackageError {
            message: e.to_string(),
        })?;
    }

    let keywords = build
        .lib
        .keywords
        .iter()
        .map(|k| format!("\"{}\"", json_escape(k)))
        .collect::<Vec<_>>()
        .join(", ");

    let mods = build.lib.mods.iter().map(|item| format!("\"{}\"", json_escape(item))).collect::<Vec<_>>().join(", ");
    let manifest = format!(
        "{{\n  \"name\": \"{}\",\n  \"version\": \"{}\",\n  \"desc\": \"{}\",\n  \"repo\": \"{}\",\n  \"keywords\": [{}],\n  \"readme\": \"{}\",\n  \"mods\": [{}],\n  \"depends\": [{}],\n  \"replaces\": \"{}\",\n  \"signature\": \"{}\",\n  \"encrypted\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
        json_escape(&build.lib.name),
        build.lib.version,
        json_escape(&build.lib.desc),
        json_escape(&build.lib.repo),
        keywords,
        json_escape(&build.lib.readme),
        mods,
        depends,
        json_escape(&build.lib.replaces),
        signature,
        build.crypto.is_some(),
        file_entries
    );

    let manifest_path = format!("{out_dir}/manifest.json");
    fs::write(&manifest_path, manifest).map_err(|e| PackageError {
        message: format!("写入清单失败: {e}"),
    })?;

    Ok(out_dir)
}

struct Staged {
    name: String,
    bytes: Vec<u8>,
    source: Option<String>,
    struct_id: String,
    already: bool,
}

fn write_staged(crypto: Option<&CryptoOperations>, item: &Staged, dest: &str) -> Result<(u64, String), PackageError> {
    use std::io::{Read, Write};
    let mut output = fs::File::create(dest).map_err(|err| PackageError { message: format!("写入文件失败 {dest}: {err}") })?;
    let mut hasher = blake3::Hasher::new();
    let mut count = 0u64;
    let mut sink = |chunk: &[u8]| -> Result<(), PackageError> {
        output.write_all(chunk).map_err(|err| PackageError { message: format!("写入文件失败 {dest}: {err}") })?;
        hasher.update(chunk);
        count += chunk.len() as u64;
        Ok(())
    };
    let encrypt = crypto.is_some() && !item.already;
    if let Some(crypto) = crypto.filter(|_| encrypt) {
        if let Some(source) = &item.source {
            let mut input = fs::File::open(source).map_err(|err| PackageError { message: format!("读取文件失败 {source}: {err}") })?;
            crypto.encrypt_to(&mut input, &mut Writer(&mut sink)).map_err(|err| PackageError { message: err.to_string() })?;
        } else {
            crypto.encrypt_to(&mut std::io::Cursor::new(&item.bytes), &mut Writer(&mut sink)).map_err(|err| PackageError { message: err.to_string() })?;
        }
    } else if let Some(source) = &item.source {
        let mut input = fs::File::open(source).map_err(|err| PackageError { message: format!("读取文件失败 {source}: {err}") })?;
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            let n = input.read(&mut buf).map_err(|err| PackageError { message: format!("读取文件失败 {source}: {err}") })?;
            if n == 0 { break; }
            sink(&buf[..n])?;
        }
    } else {
        sink(&item.bytes)?;
    }
    Ok((count, hasher.finalize().to_hex().to_string()))
}

struct Writer<'a>(&'a mut dyn FnMut(&[u8]) -> Result<(), PackageError>);

impl std::io::Write for Writer<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (self.0)(buf).map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.message))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn sign_manifest(build: &PackageBuild<'_>, written: &[(String, u64, String, String, bool)]) -> Result<String, PackageError> {
    if build.lib.sign.is_empty() {
        return Ok(String::new());
    }
    let key = load_sign_key(&build.lib.sign)?;
    let files: Vec<_> = written.iter().map(|(name, _, hash, _, _)| (name.clone(), hash.clone())).collect();
    Ok(mac_manifest(&key, &build.lib.name, &build.lib.version.to_string(), &build.lib.replaces, &build.lib.depends, &files))
}

pub(super) fn load_sign_key(path: &str) -> Result<[u8; 32], PackageError> {
    let bytes = std::fs::read(path).map_err(|err| PackageError { message: format!("读取签名密钥失败 {path}: {err}") })?;
    let text = String::from_utf8_lossy(&bytes);
    let hex = text.trim();
    if hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        let mut key = [0u8; 32];
        for i in 0..32 {
            key[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|err| PackageError { message: err.to_string() })?;
        }
        return Ok(key);
    }
    if bytes.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    Err(PackageError { message: "签名密钥必须是 32 字节或 64 位十六进制".into() })
}

pub(super) fn mac_manifest(key: &[u8; 32], name: &str, version: &str, replaces: &str, depends: &[String], files: &[(String, String)]) -> String {
    let mut hasher = blake3::Hasher::new_keyed(key);
    hasher.update(name.as_bytes());
    hasher.update(b"\n");
    hasher.update(version.as_bytes());
    hasher.update(b"\n");
    hasher.update(replaces.as_bytes());
    hasher.update(b"\n");
    for item in depends {
        hasher.update(item.as_bytes());
        hasher.update(b"\n");
    }
    for (file, hash) in files {
        hasher.update(file.as_bytes());
        hasher.update(b"\n");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
