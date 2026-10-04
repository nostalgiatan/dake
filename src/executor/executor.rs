/*
 * 执行中间表示。
 * 打包、加密、压缩和校验仍调用原有模块。
 */

use crate::data::{Compressor, RegexCache, Validator};
use crate::dsl::ast::{LibDefinition, RepoDefinition, UnaryOp, Value};
use crate::dsl::ir::{ActionIr, CallArg, Inst, Program, ResolvedPath};
use crate::executor::context::ExecutionContext;
use crate::executor::crypto::CryptoOperations;
use crate::executor::file_ops::FileOperations;
use crate::executor::package::{self, PackageBuild};
use error::{ErrorCategory, ErrorInfo, ErrorKind, ErrorSeverity};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct ExecutionError {
    info: ErrorInfo,
}

impl ExecutionError {
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message.clone())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error)
                .with_hint(crate::dsl::diagnose::hint(code, &message)),
        }
    }

    fn at(self, file: &str, line: u32, column: u32) -> Self {
        let code = self.info.error_code();
        let message = self.info.error_message();
        Self {
            info: ErrorInfo::new(code, message.clone())
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error)
                .with_hint(crate::dsl::diagnose::hint(code, &message))
                .with_place(file, line, column),
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for ExecutionError {}

impl ExecutionError {
    pub(in crate::executor) fn code(&self) -> u32 {
        self.info.error_code()
    }

    pub(in crate::executor) fn explain(&self) -> String {
        self.info.error_message().to_string()
    }
}

pub struct Executor {
    pub(in crate::executor) context: ExecutionContext,
    slots: HashMap<String, Value>,
    actions: Arc<HashMap<ResolvedPath, Arc<ActionIr>>>,
    errors: Arc<HashMap<String, String>>,
    pub(in crate::executor) structs: Arc<HashMap<String, crate::dsl::ir::StructIr>>,
    pub(in crate::executor) output_buffer: Vec<String>,
    lib: Option<LibDefinition>,
    pub(in crate::executor) repo: Option<RepoDefinition>,
    pub(in crate::executor) repo_lock: Arc<Mutex<()>>,
    pub(in crate::executor) package_files: Vec<String>,
    pub(in crate::executor) packed: Vec<package::PackedFile>,
    pub(in crate::executor) crypto: Option<CryptoOperations>,
    pub(in crate::executor) regex_cache: RegexCache,
    pub(in crate::executor) validator: Validator,
    pub(in crate::executor) compressor: Compressor,
    pub(in crate::executor) log_dir: Option<String>,
    pub(in crate::executor) log_print: bool,
    pub(in crate::executor) net_current: Option<crate::executor::net::Target>,
    pub(in crate::executor) net_named: HashMap<String, crate::executor::net::Target>,
    each_depth: u32,
    stop_each: bool,
    pub(in crate::executor) halt: bool,
    pub(in crate::executor) serve_depth: u32,
    pub(in crate::executor) serve_urls: HashMap<String, crate::executor::serve::ServeUrl>,
    pub(in crate::executor) depot_url: Option<String>,
    pub(in crate::executor) serve_dirs: HashMap<String, crate::executor::serve::DirSource>,
    pub(in crate::executor) shared: Arc<Mutex<HashMap<String, Value>>>,
    source_file: String,
    line: u32,
    column: u32,
}

impl Executor {
    pub fn new() -> Self {
        let regex_cache = RegexCache::new();
        Self {
            context: ExecutionContext::new(),
            slots: HashMap::new(),
            actions: Arc::new(HashMap::new()),
            errors: Arc::new(HashMap::new()),
            structs: Arc::new(HashMap::new()),
            output_buffer: Vec::new(),
            lib: None,
            repo: None,
            repo_lock: Arc::new(Mutex::new(())),
            package_files: Vec::new(),
            packed: Vec::new(),
            crypto: None,
            regex_cache: regex_cache.clone(),
            validator: Validator::with_cache(regex_cache),
            compressor: Compressor::new(),
            log_dir: None,
            log_print: false,
            net_current: None,
            net_named: HashMap::new(),
            each_depth: 0,
            stop_each: false,
            halt: false,
            serve_depth: 0,
            serve_urls: HashMap::new(),
            depot_url: None,
            serve_dirs: HashMap::new(),
            shared: Arc::new(Mutex::new(HashMap::new())),
            source_file: String::new(),
            line: 0,
            column: 0,
        }
    }

    #[allow(dead_code)]
    pub fn output(&self) -> &[String] {
        &self.output_buffer
    }

    pub fn execute_ir(&mut self, program: &Program) -> Result<(), ExecutionError> {
        self.actions = Arc::new(
            program.actions.iter().map(|(path, action)| (path.clone(), Arc::new(action.clone()))).collect(),
        );
        self.errors = Arc::new(program.errors.clone());
        self.structs = Arc::new(program.structs.clone());
        self.source_file = program.entry.display().to_string();
        self.run(&program.insts)
    }

    fn run(&mut self, insts: &[Inst]) -> Result<(), ExecutionError> {
        for inst in insts {
            self.step(inst).map_err(|err| {
                if self.line == 0 {
                    err
                } else {
                    err.at(&self.source_file, self.line, self.column)
                }
            })?;
            if self.stop_each || self.halt {
                return Ok(());
            }
        }
        Ok(())
    }

    fn step(&mut self, inst: &Inst) -> Result<(), ExecutionError> {
        match inst {
            Inst::Const { dest, value } => self.put(dest, value.clone()),
            Inst::Load { dest, name } => {
                let value = self.get(name)?;
                self.put(dest, value);
            }
            Inst::Format { dest, template } => {
                self.put(dest, Value::String(self.context.interpolate(template)));
            }
            Inst::Binary { dest, op, left, right } => {
                let value = super::ops::binary(op, self.get(left)?, self.get(right)?)?;
                self.put(dest, value);
            }
            Inst::Unary { dest, op, expr } => {
                let value = self.get(expr)?;
                let out = match op {
                    UnaryOp::Not => Value::Bool(!super::ops::truth(&value)),
                };
                self.put(dest, out);
            }
            Inst::CastNum { dest, src } => {
                let value = self.get(src)?;
                let n = match value {
                    Value::Number(n) => n,
                    Value::Float(n) if n.fract() == 0.0 && n.is_finite() => n as i64,
                    Value::Float(_) => return Err(ExecutionError::new(4010, "小数不能转成整数".into())),
                    Value::String(s) => s.parse::<i64>().map_err(|_| {
                        ExecutionError::new(4010, format!("无法把 {s} 转成数字"))
                    })?,
                    Value::Bool(b) => i64::from(b),
                    Value::List(_) => return Err(ExecutionError::new(4010, "列表不能转成数字".into())),
                    Value::Bytes(_) => return Err(ExecutionError::new(4010, "字节不能转成数字".into())),
                    Value::Record { .. } => return Err(ExecutionError::new(4010, "记录不能转成数字".into())),
                    Value::Object(_) => return Err(ExecutionError::new(4010, "对象不能转成数字".into())),
                };
                self.put(dest, Value::Number(n));
            }
            Inst::CastStr { dest, src } => {
                let value = self.get(src)?;
                if matches!(value, Value::Bytes(_)) {
                    return Err(ExecutionError::new(4010, "字节不能用 str 转换，请使用 base64.encode".into()));
                }
                if matches!(value, Value::Record { .. } | Value::Object(_)) {
                    return Err(ExecutionError::new(4010, "记录和对象不能用 str 转换".into()));
                }
                self.put(dest, Value::String(value.to_string()));
            }
            Inst::Call { path, args, dest } => {
                let value = self.call(path, args)?;
                if let Some(dest) = dest {
                    self.put(dest, value);
                }
            }
            Inst::Branch { cond, then_body, elifs, else_body } => {
                if super::ops::truth(&self.get(cond)?) {
                    self.run(then_body)?;
                } else {
                    let mut taken = false;
                    for (elif_cond, body) in elifs {
                        if super::ops::truth(&self.get(elif_cond)?) {
                            self.run(body)?;
                            taken = true;
                            break;
                        }
                    }
                    if !taken {
                        self.run(else_body)?;
                    }
                }
            }
            Inst::Here { line, column } => {
                self.line = *line;
                self.column = *column;
            }
            Inst::Stop => {
                if self.serve_depth > 0 {
                    self.halt = true;
                    return Ok(());
                }
                if self.each_depth == 0 {
                    return Err(ExecutionError::new(4019, "stop 不在 each 中".into()));
                }
                self.stop_each = true;
            }
            Inst::Each { var, index, source, body } => {
                let items = if let Some(source) = source {
                    match self.get(source)? {
                        Value::List(items) => items,
                        _ => return Err(ExecutionError::new(4019, "each 的来源必须是列表".into())),
                    }
                } else {
                    Arc::new(self.package_files.iter().cloned().map(Value::String).collect::<Vec<_>>())
                };
                self.each_depth += 1;
                for (i, item) in Arc::unwrap_or_clone(items).into_iter().enumerate() {
                    self.put(var, item);
                    if let Some(index) = index {
                        self.put(index, Value::Number(i as i64));
                    }
                    self.run(body)?;
                    if self.stop_each {
                        self.stop_each = false;
                        break;
                    }
                }
                self.each_depth -= 1;
            }
            Inst::Catch { error_name, var, body, fail } => {
                if let Some(error_name) = error_name {
                    let message = self.errors.get(error_name).cloned().ok_or_else(|| {
                        ExecutionError::new(4009, format!("未定义的错误 {error_name}"))
                    })?;
                    self.put(var, Value::String(message));
                } else {
                    self.put(var, Value::String(String::new()));
                }
                if let Err(err) = self.run(body) {
                    self.put(var, Value::String(err.to_string()));
                    self.run(fail)?;
                }
            }
            Inst::Doing(path) => self.doing(path)?,
            Inst::Await(paths) => self.await_paths(paths)?,
            Inst::Lib(lib) => {
                self.output_buffer.push(format!("定义数据包: {}", lib.name));
                self.lib = Some(lib.clone());
            }
            Inst::Repo(repo) => {
                self.output_buffer.push(format!("定义仓库: {}", repo.name));
                self.repo = Some(repo.clone());
            }
            Inst::Index { dest, base, index } => {
                match (self.get(base)?, self.get(index)?) {
                    (Value::List(items), Value::Number(index)) => {
                        if index < 0 || index as usize >= items.len() {
                            return Err(ExecutionError::new(4019, format!("下标越界: {index}")));
                        }
                        self.put(dest, items[index as usize].clone());
                    }
                    (Value::List(_), _) => return Err(ExecutionError::new(4012, "下标必须是整数".into())),
                    (Value::Object(fields), Value::String(key)) => {
                        let value = fields.iter().find(|(name, _)| name == &key).map(|(_, v)| v.clone())
                            .ok_or_else(|| ExecutionError::new(4017, format!("对象没有字段 {key}")))?;
                        self.put(dest, value);
                    }
                    (Value::Object(_), _) => return Err(ExecutionError::new(4012, "对象下标必须是字符串".into())),
                    _ => return Err(ExecutionError::new(4012, "下标只能用于列表或对象".into())),
                }
            }
            Inst::Slice { dest, base, start, end } => {
                let items = match self.get(base)? {
                    Value::List(items) => items,
                    _ => return Err(ExecutionError::new(4012, "切片只能用于列表".into())),
                };
                let start = match self.get(start)? {
                    Value::Number(n) => n,
                    _ => return Err(ExecutionError::new(4012, "切片边界必须是整数".into())),
                };
                let end = match self.get(end)? {
                    Value::Number(n) => n,
                    _ => return Err(ExecutionError::new(4012, "切片边界必须是整数".into())),
                };
                if start < 0 || end < start || end as usize > items.len() {
                    return Err(ExecutionError::new(4019, format!("切片越界: {start}:{end}")));
                }
                self.put(dest, Value::List(Arc::new(items[start as usize..end as usize].to_vec())));
            }
            Inst::GetField { dest, src, field } => {
                let (kind, value) = match self.get(src)? {
                    Value::Record { fields, .. } => ("记录", fields.iter().find(|(name, _)| name == field).map(|(_, v)| v.clone())),
                    Value::Object(fields) => ("对象", fields.iter().find(|(name, _)| name == field).map(|(_, v)| v.clone())),
                    _ => return Err(ExecutionError::new(4017, "字段读取需要记录或对象".into())),
                };
                let value = value.ok_or_else(|| ExecutionError::new(4017, format!("{kind}没有字段 {field}")))?;
                self.put(dest, value);
            }
            Inst::Print(temp) => {
                let value = self.get(temp)?;
                if matches!(value, Value::Bytes(_)) {
                    return Err(ExecutionError::new(4014, "字节不能直接打印，请先 base64.encode".into()));
                }
                let text = value.to_string();
                println!("{text}");
                self.output_buffer.push(text);
            }
            Inst::Url { name, address, cert, key } => {
                self.serve_urls.insert(name.clone(), crate::executor::serve::ServeUrl {
                    address: address.clone(),
                    cert: cert.clone(),
                    key: key.clone(),
                });
            }
            Inst::Dir { name, path, suffix, deep, exclude } => {
                self.serve_dirs.insert(name.clone(), crate::executor::serve::DirSource {
                    path: path.clone(),
                    suffix: suffix.clone(),
                    deep: *deep,
                    exclude: exclude.clone(),
                });
            }
            Inst::Share { name } => {
                let value = self.get(name)?;
                self.shared.lock().expect("共享锁").insert(name.clone(), value);
            }
            Inst::Serve { workers, routes, repo } => super::serve::run(self, routes, *workers, repo.clone())?,
            Inst::Package => self.materialize()?,
        }
        Ok(())
    }

    fn doing(&mut self, path: &ResolvedPath) -> Result<(), ExecutionError> {
        let action = self.actions.get(path).cloned().ok_or_else(|| {
            ExecutionError::new(4005, format!("未找到行为或管道: {path}"))
        })?;
        self.invoke(&action, &[])
    }

    pub(in crate::executor) fn fork(&self) -> Executor {
        let mut worker = Executor::new();
        worker.context = self.context.clone();
        worker.actions = Arc::clone(&self.actions);
        worker.errors = Arc::clone(&self.errors);
        worker.structs = Arc::clone(&self.structs);
        worker.lib = self.lib.clone();
        worker.repo = self.repo.clone();
        worker.repo_lock = Arc::clone(&self.repo_lock);
        worker.crypto = self.crypto.clone();
        worker.regex_cache = self.regex_cache.clone();
        worker.validator = Validator::with_cache(self.regex_cache.clone());
        worker.log_dir = self.log_dir.clone();
        worker.log_print = self.log_print;
        worker.net_current = self.net_current.clone();
        worker.net_named = self.net_named.clone();
        worker.source_file = self.source_file.clone();
        worker.shared = Arc::clone(&self.shared);
        worker.depot_url = self.depot_url.clone();
        worker
    }

    pub(in crate::executor) fn absorb(&mut self, worker: Executor) {
        self.output_buffer.extend(worker.output_buffer);
        self.package_files.extend(worker.package_files);
        self.packed.extend(worker.packed);
        if worker.halt {
            self.halt = true;
        }
    }

    fn await_paths(&mut self, paths: &[ResolvedPath]) -> Result<(), ExecutionError> {
        if paths.len() <= 1 {
            for path in paths {
                self.doing(path)?;
            }
            return Ok(());
        }
        let actions: Vec<Arc<ActionIr>> = paths
            .iter()
            .map(|path| {
                self.actions.get(path).cloned().ok_or_else(|| {
                    ExecutionError::new(4005, format!("未找到行为或管道: {path}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let context = self.context.clone();
        let shared_actions = Arc::clone(&self.actions);
        let shared_errors = Arc::clone(&self.errors);
        let shared_structs = Arc::clone(&self.structs);
        let net_current = self.net_current.clone();
        let net_named = self.net_named.clone();
        let crypto = self.crypto.clone();
        let log_dir = self.log_dir.clone();
        let log_print = self.log_print;
        let shared_map = Arc::clone(&self.shared);
        let outputs = Arc::new(Mutex::new(Vec::<String>::new()));
        let errors = Arc::new(Mutex::new(Vec::<String>::new()));
        let merged = Arc::new(Mutex::new(Vec::<(Vec<(String, Value)>, Vec<String>, Vec<package::PackedFile>)>::new()));
        smol::block_on(async {
            let mut tasks = Vec::new();
            for action in actions {
                let outputs = Arc::clone(&outputs);
                let errors = Arc::clone(&errors);
                let merged = Arc::clone(&merged);
                let context = context.clone();
                let shared_actions = Arc::clone(&shared_actions);
                let shared_errors = Arc::clone(&shared_errors);
                let shared_structs = Arc::clone(&shared_structs);
                let net_current = net_current.clone();
                let net_named = net_named.clone();
                let crypto = crypto.clone();
                let log_dir = log_dir.clone();
                let shared_map = Arc::clone(&shared_map);
                tasks.push(smol::unblock(move || {
                    let mut worker = Executor::new();
                    worker.context = context;
                    worker.actions = shared_actions;
                    worker.errors = shared_errors;
                    worker.structs = shared_structs;
                    worker.net_current = net_current;
                    worker.net_named = net_named;
                    worker.crypto = crypto;
                    worker.log_dir = log_dir;
                    worker.log_print = log_print;
                    worker.shared = Arc::clone(&shared_map);
                    match worker.invoke(&action, &[]) {
                        Ok(()) => {
                            for line in &worker.output_buffer {
                                outputs.lock().expect("输出锁").push(line.clone());
                            }
                            let mut vars = Vec::new();
                            for name in &action.writes {
                                if let Some(value) = worker.context.get(name) {
                                    vars.push((name.clone(), value));
                                }
                            }
                            merged.lock().expect("合并锁").push((vars, worker.package_files, worker.packed));
                        }
                        Err(err) => errors.lock().expect("错误锁").push(err.to_string()),
                    }
                }));
            }
            for task in tasks {
                task.await;
            }
        });
        for line in outputs.lock().expect("输出锁").iter() {
            self.output_buffer.push(line.clone());
        }
        for (vars, files, packed) in merged.lock().expect("合并锁").iter() {
            for (name, value) in vars {
                self.put(name, value.clone());
            }
            self.package_files.extend(files.clone());
            self.packed.extend(packed.clone());
        }
        let errs = errors.lock().expect("错误锁");
        if !errs.is_empty() {
            return Err(ExecutionError::new(4007, format!("await 执行失败: {errs:?}")));
        }
        Ok(())
    }

    pub(in crate::executor) fn call_named(&mut self, file: &str, name: &str, arg: Value) -> Result<Value, ExecutionError> {
        let path = ResolvedPath::User { file: std::path::PathBuf::from(file), name: name.to_string() };
        let action = self.actions.get(&path).cloned().ok_or_else(|| {
            ExecutionError::new(4005, format!("未找到行为: {name}"))
        })?;
        self.slots.remove("result");
        self.context.local_vars_remove("result");
        let temp = "#map".to_string();
        self.put(&temp, arg);
        self.invoke(&action, &[CallArg { name: None, temp }])?;
        self.slots.remove("#map");
        self.get("result").map_err(|_| ExecutionError::new(4011, format!("行为 {name} 没有 set(result)")))
    }

    pub(in crate::executor) fn call_named_drop_many(&mut self, file: &str, name: &str, args: Vec<Value>) -> Result<(), ExecutionError> {
        let path = ResolvedPath::User { file: std::path::PathBuf::from(file), name: name.to_string() };
        let action = self.actions.get(&path).cloned().ok_or_else(|| {
            ExecutionError::new(4005, format!("未找到行为: {name}"))
        })?;
        self.slots.remove("result");
        self.context.local_vars_remove("result");
        let temps: Vec<_> = (0..args.len()).map(|index| format!("#field{index}")).collect();
        for (temp, value) in temps.iter().zip(args) {
            self.put(temp, value);
        }
        let call_args: Vec<_> = temps.iter().map(|temp| CallArg { name: None, temp: temp.clone() }).collect();
        self.invoke(&action, &call_args)?;
        for temp in &temps {
            self.slots.remove(temp);
        }
        self.slots.remove("result");
        self.context.local_vars_remove("result");
        Ok(())
    }

    pub(in crate::executor) fn call_named_drop(&mut self, file: &str, name: &str, arg: Value) -> Result<(), ExecutionError> {
        self.call_named_drop_many(file, name, vec![arg])
    }

    pub(in crate::executor) fn param_count(&self, path: &ResolvedPath) -> Result<usize, ExecutionError> {
        self.actions.get(path).map(|action| action.params.len()).ok_or_else(|| {
            ExecutionError::new(4005, format!("未找到行为: {path}"))
        })
    }

    pub(in crate::executor) fn call_action(&mut self, path: &ResolvedPath, args: Vec<Value>) -> Result<Value, ExecutionError> {
        let action = self.actions.get(path).cloned().ok_or_else(|| {
            ExecutionError::new(4005, format!("未找到行为: {path}"))
        })?;
        for name in ["result", "status", "headers"] {
            self.slots.remove(name);
            self.context.local_vars_remove(name);
        }
        let temps: Vec<_> = (0..args.len()).map(|index| format!("#serve{index}")).collect();
        for (temp, value) in temps.iter().zip(args) {
            self.put(temp, value);
        }
        let call_args: Vec<_> = temps.iter().map(|temp| CallArg { name: None, temp: temp.clone() }).collect();
        self.invoke(&action, &call_args)?;
        for temp in &temps {
            self.slots.remove(temp);
        }
        let name = match path {
            ResolvedPath::User { name, .. } => name.as_str(),
            ResolvedPath::Builtin(name) => name.as_str(),
        };
        self.get("result").map_err(|_| ExecutionError::new(4011, format!("行为 {name} 没有 set(result)")))
    }

    pub(in crate::executor) fn take_http_meta(&mut self) -> Result<(u16, Vec<(String, String)>), ExecutionError> {
        let status = match self.slots.get("status") {
            None => 200,
            Some(Value::Number(code)) if *code >= 100 && *code < 600 => *code as u16,
            Some(_) => return Err(ExecutionError::new(4034, "status 需要 100 到 599 的整数".into())),
        };
        let headers = match self.slots.get("headers") {
            None => Vec::new(),
            Some(Value::Object(fields)) => {
                let mut out = Vec::new();
                for (name, value) in fields.iter() {
                    if name.is_empty() || name.bytes().any(|byte| byte == b'\r' || byte == b'\n' || byte == b':') {
                        return Err(ExecutionError::new(4034, format!("响应头名字不合法: {name}")));
                    }
                    let Value::String(text) = value else {
                        return Err(ExecutionError::new(4034, "响应头的值需要字符串".into()));
                    };
                    if text.bytes().any(|byte| byte == b'\r' || byte == b'\n') {
                        return Err(ExecutionError::new(4034, format!("响应头 {name} 的值不合法")));
                    }
                    out.push((name.clone(), text.clone()));
                }
                out
            }
            Some(_) => return Err(ExecutionError::new(4034, "headers 需要对象".into())),
        };
        for name in ["status", "headers"] {
            self.slots.remove(name);
            self.context.local_vars_remove(name);
        }
        Ok((status, headers))
    }

    fn invoke(&mut self, action: &ActionIr, args: &[CallArg]) -> Result<(), ExecutionError> {
        if args.len() != action.params.len() {
            return Err(ExecutionError::new(4011, format!(
                "参数数量不符: 期望 {} 得到 {}",
                action.params.len(),
                args.len()
            )));
        }
        for (param, arg) in action.params.iter().zip(args.iter()) {
            self.put(param, self.get(&arg.temp)?);
        }
        self.run(&action.body)
    }

    fn call(&mut self, path: &ResolvedPath, args: &[CallArg]) -> Result<Value, ExecutionError> {
        match path {
            ResolvedPath::Builtin(name) => self.builtin(name, args),
            ResolvedPath::User { .. } => {
                let action = self.actions.get(path).cloned().ok_or_else(|| {
                    ExecutionError::new(4005, format!("未找到行为: {path}"))
                })?;
                self.invoke(&action, args)?;
                Ok(self.get("result").unwrap_or(Value::Bool(true)))
            }
        }
    }

    fn builtin(&mut self, name: &str, args: &[CallArg]) -> Result<Value, ExecutionError> {
        super::builtin::dispatch(self, name, args)
    }


    pub(in crate::executor) fn has_lib(&self) -> bool {
        self.lib.is_some()
    }

    pub(in crate::executor) fn keep_file(&mut self, path: &str) -> Result<(), ExecutionError> {
        let check = super::paths::safe_path(path)?;
        if !FileOperations::is_file(&check) {
            return Err(ExecutionError::new(3002, format!("文件不存在: {path}")));
        }
        self.package_files.push(check);
        self.output_buffer.push(format!("  - {path}"));
        Ok(())
    }

    fn materialize(&mut self) -> Result<(), ExecutionError> {
        if let (Some(lib), Some(repo)) = (&self.lib, &self.repo) {
            if lib.repo.is_empty() {
                self.lib.as_mut().unwrap().repo = repo.name.clone();
            } else if lib.repo != repo.name {
                return Err(ExecutionError::new(4012, format!("lib 的 repo 是 {}，仓库名是 {}", lib.repo, repo.name)));
            }
        }
        let Some(lib) = self.lib.clone() else {
            return Ok(());
        };
        let out = package::materialize(PackageBuild {
            lib: &lib,
            repo: self.repo.as_ref(),
            files: &self.package_files,
            packed: &self.packed,
            crypto: self.crypto.as_ref(),
        })
        .map_err(|e| ExecutionError::new(3010, e.message))?;
        self.output_buffer.push(format!(
            "数据包已写入 {out}，文件 {} 个",
            self.package_files.len() + self.packed.len()
        ));
        if let Some(repo) = self.repo.clone() {
            let dest = super::store::admit(&self.repo_lock, &repo.dir, &out, repo.capacity, repo.max_pkgs)?;
            self.output_buffer.push(format!("已放入仓库 {dest}"));
        }
        Ok(())
    }

    pub(in crate::executor) fn write_log(&mut self, line: &str) -> Result<(), ExecutionError> {
        self.output_buffer.push(line.to_string());
        if self.log_print {
            println!("{line}");
        }
        if let Some(dir) = &self.log_dir {
            use std::io::Write;
            let path = format!("{dir}/dake.log");
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)
                .map_err(|e| ExecutionError::new(3004, e.to_string()))?;
            writeln!(file, "{line}").map_err(|e| ExecutionError::new(3004, e.to_string()))?;
        }
        Ok(())
    }

    fn put(&mut self, name: &str, value: Value) {
        if self.shared.lock().expect("共享锁").contains_key(name) {
            self.shared.lock().expect("共享锁").insert(name.to_string(), value);
            return;
        }
        if name.starts_with('#') {
            self.slots.insert(name.to_string(), value);
            return;
        }
        self.slots.insert(name.to_string(), value.clone());
        self.context.set_local(name.to_string(), value);
    }

    pub(in crate::executor) fn get(&self, name: &str) -> Result<Value, ExecutionError> {
        if let Some(value) = self.shared.lock().expect("共享锁").get(name) {
            return Ok(value.clone());
        }
        self.slots.get(name).cloned().or_else(|| self.context.get(name)).ok_or_else(|| {
            ExecutionError::new(4006, format!("变量未定义: {name}"))
        })
    }

    pub(in crate::executor) fn arg_string(&self, args: &[CallArg], index: usize) -> Result<String, ExecutionError> {
        let arg = args.get(index).ok_or_else(|| ExecutionError::new(4011, "缺少参数".into()))?;
        self.arg_value_string(&arg.temp)
    }

    pub(in crate::executor) fn arg_value_string(&self, temp: &str) -> Result<String, ExecutionError> {
        match self.get(temp)? {
            Value::String(s) => Ok(self.context.interpolate(&s)),
            Value::Bytes(_) => Err(ExecutionError::new(4012, "这里需要字符串，得到字节".into())),
            Value::Record { .. } => Err(ExecutionError::new(4012, "这里需要字符串，得到记录".into())),
            Value::Object(_) => Err(ExecutionError::new(4012, "这里需要字符串，得到对象".into())),
            other => Ok(other.to_string()),
        }
    }

    pub(in crate::executor) fn named_string_list(&self, args: &[CallArg], name: &str) -> Result<Vec<String>, ExecutionError> {
        let arg = args.iter().find(|arg| arg.name.as_deref() == Some(name)).ok_or_else(|| {
            ExecutionError::new(4011, format!("缺少参数 {name}"))
        })?;
        match self.get(&arg.temp)? {
            Value::List(items) => Arc::unwrap_or_clone(items).into_iter().map(|item| match item {
                Value::String(s) => Ok(s),
                other => Ok(other.to_string()),
            }).collect(),
            other => Err(ExecutionError::new(4011, format!("{name} 必须是列表，得到 {other}"))),
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::ir::lower;
    use crate::dsl::resolve::resolve_file;
    use std::io::Write;

    fn run_src(src: &str) -> (Executor, Result<(), ExecutionError>) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("dake_ir_test_{id}.dake"));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(src.as_bytes()).unwrap();
        let image = resolve_file(&path).unwrap();
        let program = lower(&image).unwrap();
        let mut executor = Executor::new();
        let result = executor.execute_ir(&program);
        (executor, result)
    }

    #[test]
    fn set_print_and_arith() {
        let (executor, result) = run_src("set(n, 1 + 2)\nprint(str(${n}))\n");
        let err = result.err();
        assert!(err.is_none(), "{err:?} output={:?}", executor.output());
        assert!(executor.output().iter().any(|line| line.contains('3')), "{:?}", executor.output());
    }

    #[test]
    fn package_manifest() {
        let dir = "/tmp/dake_ir_src";
        let out = "/tmp/dake_ir_out";
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(out);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(format!("{dir}/note.txt"), b"hello-dake").unwrap();
        let src = format!(
            "lib:\n    name: \"sample\"\n    version: \"0.3.0\"\n    out_dir: \"{out}\"\nfiles(\"{dir}/note.txt\")\n"
        );
        let (_executor, result) = run_src(&src);
        result.expect("打包");
        let manifest = std::fs::read_to_string(format!("{out}/manifest.json")).unwrap();
        assert!(manifest.contains("sample"));
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(out);
    }
}
