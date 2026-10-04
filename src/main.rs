/*
 * dake - 数据处理和打包工具
 *
 * 使用 DSL 文件定义数据包结构和处理流程
 */

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod dsl;
mod executor;
mod cli;
mod data;

/// dake - 数据处理和打包工具
#[derive(Parser, Debug)]
#[command(name = "dake")]
#[command(about = "数据处理和打包工具", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 解析并执行 DSL 文件
    Run {
        /// DSL 文件路径
        #[arg(value_name = "FILE")]
        file: PathBuf,
        
        /// 详细输出
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// 验证 DSL 文件语法
    Check {
        /// DSL 文件路径
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    
    /// 显示 DSL 文件的 AST
    Ast {
        /// DSL 文件路径
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Run { file, verbose } => {
            cli::run_file(&file, verbose);
        }
        Commands::Check { file } => {
            cli::check_file(&file);
        }
        Commands::Ast { file } => {
            cli::show_ast(&file);
        }
    }
}

#[cfg(test)]
mod lang_tests;
