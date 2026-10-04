/*
 * 把语法树降成中间表示。路径在这里解析一次。
 */

use crate::dsl::ast::*;
use crate::dsl::pathcheck;
use crate::dsl::resolve::Image;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolvedPath {
    Builtin(String),
    User { file: PathBuf, name: String },
}

impl std::fmt::Display for ResolvedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedPath::Builtin(name) => write!(f, "{name}"),
            ResolvedPath::User { file, name } => write!(f, "{}::{name}", file.display()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub entry: PathBuf,
    pub insts: Vec<Inst>,
    pub actions: HashMap<ResolvedPath, ActionIr>,
    pub errors: HashMap<String, String>,
    pub structs: HashMap<String, StructIr>,
}

#[derive(Debug, Clone)]
pub struct StructIr {
    pub from: Carrier,
    pub layout: LayoutKind,
    pub sep: Option<String>,
    pub order: Endian,
    pub replaces: Option<String>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct ActionIr {
    pub params: Vec<String>,
    pub body: Vec<Inst>,
    #[allow(dead_code)]
    pub writes: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CallArg {
    pub name: Option<String>,
    pub temp: String,
}

#[derive(Debug, Clone)]
pub enum Inst {
    Const { dest: String, value: Value },
    Load { dest: String, name: String },
    Format { dest: String, template: String },
    Binary { dest: String, op: BinaryOp, left: String, right: String },
    Unary { dest: String, op: UnaryOp, expr: String },
    CastNum { dest: String, src: String },
    CastStr { dest: String, src: String },
    Call { path: ResolvedPath, args: Vec<CallArg>, dest: Option<String> },
    GetField { dest: String, src: String, field: String },
    Index { dest: String, base: String, index: String },
    Slice { dest: String, base: String, start: String, end: String },
    Branch { cond: String, then_body: Vec<Inst>, elifs: Vec<(String, Vec<Inst>)>, else_body: Vec<Inst> },
    Each { var: String, index: Option<String>, source: Option<String>, body: Vec<Inst> },
    Catch { error_name: Option<String>, var: String, body: Vec<Inst>, fail: Vec<Inst> },
    Here { line: u32, column: u32 },
    Stop,
    Doing(ResolvedPath),
    Await(Vec<ResolvedPath>),
    Lib(LibDefinition),
    Repo(RepoDefinition),
    Print(String),
    Url { name: String, address: String, cert: Option<String>, key: Option<String> },
    Dir { name: String, path: String, suffix: Option<String>, deep: bool, exclude: Vec<String> },
    Share { name: String },
    Serve { workers: u32, routes: Vec<RouteIr>, repo: Option<String> },
    Package,
}

#[derive(Debug, Clone)]
pub struct RouteIr {
    pub source: String,
    pub url: bool,
    pub pattern: Option<crate::dsl::route::Pattern>,
    pub struct_id: String,
    pub action: ResolvedPath,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind { Url, Dir }

#[derive(Default)]
struct Gate {
    names: HashMap<String, SourceKind>,
    seen_serve: bool,
    seen_lib: bool,
    seen_repo: bool,
}

pub fn lower(image: &Image) -> Result<Program, String> {
    let mut actions = HashMap::new();
    let mut errors = HashMap::new();
    let mut structs = HashMap::new();
    for (file, ast) in &image.files {
        for stmt in &ast.statements {
            match stmt {
                Statement::Action { name, params, body } => {
                    let path = ResolvedPath::User { file: file.clone(), name: name.clone() };
                    if actions.contains_key(&path) {
                        return Err(format!("重复的行为 {name} 于 {}", file.display()));
                    }
                    let mut temps = 0u32;
                    let mut insts = Vec::new();
                    let mut writes = HashSet::new();
                    lower_stmts(image, file, body, &mut insts, &mut temps, &mut writes, false, &mut Gate::default())?;
                    actions.insert(path, ActionIr { params: params.clone(), body: insts, writes });
                }
                Statement::ErrorDef { name, message } => {
                    errors.insert(name.clone(), message.clone());
                }
                Statement::Pipe { name, params, steps } => {
                    let path = ResolvedPath::User { file: file.clone(), name: name.clone() };
                    if actions.contains_key(&path) {
                        return Err(format!("管道 {name} 与行为重名"));
                    }
                    let mut body = Vec::new();
                    let mut temps = 0u32;
                    let mut writes = HashSet::new();
                    lower_stmts(image, file, steps, &mut body, &mut temps, &mut writes, false, &mut Gate::default())?;
                    actions.insert(path, ActionIr { params: params.clone(), body, writes });
                }
                Statement::Struct { name, from, layout, sep, order, replaces, fields } => {
                    check_struct(name, from.clone(), layout, sep.as_deref(), *order, fields)?;
                    let id = struct_id(file, name);
                    if structs.contains_key(&id) {
                        return Err(format!("重复的结构 {name} 于 {}", file.display()));
                    }
                    let fields = resolve_links(image, file, fields)?;
                    for field in &fields {
                        check_field_clause(name, field)?;
                    }
                    let replaces = match replaces {
                        Some(path) => Some(struct_ref(image, file, &Arg::Pos(Expr::Name(path.clone())))?),
                        None => None,
                    };
                    structs.insert(id, StructIr {
                        from: from.clone(),
                        layout: layout.clone(),
                        sep: sep.clone(),
                        order: *order,
                        replaces,
                        fields,
                    });
                }
                _ => {}
            }
        }
    }

    let mut temps = 0u32;
    let mut insts = Vec::new();
    let mut writes = HashSet::new();
    let entry_ast = image.files.get(&image.entry).ok_or("缺少入口文件")?;
    let mut gate = Gate::default();
    lower_stmts(image, &image.entry, &entry_ast.statements, &mut insts, &mut temps, &mut writes, true, &mut gate)?;
    insts.push(Inst::Package);

    for action in actions.values() {
        for path in calls_of(&action.body) {
            if let ResolvedPath::User { .. } = path {
                // 已在 actions 或是管道
            }
        }
    }

    check_replacements(&structs)?;

    let program = Program { entry: image.entry.clone(), insts, actions, errors, structs };
    check_parallel(&program)?;
    Ok(program)
}

fn check_field_clause(struct_name: &str, field: &StructField) -> Result<(), String> {
    if field.check.is_some() && field.ty != FieldType::Str {
        return Err(format!("结构 {struct_name} 的字段 {} 只有 str 能写 check", field.name));
    }
    if let Some(default) = &field.default {
        let ok = match (&field.ty, default) {
            (FieldType::Str, Value::String(_)) => true,
            (FieldType::Int, Value::Number(_)) => true,
            (FieldType::Float, Value::Float(_)) => true,
            (FieldType::Bool, Value::Bool(_)) => true,
            _ => false,
        };
        if !ok {
            return Err(format!("结构 {struct_name} 的字段 {} 的 default 类型不符", field.name));
        }
    }
    if let (Some(check), Some(Value::String(text))) = (&field.check, &field.default) {
        let validator = crate::data::Validator::new();
        let result = match check.as_str() {
            "not_empty" => validator.validate_not_empty(&field.name, text),
            "email" => validator.validate_email(&field.name, text),
            "url" => validator.validate_url(&field.name, text),
            "numeric" => validator.validate_numeric(&field.name, text),
            "alpha" => validator.validate_alpha(&field.name, text),
            "alphanumeric" => validator.validate_alphanumeric(&field.name, text),
            other => return Err(format!("未知的验证器: {other}")),
        };
        result.map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn check_replacements(structs: &HashMap<String, StructIr>) -> Result<(), String> {
    let mut owner: HashMap<String, String> = HashMap::new();
    for (id, def) in structs {
        let Some(old) = &def.replaces else { continue };
        if !structs.contains_key(old) {
            return Err(format!("replaces 找不到结构 {old}"));
        }
        if owner.insert(old.clone(), id.clone()).is_some() {
            return Err(format!("结构 {old} 被替换了两次"));
        }
    }
    for start in owner.keys() {
        let mut current = start.clone();
        let mut seen = HashSet::new();
        while let Some(next) = owner.get(&current) {
            if !seen.insert(current.clone()) {
                return Err(format!("结构替换成环: {start}"));
            }
            current = next.clone();
        }
    }
    for def in structs.values() {
        let Some(old_id) = &def.replaces else { continue };
        let old = &structs[old_id];
        for field in &def.fields {
            let source_name = field.take.clone().unwrap_or_else(|| field.name.clone());
            if let Some(source) = old.fields.iter().find(|item| item.name == source_name) {
                if !same_type(&field.ty, &source.ty) {
                    return Err(format!("字段 {} 与旧字段 {source_name} 的类型不符", field.name));
                }
            } else if field.default.is_none() {
                return Err(format!("字段 {} 在旧结构里没有，需要 default", field.name));
            }
        }
    }
    Ok(())
}

fn same_type(left: &FieldType, right: &FieldType) -> bool {
    match (left, right) {
        (FieldType::Str, FieldType::Str)
        | (FieldType::Int, FieldType::Int)
        | (FieldType::Float, FieldType::Float)
        | (FieldType::Bool, FieldType::Bool)
        | (FieldType::Bytes, FieldType::Bytes) => true,
        (FieldType::List(a), FieldType::List(b)) => same_type(a, b),
        (FieldType::Linked(a), FieldType::Linked(b)) => a == b,
        _ => false,
    }
}

fn check_parallel(program: &Program) -> Result<(), String> {
    let mut shared = HashSet::new();
    for inst in &program.insts {
        match inst {
            Inst::Share { name } => {
                shared.insert(name.clone());
            }
            Inst::Serve { workers, routes, .. } if *workers > 1 => {
                for route in routes {
                    let action = program.actions.get(&route.action).ok_or_else(|| format!("未找到行为 {}", route.action))?;
                    for name in &action.writes {
                        if name == "result" || name == "status" || name == "headers" || shared.contains(name) {
                            continue;
                        }
                        return Err(format!("行为 {} 写入 {name}，并行时只能写 result、status、headers 或 share 过的名字", route.action));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_struct(name: &str, from: Carrier, layout: &LayoutKind, sep: Option<&str>, order: Endian, fields: &[StructField]) -> Result<(), String> {
    let names: HashSet<_> = fields.iter().map(|f| f.name.as_str()).collect();
    if names.len() != fields.len() {
        return Err(format!("结构 {name} 的字段名重复"));
    }
    if matches!(layout, LayoutKind::Width) {
        if from != Carrier::Bytes {
            return Err(format!("结构 {name} 的 width 只适用于 from: bytes"));
        }
        if sep.is_some() {
            return Err(format!("结构 {name} 的 width 不能写 sep"));
        }
        if fields.is_empty() {
            return Err(format!("结构 {name} 没有字段"));
        }
        for (i, field) in fields.iter().enumerate() {
            let Some(width) = field.width else {
                if field.ty == FieldType::Bytes && i + 1 == fields.len() {
                    continue;
                }
                return Err(format!("字段 {} 的剩余字节只能是最后一个 bytes 字段", field.name));
            };
            match &field.ty {
                FieldType::Bytes => {}
                FieldType::Int if matches!(width, 1 | 2 | 4 | 8) => {}
                FieldType::Int => return Err(format!("字段 {} 的整数宽度只能是 1、2、4 或 8", field.name)),
                FieldType::Float if matches!(width, 4 | 8) => {}
                FieldType::Float => return Err(format!("字段 {} 的小数宽度只能是 4 或 8", field.name)),
                FieldType::Bool if width == 1 => {}
                FieldType::Bool => return Err(format!("字段 {} 的布尔值宽度只能是 1", field.name)),
                _ => return Err(format!("字段 {} 不能出现在 width 布局", field.name)),
            }
        }
        return Ok(());
    }
    if order != Endian::Be {
        return Err(format!("结构 {name} 只有 width 可以写 order"));
    }
    if fields.iter().any(|field| field.width.is_some()) {
        return Err(format!("结构 {name} 只有 width 可以写字段宽度"));
    }
    if matches!(layout, LayoutKind::Block) {
        if sep.is_some() {
            return Err(format!("结构 {name} 的 block 不能写 sep"));
        }
        if fields.is_empty() {
            return Err(format!("结构 {name} 没有字段"));
        }
        let ok = match from {
            Carrier::Str => fields.iter().all(|field| matches!(field.ty, FieldType::Str | FieldType::Int | FieldType::Float | FieldType::Bool)),
            Carrier::Bytes => fields.iter().all(|field| field.ty == FieldType::Bytes),
        };
        if !ok {
            return Err(format!("结构 {name} 的 block 在 from: str 时字段只能是 str、int、float 或 bool，在 from: bytes 时只能是 bytes"));
        }
        return Ok(());
    }
    if matches!(layout, LayoutKind::Json) {
        if from != Carrier::Str {
            return Err(format!("结构 {name} 的 json 只适用于 from: str"));
        }
        if sep.is_some() {
            return Err(format!("结构 {name} 的 json 不能写 sep"));
        }
        if fields.is_empty() {
            return Err(format!("结构 {name} 没有字段"));
        }
        return Ok(());
    }
    if matches!(layout, LayoutKind::Split) {
        if from != Carrier::Str {
            return Err(format!("结构 {name} 的 split 只适用于 from: str"));
        }
        if sep.map(|s| s.is_empty()).unwrap_or(true) {
            return Err(format!("结构 {name} 的 split 需要 sep"));
        }
        if fields.is_empty() || fields.iter().any(|f| !matches!(f.ty, FieldType::Str | FieldType::Int | FieldType::Float | FieldType::Bool)) {
            return Err(format!("结构 {name} 的 split 字段只能是 str、int、float 或 bool"));
        }
        return Ok(());
    }
    if sep.is_some() {
        return Err(format!("结构 {name} 只有 split 可以写 sep"));
    }
    match (from, layout) {
        (Carrier::Str, LayoutKind::Lines) => {
            if fields.len() == 2 && fields.iter().all(|f| f.ty == FieldType::Str) {
                Ok(())
            } else {
                Err(format!("结构 {name} 的 lines 需要两个 str 字段"))
            }
        }
        (Carrier::Bytes, LayoutKind::Lines) => Err(format!("结构 {name} 的 lines 只适用于 from: str")),
        (Carrier::Str, LayoutKind::Whole) => match fields {
            [field] if field.ty == FieldType::Str => Ok(()),
            [_] => Err(format!("结构 {name} 的 whole 字段类型必须与 from 一致")),
            _ => Err(format!("结构 {name} 的 whole 只能有一个字段")),
        },
        (_, LayoutKind::Split | LayoutKind::Json | LayoutKind::Width | LayoutKind::Block) => Ok(()),
        (Carrier::Bytes, LayoutKind::Whole) => match fields {
            [field] if field.ty == FieldType::Bytes => Ok(()),
            [_] => Err(format!("结构 {name} 的 whole 字段类型必须与 from 一致")),
            _ => Err(format!("结构 {name} 的 whole 只能有一个字段")),
        },
    }
}

fn struct_id(file: &PathBuf, name: &str) -> String {
    format!("{}::{name}", file.display())
}

fn calls_of(insts: &[Inst]) -> Vec<ResolvedPath> {
    let mut out = Vec::new();
    for inst in insts {
        match inst {
            Inst::Call { path, .. } | Inst::Doing(path) => out.push(path.clone()),
            Inst::Await(paths) => out.extend(paths.clone()),
            Inst::Branch { then_body, elifs, else_body, .. } => {
                out.extend(calls_of(then_body));
                for (_, body) in elifs {
                    out.extend(calls_of(body));
                }
                out.extend(calls_of(else_body));
            }
            Inst::Each { body, .. } => out.extend(calls_of(body)),
            Inst::Catch { body, fail, .. } => {
                out.extend(calls_of(body));
                out.extend(calls_of(fail));
            }
            _ => {}
        }
    }
    out
}

fn lower_stmts(
    image: &Image,
    file: &PathBuf,
    stmts: &[Statement],
    out: &mut Vec<Inst>,
    temps: &mut u32,
    writes: &mut HashSet<String>,
    top: bool,
    gate: &mut Gate,
) -> Result<(), String> {
    for stmt in stmts {
        if top && gate.seen_serve && !matches!(stmt, Statement::At { .. }) {
            return Err("serve 后面不能再写语句".into());
        }
        match stmt {
                Statement::Use { .. } | Statement::Action { .. } | Statement::ErrorDef { .. } | Statement::Struct { .. } => {}
            Statement::Url { name, address, cert, key } => {
                bind_source(top, gate, name, SourceKind::Url)?;
                check_host_port(address)?;
                let cert = cert.as_ref().map(literal_string).transpose()?;
                let key = key.as_ref().map(literal_string).transpose()?;
                match (&cert, &key) {
                    (Some(cert), Some(key)) => {
                        pathcheck::tighten(cert)?;
                        pathcheck::tighten(key)?;
                    }
                    (None, None) => {}
                    _ => return Err("cert 与 key 必须同时给出".into()),
                }
                out.push(Inst::Url { name: name.clone(), address: address.clone(), cert, key });
            }
            Statement::Dir { name, path, suffix, deep, exclude } => {
                bind_source(top, gate, name, SourceKind::Dir)?;
                let path = pathcheck::tighten(path).map_err(|message| message)?;
                out.push(Inst::Dir {
                    name: name.clone(),
                    path,
                    suffix: suffix.as_ref().map(literal_string).transpose()?,
                    deep: deep.as_ref().map(literal_bool).transpose()?.unwrap_or(false),
                    exclude: exclude.as_ref().map(literal_list).transpose()?.unwrap_or_default(),
                });
            }
            Statement::Share { name } => out.push(Inst::Share { name: name.clone() }),
            Statement::Serve { workers, routes, repo } => {
                if !top {
                    return Err("serve 不能写在行为、if 或 each 里面".into());
                }
                if gate.seen_serve {
                    return Err("serve 只能有一个".into());
                }
                gate.seen_serve = true;
                if let Some(name) = repo {
                    if gate.names.get(name) != Some(&SourceKind::Url) {
                        return Err(format!("serve 的 repo 要写在 url 上: {name}"));
                    }
                }
                let workers = workers.as_ref().map(literal_workers).transpose()?.unwrap_or(1);
                out.push(Inst::Serve { workers, routes: lower_routes(image, file, routes, gate)?, repo: repo.clone() });
            }
            Statement::Route(_) => return Err("route 不能写在 serve 外面".into()),
            Statement::Set { key, value } => {
                lower_expr(image, file, value, key, out, temps)?;
                writes.insert(key.clone());
            }
            Statement::SetEnv { key, value } => {
                let key_t = fresh(temps);
                out.push(Inst::Const { dest: key_t.clone(), value: Value::String(key.clone()) });
                let temp = fresh(temps);
                lower_expr(image, file, value, &temp, out, temps)?;
                out.push(Inst::Call {
                    path: ResolvedPath::Builtin("env.set".into()),
                    args: vec![
                        CallArg { name: None, temp: key_t },
                        CallArg { name: None, temp },
                    ],
                    dest: None,
                });
            }
            Statement::Print(expr) => {
                let temp = fresh(temps);
                lower_expr(image, file, expr, &temp, out, temps)?;
                out.push(Inst::Print(temp));
            }
            Statement::Call { path, args } => {
                let (resolved, args) = specialize_io(image, file, path, args)?;
                let lowered = lower_args(image, file, &args, out, temps)?;
                out.push(Inst::Call { path: resolved, args: lowered, dest: None });
            }
            Statement::Doing(path) => {
                out.push(Inst::Doing(resolve_path(image, file, path)?));
            }
            Statement::Await(paths) => {
                let mut resolved = Vec::new();
                let mut seen: HashSet<String> = HashSet::new();
                for path in paths {
                    let resolved_path = resolve_path(image, file, path)?;
                    let w = write_set(image, file, &resolved_path)?;
                    for name in &w {
                        if !seen.insert(name.clone()) {
                            return Err(format!("await 的步骤都写入 {name}，不能并行"));
                        }
                    }
                    resolved.push(resolved_path);
                }
                out.push(Inst::Await(resolved));
            }
            Statement::Pipe { .. } => {}
            Statement::If { cond, then_body, elifs, else_body } => {
                let cond_t = fresh(temps);
                lower_expr(image, file, cond, &cond_t, out, temps)?;
                let mut then_i = Vec::new();
                lower_stmts(image, file, then_body, &mut then_i, temps, writes, false, gate)?;
                let mut elif_i = Vec::new();
                for (elif_cond, body) in elifs {
                    let t = fresh(temps);
                    lower_expr(image, file, elif_cond, &t, out, temps)?;
                    let mut insts = Vec::new();
                    lower_stmts(image, file, body, &mut insts, temps, writes, false, gate)?;
                    elif_i.push((t, insts));
                }
                let mut else_i = Vec::new();
                if let Some(body) = else_body {
                    lower_stmts(image, file, body, &mut else_i, temps, writes, false, gate)?;
                }
                out.push(Inst::Branch { cond: cond_t, then_body: then_i, elifs: elif_i, else_body: else_i });
            }
            Statement::Each { var, index, source, body } => {
                let source = match source {
                    EachSource::Files => None,
                    EachSource::Expr(expr) => {
                        let temp = fresh(temps);
                        lower_expr(image, file, expr, &temp, out, temps)?;
                        Some(temp)
                    }
                };
                let mut insts = Vec::new();
                lower_stmts(image, file, body, &mut insts, temps, writes, false, gate)?;
                if let Some(index) = index {
                    writes.insert(index.clone());
                }
                out.push(Inst::Each { var: var.clone(), index: index.clone(), source, body: insts });
            }
            Statement::Catch { error_name, var, body, fail } => {
                let mut insts = Vec::new();
                lower_stmts(image, file, body, &mut insts, temps, writes, false, gate)?;
                let mut fail_insts = Vec::new();
                lower_stmts(image, file, fail, &mut fail_insts, temps, writes, false, gate)?;
                writes.insert(var.clone());
                out.push(Inst::Catch {
                    error_name: error_name.clone(),
                    var: var.clone(),
                    body: insts,
                    fail: fail_insts,
                });
            }
            Statement::At { line, column } => {
                out.push(Inst::Here { line: *line, column: *column });
            }
            Statement::Stop => out.push(Inst::Stop),
            Statement::Lib(lib) => {
                if !top {
                    return Err("lib 不能写在行为、if 或 each 里面".into());
                }
                if gate.seen_lib {
                    return Err("lib 只能有一个".into());
                }
                gate.seen_lib = true;
                let mut lib = lib.clone();
                let path = pathcheck::tighten(&lib.out_dir).map_err(|_| format!("输出目录不安全: {}", lib.out_dir))?;
                lib.out_dir = path;
                if !lib.sign.is_empty() {
                    lib.sign = pathcheck::tighten(&lib.sign).map_err(|_| format!("签名密钥路径不安全: {}", lib.sign))?;
                }
                out.push(Inst::Lib(lib));
            }
            Statement::Repo(repo) => {
                if !top {
                    return Err("repo 不能写在行为、if 或 each 里面".into());
                }
                if gate.seen_repo {
                    return Err("repo 只能有一个".into());
                }
                gate.seen_repo = true;
                let mut repo = repo.clone();
                repo.dir = pathcheck::tighten(&repo.dir).map_err(|_| format!("仓库目录不安全: {}", repo.dir))?;
                out.push(Inst::Repo(repo));
            }
        }
    }
    Ok(())
}

fn write_set(image: &Image, file: &PathBuf, path: &ResolvedPath) -> Result<HashSet<String>, String> {
    match path {
        ResolvedPath::Builtin(_) => Ok(HashSet::new()),
        ResolvedPath::User { file: target, name } => {
            if let Some(ast) = image.files.get(target) {
                if let Some(Statement::Action { body, .. }) = ast.statements.iter().find(|s| matches!(s, Statement::Action { name: n, .. } if n == name)) {
                    let mut set = HashSet::new();
                    collect_sets(body, &mut set);
                    return Ok(set);
                }
                if ast.statements.iter().any(|s| matches!(s, Statement::Pipe { name: n, .. } if n == name)) {
                    return Ok(HashSet::new());
                }
            }
            let _ = file;
            Err(format!("找不到行为 {path}"))
        }
    }
}

fn bind_source(top: bool, gate: &mut Gate, name: &str, kind: SourceKind) -> Result<(), String> {
    let label = if kind == SourceKind::Url { "url" } else { "dir" };
    if !top {
        return Err(format!("{label} 只能写在顶层"));
    }
    if gate.seen_serve {
        return Err(format!("{label} 不能写在 serve 后面"));
    }
    if gate.names.contains_key(name) {
        return Err(format!("名字 {name} 重复"));
    }
    gate.names.insert(name.to_string(), kind);
    Ok(())
}

fn check_host_port(address: &str) -> Result<(), String> {
    if address.contains("://") {
        return Err(format!("地址必须是 主机:端口: {address}"));
    }
    let Some((host, port)) = address.rsplit_once(':') else {
        return Err(format!("地址必须是 主机:端口: {address}"));
    };
    if host.is_empty() || port.parse::<u16>().ok().filter(|port| *port != 0).is_none() {
        return Err(format!("地址必须是 主机:端口: {address}"));
    }
    Ok(())
}

fn specialize_shared(key: &str, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
    let need = if key == "shared.add" { 2 } else { 3 };
    if positional.len() != need {
        return Err(format!("{key} 的参数个数不符"));
    }
    let Arg::Pos(name_expr) = &positional[0] else { unreachable!() };
    let Expr::Name(path) = name_expr else {
        return Err(format!("{key} 的名字必须是标识符"));
    };
    if !path.modules.is_empty() {
        return Err(format!("{key} 的名字必须是标识符"));
    }
    let mut out = vec![Arg::Pos(Expr::Literal(Value::String(path.behavior.clone())))];
    if key == "shared.set" {
        let Arg::Pos(field_expr) = &positional[1] else { unreachable!() };
        let Expr::Name(field) = field_expr else {
            return Err("shared.set 的字段必须是标识符".into());
        };
        if !field.modules.is_empty() {
            return Err("shared.set 的字段必须是标识符".into());
        }
        out.push(Arg::Pos(Expr::Literal(Value::String(field.behavior.clone()))));
        out.push(positional[2].clone());
    } else {
        out.push(positional[1].clone());
    }
    Ok((ResolvedPath::Builtin(key.into()), out))
}

fn literal_workers(expr: &Expr) -> Result<u32, String> {
    match expr {
        Expr::Literal(Value::Number(n)) if (1..=64).contains(n) => Ok(*n as u32),
        Expr::Literal(Value::Number(_)) => Err("workers 要在 1 到 64 之间".into()),
        _ => Err("workers 需要整数".into()),
    }
}

fn literal_string(expr: &Expr) -> Result<String, String> {
    match expr {
        Expr::Literal(Value::String(text)) => Ok(text.clone()),
        _ => Err("suffix 需要字符串字面量".into()),
    }
}

fn literal_bool(expr: &Expr) -> Result<bool, String> {
    match expr {
        Expr::Literal(Value::Bool(value)) => Ok(*value),
        _ => Err("deep 需要 true 或 false".into()),
    }
}

fn literal_list(expr: &Expr) -> Result<Vec<String>, String> {
    let Expr::Call { path, args } = expr else {
        return Err("exclude 需要字符串列表".into());
    };
    if path.behavior != "of" || path.modules != ["list"] {
        return Err("exclude 需要字符串列表".into());
    }
    args.iter().map(|arg| match arg {
        Arg::Pos(Expr::Literal(Value::String(text))) => Ok(text.clone()),
        _ => Err("exclude 的元素需要字符串字面量".into()),
    }).collect()
}

fn lower_routes(image: &Image, file: &PathBuf, routes: &[RouteDecl], gate: &Gate) -> Result<Vec<RouteIr>, String> {
    use crate::dsl::route::{ambiguous, parse_pattern};
    let mut out = Vec::new();
    let mut seen: HashMap<String, Vec<crate::dsl::route::Pattern>> = HashMap::new();
    let mut defaults = HashSet::new();
    for route in routes {
        let kind = gate.names.get(&route.source).copied().ok_or_else(|| format!("没有 {}", route.source))?;
        let url = kind == SourceKind::Url;
        let pattern = match &route.pattern {
            Some(text) => Some(parse_pattern(text).map_err(|err| format!("{}: {err}", route.source))?),
            None if url => return Err(format!("{} 的路由必须写路径", route.source)),
            None => None,
        };
        if let Some(pattern) = &pattern {
            let peers = seen.entry(route.source.clone()).or_default();
            for other in peers.iter() {
                if other.segs == pattern.segs {
                    return Err(format!("{} 的路径重复", route.source));
                }
                if ambiguous(other, pattern) {
                    return Err(format!("{} 的路由特异度相同", route.source));
                }
            }
            peers.push(pattern.clone());
        } else if !defaults.insert(route.source.clone()) {
            return Err(format!("{} 只能有一条默认路由", route.source));
        }
        let struct_id = struct_ref(image, file, &Arg::Pos(Expr::Name(route.struct_name.clone())))?;
        let action = resolve_path(image, file, &route.action)?;
        let ResolvedPath::User { file: action_file, name } = &action else {
            return Err("route 的行为必须是脚本里的行为".into());
        };
        let params = image.files.get(action_file).and_then(|ast| {
            ast.statements.iter().find_map(|stmt| match stmt {
                Statement::Action { name: found, params, .. } if found == name => Some(params.clone()),
                _ => None,
            })
        }).ok_or_else(|| format!("找不到行为 {name}"))?;
        let captures = pattern.as_ref().map(|item| item.captures.len()).unwrap_or(0);
        if params.len() != 1 + captures && params.len() != 2 + captures {
            return Err(format!("行为 {name} 的参数个数不符"));
        }
        out.push(RouteIr {
            source: route.source.clone(),
            url,
            pattern,
            struct_id,
            action,
        });
    }
    Ok(out)
}

fn collect_sets(stmts: &[Statement], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Statement::Set { key, .. } | Statement::SetEnv { key, .. } => {
                out.insert(key.clone());
            }
            Statement::If { then_body, elifs, else_body, .. } => {
                collect_sets(then_body, out);
                for (_, body) in elifs {
                    collect_sets(body, out);
                }
                if let Some(body) = else_body {
                    collect_sets(body, out);
                }
            }
            Statement::Each { body, .. } | Statement::Catch { body, .. } | Statement::Action { body, .. } => {
                collect_sets(body, out);
            }
            _ => {}
        }
    }
}

pub fn resolve_path(image: &Image, file: &PathBuf, path: &NamePath) -> Result<ResolvedPath, String> {
    let builtin = builtin_name(path);
    if let Some(name) = builtin {
        return Ok(ResolvedPath::Builtin(name));
    }
    if path.modules.is_empty() {
        return Ok(ResolvedPath::User { file: file.clone(), name: path.behavior.clone() });
    }
    let target = image
        .aliases
        .get(&(file.clone(), path.modules.clone()))
        .cloned()
        .ok_or_else(|| format!("未导入的路径 {path}"))?;
    let ast = image.files.get(&target).ok_or_else(|| format!("未加载 {}", target.display()))?;
    let found = ast.statements.iter().any(|stmt| match stmt {
        Statement::Action { name, .. } | Statement::Pipe { name, .. } => name == &path.behavior,
        _ => false,
    });
    if !found {
        return Err(format!("在 {} 中找不到 {}", target.display(), path.behavior));
    }
    Ok(ResolvedPath::User { file: target, name: path.behavior.clone() })
}

fn builtin_name(path: &NamePath) -> Option<String> {
    let full = if path.modules.is_empty() {
        path.behavior.clone()
    } else {
        format!("{}.{}", path.modules.join("."), path.behavior)
    };
    const BUILTINS: &[&str] = &[
        "files", "files.all", "files.encry", "files.decry",
        "files.read", "files.write", "files.rows", "files.each", "files.field", "files.write.rows", "files.write.row", "files.list", "files.verify", "files.seal", "files.unseal", "pack", "unpack",
        "shared.add", "shared.set",
        "net.url", "net.get", "net.post", "net.accept",
        "files.read.bytes", "files.read.str", "files.write.bytes", "files.write.str",
        "base64.encode", "base64.decode",
        "text.lines", "text.join", "text.decode", "text.encode", "update",
        "list.add", "list.set", "list.remove", "list.map", "list.keep", "list.update", "list.join",
        "list.where", "list.pick", "list.sort", "list.group", "files.name", "files.dir", "object",
        "log.init", "log.info", "log.error",
        "data.re", "data.re.find", "data.re.group", "data.re.all", "data.re.replace",
        "data.vali", "data.seria", "data.deseria", "data.comp", "data.decomp",
        "utf8.encode", "utf8.decode",
        "cmd", "list.of", "list.len", "record",
        "sys.host", "sys.cpu", "sys.mem", "sys.disk", "sys.disks",
        "crypto.pair", "repo.put", "repo.get", "repo.fetch", "repo.push",
    ];
    BUILTINS.iter().find(|name| **name == full).map(|name| (*name).to_string())
}

fn lower_args(
    image: &Image,
    file: &PathBuf,
    args: &[Arg],
    out: &mut Vec<Inst>,
    temps: &mut u32,
) -> Result<Vec<CallArg>, String> {
    let mut lowered = Vec::new();
    for arg in args {
        match arg {
            Arg::Pos(expr) => {
                let temp = fresh(temps);
                lower_expr(image, file, expr, &temp, out, temps)?;
                lowered.push(CallArg { name: None, temp });
            }
            Arg::Named { name, value } => {
                let temp = fresh(temps);
                lower_expr(image, file, value, &temp, out, temps)?;
                lowered.push(CallArg { name: Some(name.clone()), temp });
            }
        }
    }
    Ok(lowered)
}

fn lower_expr(
    image: &Image,
    file: &PathBuf,
    expr: &Expr,
    dest: &str,
    out: &mut Vec<Inst>,
    temps: &mut u32,
) -> Result<(), String> {
    match expr {
        Expr::Literal(Value::String(s)) if s.contains("${") => {
            out.push(Inst::Format { dest: dest.into(), template: s.clone() });
        }
        Expr::Literal(value) => out.push(Inst::Const { dest: dest.into(), value: value.clone() }),
        Expr::Var(name) => out.push(Inst::Load { dest: dest.into(), name: name.clone() }),
        Expr::Num(inner) => {
            let src = fresh(temps);
            lower_expr(image, file, inner, &src, out, temps)?;
            out.push(Inst::CastNum { dest: dest.into(), src });
        }
        Expr::Str(inner) => {
            let src = fresh(temps);
            lower_expr(image, file, inner, &src, out, temps)?;
            out.push(Inst::CastStr { dest: dest.into(), src });
        }
        Expr::Unary { op, expr } => {
            let src = fresh(temps);
            lower_expr(image, file, expr, &src, out, temps)?;
            out.push(Inst::Unary { dest: dest.into(), op: op.clone(), expr: src });
        }
        Expr::Binary { op, left, right } => {
            let l = fresh(temps);
            let r = fresh(temps);
            lower_expr(image, file, left, &l, out, temps)?;
            lower_expr(image, file, right, &r, out, temps)?;
            out.push(Inst::Binary { dest: dest.into(), op: op.clone(), left: l, right: r });
        }
        Expr::Name(_) => {
            return Err("结构名不能当作值，只能作为 files.read 的参数".into());
        }
        Expr::Field { record, field } => {
            let src = fresh(temps);
            lower_expr(image, file, record, &src, out, temps)?;
            out.push(Inst::GetField { dest: dest.into(), src, field: field.clone() });
        }
        Expr::Index { base, index } => {
            let base_t = fresh(temps);
            let index_t = fresh(temps);
            lower_expr(image, file, base, &base_t, out, temps)?;
            lower_expr(image, file, index, &index_t, out, temps)?;
            out.push(Inst::Index { dest: dest.into(), base: base_t, index: index_t });
        }
        Expr::Slice { base, start, end } => {
            let base_t = fresh(temps);
            let start_t = fresh(temps);
            let end_t = fresh(temps);
            lower_expr(image, file, base, &base_t, out, temps)?;
            lower_expr(image, file, start, &start_t, out, temps)?;
            lower_expr(image, file, end, &end_t, out, temps)?;
            out.push(Inst::Slice { dest: dest.into(), base: base_t, start: start_t, end: end_t });
        }
        Expr::Call { path, args } => {
            if path.modules == ["list"] && path.behavior == "of" {
                let mut items = Vec::new();
                for arg in args {
                    let Arg::Pos(arg) = arg else {
                        return Err("list.of 不接受命名参数".into());
                    };
                    let temp = fresh(temps);
                    lower_expr(image, file, arg, &temp, out, temps)?;
                    items.push(temp);
                }
                out.push(Inst::Call {
                    path: ResolvedPath::Builtin("list.of".into()),
                    args: items.into_iter().map(|temp| CallArg { name: None, temp }).collect(),
                    dest: Some(dest.into()),
                });
                return Ok(());
            }
            let (resolved, arg_exprs) = specialize_io(image, file, path, args)?;
            let mut call_args = Vec::new();
            for arg in &arg_exprs {
                let (name, expr) = match arg {
                    Arg::Pos(expr) => (None, expr),
                    Arg::Named { name, value } => (Some(name.clone()), value),
                };
                let temp = fresh(temps);
                lower_expr(image, file, expr, &temp, out, temps)?;
                call_args.push(CallArg { name, temp });
            }
            out.push(Inst::Call { path: resolved, args: call_args, dest: Some(dest.into()) });
        }
    }
    Ok(())
}

fn specialize_io(
    image: &Image,
    file: &PathBuf,
    path: &NamePath,
    args: &[Arg],
) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let key = if path.modules.is_empty() {
        path.behavior.clone()
    } else {
        format!("{}.{}", path.modules.join("."), path.behavior)
    };
    if key == "shared.add" || key == "shared.set" {
        return specialize_shared(&key, args);
    }
    if key == "files.read" || key == "files.rows" || key == "text.decode" {
        let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
        if positional.len() >= 2 && (key != "files.rows" || positional.len() == 2) {
            let id = struct_ref(image, file, &positional[1])?;
            let mut rest = vec![positional[0].clone(), Arg::Pos(Expr::Literal(Value::String(id)))];
            if positional.len() > 2 {
                rest.extend(positional.into_iter().skip(2));
            }
            rest.extend(args.iter().filter(|arg| matches!(arg, Arg::Named { .. })).cloned());
            return Ok((ResolvedPath::Builtin(key), rest));
        }
    }
    if key == "net.get" || key == "net.post" {
        return specialize_net(image, file, &key, args);
    }
    if key == "net.accept" {
        return specialize_accept(image, file, args);
    }
    if key == "list.map" || key == "list.keep" {
        return specialize_each_action(image, file, &key, args);
    }
    if key == "files.each" {
        return specialize_files_each(image, file, args);
    }
    if key == "files.field" {
        return specialize_files_field(image, file, args);
    }
    if key == "record" {
        return specialize_record(image, file, args);
    }
    if key == "update" && args.len() == 3 {
        return Ok((
            ResolvedPath::Builtin("update".into()),
            vec![args[0].clone(), field_name(&args[1])?, args[2].clone()],
        ));
    }
    if key == "list.update" && args.len() == 3 {
        return Ok((
            ResolvedPath::Builtin("list.update".into()),
            vec![args[0].clone(), field_name(&args[1])?, args[2].clone()],
        ));
    }
    if key == "list.where" && args.len() == 3 {
        return Ok((
            ResolvedPath::Builtin("list.where".into()),
            vec![args[0].clone(), field_name(&args[1])?, args[2].clone()],
        ));
    }
    if key == "list.pick" && args.len() == 2 {
        let id = struct_ref(image, file, &args[1])?;
        return Ok((
            ResolvedPath::Builtin("list.pick".into()),
            vec![args[0].clone(), Arg::Pos(Expr::Literal(Value::String(id)))],
        ));
    }
    if key == "list.sort" {
        let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
        if positional.len() == 2 {
            let mut rest = vec![positional[0].clone(), field_name(&positional[1])?];
            rest.extend(args.iter().filter(|arg| matches!(arg, Arg::Named { .. })).cloned());
            return Ok((ResolvedPath::Builtin("list.sort".into()), rest));
        }
    }
    if key == "list.group" && args.len() == 3 {
        let id = struct_ref(image, file, &args[2])?;
        return Ok((
            ResolvedPath::Builtin("list.group".into()),
            vec![
                args[0].clone(),
                field_name(&args[1])?,
                Arg::Pos(Expr::Literal(Value::String(id))),
            ],
        ));
    }
    if key == "list.join" && args.len() == 5 {
        let id = struct_ref(image, file, &args[4])?;
        return Ok((
            ResolvedPath::Builtin("list.join".into()),
            vec![
                args[0].clone(),
                field_name(&args[1])?,
                args[2].clone(),
                field_name(&args[3])?,
                Arg::Pos(Expr::Literal(Value::String(id))),
            ],
        ));
    }
    Ok((resolve_path(image, file, path)?, args.to_vec()))
}

fn specialize_net(image: &Image, file: &PathBuf, key: &str, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let mut positional = Vec::new();
    let mut struct_arg = None;
    let mut named = Vec::new();
    for arg in args {
        match arg {
            Arg::Named { .. } => named.push(arg.clone()),
            Arg::Pos(Expr::Name(_)) => {
                if struct_arg.is_some() {
                    return Err(format!("{key} 只能有一个结构"));
                }
                let id = struct_ref(image, file, arg)?;
                struct_arg = Some(Arg::Named {
                    name: "struct".into(),
                    value: Expr::Literal(Value::String(id)),
                });
            }
            Arg::Pos(_) => positional.push(arg.clone()),
        }
    }
    if key == "net.post" && !(1..=2).contains(&positional.len()) {
        return Err("net.post 需要正文，路径可以写在正文前面".into());
    }
    if key == "net.get" && positional.len() > 1 {
        return Err("net.get 最多一个路径".into());
    }
    positional.extend(named);
    if let Some(struct_arg) = struct_arg {
        positional.push(struct_arg);
    }
    Ok((ResolvedPath::Builtin(key.into()), positional))
}

fn specialize_accept(image: &Image, file: &PathBuf, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
    if positional.len() != 3 {
        return Err("net.accept 需要地址、结构和行为".into());
    }
    let mut saw_cert = false;
    let mut saw_key = false;
    for arg in args {
        if let Arg::Named { name, .. } = arg {
            match name.as_str() {
                "cert" => saw_cert = true,
                "key" => saw_key = true,
                _ => return Err(format!("net.accept 没有 {name}")),
            }
        }
    }
    if saw_cert != saw_key {
        return Err("cert 与 key 必须同时给出".into());
    }
    let id = struct_ref(image, file, &positional[1])?;
    let Arg::Pos(Expr::Name(path)) = &positional[2] else {
        return Err("net.accept 的行为必须是名字".into());
    };
    let resolved = resolve_path(image, file, path)?;
    let ResolvedPath::User { file: action_file, name } = &resolved else {
        return Err("net.accept 的行为必须是脚本里的行为".into());
    };
    let params = image.files.get(action_file).and_then(|ast| {
        ast.statements.iter().find_map(|stmt| match stmt {
            Statement::Action { name: n, params, .. } if n == name => Some(params.clone()),
            Statement::Pipe { name: n, params, .. } if n == name => Some(params.clone()),
            _ => None,
        })
    }).ok_or_else(|| format!("找不到行为 {name}"))?;
    if params.len() != 1 && params.len() != 2 {
        return Err("net.accept 的行为只能有一个或两个参数".into());
    }
    let mut rest = vec![
        positional[0].clone(),
        Arg::Pos(Expr::Literal(Value::String(id))),
        Arg::Pos(Expr::Literal(Value::String(action_file.display().to_string()))),
        Arg::Pos(Expr::Literal(Value::String(name.clone()))),
    ];
    rest.extend(args.iter().filter(|arg| matches!(arg, Arg::Named { .. })).cloned());
    Ok((ResolvedPath::Builtin("net.accept".into()), rest))
}

fn specialize_files_each(image: &Image, file: &PathBuf, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
    if positional.len() != 3 {
        return Err("files.each 需要路径、结构和行为".into());
    }
    let id = struct_ref(image, file, &positional[1])?;
    let Arg::Pos(Expr::Name(path)) = &positional[2] else {
        return Err("files.each 的行为必须是名字".into());
    };
    let resolved = resolve_path(image, file, path)?;
    let ResolvedPath::User { file: action_file, name } = &resolved else {
        return Err("files.each 的行为必须是脚本里的行为".into());
    };
    let params = image.files.get(action_file).and_then(|ast| {
        ast.statements.iter().find_map(|stmt| match stmt {
            Statement::Action { name: n, params, .. } if n == name => Some(params.clone()),
            Statement::Pipe { name: n, params, .. } if n == name => Some(params.clone()),
            _ => None,
        })
    }).ok_or_else(|| format!("找不到行为 {name}"))?;
    if params.len() != 1 {
        return Err("files.each 的行为只能有一个参数".into());
    }
    let mut rest = vec![
        positional[0].clone(),
        Arg::Pos(Expr::Literal(Value::String(id))),
        Arg::Pos(Expr::Literal(Value::String(action_file.display().to_string()))),
        Arg::Pos(Expr::Literal(Value::String(name.clone()))),
    ];
    rest.extend(args.iter().filter(|arg| matches!(arg, Arg::Named { .. })).cloned());
    Ok((ResolvedPath::Builtin("files.each".into()), rest))
}

fn specialize_files_field(image: &Image, file: &PathBuf, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    let positional: Vec<_> = args.iter().filter(|arg| matches!(arg, Arg::Pos(_))).cloned().collect();
    if positional.len() != 3 {
        return Err("files.field 需要路径、结构和行为".into());
    }
    let id = struct_ref(image, file, &positional[1])?;
    let Arg::Pos(Expr::Name(path)) = &positional[2] else {
        return Err("files.field 的行为必须是名字".into());
    };
    let resolved = resolve_path(image, file, path)?;
    let ResolvedPath::User { file: action_file, name } = &resolved else {
        return Err("files.field 的行为必须是脚本里的行为".into());
    };
    let params = image.files.get(action_file).and_then(|ast| {
        ast.statements.iter().find_map(|stmt| match stmt {
            Statement::Action { name: n, params, .. } if n == name => Some(params.clone()),
            _ => None,
        })
    }).ok_or_else(|| format!("找不到行为 {name}"))?;
    if params.len() != 2 {
        return Err("files.field 的行为只能有两个参数".into());
    }
    Ok((
        ResolvedPath::Builtin("files.field".into()),
        vec![
            positional[0].clone(),
            Arg::Pos(Expr::Literal(Value::String(id))),
            Arg::Pos(Expr::Literal(Value::String(action_file.display().to_string()))),
            Arg::Pos(Expr::Literal(Value::String(name.clone()))),
        ],
    ))
}

fn specialize_each_action(image: &Image, file: &PathBuf, key: &str, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    if args.len() != 2 {
        return Err(format!("{key} 需要列表和行为"));
    }
    let Arg::Pos(Expr::Name(path)) = &args[1] else {
        return Err(format!("{key} 的行为必须是名字"));
    };
    let resolved = resolve_path(image, file, path)?;
    let ResolvedPath::User { file: action_file, name } = &resolved else {
        return Err(format!("{key} 的行为必须是脚本里的行为"));
    };
    let params = image.files.get(action_file).and_then(|ast| {
        ast.statements.iter().find_map(|stmt| match stmt {
            Statement::Action { name: n, params, .. } if n == name => Some(params.clone()),
            Statement::Pipe { name: n, params, .. } if n == name => Some(params.clone()),
            _ => None,
        })
    }).ok_or_else(|| format!("找不到行为 {name}"))?;
    if params.len() != 1 {
        return Err(format!("{key} 的行为只能有一个参数"));
    }
    Ok((
        ResolvedPath::Builtin(key.into()),
        vec![
            args[0].clone(),
            Arg::Pos(Expr::Literal(Value::String(action_file.display().to_string()))),
            Arg::Pos(Expr::Literal(Value::String(name.clone()))),
        ],
    ))
}

fn field_name(arg: &Arg) -> Result<Arg, String> {
    let Arg::Pos(Expr::Name(field)) = arg else {
        return Err("字段名必须是标识符".into());
    };
    if !field.modules.is_empty() {
        return Err("字段名不能带模块路径".into());
    }
    Ok(Arg::Pos(Expr::Literal(Value::String(field.behavior.clone()))))
}

fn specialize_record(image: &Image, file: &PathBuf, args: &[Arg]) -> Result<(ResolvedPath, Vec<Arg>), String> {
    if args.is_empty() {
        return Err("record 需要结构名".into());
    }
    let id = struct_ref(image, file, &args[0])?;
    let def = image.files.iter().find_map(|(path, ast)| {
        ast.statements.iter().find_map(|stmt| match stmt {
            Statement::Struct { name, fields, .. } if struct_id(path, name) == id => Some(fields.clone()),
            _ => None,
        })
    }).ok_or_else(|| format!("找不到结构 {id}"))?;
    if args.len() % 2 == 0 {
        return Err("record 的字段必须成对出现".into());
    }
    let mut given = Vec::new();
    let mut rest = vec![Arg::Pos(Expr::Literal(Value::String(id)))];
    let mut i = 1;
    while i < args.len() {
        let Arg::Pos(Expr::Name(field)) = &args[i] else {
            return Err("record 的字段名必须是标识符".into());
        };
        if !field.modules.is_empty() {
            return Err("record 的字段名不能带模块路径".into());
        }
        if given.iter().any(|name| name == &field.behavior) {
            return Err(format!("record 的字段 {} 重复", field.behavior));
        }
        given.push(field.behavior.clone());
        rest.push(Arg::Pos(Expr::Literal(Value::String(field.behavior.clone()))));
        rest.push(args[i + 1].clone());
        i += 2;
    }
    let missing: Vec<_> = def.iter()
        .filter(|field| field.default.is_none() && !given.contains(&field.name))
        .map(|field| field.name.clone())
        .collect();
    if !missing.is_empty() || given.iter().any(|name| !def.iter().any(|field| &field.name == name)) {
        let expected: Vec<_> = def.iter().map(|field| field.name.clone()).collect();
        return Err(format!("record 的字段必须正好是 {}", expected.join(", ")));
    }
    Ok((ResolvedPath::Builtin("record".into()), rest))
}

fn resolve_links(image: &Image, file: &PathBuf, fields: &[StructField]) -> Result<Vec<StructField>, String> {
    fields.iter().map(|field| {
        let mut field = field.clone();
        field.ty = resolve_type(image, file, &field.ty)?;
        if let FieldType::Struct(path) = &field.ty {
            field.link = Some(struct_ref(image, file, &Arg::Pos(Expr::Name(path.clone())))?);
        }
        Ok(field)
    }).collect()
}

fn resolve_type(image: &Image, file: &PathBuf, ty: &FieldType) -> Result<FieldType, String> {
    match ty {
        FieldType::List(inner) => Ok(FieldType::List(Box::new(resolve_type(image, file, inner)?))),
        FieldType::Struct(path) => {
            let id = struct_ref(image, file, &Arg::Pos(Expr::Name(path.clone())))?;
            Ok(FieldType::Linked(id))
        }
        FieldType::Linked(id) => Ok(FieldType::Linked(id.clone())),
        other => Ok(other.clone()),
    }
}

fn struct_ref(image: &Image, file: &PathBuf, arg: &Arg) -> Result<String, String> {
    let Arg::Pos(Expr::Name(path)) = arg else {
        return Err("files.read 的结构参数必须是结构名".into());
    };
    let (target, name) = if path.modules.is_empty() {
        (file.clone(), path.behavior.clone())
    } else {
        let target = image
            .aliases
            .get(&(file.clone(), path.modules.clone()))
            .cloned()
            .ok_or_else(|| format!("未导入的结构 {path}"))?;
        (target, path.behavior.clone())
    };
    let ast = image.files.get(&target).ok_or_else(|| format!("未加载 {}", target.display()))?;
    let found = ast.statements.iter().any(|stmt| matches!(stmt, Statement::Struct { name: n, .. } if n == &name));
    if !found {
        return Err(format!("找不到结构 {name}"));
    }
    Ok(struct_id(&target, &name))
}

fn fresh(temps: &mut u32) -> String {
    let name = format!("#t{temps}");
    *temps += 1;
    name
}
