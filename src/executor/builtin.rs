use std::sync::Arc;
use super::executor::{ExecutionError, Executor};
use super::ops;
use super::paths::{safe_path, walk_files};
use crate::data::{CompressionLevel, SerializableValue};
use crate::dsl::ast::{Carrier, Endian, FieldType, LayoutKind, Value};
use crate::dsl::ir::CallArg;
use crate::executor::crypto::CryptoOperations;
use crate::executor::file_ops::FileOperations;

fn need(args: &[CallArg], n: usize) -> Result<(), ExecutionError> {
    if args.len() < n {
        Err(ExecutionError::new(4011, format!("需要 {n} 个参数，得到 {}", args.len())))
    } else {
        Ok(())
    }
}

pub(super) fn dispatch(exec: &mut Executor, name: &str, args: &[CallArg]) -> Result<Value, ExecutionError> {
    match name {
        "env.set" => {
            need(args, 2)?;
            let key = exec.arg_string(args, 0)?;
            let value = exec.get(&args[1].temp)?;
            exec.context.set_env(key, value.clone());
            Ok(value)
        }
        "object" => make_object(exec, args),
        "list.set" => list_set(exec, args),
        "list.remove" => list_remove(exec, args),
        "list.of" => {
            let mut items = Vec::new();
            for arg in args {
                items.push(exec.get(&arg.temp)?);
            }
            Ok(Value::List(Arc::new(items)))
        }
        "files" => {
            for arg in args {
                let path = exec.arg_value_string(&arg.temp)?;
                super::access::probe_read(&path)?;
                exec.keep_file(&path)?;
            }
            exec.output_buffer.push(format!("处理 {} 个文件", args.len()));
            Ok(Value::Number(args.len() as i64))
        }
        "files.all" => {
            let exclude = exec.named_string_list(args, "exclude")?;
            let root = std::env::current_dir().map_err(|e| ExecutionError::new(3005, e.to_string()))?;
            super::access::probe_dir(&root.to_string_lossy()).map_err(|err| ExecutionError::new(4037, err))?;
            let found = FileOperations::list_files_recursive(&root.to_string_lossy(), &exclude)
                .map_err(|e| ExecutionError::new(3005, e.to_string()))?;
            for path in &found {
                exec.package_files.push(path.to_string_lossy().to_string());
            }
            exec.output_buffer.push(format!("递归处理文件，排除: {exclude:?}, 共 {} 个", found.len()));
            Ok(Value::Number(found.len() as i64))
        }
        "files.read" => read_record(exec, args),
        "files.rows" => read_rows(exec, args),
        "files.each" => files_each(exec, args),
        "files.field" => files_field(exec, args),
        "files.verify" => files_verify(exec, args),
        "files.seal" => files_seal(exec, args),
        "files.unseal" => files_unseal(exec, args),
        "files.list" => files_list(exec, args),
        "files.write" => write_record(exec, args),
        "files.write.rows" => write_rows(exec, args),
        "files.write.row" => write_row(exec, args),
        "pack" => pack_rows(exec, args),
        "unpack" => unpack_rows(exec, args),
        "net.url" => super::net::set_url(exec, args),
        "net.get" => super::net::get(exec, args),
        "net.post" => super::net::post(exec, args),
        "net.accept" => super::net::accept(exec, args),
        "update" => update_record(exec, args),
        "shared.add" => shared_add(exec, args),
        "shared.set" => shared_set(exec, args),
        "text.lines" => text_lines(exec, args),
        "text.join" => text_join(exec, args),
        "text.decode" => text_decode(exec, args),
        "text.encode" => text_encode(exec, args),
        "list.add" => list_add(exec, args),
        "list.map" => list_map(exec, args),
        "list.keep" => list_keep(exec, args),
        "list.update" => list_update(exec, args),
        "list.join" => list_join(exec, args),
        "list.where" => list_where(exec, args),
        "list.pick" => list_pick(exec, args),
        "list.sort" => list_sort(exec, args),
        "list.group" => list_group(exec, args),
        "files.name" => files_name(exec, args),
        "files.dir" => files_dir(exec, args),
        "list.len" => list_len(exec, args),
        "record" => make_record(exec, args),
        "files.read.bytes" => read_bytes(exec, args),
        "files.read.str" => read_str(exec, args),
        "files.write.bytes" => write_bytes(exec, args),
        "files.write.str" => write_str(exec, args),
        "base64.encode" => base64_encode(exec, args),
        "base64.decode" => base64_decode(exec, args),
        "files.encry" => {
            let (crypto, key) = CryptoOperations::new(true);
            exec.crypto = Some(crypto);
            exec.output_buffer.push(format!("启用文件加密，密钥长度: {}", key.len()));
            Ok(Value::Bool(true))
        }
        "files.decry" => files_decry(exec, args),
        "log.init" => {
            let dir = exec.arg_string(args, 0)?;
            let print = args.get(1).map(|arg| ops::truth(&exec.get(&arg.temp).unwrap_or(Value::Bool(false)))).unwrap_or(false);
            std::fs::create_dir_all(&dir).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
            exec.log_dir = Some(dir);
            exec.log_print = print;
            Ok(Value::Bool(true))
        }
        "log.info" => {
            exec.write_log(&format!("[INFO] {}", exec.arg_string(args, 0)?))?;
            Ok(Value::Bool(true))
        }
        "log.error" => {
            exec.write_log(&format!("[ERROR] {}", exec.arg_string(args, 0)?))?;
            Ok(Value::Bool(true))
        }
        "sys.host" | "sys.cpu" | "sys.mem" | "sys.disk" | "sys.disks" => super::host::call(exec, name, args),
        "crypto.pair" => crypto_pair(exec, args),
        "repo.put" => repo_put(exec, args),
        "repo.get" => repo_get(exec, args),
        "repo.fetch" => repo_fetch(exec, args),
        "repo.push" => repo_push(exec, args),
        "cmd" => {
            let cmd = exec.arg_string(args, 0)?;
            let mut cmd_args = Vec::new();
            for arg in args.iter().skip(1) {
                cmd_args.push(exec.arg_value_string(&arg.temp)?);
            }
            let output = std::process::Command::new(&cmd).args(&cmd_args).output().map_err(|e| {
                ExecutionError::new(3002, format!("无法执行命令 {cmd}: {e}"))
            })?;
            if !output.status.success() {
                return Err(ExecutionError::new(3001, format!("命令执行失败: {cmd}")));
            }
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            exec.output_buffer.push(format!("命令执行成功: {cmd} {cmd_args:?}"));
            Ok(Value::String(text))
        }
        "data.re" => {
            let pattern = exec.arg_string(args, 0)?;
            exec.regex_cache.get_or_compile(&pattern).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            exec.output_buffer.push(format!("编译正则表达式: {pattern}"));
            Ok(Value::String(pattern))
        }
        "data.re.group" => {
            need(args, 3)?;
            let pattern = exec.arg_string(args, 0)?;
            let text = exec.arg_string(args, 1)?;
            let index = match exec.get(&args[2].temp)? {
                Value::Number(n) if n >= 0 => n as usize,
                _ => return Err(ExecutionError::new(4012, "分组编号必须是非负整数".into())),
            };
            let re = exec.regex_cache.get_or_compile(&pattern).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            let found = re.captures(&text).ok_or_else(|| ExecutionError::new(4022, "没有匹配".into()))?;
            let group = found.get(index).ok_or_else(|| ExecutionError::new(4022, format!("没有分组 {index}")))?;
            Ok(Value::String(group.as_str().to_string()))
        }
        "data.re.replace" => {
            let pattern = exec.arg_string(args, 0)?;
            let text = exec.arg_string(args, 1)?;
            let replacement = exec.arg_string(args, 2)?;
            let re = exec.regex_cache.get_or_compile(&pattern).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            Ok(Value::String(re.replace_all(&text, replacement.as_str()).into_owned()))
        }
        "utf8.encode" => {
            need(args, 1)?;
            let text = match exec.get(&args[0].temp)? {
                Value::String(text) => text,
                _ => return Err(ExecutionError::new(4012, "utf8.encode 需要字符串".into())),
            };
            Ok(Value::Bytes(Arc::new(text.into_bytes())))
        }
        "utf8.decode" => {
            need(args, 1)?;
            let bytes = match exec.get(&args[0].temp)? {
                Value::Bytes(bytes) => bytes,
                _ => return Err(ExecutionError::new(4012, "utf8.decode 需要字节".into())),
            };
            let text = String::from_utf8(Arc::unwrap_or_clone(bytes)).map_err(|_| ExecutionError::new(4015, "字节不是合法的 UTF-8".into()))?;
            Ok(Value::String(text))
        }
        "data.re.find" => {
            let pattern = exec.arg_string(args, 0)?;
            let text = exec.arg_string(args, 1)?;
            let re = exec.regex_cache.get_or_compile(&pattern).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            let found = re.find(&text).ok_or_else(|| ExecutionError::new(4022, "没有匹配".into()))?;
            Ok(Value::String(found.as_str().to_string()))
        }
        "data.re.all" => {
            let pattern = exec.arg_string(args, 0)?;
            let text = exec.arg_string(args, 1)?;
            let re = exec.regex_cache.get_or_compile(&pattern).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            let items = re.find_iter(&text).map(|m| Value::String(m.as_str().to_string())).collect();
            Ok(Value::List(Arc::new(items)))
        }
        "data.vali" => {
            let kind = exec.arg_string(args, 0)?.to_uppercase();
            let (key, value) = if args.len() == 2 {
                let value = exec.arg_string(args, 1)?;
                (value.clone(), value)
            } else {
                (exec.arg_string(args, 1)?, exec.arg_string(args, 2)?)
            };
            let result = match kind.as_str() {
                "NOT_EMPTY" => exec.validator.validate_not_empty(&key, &value),
                "EMAIL" => exec.validator.validate_email(&key, &value),
                "URL" => exec.validator.validate_url(&key, &value),
                "NUMERIC" => exec.validator.validate_numeric(&key, &value),
                "ALPHA" => exec.validator.validate_alpha(&key, &value),
                "ALPHANUMERIC" => exec.validator.validate_alphanumeric(&key, &value),
                other => return Err(ExecutionError::new(4020, format!("未知的验证器: {other}"))),
            };
            result.map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?;
            exec.output_buffer.push(format!("验证成功: {key} 使用 {kind}"));
            Ok(Value::Bool(true))
        }
        "data.seria" => {
            need(args, 2)?;
            let format = exec.arg_string(args, 0)?;
            let value = exec.get(&args[1].temp)?;
            let serial = ops::to_serial(&value)?;
            let text = match format.as_str() {
                "json" => serial.to_json().map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?,
                "bin" => {
                    let bytes = serial.to_binary().map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?;
                    return Ok(Value::Bytes(Arc::new(bytes)));
                }
                other => return Err(ExecutionError::new(4021, format!("未知的序列化格式: {other}"))),
            };
            Ok(Value::String(text))
        }
        "data.deseria" => {
            need(args, 2)?;
            let format = exec.arg_string(args, 0)?;
            let value = match format.as_str() {
                "json" => {
                    let data = exec.arg_string(args, 1)?;
                    SerializableValue::from_json(&data).map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?
                }
                "bin" => {
                    let bytes = match exec.get(&args[1].temp)? {
                        Value::Bytes(bytes) => bytes,
                        _ => return Err(ExecutionError::new(4012, "bin 反序列化需要字节".into())),
                    };
                    SerializableValue::from_binary(&bytes).map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?
                }
                other => return Err(ExecutionError::new(4021, format!("未知的反序列化格式: {other}"))),
            };
            ops::from_serial(&value)
        }
        "data.comp" => {
            need(args, 1)?;
            let (level, data) = if args.len() == 1 {
                (CompressionLevel::Default, payload_bytes(exec, &args[0].temp)?)
            } else {
                let level = match exec.get(&args[0].temp)? {
                    Value::Number(1) => CompressionLevel::Fast,
                    Value::Number(19) => CompressionLevel::Best,
                    Value::Number(n) => CompressionLevel::Custom(n as i32),
                    _ => CompressionLevel::Default,
                };
                (level, payload_bytes(exec, &args[1].temp)?)
            };
            let compressed = exec.compressor.compress(&data, level).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            Ok(Value::Bytes(Arc::new(compressed)))
        }
        "data.decomp" => {
            need(args, 1)?;
            let data = match exec.get(&args[0].temp)? {
                Value::Bytes(bytes) => bytes,
                _ => return Err(ExecutionError::new(4012, "解压需要字节".into())),
            };
            let plain = exec.compressor.decompress(&data).map_err(|e| {
                ExecutionError::new(e.code(), e.message().to_string())
            })?;
            Ok(Value::Bytes(Arc::new(plain)))
        }
        other => Err(ExecutionError::new(4005, format!("未知的内置行为: {other}"))),
    }
}

