/*
 * 数据处理模块
 *
 * 提供数据处理相关的功能，包括正则表达式缓存、数据管道、
 * 数据验证、序列化和压缩等。
 * 遵循显式设计原则，确保最高性能和内存安全。
 */

pub mod regex_cache;
pub mod pipeline;
pub mod validator;
pub mod serializer;
pub mod compressor;

pub use regex_cache::RegexCache;

// 允许未使用的导入 - 这些是公开的 API，供外部使用
#[allow(unused_imports)]
pub use pipeline::{Pipeline, TryPipeline};
#[allow(unused_imports)]
pub use validator::{Validator, ValidationError};
#[allow(unused_imports)]
pub use serializer::SerializableValue;
#[allow(unused_imports)]
pub use compressor::{Compressor, CompressionLevel};
