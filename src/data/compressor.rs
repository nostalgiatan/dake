/*
 * 数据压缩模块
 *
 * 提供高性能的数据压缩和解压缩功能。
 * 使用 zstd 压缩算法，提供更好的压缩比和速度。
 * 确保极致性能和内存安全。
 */

use error::{ErrorInfo, Result};

/// 压缩级别
///
/// 定义压缩的级别，从快速到最佳压缩比。
/// zstd 支持级别 1-22，我们提供预设级别以简化使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompressionLevel {
    /// 快速压缩（级别 1）
    Fast,
    /// 默认压缩（级别 3）
    Default,
    /// 最佳压缩（级别 19）
    Best,
    /// 自定义级别（1-22）
    Custom(i32),
}

impl CompressionLevel {
    /// 转换为 zstd 压缩级别
    #[allow(dead_code)]
    pub fn to_level(self) -> i32 {
        match self {
            CompressionLevel::Fast => 1,
            CompressionLevel::Default => 3,
            CompressionLevel::Best => 19,
            CompressionLevel::Custom(level) => {
                // 确保级别在有效范围内
                level.clamp(1, 22)
            }
        }
    }
    
    /// 从数值创建压缩级别
    #[allow(dead_code)]
    pub fn from_level(level: i32) -> Self {
        match level {
            1 => CompressionLevel::Fast,
            3 => CompressionLevel::Default,
            19 => CompressionLevel::Best,
            _ => CompressionLevel::Custom(level.clamp(1, 22)),
        }
    }
}

/// 压缩器
///
/// 提供数据压缩和解压缩功能。
/// 使用 zstd 算法，提供卓越的压缩比和速度。
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
/// let compressed = compressor.compress(&data, CompressionLevel::Default)
///     .expect("压缩失败");
///
/// // 解压缩数据
/// let decompressed = compressor.decompress(&compressed)
///     .expect("解压缩失败");
///
/// assert_eq!(data, decompressed);
/// ```
#[allow(dead_code)]
pub struct Compressor;

impl Compressor {
    /// 创建新的压缩器
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    /// 使用 zstd 压缩数据
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
    #[allow(dead_code)]
    pub fn compress(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut encoder = zstd::stream::Encoder::new(&mut out, level.to_level())
            .map_err(|e| ErrorInfo::new(6001, format!("zstd 压缩失败: {}", e)))?;
        for piece in data.chunks(1024 * 1024) {
            std::io::Write::write_all(&mut encoder, piece)
                .map_err(|e| ErrorInfo::new(6001, format!("zstd 压缩失败: {}", e)))?;
        }
        encoder.finish().map_err(|e| ErrorInfo::new(6001, format!("zstd 压缩失败: {}", e)))?;
        Ok(out)
    }

    /// 使用 zstd 解压缩数据
    ///
    /// # 参数
    /// * `data` - 要解压缩的数据
    ///
    /// # 返回
    /// 成功时返回解压缩后的数据
    ///
    /// # 错误码
    /// * 6002 - 解压缩失败
    #[allow(dead_code)]
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut decoder = zstd::stream::Decoder::new(data)
            .map_err(|e| ErrorInfo::new(6002, format!("zstd 解压缩失败: {}", e)))?;
        std::io::Read::read_to_end(&mut decoder, &mut out)
            .map_err(|e| ErrorInfo::new(6002, format!("zstd 解压缩失败: {}", e)))?;
        Ok(out)
    }

    /// 计算压缩比
    ///
    /// # 参数
    /// * `original_size` - 原始数据大小
    /// * `compressed_size` - 压缩后数据大小
    ///
    /// # 返回
    /// 压缩比（百分比）
    #[allow(dead_code)]
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
    fn test_compress_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress(&data, CompressionLevel::Default)
            .expect("压缩失败");

        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_decompress_basic() {
        let compressor = Compressor::new();
        let data = b"Hello, World!".to_vec();

        let compressed = compressor
            .compress(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_roundtrip() {
        let compressor = Compressor::new();
        let data = b"This is a longer string that should compress well when repeated. ".repeat(100);

        let compressed = compressor
            .compress(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_levels() {
        let compressor = Compressor::new();
        let data = b"Repeated data. ".repeat(1000);

        let fast = compressor
            .compress(&data, CompressionLevel::Fast)
            .expect("快速压缩失败");

        let default = compressor
            .compress(&data, CompressionLevel::Default)
            .expect("默认压缩失败");

        let best = compressor
            .compress(&data, CompressionLevel::Best)
            .expect("最佳压缩失败");

        // 验证所有压缩结果都能正确解压
        assert_eq!(
            data,
            compressor.decompress(&fast).expect("解压失败")
        );
        assert_eq!(
            data,
            compressor.decompress(&default).expect("解压失败")
        );
        assert_eq!(
            data,
            compressor.decompress(&best).expect("解压失败")
        );

        // 最佳压缩通常应该产生更小的结果
        assert!(best.len() <= default.len());
    }

    #[test]
    fn test_custom_level() {
        let compressor = Compressor::new();
        let data = b"Test data for custom level. ".repeat(100);

        let compressed = compressor
            .compress(&data, CompressionLevel::Custom(10))
            .expect("自定义级别压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
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
            .compress(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_compress_binary_data() {
        let compressor = Compressor::new();
        let data: Vec<u8> = (0..255).collect();

        let compressed = compressor
            .compress(&data, CompressionLevel::Default)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
    }

    #[test]
    fn test_decompress_invalid() {
        let compressor = Compressor::new();
        let invalid_data = b"This is not compressed data";

        let result = compressor.decompress(invalid_data);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 6002);
        }
    }

    #[test]
    fn test_large_data_compression() {
        let compressor = Compressor::new();
        // 创建 1MB 的重复数据
        let data = b"Large data chunk for testing. ".repeat(35000);

        let compressed = compressor
            .compress(&data, CompressionLevel::Best)
            .expect("压缩失败");

        let decompressed = compressor
            .decompress(&compressed)
            .expect("解压缩失败");

        assert_eq!(data, decompressed);
        
        let ratio = Compressor::compression_ratio(data.len(), compressed.len());
        // 重复数据应该有很好的压缩比
        // zstd 通常比 gzip 有更好的压缩比
        assert!(ratio > 90.0, "压缩比应该大于 90%，实际为 {}%", ratio);
    }

    #[test]
    fn test_level_clamping() {
        // 测试压缩级别的边界值处理
        assert_eq!(CompressionLevel::Custom(0).to_level(), 1);
        assert_eq!(CompressionLevel::Custom(25).to_level(), 22);
        assert_eq!(CompressionLevel::Custom(10).to_level(), 10);
    }

    #[test]
    fn test_level_from_number() {
        assert_eq!(CompressionLevel::from_level(1), CompressionLevel::Fast);
        assert_eq!(CompressionLevel::from_level(3), CompressionLevel::Default);
        assert_eq!(CompressionLevel::from_level(19), CompressionLevel::Best);
        assert_eq!(CompressionLevel::from_level(10), CompressionLevel::Custom(10));
    }
}