fn payload_bytes(exec: &Executor, temp: &str) -> Result<Vec<u8>, ExecutionError> {
    match exec.get(temp)? {
        Value::Bytes(bytes) => Ok(Arc::unwrap_or_clone(bytes)),
        Value::String(text) => Ok(text.into_bytes()),
        _ => Err(ExecutionError::new(4012, "压缩需要字节或字符串".into())),
    }
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.split('\n').map(|line| line.trim_end_matches('\r').to_string()).collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}

fn read_record(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_read(&path)?;
    let id = exec.arg_string(args, 1)?;
    decode_block_file(exec, &id, &path)
}

pub(super) fn decode_body(exec: &Executor, id: &str, bytes: Vec<u8>) -> Result<Value, ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let fields = match (&def.from, &def.layout) {
        (Carrier::Str, LayoutKind::Lines) => {
            let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
            let lines = split_lines(&text);
            if lines.is_empty() {
                return Err(ExecutionError::new(4018, "lines 布局至少需要一行".into()));
            }
            let title = lines[0].clone();
            let body = lines[1..].join("\n");
            vec![(def.fields[0].name.clone(), Value::String(title)), (def.fields[1].name.clone(), Value::String(body))]
        }
        (Carrier::Str, LayoutKind::Whole) => {
            let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
            vec![(def.fields[0].name.clone(), Value::String(text))]
        }
        (Carrier::Bytes, LayoutKind::Whole) => {
            vec![(def.fields[0].name.clone(), Value::Bytes(Arc::new(bytes)))]
        }
        (Carrier::Bytes, LayoutKind::Lines) => {
            return Err(ExecutionError::new(4018, "lines 只适用于文本".into()));
        }
        (Carrier::Str, LayoutKind::Split) => {
            let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
            decode_fields(def, &text)?
        }
        (Carrier::Str, LayoutKind::Json) => {
            let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
            decode_json(exec, def, &text)?
        }
        (Carrier::Bytes, LayoutKind::Json) => {
            return Err(ExecutionError::new(4018, "json 只适用于文本".into()));
        }
        (Carrier::Bytes, LayoutKind::Split) => {
            return Err(ExecutionError::new(4018, "split 只适用于文本".into()));
        }
        (Carrier::Bytes, LayoutKind::Width) => decode_width(def, &bytes)?,
        (Carrier::Str, LayoutKind::Width) => {
            return Err(ExecutionError::new(4018, "width 只适用于字节".into()));
        }
        (_, LayoutKind::Block) => decode_block(def, &bytes)?,
    };
    finish_record(exec, id, fields)
}

pub(super) fn encode_body(exec: &Executor, id: &str, fields: &[(String, Value)]) -> Result<Vec<u8>, ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    if fields.len() != def.fields.len() || fields.iter().zip(&def.fields).any(|(got, expect)| got.0 != expect.name || !value_matches(&expect.ty, &got.1, id)) {
        return Err(ExecutionError::new(4012, "记录与结构不一致".into()));
    }
    enforce_record(exec, id, fields)?;
    match (&def.from, &def.layout) {
        (Carrier::Str, LayoutKind::Lines) => {
            let Value::String(title) = &fields[0].1 else { unreachable!() };
            let Value::String(body) = &fields[1].1 else { unreachable!() };
            Ok(format!("{title}\n{body}").into_bytes())
        }
        (Carrier::Str, LayoutKind::Whole) => {
            let Value::String(text) = &fields[0].1 else { unreachable!() };
            Ok(text.as_bytes().to_vec())
        }
        (Carrier::Bytes, LayoutKind::Whole) => {
            let Value::Bytes(bytes) = &fields[0].1 else { unreachable!() };
            Ok((**bytes).clone())
        }
        (Carrier::Bytes, LayoutKind::Lines) => Err(ExecutionError::new(4018, "lines 只适用于文本".into())),
        (_, LayoutKind::Split) => Ok(encode_fields(def, fields)?.into_bytes()),
        (_, LayoutKind::Json) => Ok(encode_json(exec, def, fields)?.into_bytes()),
        (_, LayoutKind::Width) => encode_width(def, fields),
        (_, LayoutKind::Block) => encode_block(def, fields),
    }
}

pub(super) fn content_type_for(exec: &Executor, id: &str) -> Result<&'static str, ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    Ok(match (&def.from, &def.layout) {
        (_, LayoutKind::Json) => "application/json",
        (Carrier::Bytes, _) | (_, LayoutKind::Width) | (_, LayoutKind::Block) => "application/octet-stream",
        _ => "text/plain; charset=utf-8",
    })
}

fn write_record(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_write(&path)?;
    let Value::Record { path: id, fields } = exec.get(&args[1].temp)? else {
        return Err(ExecutionError::new(4012, "files.write 需要记录".into()));
    };
    let bytes = encode_body(exec, &id, &fields)?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    std::fs::write(&path, bytes).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(Value::Record { path: id, fields })
}

fn pack_rows(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    if !exec.has_lib() {
        return Err(ExecutionError::new(4012, "pack 需要 lib".into()));
    }
    let name = exec.arg_string(args, 0)?;
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(ExecutionError::new(3001, format!("包内名字不安全: {name}")));
    }
    let items = match exec.get(&args[1].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "pack 需要记录列表".into())),
    };
    if items.is_empty() {
        return Err(ExecutionError::new(4012, "空列表没有结构".into()));
    }
    let mut lines = Vec::new();
    let mut struct_id = None;
    for item in items.iter() {
        let Value::Record { path: id, fields } = item else {
            return Err(ExecutionError::new(4012, "pack 的元素需要记录".into()));
        };
        enforce_record(exec, id, fields.as_slice())?;
        if let Some(existing) = &struct_id {
            if existing != id {
                return Err(ExecutionError::new(4012, "pack 的记录必须是同一种结构".into()));
            }
        } else {
            struct_id = Some(id.clone());
        }
        let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
        let text = match def.layout {
            LayoutKind::Split => encode_fields(def, fields.as_slice())?,
            LayoutKind::Json => encode_json(exec, def, fields.as_slice())?,
            _ => return Err(ExecutionError::new(4018, "pack 需要 split 或 json 结构".into())),
        };
        lines.push(text);
    }
    let mut bytes = lines.join("\n").into_bytes();
    let mut encrypted = false;
    if let Some(arg) = named_arg(args, "key") {
        let key_path = safe_path(&exec.arg_value_string(&arg.temp)?)?;
        let ops = load_key(&key_path, true)?;
        bytes = ops.encrypt(&bytes).map_err(|e| ExecutionError::new(2004, e.to_string()))?;
        encrypted = true;
    }
    exec.packed.push(crate::executor::package::PackedFile {
        name,
        bytes,
        struct_id: struct_id.unwrap_or_default(),
        encrypted,
    });
    Ok(Value::List(Arc::new(items)))
}

fn unpack_rows(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    let file_name = std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ExecutionError::new(3001, format!("包内名字不安全: {path}")))?
        .to_string();
    if let Some(packed) = exec.packed.iter().find(|item| item.name == file_name) {
        let bytes = open_packed(exec, args, &packed.bytes, packed.encrypted)?;
        return decode_packed_text(exec, &packed.struct_id, bytes);
    }
    let parent = std::path::Path::new(&path).parent().map(|dir| dir.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from("."));
    let manifest_path = parent.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).map_err(|e| {
        ExecutionError::new(3002, format!("{}: {e}", manifest_path.display()))
    })?;
    let value = crate::data::SerializableValue::from_json(&manifest).map_err(|e| {
        ExecutionError::new(e.code(), e.message().to_string())
    })?;
    let crate::data::SerializableValue::Object(root) = value else {
        return Err(ExecutionError::new(4012, "清单不是对象".into()));
    };
    let files = root.get("files").ok_or_else(|| ExecutionError::new(4012, "清单没有 files".into()))?;
    let crate::data::SerializableValue::Array(entries) = files else {
        return Err(ExecutionError::new(4012, "清单的 files 不是列表".into()));
    };
    let mut struct_id = None;
    let mut encrypted = false;
    for entry in entries {
        let crate::data::SerializableValue::Object(item) = entry else {
            return Err(ExecutionError::new(4012, "清单项不是对象".into()));
        };
        let name = match item.get("name") {
            Some(crate::data::SerializableValue::String(name)) => name,
            _ => return Err(ExecutionError::new(4012, "清单项没有名字".into())),
        };
        if name == &file_name {
            struct_id = Some(match item.get("struct") {
                Some(crate::data::SerializableValue::String(id)) => id.clone(),
                _ => String::new(),
            });
            encrypted = matches!(item.get("encrypted"), Some(crate::data::SerializableValue::Bool(true)));
            break;
        }
    }
    let Some(struct_id) = struct_id else {
        return Err(ExecutionError::new(4018, format!("清单里没有 {file_name}")));
    };
    if struct_id.is_empty() {
        return Err(ExecutionError::new(4018, format!("{file_name} 没有结构")));
    }
    let id = resolve_struct_id(exec, &struct_id)?;
    super::access::probe_read(&path)?;
    let bytes = std::fs::read(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    let bytes = open_packed(exec, args, &bytes, encrypted)?;
    decode_packed_text(exec, &id, bytes)
}

