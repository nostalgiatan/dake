use super::builtin::{content_type_for, decode_body, encode_body};
use super::executor::{ExecutionError, Executor};
use crate::dsl::ast::Value;
use crate::dsl::ir::{CallArg, ResolvedPath};
use rustls::pki_types::{ServerName, CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub(in crate::executor) struct Target {
    pub base: String,
    pub headers: Vec<(String, String)>,
    pub ca: Option<String>,
    pub cert: Option<String>,
    pub key: Option<String>,
    pub timeout: u64,
}

pub(super) fn set_url(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    if args.is_empty() || args[0].name.is_some() {
        return Err(ExecutionError::new(4030, "net.url 需要基址".into()));
    }
    let base = exec.arg_string(args, 0)?;
    parse_url(&base)?;
    let headers = header_pairs(exec, args)?;
    let ca = optional_string(exec, args, "ca")?;
    let cert = optional_string(exec, args, "cert")?;
    let key = optional_string(exec, args, "key")?;
    if cert.is_some() != key.is_some() {
        return Err(ExecutionError::new(4032, "cert 与 key 必须同时给出".into()));
    }
    let timeout = match named(args, "timeout") {
        Some(arg) => match exec.get(&arg.temp)? {
            Value::Number(n) if n > 0 => n as u64,
            _ => return Err(ExecutionError::new(4034, "timeout 必须是正整数".into())),
        },
        None => 30,
    };
    let target = Target { base, headers, ca, cert, key, timeout };
    if let Some(arg) = named(args, "name") {
        let name = match exec.get(&arg.temp)? {
            Value::String(name) if !name.is_empty() => name,
            _ => return Err(ExecutionError::new(4030, "name 必须是非空字符串".into())),
        };
        exec.net_named.insert(name, target.clone());
    }
    exec.net_current = Some(target.clone());
    Ok(Value::String(target.base))
}

pub(super) fn get(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    let (address, target, struct_id) = resolve_call(exec, args, false)?;
    let body = exchange(exec, &target, &address, "GET", Vec::new(), None)?;
    match struct_id {
        Some(id) => decode_body(exec, &id, body),
        None => Ok(Value::Bytes(Arc::new(body))),
    }
}

pub(super) fn accept(exec: &mut Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    if args.iter().filter(|arg| arg.name.is_none()).count() != 4 {
        return Err(ExecutionError::new(4011, "net.accept 需要地址、结构和行为".into()));
    }
    let address = exec.arg_string(args, 0)?;
    let struct_id = exec.arg_string(args, 1)?;
    let file = exec.arg_string(args, 2)?;
    let action = exec.arg_string(args, 3)?;
    let (host, port) = listen_addr(&address)?;
    let listener = TcpListener::bind(format!("{host}:{port}")).map_err(|err| ExecutionError::new(4031, format!("监听 {address} 失败: {err}")))?;
    let cert = named(args, "cert").map(|arg| exec.arg_value_string(&arg.temp)).transpose()?;
    let key = named(args, "key").map(|arg| exec.arg_value_string(&arg.temp)).transpose()?;
    let tls = match (cert, key) {
        (Some(cert), Some(key)) => Some(super::serve::server_config(&super::paths::safe_path(&cert)?, &super::paths::safe_path(&key)?)?),
        (None, None) => None,
        _ => return Err(ExecutionError::new(4032, "cert 与 key 必须同时给出".into())),
    };
    let (mut stream, _) = listener.accept().map_err(|err| ExecutionError::new(4031, format!("接受连接失败: {err}")))?;
    stream.set_read_timeout(Some(Duration::from_secs(30))).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    stream.set_write_timeout(Some(Duration::from_secs(30))).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    if let Some(config) = tls {
        let mut session = rustls::ServerConnection::new(config).map_err(|err| ExecutionError::new(4032, err.to_string()))?;
        let mut tls_stream = rustls::Stream::new(&mut session, &mut stream);
        return finish_accept(exec, &struct_id, &file, &action, &mut tls_stream);
    }
    finish_accept(exec, &struct_id, &file, &action, &mut stream)
}

fn finish_accept(exec: &mut Executor, struct_id: &str, file: &str, action: &str, stream: &mut (impl Read + Write)) -> Result<Value, ExecutionError> {
    let raw = read_raw(stream)?;
    let header_end = raw.windows(4).position(|window| window == b"\r\n\r\n").ok_or_else(|| ExecutionError::new(4031, "请求不是 HTTP".into()))?;
    let head = String::from_utf8_lossy(&raw[..header_end]);
    let body = raw[header_end + 4..].to_vec();
    let record = decode_body(exec, struct_id, body)?;
    let headers = header_object(&head);
    let path = ResolvedPath::User { file: std::path::PathBuf::from(file), name: action.to_string() };
    let mut values = vec![record];
    if exec.param_count(&path)? == 2 {
        values.push(headers);
    }
    let reply = exec.call_action(&path, values)?;
    let (status, extra) = exec.take_http_meta()?;
    let (payload, content_type) = match &reply {
        Value::Bytes(bytes) => (Arc::unwrap_or_clone(bytes.clone()), "application/octet-stream".to_string()),
        Value::Record { path, fields } => (encode_body(exec, path, fields.as_slice())?, content_type_for(exec, path)?.to_string()),
        _ => return Err(ExecutionError::new(4034, "响应需要记录或字节".into())),
    };
    super::serve::reply(stream, status, &payload, &content_type, &extra)?;
    Ok(reply)
}

pub(super) fn header_object(head: &str) -> Value {
    let mut fields = Vec::new();
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        fields.push((name.to_string(), Value::String(value.trim().to_string())));
    }
    Value::Object(Arc::new(fields))
}

