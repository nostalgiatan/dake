/*
 * 正则表达式缓存模块
 *
 * 提供高性能的正则表达式缓存机制，避免重复编译正则表达式。
 * 使用标准库的 regex crate，确保极致性能和内存安全。
 */

use error::{ErrorInfo, Result};
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// 正则表达式错误
///
/// 用于表示正则表达式相关的错误
#[derive(Debug, Clone)]
pub struct RegexError {
    code: u32,
    message: String,
}

impl RegexError {
    /// 创建新的正则表达式错误
    ///
    /// # 参数
    /// * `code` - 错误码
    /// * `message` - 错误消息
    pub fn new(code: u32, message: String) -> Self {
        Self { code, message }
    }

    /// 获取错误码
    pub fn code(&self) -> u32 {
        self.code
    }

    /// 获取错误消息
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[错误码: {}] {}", self.code, self.message)
    }
}

impl std::error::Error for RegexError {}

/// 正则表达式缓存
///
/// 提供线程安全的正则表达式缓存，避免重复编译正则表达式。
/// 使用读写锁实现，读取操作不会互相阻塞，提供极致性能。
///
/// # 示例
///
/// ```no_run
/// use dake::data::RegexCache;
///
/// let cache = RegexCache::new();
/// 
/// // 第一次使用会编译并缓存
/// let re = cache.get_or_compile(r"\d+").expect("编译失败");
/// assert!(re.is_match("123"));
/// 
/// // 第二次使用直接从缓存获取，无需重新编译
/// let re2 = cache.get_or_compile(r"\d+").expect("编译失败");
/// assert!(re2.is_match("456"));
/// ```
pub struct RegexCache {
    /// 内部缓存存储
    cache: Arc<RwLock<HashMap<String, Arc<Regex>>>>,
}

impl RegexCache {
    /// 创建新的正则表达式缓存
    ///
    /// # 返回
    /// 返回一个新的空缓存实例
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取或编译正则表达式
    ///
    /// 如果缓存中已存在，则直接返回；否则编译并缓存。
    /// 使用读写锁确保线程安全，读取操作不会互相阻塞。
    ///
    /// # 参数
    /// * `pattern` - 正则表达式模式字符串
    ///
    /// # 返回
    /// 成功时返回编译后的正则表达式，失败时返回错误信息
    ///
    /// # 错误
    /// * 错误码 2001 - 正则表达式编译失败
    /// * 错误码 2002 - 获取读锁失败
    /// * 错误码 2003 - 获取写锁失败
    ///
    /// # 示例
    ///
    /// ```no_run
    /// use dake::data::RegexCache;
    ///
    /// let cache = RegexCache::new();
    /// let re = cache.get_or_compile(r"^\d{3}-\d{4}$").expect("编译失败");
    /// assert!(re.is_match("123-4567"));
    /// ```
    pub fn get_or_compile(&self, pattern: &str) -> Result<Arc<Regex>> {
        // 首先尝试从缓存中读取
        {
            let cache_read = self.cache.read().map_err(|e| {
                ErrorInfo::new(
                    2002,
                    format!("获取正则表达式缓存读锁失败: {}", e),
                )
            })?;

            if let Some(regex) = cache_read.get(pattern) {
                return Ok(Arc::clone(regex));
            }
        }

        // 缓存中不存在，需要编译
        let regex = Regex::new(pattern).map_err(|e| {
            ErrorInfo::new(
                2001,
                format!("正则表达式编译失败: {}", e),
            )
        })?;

        let regex_arc = Arc::new(regex);

        // 写入缓存
        {
            let mut cache_write = self.cache.write().map_err(|e| {
                ErrorInfo::new(
                    2003,
                    format!("获取正则表达式缓存写锁失败: {}", e),
                )
            })?;

            cache_write.insert(pattern.to_string(), Arc::clone(&regex_arc));
        }

        Ok(regex_arc)
    }

    /// 清空缓存
    ///
    /// 清空所有已缓存的正则表达式。
    ///
    /// # 错误
    /// * 错误码 2003 - 获取写锁失败
    pub fn clear(&self) -> Result<()> {
        let mut cache_write = self.cache.write().map_err(|e| {
            ErrorInfo::new(
                2003,
                format!("获取正则表达式缓存写锁失败: {}", e),
            )
        })?;

        cache_write.clear();
        Ok(())
    }

    /// 获取缓存大小
    ///
    /// 返回当前缓存中的正则表达式数量。
    ///
    /// # 返回
    /// 缓存中的正则表达式数量
    ///
    /// # 错误
    /// * 错误码 2002 - 获取读锁失败
    pub fn size(&self) -> Result<usize> {
        let cache_read = self.cache.read().map_err(|e| {
            ErrorInfo::new(
                2002,
                format!("获取正则表达式缓存读锁失败: {}", e),
            )
        })?;

        Ok(cache_read.len())
    }

