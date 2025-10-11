/*
 * CLI 模块 - 命令行接口实现
 *
 * 提供命令行工具的具体实现
 */

use std::path::Path;
use std::fs;
use crate::dsl::Parser;
use crate::executor::Executor;

/// 运行 DSL 文件
pub fn run_file(file: &Path, verbose: bool) {
    match fs::read_to_string(file) {
        Ok(content) => {
            if verbose {
                println!("读取文件: {}", file.display());
            }
            
            match Parser::new(&content) {
                Ok(mut parser) => {
                    match parser.parse() {
                        Ok(ast) => {
                            if verbose {
                                println!("解析成功，包含 {} 个语句", ast.statements.len());
                            }
                            
                            // 执行 AST
                            let mut executor = Executor::new();
                            match executor.execute(&ast) {
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
                        Err(e) => {
                            eprintln!("✗ 解析错误: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ 创建解析器失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ 读取文件失败: {}", e);
            std::process::exit(1);
        }
    }
}

/// 检查 DSL 文件语法
pub fn check_file(file: &Path) {
    match fs::read_to_string(file) {
        Ok(content) => {
            match Parser::new(&content) {
                Ok(mut parser) => {
                    match parser.parse() {
                        Ok(ast) => {
                            println!("✓ 语法检查通过");
                            println!("  文件: {}", file.display());
                            println!("  语句数: {}", ast.statements.len());
                        }
                        Err(e) => {
                            eprintln!("✗ 语法错误: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ 词法分析失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("✗ 读取文件失败: {}", e);
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
