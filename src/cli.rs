/*
 * CLI 模块 - 命令行接口实现
 *
 * 提供命令行工具的具体实现
 */

use std::path::Path;
use crate::dsl::compile::compile_path;
use crate::dsl::parser::Parser;
use crate::executor::preflight;
use crate::executor::Executor;
use std::fs;

/// 运行 DSL 文件
pub fn run_file(file: &Path, verbose: bool) {
    let compiled = match compile_path(file) {
        Ok(compiled) => compiled,
        Err(e) => {
            eprintln!("✗ 编译失败: {e}");
            std::process::exit(1);
        }
    };
    if verbose {
        println!("读取文件: {}", file.display());
        println!("解析成功，包含 {} 个语句", compiled.statement_count);
    }
    if let Err(e) = preflight::check_image(&compiled.image) {
        eprintln!("✗ 预测试失败:\n{e}");
        std::process::exit(1);
    }
    let mut executor = Executor::new();
    match executor.execute_ir(&compiled.program) {
        Ok(()) => {
            if verbose {
                println!("✓ 执行成功");
            }
        }
        Err(e) => {
            eprintln!("✗ 执行错误: {}", e);
            std::process::exit(1);
        }
    }
}

/// 检查 DSL 文件：语法、use 和降到中间表示
pub fn check_file(file: &Path) {
    match compile_path(file) {
        Ok(compiled) => {
            if let Err(e) = preflight::check_image(&compiled.image) {
                eprintln!("✗ {e}");
                std::process::exit(1);
            }
            println!("✓ 检查通过");
            println!("  文件: {}", file.display());
            println!("  语句数: {}", compiled.statement_count);
        }
        Err(e) => {
            eprintln!("✗ {e}");
            std::process::exit(1);
        }
    }
}

/// 显示 DSL 文件的 AST
pub fn show_ast(file: &Path) {
    match fs::read_to_string(file) {
        Ok(content) => {
            match Parser::new(&content) {
                Ok(mut parser) => {
                    match parser.parse() {
                        Ok(ast) => {
                            println!("AST for {}:", file.display());
                            println!("{:#?}", ast);
                        }
                        Err(e) => {
                            eprintln!("解析错误: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("创建解析器失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("读取文件失败: {}", e);
            std::process::exit(1);
        }
    }
}
