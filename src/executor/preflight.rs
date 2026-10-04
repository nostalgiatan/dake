use std::collections::HashSet;
use crate::dsl::ast::{Arg, Expr, NamePath, Statement, Value};
use crate::dsl::resolve::Image;
use crate::executor::access::{probe_dir, probe_read, probe_write};

pub fn check_image(image: &Image) -> Result<(), String> {
    let ast = image.files.get(&image.entry).ok_or("缺少入口文件")?;
    let mut written = HashSet::new();
    let mut errors = Vec::new();
    for stmt in &ast.statements {
        match stmt {
            Statement::At { .. } | Statement::If { .. } | Statement::Each { .. } | Statement::Catch { .. }
            | Statement::Action { .. } | Statement::Pipe { .. } | Statement::Serve { .. } | Statement::Use { .. }
            | Statement::Struct { .. } | Statement::ErrorDef { .. } | Statement::Lib(_)
            | Statement::Set { .. } | Statement::SetEnv { .. } | Statement::Print(_) | Statement::Doing(_)
            | Statement::Await(_) | Statement::Stop | Statement::Url { .. } | Statement::Share { .. } | Statement::Route(_) => {}
            Statement::Repo(repo) => {
                if let Err(err) = probe_dir(&repo.dir) {
                    errors.push(format!("[错误][系统错误][错误码: 4037] {err}"));
                }
            }
            Statement::Dir { path, .. } => {
                if let Err(err) = probe_dir(path) {
                    errors.push(format!("[错误][系统错误][错误码: 4037] {err}"));
                }
            }
            Statement::Call { path, args } => {
                let Some(text) = literal_path(args) else { continue };
                let key = call_key(path);
                if is_write(&key) {
                    if let Err(err) = probe_write(&text) {
                        errors.push(err.to_string());
                    }
                    written.insert(text);
                } else if is_read(&key) && !written.contains(&text) {
                    if let Err(err) = probe_read(&text) {
                        errors.push(err.to_string());
                    }
                } else if key == "files.list" {
                    if let Err(err) = probe_dir(&text) {
                        errors.push(format!("[错误][系统错误][错误码: 4037] {err}"));
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn literal_path(args: &[Arg]) -> Option<String> {
    args.iter().find_map(|arg| match arg {
        Arg::Pos(Expr::Literal(Value::String(text))) => Some(text.clone()),
        _ => None,
    })
}

fn call_key(path: &NamePath) -> String {
    if path.modules.is_empty() {
        path.behavior.clone()
    } else {
        format!("{}.{}", path.modules.join("."), path.behavior)
    }
}

fn is_read(key: &str) -> bool {
    matches!(key, "files" | "files.read" | "files.rows" | "files.each" | "files.field" | "files.read.bytes" | "files.read.str" | "files.verify" | "files.seal" | "files.unseal" | "unpack")
}

fn is_write(key: &str) -> bool {
    matches!(key, "files.write" | "files.write.rows" | "files.write.row" | "files.write.bytes" | "files.write.str")
}