pub(super) fn listen_addr(address: &str) -> Result<(String, u16), ExecutionError> {
    if address.contains("://") {
        return Err(ExecutionError::new(4030, format!("地址必须是 主机:端口: {address}")));
    }
    let (host, port) = address.rsplit_once(':').ok_or_else(|| ExecutionError::new(4030, format!("地址必须是 主机:端口: {address}")))?;
    if host.is_empty() {
        return Err(ExecutionError::new(4030, format!("地址必须是 主机:端口: {address}")));
    }
    let port: u16 = port.parse().map_err(|_| ExecutionError::new(4030, format!("地址必须是 主机:端口: {address}")))?;
    if port == 0 {
        return Err(ExecutionError::new(4030, format!("地址必须是 主机:端口: {address}")));
    }
    Ok((host.to_string(), port))
}

pub(super) fn post(exec: &Executor, args: &[CallArg]) -> Result<Value, ExecutionError> {
    let (address, target, struct_id) = resolve_call(exec, args, true)?;
    let positional: Vec<_> = args.iter().filter(|arg| arg.name.is_none()).collect();
    let body_arg = if positional.len() == 2 { positional[1] } else { positional[0] };
    let (payload, content_type) = match exec.get(&body_arg.temp)? {
        Value::Bytes(bytes) => (Arc::unwrap_or_clone(bytes), "application/octet-stream"),
        Value::String(text) => (text.into_bytes(), "text/plain; charset=utf-8"),
        Value::Record { path, fields } => {
            let bytes = encode_body(exec, &path, fields.as_slice())?;
            (bytes, content_type_for(exec, &path)?)
        }
        _ => return Err(ExecutionError::new(4034, "请求体需要字节、字符串或记录".into())),
    };
    let body = exchange(exec, &target, &address, "POST", payload, Some(content_type))?;
    match struct_id {
        Some(id) => decode_body(exec, &id, body),
        None => Ok(Value::Bytes(Arc::new(body))),
    }
}