fn decode_packed_text(exec: &Executor, id: &str, bytes: Vec<u8>) -> Result<Value, ExecutionError> {
    let id = resolve_struct_id(exec, id)?;
    let target = successor(exec, &id)?;
    let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
    let mut rows = Vec::new();
    for line in split_lines(&text) {
        let row = decode_line(exec, &id, &line)?;
        rows.push(if target == id { row } else { evolve_record(exec, row, &target)? });
    }
    Ok(Value::List(Arc::new(rows)))
}

fn resolve_struct_id(exec: &Executor, stored: &str) -> Result<String, ExecutionError> {
    if exec.structs.contains_key(stored) {
        return Ok(stored.to_string());
    }
    let short = stored.rsplit("::").next().unwrap_or(stored);
    let mut found = None;
    for id in exec.structs.keys() {
        if id.rsplit("::").next() == Some(short) {
            if found.is_some() {
                return Err(ExecutionError::new(4018, format!("结构 {short} 不唯一")));
            }
            found = Some(id.clone());
        }
    }
    found.ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {stored}")))
}

fn files_decry(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let key_path = safe_path(&exec.arg_string(args, 0)?)?;
    let hex = std::fs::read_to_string(&key_path).map_err(|e| ExecutionError::new(3002, format!("{key_path}: {e}")))?;
    let key = decode_hex(hex.trim()).map_err(|message| ExecutionError::new(2001, message))?;
    let bytes = match exec.get(&args[1].temp)? {
        Value::Bytes(bytes) => bytes,
        _ => return Err(ExecutionError::new(4012, "files.decry 的密文需要字节".into())),
    };
    let ops = CryptoOperations::with_key(&key, true).map_err(|e| ExecutionError::new(2001, e.to_string()))?;
    let plain = ops.decrypt(&bytes).map_err(|e| ExecutionError::new(2004, e.to_string()))?;
    Ok(Value::Bytes(Arc::new(plain)))
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("密钥不是成对的十六进制".into());
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_val(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("密钥含有非十六进制字符".into()),
    }
}

fn open_packed(exec: &Executor, args: &[CallArg], bytes: &[u8], encrypted: bool) -> Result<Vec<u8>, ExecutionError> {
    if !encrypted {
        return Ok(bytes.to_vec());
    }
    let Some(arg) = named_arg(args, "key") else {
        return Err(ExecutionError::new(2004, "这一项已加密，请补上 key".into()));
    };
    let key_path = safe_path(&exec.arg_value_string(&arg.temp)?)?;
    let ops = load_key(&key_path, false)?;
    ops.decrypt(bytes).map_err(|e| ExecutionError::new(2004, e.to_string()))
}

fn load_key(path: &str, create: bool) -> Result<CryptoOperations, ExecutionError> {
    if std::path::Path::new(path).exists() {
        let hex = std::fs::read_to_string(path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
        let key = decode_hex(hex.trim()).map_err(|message| ExecutionError::new(2001, message))?;
        return CryptoOperations::with_key(&key, false).map_err(|e| ExecutionError::new(2001, e.to_string()));
    }
    if !create {
        return Err(ExecutionError::new(3002, format!("{path}: 找不到密钥")));
    }
    let (ops, key) = CryptoOperations::new(false);
    let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    std::fs::write(path, hex).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(ops)
}

fn files_list(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let dir = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_dir(&dir).map_err(|err| ExecutionError::new(4037, err))?;
    let suffix = match named_arg(args, "suffix") {
        Some(arg) => match exec.get(&arg.temp)? {
            Value::String(text) => Some(text),
            _ => return Err(ExecutionError::new(4012, "suffix 需要字符串".into())),
        },
        None => None,
    };
    let deep = named_flag(exec, args, "deep")?;
    let exclude = match named_arg(args, "exclude") {
        Some(arg) => match exec.get(&arg.temp)? {
            Value::List(items) => Arc::unwrap_or_clone(items).into_iter().map(|item| match item {
                Value::String(text) => Ok(text),
                other => Err(ExecutionError::new(4012, format!("exclude 的元素需要字符串，得到 {other}"))),
            }).collect::<Result<Vec<_>, _>>()?,
            _ => return Err(ExecutionError::new(4012, "exclude 需要列表".into())),
        },
        None => Vec::new(),
    };
    let mut found = Vec::new();
    walk_files(std::path::Path::new(&dir), deep, suffix.as_deref(), &exclude, &mut found)?;
    found.sort();
    Ok(Value::List(Arc::new(found.into_iter().map(Value::String).collect())))
}

fn shared_add(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let name = exec.arg_string(args, 0)?;
    let delta = match exec.get(&args[1].temp)? {
        Value::Number(n) => n,
        _ => return Err(ExecutionError::new(4012, "shared.add 需要整数".into())),
    };
    let mut map = exec.shared.lock().expect("共享锁");
    let slot = map.get_mut(&name).ok_or_else(|| ExecutionError::new(4006, format!("变量未共享: {name}")))?;
    let Value::Number(current) = slot else {
        return Err(ExecutionError::new(4012, "shared.add 需要整数".into()));
    };
    let next = current.checked_add(delta).ok_or_else(|| ExecutionError::new(4012, "整数溢出".into()))?;
    *slot = Value::Number(next);
    Ok(Value::Number(next))
}

fn shared_set(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let name = exec.arg_string(args, 0)?;
    let field = exec.arg_string(args, 1)?;
    let value = exec.get(&args[2].temp)?;
    let mut map = exec.shared.lock().expect("共享锁");
    let slot = map.get_mut(&name).ok_or_else(|| ExecutionError::new(4006, format!("变量未共享: {name}")))?;
    match slot {
        Value::Record { fields, .. } => {
            let fields = Arc::make_mut(fields);
            let item = fields.iter_mut().find(|(key, _)| key == &field).ok_or_else(|| {
                ExecutionError::new(4017, format!("记录没有字段 {field}"))
            })?;
            item.1 = value;
        }
        Value::Object(fields) => {
            let fields = Arc::make_mut(fields);
            if let Some(item) = fields.iter_mut().find(|(key, _)| key == &field) {
                item.1 = value;
            } else {
                fields.push((field, value));
            }
        }
        _ => return Err(ExecutionError::new(4012, "shared.set 需要记录或对象".into())),
    }
    Ok(slot.clone())
}

fn named_arg<'a>(args: &'a [CallArg], name: &str) -> Option<&'a CallArg> {
    args.iter().find(|arg| arg.name.as_deref() == Some(name))
}

fn named_flag(exec: &Executor, args: &[CallArg], name: &str) -> Result<bool, ExecutionError> {
    let Some(arg) = named_arg(args, name) else {
        return Ok(false);
    };
    match exec.get(&arg.temp)? {
        Value::Bool(value) => Ok(value),
        _ => Err(ExecutionError::new(4012, format!("{name} 需要布尔值"))),
    }
}

fn read_rows(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_read(&path)?;
    let id = exec.arg_string(args, 1)?;
    let header = named_flag(exec, args, "header")?;
    let bytes = std::fs::read(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|_| ExecutionError::new(4015, "文件不是合法的 UTF-8".into()))?;
    let mut lines = split_lines(&text);
    if header {
        let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
        if !matches!(def.layout, LayoutKind::Split) {
            return Err(ExecutionError::new(4018, "header 只适用于 split".into()));
        }
        if lines.is_empty() {
            return Err(ExecutionError::new(4018, "没有表头可核对".into()));
        }
        let sep = def.sep.as_deref().unwrap_or(",");
        let parts = split_quoted(&lines.remove(0), sep)?;
        let names: Vec<_> = def.fields.iter().map(|field| field.name.as_str()).collect();
        if parts.iter().map(String::as_str).collect::<Vec<_>>() != names {
            return Err(ExecutionError::new(4018, format!("表头应为 {}", names.join(sep))));
        }
    }
    let mut rows = Vec::new();
    for line in lines {
        rows.push(decode_line(exec, &id, &line)?);
    }
    Ok(Value::List(Arc::new(rows)))
}

fn files_each(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    use std::io::BufRead;
    need(args, 4)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_read(&path)?;
    let id = exec.arg_string(args, 1)?;
    let file = exec.arg_string(args, 2)?;
    let action = exec.arg_string(args, 3)?;
    let header = named_flag(exec, args, "header")?;
    if header {
        let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
        if !matches!(def.layout, LayoutKind::Split) {
            return Err(ExecutionError::new(4018, "header 只适用于 split".into()));
        }
    }
    let input = std::fs::File::open(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    let mut reader = std::io::BufReader::new(input);
    let mut line = String::new();
    let mut count = 0i64;
    let mut saw_line = false;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
        if n == 0 {
            break;
        }
        if line.ends_with('\n') {
            line.pop();
        }
        if line.ends_with('\r') {
            line.pop();
        }
        if !saw_line {
            saw_line = true;
            if header {
                let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
                let sep = def.sep.as_deref().unwrap_or(",");
                let parts = split_quoted(&line, sep)?;
                let names: Vec<_> = def.fields.iter().map(|field| field.name.as_str()).collect();
                if parts.iter().map(String::as_str).collect::<Vec<_>>() != names {
                    return Err(ExecutionError::new(4018, format!("表头应为 {}", names.join(sep))));
                }
                continue;
            }
        }
        let row = decode_line(exec, &id, &line)?;
        exec.call_named_drop(&file, &action, row)?;
        count += 1;
    }
    if header && !saw_line {
        return Err(ExecutionError::new(4018, "没有表头可核对".into()));
    }
    Ok(Value::Number(count))
}

fn files_field(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    use std::io::Read;
    need(args, 4)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_read(&path)?;
    let id = exec.arg_string(args, 1)?;
    let file = exec.arg_string(args, 2)?;
    let action = exec.arg_string(args, 3)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?.clone();
    if !matches!(def.layout, LayoutKind::Block) {
        return Err(ExecutionError::new(4018, "files.field 只适用于 block".into()));
    }
    let mut input = std::fs::File::open(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    let mut count = 0i64;
    for field in &def.fields {
        let mut len_buf = [0u8; 4];
        input.read_exact(&mut len_buf).map_err(|_| ExecutionError::new(4018, format!("字段 {} 的块不完整", field.name)))?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        input.read_exact(&mut payload).map_err(|_| ExecutionError::new(4018, format!("字段 {} 的块不完整", field.name)))?;
        let value = match &field.ty {
            FieldType::Bytes => Value::Bytes(Arc::new(payload)),
            FieldType::Str => Value::String(String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?),
            FieldType::Int => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                Value::Number(text.parse().map_err(|_| ExecutionError::new(4012, format!("字段 {} 不是整数", field.name)))?)
            }
            FieldType::Float => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                Value::Float(text.parse().map_err(|_| ExecutionError::new(4012, format!("字段 {} 不是小数", field.name)))?)
            }
            FieldType::Bool => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                match text.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return Err(ExecutionError::new(4012, format!("字段 {} 不是布尔值", field.name))),
                }
            }
            _ => return Err(ExecutionError::new(4018, "block 不能嵌套结构或列表".into())),
        };
        exec.call_named_drop_many(&file, &action, vec![Value::String(field.name.clone()), value])?;
        count += 1;
    }
    Ok(Value::Number(count))
}

