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
    let mut staged: Vec<(String, Vec<u8>, String, bool)> = Vec::new();

    for source in build.files {
        FileOperations::validate_path(source).map_err(|e| PackageError {
            message: e.to_string(),
        })?;
        let bytes = FileOperations::read_file(source).map_err(|e| PackageError {
            message: e.to_string(),
        })?;
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        let name = Path::new(source)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        if staged.iter().any(|(existing, _, _, _)| existing == &name) {
            return Err(PackageError {
                message: format!("数据包内文件名冲突: {}", name),
            });
        }
        let payload = if let Some(crypto) = build.crypto {
            crypto.encrypt(&bytes).map_err(|e| PackageError {
                message: e.to_string(),
            })?
        } else {
            bytes
        };
        staged.push((name, payload, String::new(), false));
    }

    for packed in build.packed {
        if packed.name.contains("..") || packed.name.contains('/') || packed.name.contains('\\') {
            return Err(PackageError {
                message: format!("包内名字不安全: {}", packed.name),
            });
        }
        total_bytes = total_bytes.saturating_add(packed.bytes.len() as u64);
        if staged.iter().any(|(existing, _, _, _)| existing == &packed.name) {
            return Err(PackageError {
                message: format!("数据包内文件名冲突: {}", packed.name),
            });
        }
        let payload = if packed.encrypted {
            packed.bytes.clone()
        } else if let Some(crypto) = build.crypto {
            crypto.encrypt(&packed.bytes).map_err(|e| PackageError {
                message: e.to_string(),
            })?
        } else {
            packed.bytes.clone()
        };
        staged.push((packed.name.clone(), payload, packed.struct_id.clone(), packed.encrypted));
    }

    if let Some(repo) = build.repo {
        if staged.len() as u64 > repo.max_pkgs {
            return Err(PackageError {
                message: format!(
                    "文件数 {} 超过仓库 {} 的上限 {}",
                    staged.len(),
                    repo.name,
                    repo.max_pkgs
                ),
            });
        }
        let limit = repo.capacity.saturating_mul(1024 * 1024);
        if total_bytes > limit {
            return Err(PackageError {
                message: format!(
                    "数据 {} 字节超过仓库 {} 的容量 {} MiB",
                    total_bytes, repo.name, repo.capacity
                ),
            });
        }
    }

    fs::create_dir_all(&out_dir).map_err(|e| PackageError {
        message: format!("创建输出目录失败 {}: {}", out_dir, e),
    })?;

    let mut file_entries = String::new();
    for (index, (name, bytes, struct_id, already)) in staged.iter().enumerate() {
        let dest_name = if *already {
            name.clone()
        } else if build.crypto.is_some() {
            format!("{name}.enc")
        } else {
            name.clone()
        };
        let dest = format!("{out_dir}/{dest_name}");
        FileOperations::write_file(&dest, bytes).map_err(|e| PackageError {
            message: e.to_string(),
        })?;
        let hash = blake3::hash(bytes);
        if index > 0 {
            file_entries.push_str(",\n");
        }
        file_entries.push_str(&format!(
            "    {{\"name\": \"{}\", \"bytes\": {}, \"blake3\": \"{}\", \"struct\": \"{}\", \"encrypted\": {}}}",
            json_escape(&dest_name),
            bytes.len(),
            hash.to_hex(),
            json_escape(struct_id),
            if *already { "true" } else { "false" }
        ));
    }

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

    let manifest = format!(
        "{{\n  \"name\": \"{}\",\n  \"version\": \"{}\",\n  \"desc\": \"{}\",\n  \"repo\": \"{}\",\n  \"keywords\": [{}],\n  \"readme\": \"{}\",\n  \"encrypted\": {},\n  \"files\": [\n{}\n  ]\n}}\n",
        json_escape(&build.lib.name),
        build.lib.version,
        json_escape(&build.lib.desc),
        json_escape(&build.lib.repo),
        keywords,
        json_escape(&build.lib.readme),
        build.crypto.is_some(),
        file_entries
    );

    let manifest_path = format!("{out_dir}/manifest.json");
    fs::write(&manifest_path, manifest).map_err(|e| PackageError {
        message: format!("写入清单失败: {e}"),
    })?;

    Ok(out_dir)
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
