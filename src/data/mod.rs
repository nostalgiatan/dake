/*
 * 数据处理模块
 *
 * 提供数据处理相关的功能，包括正则表达式缓存、数据管道等。
 * 遵循显式设计原则，确保最高性能和内存安全。
 */

pub mod regex_cache;

pub use regex_cache::{RegexCache, RegexError};