fn repo_put(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let repo = exec.repo.clone().ok_or_else(|| ExecutionError::new(4011, "没有 repo".into()))?;
    let source = safe_path(&exec.arg_string(args, 0)?)?;
    let dest = super::store::admit(&exec.repo_lock, &repo.dir, &source, repo.capacity, repo.max_pkgs)?;
    Ok(Value::String(dest))
}

fn repo_get(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let repo = exec.repo.clone().ok_or_else(|| ExecutionError::new(4011, "没有 repo".into()))?;
    let dest = super::store::get(&repo.dir, &exec.arg_string(args, 0)?, &exec.arg_string(args, 1)?)?;
    Ok(Value::String(dest))
}

fn repo_fetch(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 4)?;
    let dest = super::depot::fetch(exec, &exec.arg_string(args, 0)?, &exec.arg_string(args, 1)?, &exec.arg_string(args, 2)?, &safe_path(&exec.arg_string(args, 3)?)?)?;
    Ok(Value::String(dest))
}

fn repo_push(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    super::depot::push(exec, &exec.arg_string(args, 0)?, &safe_path(&exec.arg_string(args, 1)?)?)?;
    Ok(Value::Bool(true))
}

fn crypto_pair(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let private_path = safe_path(&exec.arg_string(args, 0)?)?;
    let public_path = safe_path(&exec.arg_string(args, 1)?)?;
    if private_path == public_path {
        return Err(ExecutionError::new(3003, "私钥和公钥不能是同一个文件".into()));
    }
    if std::fs::metadata(&private_path).is_ok() || std::fs::metadata(&public_path).is_ok() {
        return Err(ExecutionError::new(3003, "密钥文件已存在".into()));
    }
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
        .map_err(|_| ExecutionError::new(2001, "生成密钥失败".into()))?;
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| ExecutionError::new(2001, "生成密钥失败".into()))?;
    std::fs::write(&private_path, pkcs8.as_ref()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    if let Err(err) = std::fs::write(&public_path, ring::signature::KeyPair::public_key(&pair).as_ref()) {
        let _ = std::fs::remove_file(&private_path);
        return Err(ExecutionError::new(3003, err.to_string()));
    }
    Ok(Value::Bool(true))
}

fn files_seal(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let dir = safe_path(&exec.arg_string(args, 0)?)?;
    let key_path = safe_path(&exec.arg_string(args, 1)?)?;
    let manifest_path = format!("{dir}/manifest.json");
    let manifest = std::fs::read(&manifest_path).map_err(|e| ExecutionError::new(3002, format!("{manifest_path}: {e}")))?;
    let pkcs8 = std::fs::read(&key_path).map_err(|e| ExecutionError::new(3002, format!("{key_path}: {e}")))?;
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| ExecutionError::new(2001, "私钥不是 Ed25519 PKCS8".into()))?;
    let signature = pair.sign(&manifest);
    std::fs::write(format!("{dir}/dake.pub"), ring::signature::KeyPair::public_key(&pair).as_ref()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    std::fs::write(format!("{dir}/dake.seal"), signature.as_ref()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(Value::Bool(true))
}

fn files_unseal(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let dir = safe_path(&exec.arg_string(args, 0)?)?;
    let trusted_path = safe_path(&exec.arg_string(args, 1)?)?;
    let trusted = std::fs::read(&trusted_path).map_err(|e| ExecutionError::new(3002, format!("{trusted_path}: {e}")))?;
    let public = std::fs::read(format!("{dir}/dake.pub")).map_err(|e| ExecutionError::new(3002, format!("dake.pub: {e}")))?;
    if public != trusted {
        return Err(ExecutionError::new(2004, "公钥与包里的 dake.pub 不一致".into()));
    }
    let signature = std::fs::read(format!("{dir}/dake.seal")).map_err(|e| ExecutionError::new(3002, format!("dake.seal: {e}")))?;
    let manifest = std::fs::read(format!("{dir}/manifest.json")).map_err(|e| ExecutionError::new(3002, format!("manifest.json: {e}")))?;
    let key = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public);
    key.verify(&manifest, &signature).map_err(|_| ExecutionError::new(2004, "公钥签名不符".into()))?;
    check_manifest_hashes(&dir, &manifest)?;
    Ok(Value::Bool(true))
}

fn check_manifest_hashes(dir: &str, manifest: &[u8]) -> Result<(), ExecutionError> {
    let text = std::str::from_utf8(manifest).map_err(|_| ExecutionError::new(4012, "清单不是 UTF-8".into()))?;
    let value = crate::data::SerializableValue::from_json(text).map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?;
    let crate::data::SerializableValue::Object(map) = value else {
        return Err(ExecutionError::new(4012, "清单不是对象".into()));
    };
    let files = match map.get("files") {
        Some(crate::data::SerializableValue::Array(items)) => items,
        _ => return Err(ExecutionError::new(4012, "清单没有 files".into())),
    };
    for item in files {
        let crate::data::SerializableValue::Object(file) = item else {
            return Err(ExecutionError::new(4012, "清单文件项不是对象".into()));
        };
        let file_name = json_string(file.get("name"))?;
        let expect = json_string(file.get("blake3"))?;
        let path = safe_path(&format!("{dir}/{file_name}"))?;
        let got = crate::executor::paths::hash_chunks(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
        if got != expect {
            return Err(ExecutionError::new(4012, format!("{file_name} 的哈希不符")));
        }
    }
    Ok(())
}

fn files_verify(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let dir = safe_path(&exec.arg_string(args, 0)?)?;
    let key = match named_arg(args, "key") {
        Some(arg) => Some(safe_path(&exec.arg_value_string(&arg.temp)?)?),
        None => None,
    };
    let (name, version, replaces, depends) = verify_package(&dir, key.as_deref())?;
    let depend_dirs = if named_arg(args, "depend").is_some() {
        exec.named_string_list(args, "depend")?
    } else {
        Vec::new()
    };
    if depend_dirs.len() != depends.len() {
        return Err(ExecutionError::new(4011, format!("depends 有 {} 项，depend 给了 {} 个目录", depends.len(), depend_dirs.len())));
    }
    for (declared, depend_dir) in depends.iter().zip(depend_dirs) {
        let depend_dir = safe_path(&depend_dir)?;
        let (dep_name, dep_version, _, _) = verify_package(&depend_dir, key.as_deref())?;
        let expect = format!("{dep_name} {dep_version}");
        if declared != &expect {
            return Err(ExecutionError::new(4012, format!("依赖应为 {declared}，目录里是 {expect}")));
        }
    }
    match (replaces.is_empty(), named_arg(args, "replace")) {
        (true, None) => {}
        (false, Some(arg)) => {
            let replace_dir = safe_path(&exec.arg_value_string(&arg.temp)?)?;
            let (old_name, old_version, _, _) = verify_package(&replace_dir, key.as_deref())?;
            let expect = format!("{old_name} {old_version}");
            if replaces != expect {
                return Err(ExecutionError::new(4012, format!("replaces 应为 {replaces}，目录里是 {expect}")));
            }
        }
        (false, None) => return Err(ExecutionError::new(4011, "清单有 replaces，需要 replace".into())),
        (true, Some(_)) => return Err(ExecutionError::new(4011, "清单没有 replaces".into())),
    }
    let _ = (name, version);
    Ok(Value::Bool(true))
}

fn verify_package(dir: &str, key: Option<&str>) -> Result<(String, String, String, Vec<String>), ExecutionError> {
    let manifest_path = format!("{dir}/manifest.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| ExecutionError::new(3002, format!("{manifest_path}: {e}")))?;
    let value = crate::data::SerializableValue::from_json(&text).map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?;
    let crate::data::SerializableValue::Object(map) = value else {
        return Err(ExecutionError::new(4012, "清单不是对象".into()));
    };
    let name = json_string(map.get("name"))?;
    let version = json_string(map.get("version"))?;
    let replaces = json_string(map.get("replaces")).unwrap_or_default();
    let signature = json_string(map.get("signature")).unwrap_or_default();
    let depends = match map.get("depends") {
        Some(crate::data::SerializableValue::Array(items)) => items.iter().map(|item| json_string(Some(item))).collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    let files = match map.get("files") {
        Some(crate::data::SerializableValue::Array(items)) => items,
        _ => return Err(ExecutionError::new(4012, "清单没有 files".into())),
    };
    let mut pairs = Vec::new();
    for item in files {
        let crate::data::SerializableValue::Object(file) = item else {
            return Err(ExecutionError::new(4012, "清单文件项不是对象".into()));
        };
        let file_name = json_string(file.get("name"))?;
        let expect = json_string(file.get("blake3"))?;
        let path = safe_path(&format!("{dir}/{file_name}"))?;
        let got = crate::executor::paths::hash_chunks(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
        if got != expect {
            return Err(ExecutionError::new(4012, format!("{file_name} 的哈希不符").into()));
        }
        pairs.push((file_name, expect));
    }
    if !signature.is_empty() {
        let key = key.ok_or_else(|| ExecutionError::new(4011, "清单有签名，需要 key".into()))?;
        let key = super::package::load_sign_key(key).map_err(|err| ExecutionError::new(2001, err.message))?;
        let mac = super::package::mac_manifest(&key, &name, &version, &replaces, &depends, &pairs);
        if mac != signature {
            return Err(ExecutionError::new(2004, "签名不符".into()));
        }
    }
    Ok((name, version, replaces, depends))
}

fn json_string(value: Option<&crate::data::SerializableValue>) -> Result<String, ExecutionError> {
    match value {
        Some(crate::data::SerializableValue::String(text)) => Ok(text.clone()),
        _ => Err(ExecutionError::new(4012, "清单字段需要字符串".into())),
    }
}

fn write_rows(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_write(&path)?;
    let items = match exec.get(&args[1].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "files.write.rows 需要记录列表".into())),
    };
    let mut lines = Vec::new();
    if let Some(header) = named_arg(args, "header") {
        match exec.get(&header.temp)? {
            Value::Bool(false) => {}
            Value::Bool(true) => {
                let Value::Record { path: id, .. } = items.first().ok_or_else(|| {
                    ExecutionError::new(4012, "空列表没有结构，不能写表头".into())
                })? else {
                    return Err(ExecutionError::new(4012, "files.write.rows 的元素需要记录".into()));
                };
                let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
                if !matches!(def.layout, LayoutKind::Split) {
                    return Err(ExecutionError::new(4018, "header 只适用于 split".into()));
                }
                let sep = def.sep.clone().unwrap_or_else(|| ",".into());
                let names: Vec<_> = def.fields.iter().map(|field| field.name.clone()).collect();
                lines.push(names.join(&sep));
            }
            Value::String(text) => lines.push(text),
            _ => return Err(ExecutionError::new(4012, "header 需要布尔值或字符串".into())),
        }
    }
    for item in items.iter() {
        let Value::Record { path: id, fields } = item else {
            return Err(ExecutionError::new(4012, "files.write.rows 的元素需要记录".into()));
        };
        enforce_record(exec, id, fields.as_slice())?;
        let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
        let text = match def.layout {
            LayoutKind::Split => encode_fields(def, fields.as_slice())?,
            LayoutKind::Json => encode_json(exec, def, fields.as_slice())?,
            _ => return Err(ExecutionError::new(4018, "files.write.rows 需要 split 或 json 结构".into())),
        };
        lines.push(text);
    }
    let body = lines.join("\n");
    let append = named_flag(exec, args, "append")?;
    if append && named_arg(args, "header").is_some() {
        return Err(ExecutionError::new(4012, "append 不能和 header 一起用".into()));
    }
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    if append {
        use std::io::{Read, Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new().create(true).read(true).append(true).open(&path)
            .map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        let len = file.seek(SeekFrom::End(0)).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        let needs_break = if len == 0 {
            false
        } else {
            file.seek(SeekFrom::End(-1)).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
            let mut last = [0u8; 1];
            file.read_exact(&mut last).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
            last[0] != b'\n'
        };
        if needs_break && !body.is_empty() {
            file.write_all(b"\n").map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
        file.write_all(body.as_bytes()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    } else {
        crate::executor::paths::write_chunks(&path, body.as_bytes()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    }
    Ok(Value::List(Arc::new(items)))
}

fn write_row(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    use std::io::Write;
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_write(&path)?;
    let record = exec.get(&args[1].temp)?;
    let Value::Record { path: id, fields } = &record else {
        return Err(ExecutionError::new(4012, "files.write.row 需要记录".into()));
    };
    enforce_record(exec, id, fields.as_slice())?;
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let text = match def.layout {
        LayoutKind::Split => encode_fields(def, fields.as_slice())?,
        LayoutKind::Json => encode_json(exec, def, fields.as_slice())?,
        _ => return Err(ExecutionError::new(4018, "files.write.row 需要 split 或 json 结构".into())),
    };
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    let header = named_flag(exec, args, "header")?;
    let empty = std::fs::metadata(&path).map(|meta| meta.len() == 0).unwrap_or(true);
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)
        .map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    if header {
        if !matches!(def.layout, LayoutKind::Split) {
            return Err(ExecutionError::new(4018, "header 只适用于 split".into()));
        }
        if empty {
            let sep = def.sep.clone().unwrap_or_else(|| ",".into());
            let names: Vec<_> = def.fields.iter().map(|field| field.name.clone()).collect();
            writeln!(file, "{}", names.join(&sep)).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    writeln!(file, "{text}").map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(record)
}

fn decode_line(exec: &Executor, id: &str, text: &str) -> Result<Value, ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let fields = match def.layout {
        LayoutKind::Split => decode_fields(def, text)?,
        LayoutKind::Json => decode_json(exec, def, text)?,
        _ => return Err(ExecutionError::new(4018, "files.rows 需要 split 或 json 结构".into())),
    };
    finish_record(exec, id, fields)
}

fn finish_record(exec: &Executor, id: &str, fields: Vec<(String, Value)>) -> Result<Value, ExecutionError> {
    enforce_record(exec, id, &fields)?;
    Ok(Value::Record { path: id.to_string(), fields: Arc::new(fields) })
}

fn enforce_record(exec: &Executor, id: &str, fields: &[(String, Value)]) -> Result<(), ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    for field in &def.fields {
        let Some(check) = &field.check else { continue };
        let value = fields.iter().find(|(name, _)| name == &field.name).map(|(_, value)| value);
        let Some(Value::String(text)) = value else {
            return Err(ExecutionError::new(4012, format!("字段 {} 的检查需要字符串", field.name)));
        };
        run_check(exec, check, &field.name, text)?;
    }
    Ok(())
}

fn run_check(exec: &Executor, check: &str, field: &str, text: &str) -> Result<(), ExecutionError> {
    let result = match check {
        "not_empty" => exec.validator.validate_not_empty(field, text),
        "email" => exec.validator.validate_email(field, text),
        "url" => exec.validator.validate_url(field, text),
        "numeric" => exec.validator.validate_numeric(field, text),
        "alpha" => exec.validator.validate_alpha(field, text),
        "alphanumeric" => exec.validator.validate_alphanumeric(field, text),
        other => return Err(ExecutionError::new(4020, format!("未知的验证器: {other}"))),
    };
    result.map_err(|err| ExecutionError::new(err.code(), format!("字段 {field}: {}", err.message())))
}

fn successor(exec: &Executor, start: &str) -> Result<String, ExecutionError> {
    let mut current = start.to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(ExecutionError::new(4018, format!("结构替换成环: {start}")));
        }
        let next = exec.structs.iter().find(|(_, def)| def.replaces.as_deref() == Some(current.as_str()));
        match next {
            Some((id, _)) => current = id.clone(),
            None => return Ok(current),
        }
    }
}

fn evolve_record(exec: &Executor, row: Value, target: &str) -> Result<Value, ExecutionError> {
    let Value::Record { fields: source, .. } = row else {
        return Err(ExecutionError::new(4012, "替换需要记录".into()));
    };
    let def = exec.structs.get(target).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {target}")))?;
    let mut fields = Vec::new();
    for field in &def.fields {
        let source_name = field.take.clone().unwrap_or_else(|| field.name.clone());
        let value = if let Some((_, value)) = source.iter().find(|(name, _)| name == &source_name) {
            if !value_matches(&field.ty, value, target) {
                return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name)));
            }
            value.clone()
        } else if let Some(default) = &field.default {
            default.clone()
        } else {
            return Err(ExecutionError::new(4012, format!("字段 {} 在旧结构里没有", field.name)));
        };
        fields.push((field.name.clone(), value));
    }
    finish_record(exec, target, fields)
}