fn resolve_call(exec: &Executor, args: &[CallArg], post: bool) -> Result<(String, Target, Option<String>), ExecutionError> {
    let mut target = if let Some(arg) = named(args, "url") {
        let text = match exec.get(&arg.temp)? {
            Value::String(text) => text,
            _ => return Err(ExecutionError::new(4030, "url 必须是字符串".into())),
        };
        if text.starts_with("http://") || text.starts_with("https://") {
            let mut current = exec.net_current.clone().unwrap_or_else(default_target);
            current.base = text;
            current
        } else {
            exec.net_named.get(&text).cloned().ok_or_else(|| ExecutionError::new(4030, format!("没有网络目标 {text}")))?
        }
    } else {
        exec.net_current.clone().ok_or_else(|| ExecutionError::new(4030, "还没有 net.url".into()))?
    };
    let extra = header_pairs(exec, args)?;
    merge_headers(&mut target.headers, extra);
    let positional: Vec<_> = args.iter().filter(|arg| arg.name.is_none()).collect();
    let address = if post {
        match positional.len() {
            1 => target.base.clone(),
            2 => {
                let path = exec.arg_value_string(&positional[0].temp)?;
                if path.starts_with("http://") || path.starts_with("https://") {
                    path
                } else if path.starts_with('/') {
                    join_url(&target.base, &path)?
                } else {
                    return Err(ExecutionError::new(4030, format!("路径必须以 / 开头: {path}")));
                }
            }
            _ => return Err(ExecutionError::new(4030, "net.post 的参数不符".into())),
        }
    } else {
        match positional.len() {
            0 => target.base.clone(),
            1 => {
                let path = exec.arg_value_string(&positional[0].temp)?;
                if path.starts_with("http://") || path.starts_with("https://") {
                    path
                } else if path.starts_with('/') {
                    join_url(&target.base, &path)?
                } else {
                    return Err(ExecutionError::new(4030, format!("路径必须以 / 开头: {path}")));
                }
            }
            _ => return Err(ExecutionError::new(4030, "net.get 的参数不符".into())),
        }
    };
    parse_url(&address)?;
    let struct_id = named(args, "struct").map(|arg| exec.arg_value_string(&arg.temp)).transpose()?;
    Ok((address, target, struct_id))
}

fn default_target() -> Target {
    Target { base: String::new(), headers: Vec::new(), ca: None, cert: None, key: None, timeout: 30 }
}

fn join_url(base: &str, path: &str) -> Result<String, ExecutionError> {
    let parts = parse_url(base)?;
    let prefix = parts.path.trim_end_matches('/');
    let joined = if prefix.is_empty() { path.to_string() } else { format!("{prefix}{path}") };
    Ok(format!("{}://{}:{}{joined}", parts.scheme, parts.host, parts.port))
}

pub(super) struct Parts {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

pub(super) fn parse_url(raw: &str) -> Result<Parts, ExecutionError> {
    let (scheme, rest) = if let Some(rest) = raw.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err(ExecutionError::new(4030, format!("地址必须是 http 或 https: {raw}")));
    };
    if rest.is_empty() {
        return Err(ExecutionError::new(4030, "地址缺少主机".into()));
    }
    let (hostport, path) = match rest.find('/') {
        Some(index) => (&rest[..index], rest[index..].to_string()),
        None => (rest, "/".into()),
    };
    let hostport = hostport.split('?').next().unwrap_or(hostport);
    let (host, port) = if let Some((host, port)) = hostport.rsplit_once(':') {
        if host.is_empty() {
            return Err(ExecutionError::new(4030, "地址缺少主机".into()));
        }
        let port: u16 = port.parse().map_err(|_| ExecutionError::new(4030, format!("端口无效: {port}")))?;
        (host.to_string(), port)
    } else {
        (hostport.to_string(), if scheme == "https" { 443 } else { 80 })
    };
    if host.is_empty() {
        return Err(ExecutionError::new(4030, "地址缺少主机".into()));
    }
    Ok(Parts { scheme: scheme.into(), host, port, path })
}