    /// 检查是否包含指定模式
    ///
    /// # 参数
    /// * `pattern` - 正则表达式模式字符串
    ///
    /// # 返回
    /// 如果缓存中包含该模式返回 true，否则返回 false
    ///
    /// # 错误
    /// * 错误码 2002 - 获取读锁失败
    pub fn contains(&self, pattern: &str) -> Result<bool> {
        let cache_read = self.cache.read().map_err(|e| {
            ErrorInfo::new(
                2002,
                format!("获取正则表达式缓存读锁失败: {}", e),
            )
        })?;

        Ok(cache_read.contains_key(pattern))
    }
}

impl Default for RegexCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RegexCache {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_cache_basic() {
        let cache = RegexCache::new();
        
        // 第一次编译
        let re1 = cache.get_or_compile(r"\d+").expect("编译失败");
        assert!(re1.is_match("123"));
        assert!(!re1.is_match("abc"));
        
        // 第二次应该从缓存获取
        let re2 = cache.get_or_compile(r"\d+").expect("编译失败");
        assert!(re2.is_match("456"));
        
        // 验证是同一个实例
        assert_eq!(Arc::strong_count(&re1), 3); // cache + re1 + re2
    }

    #[test]
    fn test_regex_cache_multiple_patterns() {
        let cache = RegexCache::new();
        
        let re1 = cache.get_or_compile(r"\d+").expect("编译失败");
        let re2 = cache.get_or_compile(r"[a-z]+").expect("编译失败");
        let re3 = cache.get_or_compile(r"\w+").expect("编译失败");
        
        assert!(re1.is_match("123"));
        assert!(re2.is_match("abc"));
        assert!(re3.is_match("abc123"));
        
        assert_eq!(cache.size().expect("获取大小失败"), 3);
    }

    #[test]
    fn test_regex_cache_invalid_pattern() {
        let cache = RegexCache::new();
        
        // 无效的正则表达式
        let result = cache.get_or_compile(r"[");
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        assert_eq!(err.code(), 2001);
    }

    #[test]
    fn test_regex_cache_clear() {
        let cache = RegexCache::new();
        
        cache.get_or_compile(r"\d+").expect("编译失败");
        cache.get_or_compile(r"[a-z]+").expect("编译失败");
        
        assert_eq!(cache.size().expect("获取大小失败"), 2);
        
        cache.clear().expect("清空失败");
        assert_eq!(cache.size().expect("获取大小失败"), 0);
    }

    #[test]
    fn test_regex_cache_contains() {
        let cache = RegexCache::new();
        
        assert!(!cache.contains(r"\d+").expect("检查失败"));
        
        cache.get_or_compile(r"\d+").expect("编译失败");
        
        assert!(cache.contains(r"\d+").expect("检查失败"));
        assert!(!cache.contains(r"[a-z]+").expect("检查失败"));
    }

    #[test]
    fn test_regex_cache_clone() {
        let cache1 = RegexCache::new();
        cache1.get_or_compile(r"\d+").expect("编译失败");
        
        let cache2 = cache1.clone();
        
        // 验证共享相同的缓存
        assert!(cache2.contains(r"\d+").expect("检查失败"));
        assert_eq!(cache1.size().expect("获取大小失败"), 1);
        assert_eq!(cache2.size().expect("获取大小失败"), 1);
    }

    #[test]
    fn test_regex_cache_complex_patterns() {
        let cache = RegexCache::new();
        
        // 测试复杂的正则表达式
        let email_re = cache.get_or_compile(
            r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        ).expect("编译失败");
        
        assert!(email_re.is_match("test@example.com"));
        assert!(!email_re.is_match("invalid-email"));
        
        // 测试 URL 模式
        let url_re = cache.get_or_compile(
            r"^https?://[^\s/$.?#].[^\s]*$"
        ).expect("编译失败");
        
        assert!(url_re.is_match("https://example.com"));
        assert!(url_re.is_match("http://example.com/path"));
        assert!(!url_re.is_match("invalid-url"));
    }

    #[test]
    fn test_regex_cache_thread_safety() {
        use std::thread;
        
        let cache = RegexCache::new();
        let cache_clone = cache.clone();
        
        // 在不同线程中使用缓存
        let handle = thread::spawn(move || {
            let re = cache_clone.get_or_compile(r"\d+").expect("编译失败");
            assert!(re.is_match("123"));
        });
        
        let re = cache.get_or_compile(r"\d+").expect("编译失败");
        assert!(re.is_match("456"));
        
        handle.join().expect("线程失败");
        
        // 验证只编译了一次
        assert_eq!(cache.size().expect("获取大小失败"), 1);
    }
}