fn value_matches(ty: &FieldType, value: &Value, record_path: &str) -> bool {
    match (ty, value) {
        (FieldType::Str, Value::String(_)) => true,
        (FieldType::Bytes, Value::Bytes(_)) => true,
        (FieldType::Int, Value::Number(_)) => true,
        (FieldType::Float, Value::Float(_)) => true,
        (FieldType::Bool, Value::Bool(_)) => true,
        (FieldType::List(inner), Value::List(items)) => items.iter().all(|item| value_matches(inner, item, record_path)),
        (FieldType::Struct(_), Value::Record { path, .. }) => path == record_path,
        (FieldType::Linked(id), Value::Record { path, .. }) => path == id,
        _ => false,
    }
}

fn list_update(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "list.update 需要记录列表".into())),
    };
    let field = exec.arg_string(args, 1)?;
    let value = exec.get(&args[2].temp)?;
    let mut seen = None;
    let mut out = Vec::new();
    for item in items {
        let Value::Record { path: id, .. } = &item else {
            return Err(ExecutionError::new(4012, "list.update 的元素需要记录".into()));
        };
        if let Some(existing) = &seen {
            if existing != id {
                return Err(ExecutionError::new(4012, "list.update 的记录必须是同一种结构".into()));
            }
        } else {
            seen = Some(id.clone());
        }
        out.push(replace_field(exec, item, &field, value.clone())?);
    }
    Ok(Value::List(Arc::new(out)))
}

fn replace_field(exec: &Executor, record: Value, field: &str, value: Value) -> Result<Value, ExecutionError> {
    let Value::Record { path: id, fields } = record else {
        return Err(ExecutionError::new(4012, "update 需要记录".into()));
    };
    let mut fields = Arc::unwrap_or_clone(fields);
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let expect = def.fields.iter().find(|item| item.name == field).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {field}"))
    })?;
    if !value_matches(&expect.ty, &value, &id) {
        return Err(ExecutionError::new(4012, format!("字段 {field} 的类型不符")));
    }
    let slot = fields.iter_mut().find(|(name, _)| name == field).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {field}"))
    })?;
    slot.1 = value;
    finish_record(exec, &id, fields)
}

fn list_join(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 5)?;
    let left = record_list(exec, &args[0].temp, "list.join 的左边")?;
    let left_key = exec.arg_string(args, 1)?;
    let right = record_list(exec, &args[2].temp, "list.join 的右边")?;
    let right_key = exec.arg_string(args, 3)?;
    let id = exec.arg_string(args, 4)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    if left.is_empty() || right.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }
    let left_ty = key_type(exec, &left, &left_key)?;
    let right_ty = key_type(exec, &right, &right_key)?;
    if left_ty != right_ty {
        return Err(ExecutionError::new(4012, "连接字段的类型不符".into()));
    }
    let mut out = Vec::new();
    for left_row in &left {
        let Value::Record { fields: left_fields, .. } = left_row else { continue };
        let left_value = field_value(left_fields, &left_key)?;
        for right_row in &right {
            let Value::Record { fields: right_fields, .. } = right_row else { continue };
            if field_value(right_fields, &right_key)? != left_value {
                continue;
            }
            let mut fields = Vec::new();
            for field in &def.fields {
                let from_left = left_fields.iter().find(|(name, _)| name == &field.name).map(|(_, value)| value);
                let from_right = right_fields.iter().find(|(name, _)| name == &field.name).map(|(_, value)| value);
                let value = match (from_left, from_right) {
                    (Some(left_value), Some(right_value)) if left_value == right_value => left_value.clone(),
                    (Some(_), Some(_)) => {
                        return Err(ExecutionError::new(4012, format!("字段 {} 两边不一致", field.name)));
                    }
                    (Some(value), None) | (None, Some(value)) => value.clone(),
                    (None, None) => {
                        return Err(ExecutionError::new(4012, format!("字段 {} 两边都没有", field.name)));
                    }
                };
                if !value_matches(&field.ty, &value, &id) {
                    return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name)));
                }
                fields.push((field.name.clone(), value));
            }
            enforce_record(exec, &id, &fields)?;
            out.push(Value::Record { path: id.clone(), fields: Arc::new(fields) });
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn list_where(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let items = record_list(exec, &args[0].temp, "list.where")?;
    let field = exec.arg_string(args, 1)?;
    let want = exec.get(&args[2].temp)?;
    if items.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }
    let _ = key_type(exec, &items, &field)?;
    let mut out = Vec::new();
    for item in items.iter() {
        let Value::Record { fields, .. } = item else { continue };
        if field_value(fields, &field)? == want {
            out.push(item.clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn list_pick(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let items = record_list(exec, &args[0].temp, "list.pick")?;
    let id = exec.arg_string(args, 1)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    if items.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }
    let mut out = Vec::new();
    for item in items.iter() {
        let Value::Record { fields: source, .. } = item else { continue };
        let mut fields = Vec::new();
        for field in &def.fields {
            let value = field_value(source, &field.name).map_err(|_| {
                ExecutionError::new(4017, format!("来源没有字段 {}", field.name))
            })?;
            if !value_matches(&field.ty, &value, &id) {
                return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name)));
            }
            fields.push((field.name.clone(), value));
        }
        enforce_record(exec, &id, &fields)?;
        out.push(Value::Record { path: id.clone(), fields: Arc::new(fields) });
    }
    Ok(Value::List(Arc::new(out)))
}

