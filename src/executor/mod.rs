/*
 * executor 模块 - DSL 执行器
 *
 * 负责执行解析后的 AST，包括变量解析、控制流评估、
 * 数据管道执行、文件加密等功能。
 */

/// 执行上下文
pub mod context;

/// 执行器核心
pub mod executor;

mod access;
mod host;
mod builtin;
mod net;
mod ops;
mod paths;
pub mod preflight;
mod select;
mod serve;

/// 文件操作
pub mod file_ops;

/// 加密操作
pub mod crypto;

/// 数据包落盘
pub mod package;

// 重新导出核心类型
pub use executor::Executor;
