/*
 * dsl 模块 - DSL 解析和处理
 *
 * 本模块提供完整的 DSL 解析功能，包括词法分析、语法分析和 AST 构建。
 */

/// AST 定义模块
pub mod ast;

/// 词法分析器模块
pub mod lexer;

/// 语法分析器模块
pub mod parser;

// 重新导出核心类型
#[allow(unused_imports)]
pub use ast::{Ast, Statement};
#[allow(unused_imports)]
pub use lexer::Lexer;
pub use parser::Parser;