fn list_sort(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let mut items = record_list(exec, &args[0].temp, "list.sort")?;
    let field = exec.arg_string(args, 1)?;
    let desc = match named_arg(args, "order") {
        Some(arg) => match exec.get(&arg.temp)? {
            Value::String(text) if text == "asc" => false,
            Value::String(text) if text == "desc" => true,
            _ => return Err(ExecutionError::new(4012, "order 只能是 asc 或 desc".into())),
        },
        None => false,
    };
    if items.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }
    let ty = key_type(exec, &items, &field)?;
    if !matches!(ty, FieldType::Str | FieldType::Int | FieldType::Float | FieldType::Bool) {
        return Err(ExecutionError::new(4012, format!("字段 {field} 不能排序")));
    }
    items.sort_by(|left, right| {
        let Value::Record { fields: left_fields, .. } = left else { return std::cmp::Ordering::Equal };
        let Value::Record { fields: right_fields, .. } = right else { return std::cmp::Ordering::Equal };
        let left_value = left_fields.iter().find(|(name, _)| name == &field).map(|(_, value)| value);
        let right_value = right_fields.iter().find(|(name, _)| name == &field).map(|(_, value)| value);
        let order = cmp_values(left_value, right_value);
        if desc { order.reverse() } else { order }
    });
    Ok(Value::List(Arc::new(items)))
}

fn cmp_values(left: Option<&Value>, right: Option<&Value>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
        (Some(Value::Number(a)), Some(Value::Number(b))) => a.cmp(b),
        (Some(Value::Float(a)), Some(Value::Float(b))) => a.total_cmp(b),
        (Some(Value::Bool(a)), Some(Value::Bool(b))) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}

fn list_group(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let items = record_list(exec, &args[0].temp, "list.group")?;
    let field = exec.arg_string(args, 1)?;
    let id = exec.arg_string(args, 2)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    if items.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }
    let source_id = match &items[0] {
        Value::Record { path, .. } => path.clone(),
        _ => return Err(ExecutionError::new(4012, "list.group 需要记录".into())),
    };
    let source_ty = key_type(exec, &items, &field)?.clone();
    if def.fields.len() != 2 {
        return Err(ExecutionError::new(4012, "分组结构必须正好是组键和一个列表".into()));
    }
    let key_field = def.fields.iter().find(|item| item.name == field).ok_or_else(|| {
        ExecutionError::new(4012, format!("分组结构没有字段 {field}"))
    })?;
    if key_field.ty != source_ty {
        return Err(ExecutionError::new(4012, "组键的类型不符".into()));
    }
    let list_field = def.fields.iter().find(|item| item.name != field).ok_or_else(|| {
        ExecutionError::new(4012, "分组结构必须正好是组键和一个列表".into())
    })?;
    let FieldType::List(inner) = &list_field.ty else {
        return Err(ExecutionError::new(4012, "分组结构必须正好是组键和一个列表".into()));
    };
    let FieldType::Linked(linked) = inner.as_ref() else {
        return Err(ExecutionError::new(4012, "分组列表的元素必须是来源结构".into()));
    };
    if linked != &source_id {
        return Err(ExecutionError::new(4012, "分组列表的元素必须是来源结构".into()));
    }
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for item in items {
        let Value::Record { fields, .. } = &item else { continue };
        let key = field_value(fields, &field)?;
        if let Some((_, rows)) = groups.iter_mut().find(|(existing, _)| existing == &key) {
            rows.push(item);
        } else {
            groups.push((key, vec![item]));
        }
    }
    let mut out = Vec::new();
    for (key, rows) in groups {
        let mut fields = Vec::new();
        for item in &def.fields {
            let value = if item.name == field {
                key.clone()
            } else {
                Value::List(Arc::new(rows.clone()))
            };
            fields.push((item.name.clone(), value));
        }
        enforce_record(exec, &id, &fields)?;
        out.push(Value::Record { path: id.clone(), fields: Arc::new(fields) });
    }
    Ok(Value::List(Arc::new(out)))
}

fn files_name(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    let name = std::path::Path::new(&path).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if name.is_empty() {
        return Err(ExecutionError::new(3001, format!("路径没有文件名: {path}")));
    }
    Ok(Value::String(name.to_string()))
}

fn files_dir(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    let parent = std::path::Path::new(&path).parent().map(|dir| dir.to_string_lossy().to_string()).unwrap_or_default();
    if parent.is_empty() {
        Ok(Value::String(".".into()))
    } else {
        Ok(Value::String(parent))
    }
}

fn record_list(exec: &Executor, temp: &str, what: &str) -> Result<Vec<Value>, ExecutionError> {
    let items = match exec.get(temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, format!("{what}需要记录列表"))),
    };
    let mut seen = None;
    for item in items.iter() {
        let Value::Record { path, .. } = item else {
            return Err(ExecutionError::new(4012, format!("{what}的元素需要记录")));
        };
        if let Some(existing) = &seen {
            if existing != path {
                return Err(ExecutionError::new(4012, format!("{what}的记录必须是同一种结构")));
            }
        } else {
            seen = Some(path.clone());
        }
    }
    Ok(items)
}

fn key_type<'a>(exec: &'a Executor, rows: &[Value], field: &str) -> Result<&'a FieldType, ExecutionError> {
    let Value::Record { path: id, .. } = &rows[0] else {
        return Err(ExecutionError::new(4012, "连接需要记录".into()));
    };
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let expect = def.fields.iter().find(|item| item.name == field).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {field}"))
    })?;
    Ok(&expect.ty)
}

fn field_value(fields: &[(String, Value)], name: &str) -> Result<Value, ExecutionError> {
    fields.iter().find(|(field, _)| field == name).map(|(_, value)| value.clone()).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {name}"))
    })
}

fn update_record(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let Value::Record { path: id, fields } = exec.get(&args[0].temp)? else {
        return Err(ExecutionError::new(4012, "update 需要记录".into()));
    };
    let mut fields = Arc::unwrap_or_clone(fields);
    let field = exec.arg_string(args, 1)?;
    let value = exec.get(&args[2].temp)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let expect = def.fields.iter().find(|item| item.name == field).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {field}"))
    })?;
    if !value_matches(&expect.ty, &value, &id) {
        return Err(ExecutionError::new(4012, format!("字段 {field} 的类型不符")));
    }
    let slot = fields.iter_mut().find(|(name, _)| name == &field).ok_or_else(|| {
        ExecutionError::new(4017, format!("记录没有字段 {field}"))
    })?;
    slot.1 = value;
    finish_record(exec, &id, fields)
}

fn decode_width(def: &crate::dsl::ir::StructIr, bytes: &[u8]) -> Result<Vec<(String, Value)>, ExecutionError> {
    let mut offset: usize = 0;
    let mut fields = Vec::new();
    for field in &def.fields {
        let slice = if let Some(width) = field.width {
            let width = width as usize;
            let end = offset.checked_add(width).ok_or_else(|| ExecutionError::new(4018, "字段宽度溢出".into()))?;
            if end > bytes.len() {
                return Err(ExecutionError::new(4018, "字节长度不足以切开字段".into()));
            }
            let slice = &bytes[offset..end];
            offset = end;
            slice
        } else {
            let slice = &bytes[offset..];
            offset = bytes.len();
            slice
        };
        let value = match &field.ty {
            FieldType::Bytes => Value::Bytes(Arc::new(slice.to_vec())),
            FieldType::Int => Value::Number(read_int(slice, def.order)?),
            FieldType::Float => Value::Float(read_float(slice, def.order)?),
            FieldType::Bool => Value::Bool(match slice {
                [0] => false,
                [1] => true,
                _ => return Err(ExecutionError::new(4012, format!("字段 {} 不是布尔值", field.name))),
            }),
            _ => return Err(ExecutionError::new(4018, format!("字段 {} 不能出现在 width 布局", field.name))),
        };
        fields.push((field.name.clone(), value));
    }
    if offset != bytes.len() {
        return Err(ExecutionError::new(4018, "字节长度超出字段宽度之和".into()));
    }
    Ok(fields)
}

fn read_int(slice: &[u8], order: Endian) -> Result<i64, ExecutionError> {
    let le = order == Endian::Le;
    match (slice.len(), le) {
        (1, _) => Ok(slice[0] as i8 as i64),
        (2, false) => Ok(i16::from_be_bytes([slice[0], slice[1]]) as i64),
        (2, true) => Ok(i16::from_le_bytes([slice[0], slice[1]]) as i64),
        (4, false) => Ok(i32::from_be_bytes(slice.try_into().unwrap()) as i64),
        (4, true) => Ok(i32::from_le_bytes(slice.try_into().unwrap()) as i64),
        (8, false) => Ok(i64::from_be_bytes(slice.try_into().unwrap())),
        (8, true) => Ok(i64::from_le_bytes(slice.try_into().unwrap())),
        _ => Err(ExecutionError::new(4018, "整数宽度只能是 1、2、4 或 8".into())),
    }
}

fn read_float(slice: &[u8], order: Endian) -> Result<f64, ExecutionError> {
    let le = order == Endian::Le;
    match (slice.len(), le) {
        (4, false) => Ok(f32::from_be_bytes(slice.try_into().unwrap()) as f64),
        (4, true) => Ok(f32::from_le_bytes(slice.try_into().unwrap()) as f64),
        (8, false) => Ok(f64::from_be_bytes(slice.try_into().unwrap())),
        (8, true) => Ok(f64::from_le_bytes(slice.try_into().unwrap())),
        _ => Err(ExecutionError::new(4018, "小数宽度只能是 4 或 8".into())),
    }
}

fn encode_width(def: &crate::dsl::ir::StructIr, fields: &[(String, Value)]) -> Result<Vec<u8>, ExecutionError> {
    let mut out = Vec::new();
    for (field, (_, value)) in def.fields.iter().zip(fields) {
        match (&field.ty, value, field.width) {
            (FieldType::Bytes, Value::Bytes(bytes), None) => out.extend_from_slice(bytes),
            (FieldType::Bytes, Value::Bytes(bytes), Some(width)) if bytes.len() == width as usize => out.extend_from_slice(bytes),
            (FieldType::Bytes, Value::Bytes(_), _) => {
                return Err(ExecutionError::new(4012, format!("字段 {} 的字节长度与宽度不符", field.name)));
            }
            (FieldType::Int, Value::Number(n), Some(width)) => out.extend(write_int(*n, width as usize, def.order)?),
            (FieldType::Float, Value::Float(n), Some(width)) => out.extend(write_float(*n, width as usize, def.order)?),
            (FieldType::Bool, Value::Bool(b), _) => out.push(if *b { 1 } else { 0 }),
            _ => return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name))),
        }
    }
    Ok(out)
}