fn header_pairs(exec: &Executor, args: &[CallArg]) -> Result<Vec<(String, String)>, ExecutionError> {
    let Some(arg) = named(args, "headers") else {
        return Ok(Vec::new());
    };
    let Value::Record { fields, .. } = exec.get(&arg.temp)? else {
        return Err(ExecutionError::new(4034, "headers 需要记录".into()));
    };
    let mut pairs = Vec::new();
    for (name, value) in fields.iter() {
        let Value::String(text) = value else {
            return Err(ExecutionError::new(4034, format!("请求头 {name} 必须是字符串")));
        };
        pairs.push((wire_name(name), text.clone()));
    }
    Ok(pairs)
}

fn wire_name(field: &str) -> String {
    field.split('_').filter(|part| !part.is_empty()).map(|part| {
        let mut chars = part.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join("-")
}

fn merge_headers(base: &mut Vec<(String, String)>, extra: Vec<(String, String)>) {
    for (name, value) in extra {
        if let Some(slot) = base.iter_mut().find(|(existing, _)| existing.eq_ignore_ascii_case(&name)) {
            slot.1 = value;
        } else {
            base.push((name, value));
        }
    }
}

fn named<'a>(args: &'a [CallArg], name: &str) -> Option<&'a CallArg> {
    args.iter().find(|arg| arg.name.as_deref() == Some(name))
}

fn optional_string(exec: &Executor, args: &[CallArg], name: &str) -> Result<Option<String>, ExecutionError> {
    let Some(arg) = named(args, name) else {
        return Ok(None);
    };
    match exec.get(&arg.temp)? {
        Value::String(text) if !text.is_empty() => Ok(Some(text)),
        _ => Err(ExecutionError::new(4032, format!("{name} 必须是非空字符串"))),
    }
}

fn exchange(exec: &Executor, target: &Target, address: &str, method: &str, payload: Vec<u8>, content_type: Option<&str>) -> Result<Vec<u8>, ExecutionError> {
    let _ = exec;
    let parts = parse_url(address)?;
    let timeout = Duration::from_secs(target.timeout);
    let socket = format!("{}:{}", parts.host, parts.port);
    let addr = socket.to_socket_addrs().map_err(|err| ExecutionError::new(4031, format!("{socket}: {err}")))?.next()
        .ok_or_else(|| ExecutionError::new(4031, format!("无法解析 {socket}")))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|err| ExecutionError::new(4031, format!("连接 {socket} 失败: {err}")))?;
    stream.set_read_timeout(Some(timeout)).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    stream.set_write_timeout(Some(timeout)).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    let mut headers = target.headers.clone();
    if let Some(content_type) = content_type {
        if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("Content-Type")) {
            headers.push(("Content-Type".into(), content_type.into()));
        }
    }
    let mut request = format!("{method} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {}\r\n", parts.path, parts.host, parts.port, payload.len());
    for (name, value) in &headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    let request = request.into_bytes();
    if parts.scheme == "https" {
        let config = tls_config(target)?;
        let name = ServerName::try_from(parts.host.clone()).map_err(|_| ExecutionError::new(4032, format!("主机名无效: {}", parts.host)))?;
        let mut tls = rustls::ClientConnection::new(Arc::new(config), name).map_err(|err| ExecutionError::new(4032, err.to_string()))?;
        let mut tls_stream = rustls::Stream::new(&mut tls, &mut stream);
        write_all(&mut tls_stream, &request, &payload)?;
        read_response(&mut tls_stream)
    } else {
        write_all(&mut stream, &request, &payload)?;
        read_response(&mut stream)
    }
}

fn io_code(err: &std::io::Error) -> u32 {
    let text = err.to_string();
    if text.contains("certificate") || text.contains("cert") || text.contains("TLS") || text.contains("tls") {
        4032
    } else {
        4031
    }
}

