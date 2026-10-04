/*
 * 解析 use，把别名登记到被引入的文件。
 */

use crate::dsl::ast::{Ast, Statement};
use crate::dsl::parser::Parser;
use crate::dsl::pathcheck;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Image {
    pub entry: PathBuf,
    pub files: HashMap<PathBuf, Ast>,
    /// (所在文件, 别名段) -> 目标文件
    pub aliases: HashMap<(PathBuf, Vec<String>), PathBuf>,
}

pub fn resolve_file(entry: &Path) -> Result<Image, String> {
    let entry = fs::canonicalize(entry).unwrap_or_else(|_| entry.to_path_buf());
    let mut image = Image {
        entry: entry.clone(),
        files: HashMap::new(),
        aliases: HashMap::new(),
    };
    let mut stack = HashSet::new();
    load(&mut image, &entry, &mut stack)?;
    Ok(image)
}

fn load(image: &mut Image, file: &Path, stack: &mut HashSet<PathBuf>) -> Result<(), String> {
    if image.files.contains_key(file) {
        return Ok(());
    }
    if !stack.insert(file.to_path_buf()) {
        return Err(format!("循环 use: {}", file.display()));
    }
    let text = fs::read_to_string(file).map_err(|e| format!("读取 {} 失败: {e}", file.display()))?;
    let mut parser = Parser::new(&text).map_err(|e| e.to_string())?;
    let ast = parser.parse().map_err(|e| e.to_string())?;
    let uses: Vec<(String, Vec<String>)> = ast
        .statements
        .iter()
        .filter_map(|stmt| match stmt {
            Statement::Use { file, alias } => Some((file.clone(), alias.clone())),
            _ => None,
        })
        .collect();
    image.files.insert(file.to_path_buf(), ast);
    for (rel, alias) in uses {
        if pathcheck::tighten(&rel).is_err() {
            return Err(format!("use 路径不允许 ..: {rel}"));
        }
        let next = file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(rel);
        let next = fs::canonicalize(&next).map_err(|e| format!("找不到 {}: {e}", next.display()))?;
        if image.aliases.insert((file.to_path_buf(), alias.clone()), next.clone()).is_some() {
            return Err(format!("重复的 use 别名 {}", alias.join("::")));
        }
        load(image, &next, stack)?;
    }
    stack.remove(file);
    Ok(())
}