fn write_int(n: i64, width: usize, order: Endian) -> Result<Vec<u8>, ExecutionError> {
    let le = order == Endian::Le;
    let bytes = match width {
        1 => {
            if n < i8::MIN as i64 || n > i8::MAX as i64 {
                return Err(ExecutionError::new(4012, "整数超出 1 字节范围".into()));
            }
            vec![n as i8 as u8]
        }
        2 => {
            if n < i16::MIN as i64 || n > i16::MAX as i64 {
                return Err(ExecutionError::new(4012, "整数超出 2 字节范围".into()));
            }
            if le { (n as i16).to_le_bytes().to_vec() } else { (n as i16).to_be_bytes().to_vec() }
        }
        4 => {
            if n < i32::MIN as i64 || n > i32::MAX as i64 {
                return Err(ExecutionError::new(4012, "整数超出 4 字节范围".into()));
            }
            if le { (n as i32).to_le_bytes().to_vec() } else { (n as i32).to_be_bytes().to_vec() }
        }
        8 => if le { n.to_le_bytes().to_vec() } else { n.to_be_bytes().to_vec() },
        _ => return Err(ExecutionError::new(4018, "整数宽度只能是 1、2、4 或 8".into())),
    };
    Ok(bytes)
}

fn write_float(n: f64, width: usize, order: Endian) -> Result<Vec<u8>, ExecutionError> {
    let le = order == Endian::Le;
    match width {
        4 => {
            if !(n as f32).is_finite() {
                return Err(ExecutionError::new(4012, "小数超出 4 字节范围".into()));
            }
            let bits = (n as f32).to_bits();
            Ok(if le { bits.to_le_bytes().to_vec() } else { bits.to_be_bytes().to_vec() })
        }
        8 => {
            let bits = n.to_bits();
            Ok(if le { bits.to_le_bytes().to_vec() } else { bits.to_be_bytes().to_vec() })
        }
        _ => Err(ExecutionError::new(4018, "小数宽度只能是 4 或 8".into())),
    }
}

fn split_quoted(text: &str, sep: &str) -> Result<Vec<String>, ExecutionError> {
    let chars: Vec<char> = text.chars().collect();
    let sep: Vec<char> = sep.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut i = 0;
    while i < chars.len() {
        if quoted {
            if chars[i] == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    cur.push('"');
                    i += 2;
                    continue;
                }
                quoted = false;
                i += 1;
                continue;
            }
            cur.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == '"' && cur.is_empty() {
            quoted = true;
            i += 1;
            continue;
        }
        if chars[i..].starts_with(&sep) {
            parts.push(std::mem::take(&mut cur));
            i += sep.len();
            continue;
        }
        cur.push(chars[i]);
        i += 1;
    }
    if quoted {
        return Err(ExecutionError::new(4018, "引号未闭合".into()));
    }
    parts.push(cur);
    Ok(parts)
}

fn quote_field(text: &str, sep: &str) -> String {
    if text.contains(sep) || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn decode_block(def: &crate::dsl::ir::StructIr, bytes: &[u8]) -> Result<Vec<(String, Value)>, ExecutionError> {
    let mut reader = std::io::Cursor::new(bytes);
    read_block_fields(def, &mut reader)
}

fn read_block_fields(def: &crate::dsl::ir::StructIr, reader: &mut impl std::io::Read) -> Result<Vec<(String, Value)>, ExecutionError> {
    use std::io::Read;
    let mut fields = Vec::new();
    for field in &def.fields {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).map_err(|_| ExecutionError::new(4018, format!("字段 {} 的块不完整", field.name)))?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        reader.read_exact(&mut payload).map_err(|_| ExecutionError::new(4018, format!("字段 {} 的块不完整", field.name)))?;
        let value = match &field.ty {
            FieldType::Bytes => Value::Bytes(Arc::new(payload)),
            FieldType::Str => Value::String(String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?),
            FieldType::Int => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                Value::Number(text.parse().map_err(|_| ExecutionError::new(4012, format!("字段 {} 不是整数", field.name)))?)
            }
            FieldType::Float => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                Value::Float(text.parse().map_err(|_| ExecutionError::new(4012, format!("字段 {} 不是小数", field.name)))?)
            }
            FieldType::Bool => {
                let text = String::from_utf8(payload).map_err(|_| ExecutionError::new(4015, format!("字段 {} 不是合法的 UTF-8", field.name)))?;
                match text.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return Err(ExecutionError::new(4012, format!("字段 {} 不是布尔值", field.name))),
                }
            }
            _ => return Err(ExecutionError::new(4018, "block 不能嵌套结构或列表".into())),
        };
        fields.push((field.name.clone(), value));
    }
    let mut extra = [0u8; 1];
    match reader.read(&mut extra) {
        Ok(0) => {}
        Ok(_) => return Err(ExecutionError::new(4018, "block 后面还有多余的字节".into())),
        Err(err) => return Err(ExecutionError::new(3002, err.to_string())),
    }
    Ok(fields)
}

pub(super) fn decode_block_file(exec: &Executor, id: &str, path: &str) -> Result<Value, ExecutionError> {
    let def = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    if !matches!(def.layout, LayoutKind::Block) {
        let bytes = crate::executor::paths::read_chunks(path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
        return decode_body(exec, id, bytes);
    }
    let mut file = std::fs::File::open(path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    let fields = read_block_fields(def, &mut file)?;
    finish_record(exec, id, fields)
}

fn encode_block(def: &crate::dsl::ir::StructIr, fields: &[(String, Value)]) -> Result<Vec<u8>, ExecutionError> {
    let mut out = Vec::new();
    for (field, (_, value)) in def.fields.iter().zip(fields) {
        let payload = match (&field.ty, value) {
            (FieldType::Bytes, Value::Bytes(bytes)) => bytes.as_ref().clone(),
            (FieldType::Str, Value::String(text)) => text.as_bytes().to_vec(),
            (FieldType::Int, Value::Number(n)) => n.to_string().into_bytes(),
            (FieldType::Float, Value::Float(n)) => n.to_string().into_bytes(),
            (FieldType::Bool, Value::Bool(bit)) => bit.to_string().into_bytes(),
            _ => return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name))),
        };
        let len = u32::try_from(payload.len()).map_err(|_| ExecutionError::new(4012, format!("字段 {} 的块超过 4 GiB", field.name)))?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&payload);
    }
    Ok(out)
}

fn decode_fields(def: &crate::dsl::ir::StructIr, text: &str) -> Result<Vec<(String, Value)>, ExecutionError> {
    let sep = def.sep.as_deref().ok_or_else(|| ExecutionError::new(4018, "split 缺少 sep".into()))?;
    let parts = split_quoted(text, sep)?;
    if parts.len() != def.fields.len() {
        return Err(ExecutionError::new(4018, format!("字段个数不符，期望 {} 得到 {}", def.fields.len(), parts.len())));
    }
    let mut fields = Vec::new();
    for (field, part) in def.fields.iter().zip(parts.iter()) {
        let value = match field.ty {
            FieldType::Str => Value::String(part.clone()),
            FieldType::Int => Value::Number(part.parse().map_err(|_| {
                ExecutionError::new(4012, format!("字段 {} 不是整数", field.name))
            })?),
            FieldType::Float => Value::Float(part.parse().map_err(|_| {
                ExecutionError::new(4012, format!("字段 {} 不是小数", field.name))
            })?),
            FieldType::Bool => match part.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(ExecutionError::new(4012, format!("字段 {} 不是布尔值", field.name))),
            },
            _ => return Err(ExecutionError::new(4018, "split 字段只能是 str、int 或 bool".into())),
        };
        fields.push((field.name.clone(), value));
    }
    Ok(fields)
}

fn encode_fields(def: &crate::dsl::ir::StructIr, fields: &[(String, Value)]) -> Result<String, ExecutionError> {
    let sep = def.sep.as_deref().ok_or_else(|| ExecutionError::new(4018, "split 缺少 sep".into()))?;
    let mut parts = Vec::new();
    for (field, (_, value)) in def.fields.iter().zip(fields) {
        let text = match (&field.ty, value) {
            (FieldType::Str, Value::String(text)) => quote_field(text, sep),
            (FieldType::Int, Value::Number(n)) => n.to_string(),
            (FieldType::Float, Value::Float(n)) => n.to_string(),
            (FieldType::Bool, Value::Bool(true)) => "true".into(),
            (FieldType::Bool, Value::Bool(false)) => "false".into(),
            _ => return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name))),
        };
        parts.push(text);
    }
    Ok(parts.join(sep))
}

fn text_decode(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let text = match exec.get(&args[0].temp)? {
        Value::String(text) => text,
        _ => return Err(ExecutionError::new(4012, "text.decode 需要字符串".into())),
    };
    let id = exec.arg_string(args, 1)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let fields = match def.layout {
        LayoutKind::Split => decode_fields(def, &text)?,
        LayoutKind::Json => decode_json(exec, def, &text)?,
        _ => return Err(ExecutionError::new(4018, "text.decode 需要 split 或 json 结构".into())),
    };
    finish_record(exec, &id, fields)
}

fn text_encode(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let Value::Record { path: id, fields } = exec.get(&args[0].temp)? else {
        return Err(ExecutionError::new(4012, "text.encode 需要记录".into()));
    };
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let text = match def.layout {
        LayoutKind::Split => encode_fields(def, &fields)?,
        LayoutKind::Json => encode_json(exec, def, &fields)?,
        _ => return Err(ExecutionError::new(4018, "text.encode 需要 split 或 json 结构".into())),
    };
    Ok(Value::String(text))
}

fn decode_json(exec: &Executor, def: &crate::dsl::ir::StructIr, text: &str) -> Result<Vec<(String, Value)>, ExecutionError> {
    let value = crate::data::SerializableValue::from_json(text).map_err(|e| {
        ExecutionError::new(e.code(), e.message().to_string())
    })?;
    let crate::data::SerializableValue::Object(map) = value else {
        return Err(ExecutionError::new(4012, "json 结构需要对象".into()));
    };
    if map.len() != def.fields.len() {
        return Err(ExecutionError::new(4018, "对象字段个数不符".into()));
    }
    let mut fields = Vec::new();
    for field in &def.fields {
        let item = map.get(&field.name).ok_or_else(|| {
            ExecutionError::new(4018, format!("对象缺少字段 {}", field.name))
        })?;
        fields.push((field.name.clone(), json_value(exec, &field.ty, item)?));
    }
    Ok(fields)
}