fn write_all(stream: &mut impl Write, head: &[u8], body: &[u8]) -> Result<(), ExecutionError> {
    stream.write_all(head).map_err(|err| ExecutionError::new(io_code(&err), err.to_string()))?;
    stream.write_all(body).map_err(|err| ExecutionError::new(io_code(&err), err.to_string()))?;
    stream.flush().map_err(|err| ExecutionError::new(io_code(&err), err.to_string()))?;
    Ok(())
}

pub(super) fn read_raw(stream: &mut impl Read) -> Result<Vec<u8>, ExecutionError> {
    let mut raw = Vec::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        if response_complete(&raw) {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) if response_complete(&raw) => break,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut => {
                return Err(ExecutionError::new(4031, "读取请求超时".into()));
            }
            Err(err) => return Err(ExecutionError::new(4031, err.to_string())),
        }
    }
    if !response_complete(&raw) {
        return Err(ExecutionError::new(4031, "请求不是 HTTP".into()));
    }
    Ok(raw)
}

fn response_complete(raw: &[u8]) -> bool {
    let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&raw[..index]);
    let length = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok())?
    });
    match length {
        Some(length) => raw.len() - (index + 4) >= length,
        None => false,
    }
}

fn read_response(stream: &mut impl Read) -> Result<Vec<u8>, ExecutionError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if response_complete(&raw) {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) if response_complete(&raw) => break,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut => {
                if raw.is_empty() {
                    return Err(ExecutionError::new(4031, "读取响应超时".into()));
                }
                break;
            }
            Err(err) => return Err(ExecutionError::new(io_code(&err), err.to_string())),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, body_at) = text.split_once("\r\n\r\n").ok_or_else(|| ExecutionError::new(4031, "响应不是 HTTP".into()))?;
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line.split_whitespace().nth(1).and_then(|item| item.parse().ok()).unwrap_or(0);
    if !(200..300).contains(&status) {
        return Err(ExecutionError::new(4033, format!("HTTP 状态 {status}")));
    }
    let header_len = raw.windows(4).position(|window| window == b"\r\n\r\n").unwrap_or(0) + 4;
    let mut body = raw.get(header_len..).unwrap_or_default().to_vec();
    let length = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("Content-Length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    });
    if let Some(length) = length {
        body.truncate(length);
    }
    let _ = body_at;
    Ok(body)
}

pub(super) fn tls_config(target: &Target) -> Result<ClientConfig, ExecutionError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    if let Some(path) = &target.ca {
        let bytes = std::fs::read(path).map_err(|err| ExecutionError::new(4032, format!("{path}: {err}")))?;
        let mut cursor = std::io::Cursor::new(bytes);
        let certs = rustls_pemfile::certs(&mut cursor).collect::<Result<Vec<_>, _>>()
            .map_err(|err| ExecutionError::new(4032, format!("ca 不是合法的 PEM: {err}")))?;
        if certs.is_empty() {
            return Err(ExecutionError::new(4032, "ca 里没有证书".into()));
        }
        for cert in certs {
            roots.add(cert).map_err(|err| ExecutionError::new(4032, err.to_string()))?;
        }
    } else {
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            return Err(ExecutionError::new(4032, "系统里没有可用的根证书".into()));
        }
    }
    let builder = ClientConfig::builder().with_root_certificates(roots);
    match (&target.cert, &target.key) {
        (Some(cert_path), Some(key_path)) => {
            let cert_bytes = std::fs::read(cert_path).map_err(|err| ExecutionError::new(4032, format!("{cert_path}: {err}")))?;
            let key_bytes = std::fs::read(key_path).map_err(|err| ExecutionError::new(4032, format!("{key_path}: {err}")))?;
            let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| ExecutionError::new(4032, format!("cert 不是合法的 PEM: {err}")))?;
            let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_bytes))
                .map_err(|err| ExecutionError::new(4032, format!("key 不是合法的 PEM: {err}")))?
                .ok_or_else(|| ExecutionError::new(4032, "key 里没有私钥".into()))?;
            builder.with_client_auth_cert(certs, key).map_err(|err| ExecutionError::new(4032, err.to_string()))
        }
        (None, None) => Ok(builder.with_no_client_auth()),
        _ => Err(ExecutionError::new(4032, "cert 与 key 必须同时给出".into())),
    }
}

