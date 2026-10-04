/*
 * 从脚本文件编译到中间表示。
 */

use crate::dsl::diagnose;
use crate::dsl::ir::{lower, Program};
use crate::dsl::resolve::{resolve_file, Image};
use std::path::Path;

pub struct Compiled {
    pub statement_count: usize,
    pub program: Program,
    pub image: Image,
}

pub fn compile_path(file: &Path) -> Result<Compiled, String> {
    let image = resolve_file(file).map_err(|e| explain(file, e))?;
    let statement_count = image.files.get(&image.entry).map(|ast| ast.statements.len()).unwrap_or(0);
    let program = lower(&image).map_err(|e| explain(file, e))?;
    Ok(Compiled { statement_count, program, image })
}

fn explain(file: &Path, err: String) -> String {
    if err.contains("建议:") {
        format!("{}:\n{err}", file.display())
    } else {
        format!("{}:\n{err}\n  建议: {}", file.display(), diagnose::hint(0, &err))
    }
}
