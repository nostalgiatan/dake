/*
 * 数据压缩模块
 *
 * 提供高性能的数据压缩和解压缩功能。
 * 支持 gzip 和 deflate 格式，确保极致性能和内存安全。
 */

use error::{ErrorInfo, Result};
use flate2::read::{GzDecoder, DeflateDecoder};
use flate2::write::{GzEncoder, DeflateEncoder};
use flate2::Compression;
use std::io::{Read, Write};

/// 压缩级别
///
/// 定义压缩的级别，从快速到最佳压缩比。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// 不压缩
    None,
    /// 快速压缩（级别 1）
    Fast,
    /// 默认压缩（级别 6）
    Default,
    /// 最佳压缩（级别 9）
    Best,
}

impl From<CompressionLevel> for Compression {
    fn from(level: CompressionLevel) -> Self {
        match level {
            CompressionLevel::None => Compression::none(),
            CompressionLevel::Fast => Compression::fast(),
            CompressionLevel::Default => Compression::default(),
            CompressionLevel::Best => Compression::best(),
        }
    }
}

/// 压缩器
///
/// 提供数据压缩和解压缩功能。
///
/// # 示例
///
/// ```no_run
/// use dake::data::{Compressor, CompressionLevel};
///
/// let compressor = Compressor::new();
/// let data = b"Hello, World!".to_vec();
///
/// // 压缩数据
/// let compressed = compressor.compress_gzip(&data, CompressionLevel::Default)
///     .expect("压缩失败");
///
/// // 解压缩数据
/// let decompressed = compressor.decompress_gzip(&compressed)
///     .expect("解压缩失败");
///
/// assert_eq!(data, decompressed);
/// ```
pub struct Compressor;

impl Compressor {
    /// 创建新的压缩器
    pub fn new() -> Self {
        Self
    }

    /// 使用 Gzip 格式压缩数据
    ///
    /// # 参数
    /// * `data` - 要压缩的数据
    /// * `level` - 压缩级别
    ///
    /// # 返回
    /// 成功时返回压缩后的数据
    ///
    /// # 错误码
    /// * 6001 - 压缩失败
    pub fn compress_gzip(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), level.into());
        encoder
            .write_all(data)
            .map_err(|e| ErrorInfo::new(6001, format!("Gzip 压缩失败: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| ErrorInfo::new(6001, format!("Gzip 压缩完成失败: {}", e)))
    }

    /// 使用 Gzip 格式解压缩数据
    ///
    /// # 参数
    /// * `data` - 要解压缩的数据
    ///
    /// # 返回
    /// 成功时返回解压缩后的数据
    ///
    /// # 错误码
    /// * 6002 - 解压缩失败
    pub fn decompress_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(data);
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| ErrorInfo::new(6002, format!("Gzip 解压缩失败: {}", e)))?;
        Ok(result)
    }

    /// 使用 Deflate 格式压缩数据
    ///
    /// # 参数
    /// * `data` - 要压缩的数据
    /// * `level` - 压缩级别
    ///
    /// # 返回
    /// 成功时返回压缩后的数据
    ///
    /// # 错误码
    /// * 6003 - 压缩失败
    pub fn compress_deflate(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>> {
        let mut encoder = DeflateEncoder::new(Vec::new(), level.into());
        encoder
            .write_all(data)
            .map_err(|e| ErrorInfo::new(6003, format!("Deflate 压缩失败: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| ErrorInfo::new(6003, format!("Deflate 压缩完成失败: {}", e)))
    }

    /// 使用 Deflate 格式解压缩数据
    ///
    /// # 参数
    /// * `data` - 要解压缩的数据
    ///
    /// # 返回
    /// 成功时返回解压缩后的数据
    ///
    /// # 错误码
    /// * 6004 - 解压缩失败
    pub fn decompress_deflate(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = DeflateDecoder::new(data);
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| ErrorInfo::new(6004, format!("Deflate 解压缩失败: {}", e)))?;
        Ok(result)
    }

    /// 计算压缩比
    ///
    /// # 参数
    /// * `original_size` - 原始数据大小
    /// * `compressed_size` - 压缩后数据大小
    ///
    /// # 返回
    /// 压缩比（百分比）
    pub fn compression_ratio(original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            0.0
        } else {
            (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0
        }
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_gzip_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("压缩失败");

        assert!(!compressed.is_empty());
        // Gzip 头部会使小数据变大，但这是正常的
    }

    #[test]
    fn test_decompress_gzip_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_gzip(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_gzip_roundtrip() {
        let compressor = Compressor::new();
        let data = b"This is a longer string that should compress well when repeated. ".repeat(100);

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_gzip(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_gzip_compression_levels() {
        let compressor = Compressor::new();
        let data = b"Repeated data. ".repeat(1000);

        let fast = compressor
            .compress_gzip(&data, CompressionLevel::Fast)
            .expect("快速压缩失败");

        let default = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("默认压缩失败");

        let best = compressor
            .compress_gzip(&data, CompressionLevel::Best)
            .expect("最佳压缩失败");

        // 验证所有压缩结果都能正确解压
        assert_eq!(
            data,
            compressor.decompress_gzip(&fast).expect("解压失败")
        );
        assert_eq!(
            data,
            compressor.decompress_gzip(&default).expect("解压失败")
        );
        assert_eq!(
            data,
            compressor.decompress_gzip(&best).expect("解压失败")
        );

        // 最佳压缩通常应该产生更小的结果
        // 但对于某些数据，差异可能很小
        assert!(best.len() <= default.len());
    }

    #[test]
    fn test_compress_deflate_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress_deflate(&data, CompressionLevel::Default)
            .expect("压缩失败");

        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_decompress_deflate_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress_deflate(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_deflate(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_deflate_roundtrip() {
        let compressor = Compressor::new();
        let data = b"This is a longer string that should compress well when repeated. ".repeat(100);

        let compressed = compressor
            .compress_deflate(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_deflate(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_ratio() {
        let ratio = Compressor::compression_ratio(1000, 500);
        assert!((ratio - 50.0).abs() < 0.001); // 50% 压缩

        let ratio = Compressor::compression_ratio(1000, 900);
        assert!((ratio - 10.0).abs() < 0.001); // 10% 压缩

        let ratio = Compressor::compression_ratio(0, 0);
        assert_eq!(ratio, 0.0); // 边界情况
    }

    #[test]
    fn test_compress_empty_data() {
        let compressor = Compressor::new();
        let data = Vec::new();

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_gzip(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_compress_binary_data() {
        let compressor = Compressor::new();
        let data: Vec<u8> = (0..255).collect();

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_gzip(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_decompress_invalid_gzip() {
        let compressor = Compressor::new();
        let invalid_data = b"This is not compressed data";

        let result = compressor.decompress_gzip(invalid_data);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 6002);
        }
    }

    #[test]
    fn test_decompress_invalid_deflate() {
        let compressor = Compressor::new();
        let invalid_data = b"This is not compressed data";

        let result = compressor.decompress_deflate(invalid_data);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 6004);
        }
    }

    #[test]
    fn test_large_data_compression() {
        let compressor = Compressor::new();
        // 创建 1MB 的重复数据
        let data = b"Large data chunk for testing. ".repeat(35000);

        let compressed = compressor
            .compress_gzip(&data, CompressionLevel::Best)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress_gzip(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
        
        let ratio = Compressor::compression_ratio(data.len(), compressed.len());
        // 重复数据应该有很好的压缩比
        assert!(ratio > 90.0, "压缩比应该大于 90%，实际为 {}%", ratio);
    }
}
