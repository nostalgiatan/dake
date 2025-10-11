/*
 * 数据验证模块
 *
 * 提供高性能的数据验证功能，支持常用的验证规则。
 * 使用显式设计，确保类型安全和内存安全。
 */

use error::{ErrorInfo, Result};
use std::fmt;

/// 验证错误信息
///
/// 包含验证失败的详细信息
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// 字段名称
    field: String,
    /// 错误消息
    message: String,
}

impl ValidationError {
    /// 创建新的验证错误
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `message` - 错误消息
    pub fn new(field: String, message: String) -> Self {
        Self { field, message }
    }

    /// 获取字段名称
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 获取错误消息
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "字段 '{}': {}", self.field, self.message)
    }
}

/// 验证器
///
/// 提供数据验证功能，支持链式验证规则。
///
/// # 示例
///
/// ```no_run
/// use dake::data::Validator;
///
/// let validator = Validator::new();
/// let result = validator
///     .validate_not_empty("name", "")
///     .and_then(|_| validator.validate_min_length("name", "ab", 3));
///
/// assert!(result.is_err());
/// ```
pub struct Validator;

impl Validator {
    /// 创建新的验证器
    pub fn new() -> Self {
        Self
    }

    /// 验证非空
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    ///
    /// # 返回
    /// 如果值非空返回 Ok(())，否则返回验证错误
    pub fn validate_not_empty(&self, field: &str, value: &str) -> Result<()> {
        if value.is_empty() {
            Err(ErrorInfo::new(
                4001,
                format!("字段 '{}' 不能为空", field),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证非 null (Option 类型)
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    ///
    /// # 返回
    /// 如果值非 None 返回 Ok(())，否则返回验证错误
    ///
    /// # 错误码
    /// * 4012 - 值为 null
    pub fn validate_not_null<T>(&self, field: &str, value: &Option<T>) -> Result<()> {
        if value.is_none() {
            Err(ErrorInfo::new(
                4012,
                format!("字段 '{}' 不能为 null", field),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证最小长度
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    /// * `min_len` - 最小长度
    ///
    /// # 返回
    /// 如果值长度大于等于最小长度返回 Ok(())，否则返回验证错误
    pub fn validate_min_length(&self, field: &str, value: &str, min_len: usize) -> Result<()> {
        if value.len() < min_len {
            Err(ErrorInfo::new(
                4002,
                format!(
                    "字段 '{}' 长度不能小于 {}，当前长度为 {}",
                    field,
                    min_len,
                    value.len()
                ),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证最大长度
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    /// * `max_len` - 最大长度
    ///
    /// # 返回
    /// 如果值长度小于等于最大长度返回 Ok(())，否则返回验证错误
    pub fn validate_max_length(&self, field: &str, value: &str, max_len: usize) -> Result<()> {
        if value.len() > max_len {
            Err(ErrorInfo::new(
                4003,
                format!(
                    "字段 '{}' 长度不能大于 {}，当前长度为 {}",
                    field,
                    max_len,
                    value.len()
                ),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证范围
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    /// * `min` - 最小值
    /// * `max` - 最大值
    ///
    /// # 返回
    /// 如果值在范围内返回 Ok(())，否则返回验证错误
    pub fn validate_range<T>(&self, field: &str, value: T, min: T, max: T) -> Result<()>
    where
        T: PartialOrd + fmt::Display,
    {
        if value < min || value > max {
            Err(ErrorInfo::new(
                4004,
                format!(
                    "字段 '{}' 的值必须在 {} 和 {} 之间，当前值为 {}",
                    field, min, max, value
                ),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证是否匹配正则表达式
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    /// * `pattern` - 正则表达式模式
    ///
    /// # 返回
    /// 如果值匹配模式返回 Ok(())，否则返回验证错误
    ///
    /// # 错误码
    /// * 4005 - 正则表达式编译失败
    /// * 4006 - 值不匹配正则表达式
    pub fn validate_pattern(&self, field: &str, value: &str, pattern: &str) -> Result<()> {
        use regex::Regex;

        let re = Regex::new(pattern).map_err(|e| {
            ErrorInfo::new(4005, format!("正则表达式编译失败: {}", e))
        })?;

        if !re.is_match(value) {
            Err(ErrorInfo::new(
                4006,
                format!("字段 '{}' 的值不符合要求的格式", field),
            ))
        } else {
            Ok(())
        }
    }

    /// 验证邮箱格式
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的邮箱
    ///
    /// # 返回
    /// 如果邮箱格式正确返回 Ok(())，否则返回验证错误
    pub fn validate_email(&self, field: &str, value: &str) -> Result<()> {
        let pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
        self.validate_pattern(field, value, pattern)
            .map_err(|_| ErrorInfo::new(4007, format!("字段 '{}' 必须是有效的邮箱地址", field)))
    }

    /// 验证 URL 格式
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的 URL
    ///
    /// # 返回
    /// 如果 URL 格式正确返回 Ok(())，否则返回验证错误
    pub fn validate_url(&self, field: &str, value: &str) -> Result<()> {
        let pattern = r"^https?://[^\s/$.?#].[^\s]*$";
        self.validate_pattern(field, value, pattern)
            .map_err(|_| ErrorInfo::new(4008, format!("字段 '{}' 必须是有效的 URL", field)))
    }

    /// 验证数字字符串
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    ///
    /// # 返回
    /// 如果值是数字返回 Ok(())，否则返回验证错误
    pub fn validate_numeric(&self, field: &str, value: &str) -> Result<()> {
        let pattern = r"^\d+$";
        self.validate_pattern(field, value, pattern)
            .map_err(|_| ErrorInfo::new(4009, format!("字段 '{}' 必须是数字", field)))
    }

    /// 验证字母字符串
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    ///
    /// # 返回
    /// 如果值只包含字母返回 Ok(())，否则返回验证错误
    pub fn validate_alpha(&self, field: &str, value: &str) -> Result<()> {
        let pattern = r"^[a-zA-Z]+$";
        self.validate_pattern(field, value, pattern)
            .map_err(|_| ErrorInfo::new(4010, format!("字段 '{}' 只能包含字母", field)))
    }

    /// 验证字母数字字符串
    ///
    /// # 参数
    /// * `field` - 字段名称
    /// * `value` - 要验证的值
    ///
    /// # 返回
    /// 如果值只包含字母和数字返回 Ok(())，否则返回验证错误
    pub fn validate_alphanumeric(&self, field: &str, value: &str) -> Result<()> {
        let pattern = r"^[a-zA-Z0-9]+$";
        self.validate_pattern(field, value, pattern)
            .map_err(|_| {
                ErrorInfo::new(
                    4011,
                    format!("字段 '{}' 只能包含字母和数字", field),
                )
            })
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_not_empty_success() {
        let validator = Validator::new();
        assert!(validator.validate_not_empty("name", "test").is_ok());
    }

    #[test]
    fn test_validate_not_empty_failure() {
        let validator = Validator::new();
        let result = validator.validate_not_empty("name", "");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4001);
        }
    }

    #[test]
    fn test_validate_min_length_success() {
        let validator = Validator::new();
        assert!(validator.validate_min_length("name", "test", 3).is_ok());
    }

    #[test]
    fn test_validate_min_length_failure() {
        let validator = Validator::new();
        let result = validator.validate_min_length("name", "ab", 3);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4002);
        }
    }

    #[test]
    fn test_validate_max_length_success() {
        let validator = Validator::new();
        assert!(validator.validate_max_length("name", "test", 5).is_ok());
    }

    #[test]
    fn test_validate_max_length_failure() {
        let validator = Validator::new();
        let result = validator.validate_max_length("name", "toolong", 5);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4003);
        }
    }

    #[test]
    fn test_validate_range_success() {
        let validator = Validator::new();
        assert!(validator.validate_range("age", 25, 18, 100).is_ok());
    }

    #[test]
    fn test_validate_range_failure() {
        let validator = Validator::new();
        let result = validator.validate_range("age", 15, 18, 100);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4004);
        }
    }

    #[test]
    fn test_validate_email_success() {
        let validator = Validator::new();
        assert!(validator
            .validate_email("email", "test@example.com")
            .is_ok());
        assert!(validator.validate_email("email", "user+tag@domain.co").is_ok());
    }

    #[test]
    fn test_validate_email_failure() {
        let validator = Validator::new();
        let result = validator.validate_email("email", "invalid-email");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4007);
        }
    }

    #[test]
    fn test_validate_url_success() {
        let validator = Validator::new();
        assert!(validator
            .validate_url("website", "https://example.com")
            .is_ok());
        assert!(validator
            .validate_url("website", "http://example.com/path?query=value")
            .is_ok());
    }

    #[test]
    fn test_validate_url_failure() {
        let validator = Validator::new();
        let result = validator.validate_url("website", "not-a-url");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4008);
        }
    }

    #[test]
    fn test_validate_numeric_success() {
        let validator = Validator::new();
        assert!(validator.validate_numeric("count", "12345").is_ok());
        assert!(validator.validate_numeric("count", "0").is_ok());
    }

    #[test]
    fn test_validate_numeric_failure() {
        let validator = Validator::new();
        let result = validator.validate_numeric("count", "abc");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4009);
        }
    }

    #[test]
    fn test_validate_alpha_success() {
        let validator = Validator::new();
        assert!(validator.validate_alpha("name", "John").is_ok());
    }

    #[test]
    fn test_validate_alpha_failure() {
        let validator = Validator::new();
        let result = validator.validate_alpha("name", "John123");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4010);
        }
    }

    #[test]
    fn test_validate_alphanumeric_success() {
        let validator = Validator::new();
        assert!(validator.validate_alphanumeric("username", "user123").is_ok());
    }

    #[test]
    fn test_validate_alphanumeric_failure() {
        let validator = Validator::new();
        let result = validator.validate_alphanumeric("username", "user@123");
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4011);
        }
    }

    #[test]
    fn test_validate_chain() {
        let validator = Validator::new();
        let username = "testuser";

        let result = validator
            .validate_not_empty("username", username)
            .and_then(|_| validator.validate_min_length("username", username, 6))
            .and_then(|_| validator.validate_max_length("username", username, 20))
            .and_then(|_| validator.validate_alphanumeric("username", username));

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_chain_failure() {
        let validator = Validator::new();
        let username = "ab";

        let result = validator
            .validate_not_empty("username", username)
            .and_then(|_| validator.validate_min_length("username", username, 6));

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4002);
        }
    }

    #[test]
    fn test_validate_not_null_success() {
        let validator = Validator::new();
        let value: Option<String> = Some("data".to_string());

        let result = validator.validate_not_null("field", &value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_not_null_failure() {
        let validator = Validator::new();
        let value: Option<String> = None;

        let result = validator.validate_not_null("field", &value);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code(), 4012);
        }
    }
}