pub(super) fn http_call(exec: &Executor, method: &str, url: &str, upload: Option<&str>, save: Option<&str>) -> Result<(u16, String), ExecutionError> {
    let target = exec.net_current.clone().unwrap_or_else(default_target);
    let parts = parse_url(url)?;
    let timeout = Duration::from_secs(if target.timeout == 0 { 30 } else { target.timeout });
    let socket = format!("{}:{}", parts.host, parts.port);
    let addr = socket.to_socket_addrs().map_err(|err| ExecutionError::new(4031, format!("{socket}: {err}")))?.next()
        .ok_or_else(|| ExecutionError::new(4031, format!("无法解析 {socket}")))?;
    let mut tcp = TcpStream::connect_timeout(&addr, timeout).map_err(|err| ExecutionError::new(4031, format!("连接 {socket} 失败: {err}")))?;
    tcp.set_read_timeout(Some(timeout)).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    tcp.set_write_timeout(Some(timeout)).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    let length = match upload {
        Some(path) => std::fs::metadata(path).map_err(|err| ExecutionError::new(3002, err.to_string()))?.len(),
        None => 0,
    };
    let request = format!("{method} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Length: {length}\r\n\r\n", parts.path, parts.host, parts.port);
    if parts.scheme == "https" {
        let config = tls_config(&target)?;
        let name = ServerName::try_from(parts.host.clone()).map_err(|_| ExecutionError::new(4032, format!("主机名无效: {}", parts.host)))?;
        let mut tls = rustls::ClientConnection::new(Arc::new(config), name).map_err(|err| ExecutionError::new(4032, err.to_string()))?;
        let mut tls_stream = rustls::Stream::new(&mut tls, &mut tcp);
        send_and_read(&mut tls_stream, request.as_bytes(), upload, length, save)
    } else {
        send_and_read(&mut tcp, request.as_bytes(), upload, length, save)
    }
}

fn send_and_read(stream: &mut (impl Read + Write), head: &[u8], upload: Option<&str>, length: u64, save: Option<&str>) -> Result<(u16, String), ExecutionError> {
    stream.write_all(head).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    if let Some(path) = upload {
        let mut file = std::fs::File::open(path).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
        let mut left = length;
        let mut buf = vec![0u8; 1024 * 1024];
        while left > 0 {
            let n = file.read(&mut buf).map_err(|err| ExecutionError::new(3002, err.to_string()))?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
            left -= n as u64;
        }
    }
    stream.flush().map_err(|err| ExecutionError::new(4031, err.to_string()))?;
    let (status, head, rest) = read_status(stream)?;
    let size = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<u64>().ok())?
    }).unwrap_or(0);
    if let Some(path) = save {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|err| ExecutionError::new(3003, err.to_string()))?;
        }
        let mut chained = std::io::Cursor::new(rest).chain(stream);
        super::store::write_stream(path, &mut chained, size)?;
        Ok((status, String::new()))
    } else {
        let mut body = rest;
        let mut buf = vec![0u8; 8192];
        while (body.len() as u64) < size {
            let n = stream.read(&mut buf).map_err(|err| ExecutionError::new(4031, err.to_string()))?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
        }
        body.truncate(size as usize);
        Ok((status, String::from_utf8_lossy(&body).into_owned()))
    }
}

fn read_status(stream: &mut impl Read) -> Result<(u16, String, Vec<u8>), ExecutionError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if let Some(index) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..index]).into_owned();
            let status = head.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("0").parse::<u16>().unwrap_or(0);
            return Ok((status, head, raw[index + 4..].to_vec()));
        }
        match stream.read(&mut buf) {
            Ok(0) => return Err(ExecutionError::new(4031, "响应不是 HTTP".into())),
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(err) => return Err(ExecutionError::new(4031, err.to_string())),
        }
    }
}