fn json_value(exec: &Executor, ty: &FieldType, item: &crate::data::SerializableValue) -> Result<Value, ExecutionError> {
    use crate::data::SerializableValue as S;
    match (ty, item) {
        (FieldType::Str, S::String(text)) => Ok(Value::String(text.clone())),
        (FieldType::Int, S::Int(n)) => Ok(Value::Number(*n)),
        (FieldType::Float, S::Float(n)) => Ok(Value::Float(*n)),
        (FieldType::Float, S::Int(n)) => Ok(Value::Float(*n as f64)),
        (FieldType::Bool, S::Bool(b)) => Ok(Value::Bool(*b)),
        (FieldType::Bytes, S::String(text)) => Ok(Value::Bytes(Arc::new(text.as_bytes().to_vec()))),
        (FieldType::List(inner), S::Array(items)) => {
            let mut values = Vec::new();
            for item in items {
                values.push(json_value(exec, inner, item)?);
            }
            Ok(Value::List(Arc::new(values)))
        }
        (FieldType::Linked(id), S::Object(_)) => {
            let nested = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
            let text = item_json(item);
            Ok(Value::Record { path: id.clone(), fields: Arc::new(decode_json(exec, nested, &text)?) })
        }
        _ => Err(ExecutionError::new(4012, "对象字段类型不符".into())),
    }
}

fn item_json(item: &crate::data::SerializableValue) -> String {
    item.to_json().unwrap_or_else(|_| "{}".into())
}

fn encode_json(exec: &Executor, def: &crate::dsl::ir::StructIr, fields: &[(String, Value)]) -> Result<String, ExecutionError> {
    let mut parts = Vec::new();
    for (field, (_, value)) in def.fields.iter().zip(fields) {
        parts.push(format!("\"{}\":{}", field.name, value_json(exec, &field.ty, value)?));
    }
    Ok(format!("{{{}}}", parts.join(",")))
}

fn value_json(exec: &Executor, ty: &FieldType, value: &Value) -> Result<String, ExecutionError> {
    match (ty, value) {
        (FieldType::Str, Value::String(text)) => Ok(format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))),
        (FieldType::Int, Value::Number(n)) => Ok(n.to_string()),
        (FieldType::Float, Value::Float(n)) => Ok(n.to_string()),
        (FieldType::Bool, Value::Bool(b)) => Ok(b.to_string()),
        (FieldType::Bytes, Value::Bytes(bytes)) => {
            let text = String::from_utf8((**bytes).clone()).map_err(|_| ExecutionError::new(4015, "字节不是合法的 UTF-8".into()))?;
            Ok(format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")))
        }
        (FieldType::List(inner), Value::List(items)) => {
            let mut parts = Vec::new();
            for item in items.iter() {
                parts.push(value_json(exec, inner, item)?);
            }
            Ok(format!("[{}]", parts.join(",")))
        }
        (FieldType::Linked(id), Value::Record { path, fields }) if path == id => {
            let nested = exec.structs.get(id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
            encode_json(exec, nested, fields)
        }
        _ => Err(ExecutionError::new(4012, "记录字段不能编码为 json".into())),
    }
}

fn list_len(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    match exec.get(&args[0].temp)? {
        Value::List(items) => Ok(Value::Number(items.len() as i64)),
        _ => Err(ExecutionError::new(4012, "list.len 需要列表".into())),
    }
}

fn make_record(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    let id = exec.arg_string(args, 0)?;
    let def = exec.structs.get(&id).ok_or_else(|| ExecutionError::new(4018, format!("找不到结构 {id}")))?;
    let mut given = std::collections::HashMap::new();
    let mut index = 1;
    while index + 1 < args.len() {
        let name = exec.arg_string(args, index)?;
        if given.insert(name.clone(), exec.get(&args[index + 1].temp)?).is_some() {
            return Err(ExecutionError::new(4012, format!("字段 {name} 重复")));
        }
        index += 2;
    }
    let mut fields = Vec::new();
    for field in &def.fields {
        let value = match given.remove(&field.name) {
            Some(value) => value,
            None => field.default.clone().ok_or_else(|| ExecutionError::new(4012, format!("缺少字段 {}", field.name)))?,
        };
        if !value_matches(&field.ty, &value, &id) {
            return Err(ExecutionError::new(4012, format!("字段 {} 的类型不符", field.name)));
        }
        fields.push((field.name.clone(), value));
    }
    if !given.is_empty() {
        return Err(ExecutionError::new(4012, "record 含有结构之外的字段".into()));
    }
    finish_record(exec, &id, fields)
}

fn make_object(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    if args.len() % 2 != 0 {
        return Err(ExecutionError::new(4012, "object 的键和值必须成对".into()));
    }
    let mut fields = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let key = match exec.get(&args[index].temp)? {
            Value::String(key) => key,
            _ => return Err(ExecutionError::new(4012, "object 的键需要字符串".into())),
        };
        if fields.iter().any(|(name, _)| name == &key) {
            return Err(ExecutionError::new(4012, format!("字段 {key} 重复")));
        }
        fields.push((key, exec.get(&args[index + 1].temp)?));
        index += 2;
    }
    Ok(Value::Object(Arc::new(fields)))
}

fn list_set(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let mut items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "list.set 需要列表".into())),
    };
    let index = match exec.get(&args[1].temp)? {
        Value::Number(n) => n,
        _ => return Err(ExecutionError::new(4012, "list.set 的下标需要整数".into())),
    };
    if index < 0 || index as usize >= items.len() {
        return Err(ExecutionError::new(4019, format!("下标越界: {index}")));
    }
    items[index as usize] = exec.get(&args[2].temp)?;
    Ok(Value::List(Arc::new(items)))
}

fn list_remove(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let mut items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "list.remove 需要列表".into())),
    };
    let index = match exec.get(&args[1].temp)? {
        Value::Number(n) => n,
        _ => return Err(ExecutionError::new(4012, "list.remove 的下标需要整数".into())),
    };
    if index < 0 || index as usize >= items.len() {
        return Err(ExecutionError::new(4019, format!("下标越界: {index}")));
    }
    items.remove(index as usize);
    Ok(Value::List(Arc::new(items)))
}

fn list_map(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    apply_each(exec, args, "list.map", false)
}

fn list_keep(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    apply_each(exec, args, "list.keep", true)
}

fn apply_each(exec: &mut Executor, args: &[CallArg], name: &str, keep: bool) -> Result<Value, ExecutionError> {
    need(args, 3)?;
    let items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, format!("{name} 需要列表"))),
    };
    let file = exec.arg_string(args, 1)?;
    let action = exec.arg_string(args, 2)?;
    let mut out = Vec::new();
    for item in items {
        let value = exec.call_named(&file, &action, item.clone())?;
        if keep {
            if ops::truth(&value) {
                out.push(item);
            }
        } else {
            out.push(value);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn list_add(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let mut items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "list.add 需要列表".into())),
    };
    items.push(exec.get(&args[1].temp)?);
    Ok(Value::List(Arc::new(items)))
}

fn text_lines(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let text = match exec.get(&args[0].temp)? {
        Value::String(text) => text,
        _ => return Err(ExecutionError::new(4012, "text.lines 需要字符串".into())),
    };
    Ok(Value::List(Arc::new(split_lines(&text).into_iter().map(Value::String).collect())))
}

fn text_join(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let items = match exec.get(&args[0].temp)? {
        Value::List(items) => Arc::unwrap_or_clone(items),
        _ => return Err(ExecutionError::new(4012, "text.join 需要列表".into())),
    };
    let sep = match exec.get(&args[1].temp)? {
        Value::String(sep) => sep,
        _ => return Err(ExecutionError::new(4012, "text.join 的分隔符需要字符串".into())),
    };
    let mut parts = Vec::new();
    for item in items {
        let Value::String(text) = item else {
            return Err(ExecutionError::new(4012, "text.join 的列表元素需要字符串".into()));
        };
        parts.push(text);
    }
    Ok(Value::String(parts.join(&sep)))
}

fn read_bytes(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_read(&path)?;
    let bytes = crate::executor::paths::read_chunks(&path).map_err(|e| ExecutionError::new(3002, format!("{path}: {e}")))?;
    Ok(Value::Bytes(Arc::new(bytes)))
}

fn read_str(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    let Value::Bytes(bytes) = read_bytes(exec, args)? else { unreachable!() };
    let text = String::from_utf8(Arc::unwrap_or_clone(bytes)).map_err(|_| {
        ExecutionError::new(4015, "文件不是合法的 UTF-8".into())
    })?;
    Ok(Value::String(text))
}

fn write_bytes(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_write(&path)?;
    let bytes = match exec.get(&args[1].temp)? {
        Value::Bytes(bytes) => bytes,
        _ => return Err(ExecutionError::new(4012, "files.write.bytes 需要字节".into())),
    };
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    crate::executor::paths::write_chunks(&path, bytes.as_slice()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(Value::Bytes(bytes))
}

fn write_str(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 2)?;
    let path = safe_path(&exec.arg_string(args, 0)?)?;
    super::access::probe_write(&path)?;
    let text = match exec.get(&args[1].temp)? {
        Value::String(text) => text,
        _ => return Err(ExecutionError::new(4012, "files.write.str 需要字符串".into())),
    };
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
        }
    }
    std::fs::write(&path, text.as_bytes()).map_err(|e| ExecutionError::new(3003, e.to_string()))?;
    Ok(Value::String(text))
}

fn base64_encode(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let bytes = match exec.get(&args[0].temp)? {
        Value::Bytes(bytes) => bytes,
        _ => return Err(ExecutionError::new(4012, "base64.encode 只接受字节".into())),
    };
    Ok(Value::String(b64_encode(&bytes)))
}

fn base64_decode(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    need(args, 1)?;
    let text = match exec.get(&args[0].temp)? {
        Value::String(text) => text,
        _ => return Err(ExecutionError::new(4012, "base64.decode 只接受字符串".into())),
    };
    Ok(Value::Bytes(Arc::new(b64_decode(&text)?)))
}

fn b64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 { out.push(T[((n >> 6) & 63) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(T[(n & 63) as usize] as char); } else { out.push('='); }
    }
    out
}

fn b64_decode(text: &str) -> Result<Vec<u8>, ExecutionError> {
    fn val(c: u8) -> Result<u8, ExecutionError> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(ExecutionError::new(4016, "base64 含有非法字符".into())),
        }
    }
    let raw: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if raw.len() % 4 != 0 {
        return Err(ExecutionError::new(4016, "base64 长度不正确".into()));
    }
    let mut out = Vec::new();
    for chunk in raw.chunks(4) {
        let pad = chunk.iter().filter(|b| **b == b'=').count();
        if pad > 2 || chunk[..4 - pad].contains(&b'=') {
            return Err(ExecutionError::new(4016, "base64 含有非法字符".into()));
        }
        let n = (val(chunk[0])? as u32) << 18
            | (val(if pad < 3 { chunk[1] } else { b'A' })? as u32) << 12
            | (val(if pad < 2 { chunk[2] } else { b'A' })? as u32) << 6
            | val(if pad < 1 { chunk[3] } else { b'A' })? as u32;
        out.push((n >> 16) as u8);
        if pad < 2 { out.push((n >> 8) as u8); }
        if pad < 1 { out.push(n as u8); }
    }
    Ok(out)
}
