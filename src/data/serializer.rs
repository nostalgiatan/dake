/*
 * 数据序列化模块
 *
 * 提供简单的数据序列化和反序列化功能。
 * 使用显式设计，避免依赖外部序列化库，确保极致性能。
 */

use error::{ErrorInfo, Result};
use std::collections::HashMap;

/// 可序列化的值类型
///
/// 支持常用的数据类型，可以转换为 JSON 或二进制格式。
#[derive(Debug, Clone, PartialEq)]
pub enum SerializableValue {
    /// 空值
    Null,
    /// 布尔值
    Bool(bool),
    /// 整数
    Int(i64),
    /// 浮点数
    Float(f64),
    /// 字符串
    String(String),
    /// 数组
    Array(Vec<SerializableValue>),
    /// 对象（键值对）
    Object(HashMap<String, SerializableValue>),
}

impl SerializableValue {
    /// 序列化为 JSON 字符串
    ///
    /// # 返回
    /// 成功时返回 JSON 字符串
    pub fn to_json(&self) -> Result<String> {
        Ok(self.to_json_internal())
    }

    /// 内部 JSON 序列化实现
    fn to_json_internal(&self) -> String {
        match self {
            SerializableValue::Null => "null".to_string(),
            SerializableValue::Bool(b) => b.to_string(),
            SerializableValue::Int(i) => i.to_string(),
            SerializableValue::Float(f) => {
                if f.is_finite() {
                    f.to_string()
                } else {
                    "null".to_string()
                }
            }
            SerializableValue::String(s) => {
                format!("\"{}\"", Self::escape_json_string(s))
            }
            SerializableValue::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_json_internal()).collect();
                format!("[{}]", items.join(","))
            }
            SerializableValue::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "\"{}\":{}",
                            Self::escape_json_string(k),
                            v.to_json_internal()
                        )
                    })
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }

    /// 转义 JSON 字符串中的特殊字符
    fn escape_json_string(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                '\n' => "\\n".to_string(),
                '\r' => "\\r".to_string(),
                '\t' => "\\t".to_string(),
                c if c.is_control() => format!("\\u{:04x}", c as u32),
                c => c.to_string(),
            })
            .collect()
    }

    /// 从 JSON 字符串反序列化
    ///
    /// # 参数
    /// * `json` - JSON 字符串
    ///
    /// # 返回
    /// 成功时返回反序列化的值
    ///
    /// # 错误码
    /// * 5001 - JSON 解析失败
    pub fn from_json(json: &str) -> Result<Self> {
        let mut parser = JsonParser::new(json);
        parser.parse()
    }

    /// 序列化为二进制格式
    ///
    /// 使用简单的二进制格式，比 JSON 更紧凑。
    ///
    /// # 返回
    /// 成功时返回二进制数据
    pub fn to_binary(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.encode_binary(&mut bytes);
        Ok(bytes)
    }

    /// 内部二进制编码实现
    fn encode_binary(&self, bytes: &mut Vec<u8>) {
        match self {
            SerializableValue::Null => {
                bytes.push(0);
            }
            SerializableValue::Bool(b) => {
                bytes.push(1);
                bytes.push(if *b { 1 } else { 0 });
            }
            SerializableValue::Int(i) => {
                bytes.push(2);
                bytes.extend_from_slice(&i.to_le_bytes());
            }
            SerializableValue::Float(f) => {
                bytes.push(3);
                bytes.extend_from_slice(&f.to_le_bytes());
            }
            SerializableValue::String(s) => {
                bytes.push(4);
                let s_bytes = s.as_bytes();
                bytes.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
                bytes.extend_from_slice(s_bytes);
            }
            SerializableValue::Array(arr) => {
                bytes.push(5);
                bytes.extend_from_slice(&(arr.len() as u32).to_le_bytes());
                for item in arr {
                    item.encode_binary(bytes);
                }
            }
            SerializableValue::Object(obj) => {
                bytes.push(6);
                bytes.extend_from_slice(&(obj.len() as u32).to_le_bytes());
                for (k, v) in obj {
                    let k_bytes = k.as_bytes();
                    bytes.extend_from_slice(&(k_bytes.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(k_bytes);
                    v.encode_binary(bytes);
                }
            }
        }
    }

    /// 从二进制格式反序列化
    ///
    /// # 参数
    /// * `bytes` - 二进制数据
    ///
    /// # 返回
    /// 成功时返回反序列化的值
    ///
    /// # 错误码
    /// * 5002 - 二进制格式无效
    pub fn from_binary(bytes: &[u8]) -> Result<Self> {
        let mut cursor = 0;
        Self::decode_binary(bytes, &mut cursor)
    }

    /// 内部二进制解码实现
    fn decode_binary(bytes: &[u8], cursor: &mut usize) -> Result<Self> {
        if *cursor >= bytes.len() {
            return Err(ErrorInfo::new(5002, "二进制数据不完整".to_string()));
        }

        let type_byte = bytes[*cursor];
        *cursor += 1;

        match type_byte {
            0 => Ok(SerializableValue::Null),
            1 => {
                if *cursor >= bytes.len() {
                    return Err(ErrorInfo::new(5002, "布尔值数据不完整".to_string()));
                }
                let value = bytes[*cursor] != 0;
                *cursor += 1;
                Ok(SerializableValue::Bool(value))
            }
            2 => {
                if *cursor + 8 > bytes.len() {
                    return Err(ErrorInfo::new(5002, "整数数据不完整".to_string()));
                }
                let mut int_bytes = [0u8; 8];
                int_bytes.copy_from_slice(&bytes[*cursor..*cursor + 8]);
                *cursor += 8;
                Ok(SerializableValue::Int(i64::from_le_bytes(int_bytes)))
            }
            3 => {
                if *cursor + 8 > bytes.len() {
                    return Err(ErrorInfo::new(5002, "浮点数数据不完整".to_string()));
                }
                let mut float_bytes = [0u8; 8];
                float_bytes.copy_from_slice(&bytes[*cursor..*cursor + 8]);
                *cursor += 8;
                Ok(SerializableValue::Float(f64::from_le_bytes(float_bytes)))
            }
            4 => {
                if *cursor + 4 > bytes.len() {
                    return Err(ErrorInfo::new(5002, "字符串长度数据不完整".to_string()));
                }
                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&bytes[*cursor..*cursor + 4]);
                *cursor += 4;
                let len = u32::from_le_bytes(len_bytes) as usize;

                if *cursor + len > bytes.len() {
                    return Err(ErrorInfo::new(5002, "字符串数据不完整".to_string()));
                }
                let string_bytes = &bytes[*cursor..*cursor + len];
                *cursor += len;
                let s = String::from_utf8(string_bytes.to_vec())
                    .map_err(|e| ErrorInfo::new(5002, format!("UTF-8 解码失败: {}", e)))?;
                Ok(SerializableValue::String(s))
            }
            5 => {
                if *cursor + 4 > bytes.len() {
                    return Err(ErrorInfo::new(5002, "数组长度数据不完整".to_string()));
                }
                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&bytes[*cursor..*cursor + 4]);
                *cursor += 4;
                let len = u32::from_le_bytes(len_bytes) as usize;

                let mut arr = Vec::with_capacity(len);
                for _ in 0..len {
                    arr.push(Self::decode_binary(bytes, cursor)?);
                }
                Ok(SerializableValue::Array(arr))
            }
            6 => {
                if *cursor + 4 > bytes.len() {
                    return Err(ErrorInfo::new(5002, "对象长度数据不完整".to_string()));
                }
                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&bytes[*cursor..*cursor + 4]);
                *cursor += 4;
                let len = u32::from_le_bytes(len_bytes) as usize;

                let mut obj = HashMap::new();
                for _ in 0..len {
                    if *cursor + 4 > bytes.len() {
                        return Err(ErrorInfo::new(5002, "对象键长度数据不完整".to_string()));
                    }
                    let mut key_len_bytes = [0u8; 4];
                    key_len_bytes.copy_from_slice(&bytes[*cursor..*cursor + 4]);
                    *cursor += 4;
                    let key_len = u32::from_le_bytes(key_len_bytes) as usize;

                    if *cursor + key_len > bytes.len() {
                        return Err(ErrorInfo::new(5002, "对象键数据不完整".to_string()));
                    }
                    let key_bytes = &bytes[*cursor..*cursor + key_len];
                    *cursor += key_len;
                    let key = String::from_utf8(key_bytes.to_vec())
                        .map_err(|e| ErrorInfo::new(5002, format!("键 UTF-8 解码失败: {}", e)))?;

                    let value = Self::decode_binary(bytes, cursor)?;
                    obj.insert(key, value);
                }
                Ok(SerializableValue::Object(obj))
            }
            _ => Err(ErrorInfo::new(
                5002,
                format!("未知的类型字节: {}", type_byte),
            )),
        }
    }
}

/// JSON 解析器
struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn new(json: &str) -> Self {
        Self {
            chars: json.chars().collect(),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Result<SerializableValue> {
        self.skip_whitespace();
        self.parse_value()
    }

    fn parse_value(&mut self) -> Result<SerializableValue> {
        self.skip_whitespace();

        if self.pos >= self.chars.len() {
            return Err(ErrorInfo::new(5001, "意外的输入结束".to_string()));
        }

        match self.chars[self.pos] {
            'n' => self.parse_null(),
            't' | 'f' => self.parse_bool(),
            '"' => self.parse_string(),
            '[' => self.parse_array(),
            '{' => self.parse_object(),
            '-' | '0'..='9' => self.parse_number(),
            c => Err(ErrorInfo::new(
                5001,
                format!("意外的字符: {}", c),
            )),
        }
    }

    fn parse_null(&mut self) -> Result<SerializableValue> {
        if self.match_str("null") {
            Ok(SerializableValue::Null)
        } else {
            Err(ErrorInfo::new(5001, "无效的 null 值".to_string()))
        }
    }

    fn parse_bool(&mut self) -> Result<SerializableValue> {
        if self.match_str("true") {
            Ok(SerializableValue::Bool(true))
        } else if self.match_str("false") {
            Ok(SerializableValue::Bool(false))
        } else {
            Err(ErrorInfo::new(5001, "无效的布尔值".to_string()))
        }
    }

    fn parse_string(&mut self) -> Result<SerializableValue> {
        if self.chars[self.pos] != '"' {
            return Err(ErrorInfo::new(5001, "期望字符串开始引号".to_string()));
        }
        self.pos += 1;

        let mut result = String::new();
        while self.pos < self.chars.len() && self.chars[self.pos] != '"' {
            if self.chars[self.pos] == '\\' {
                self.pos += 1;
                if self.pos >= self.chars.len() {
                    return Err(ErrorInfo::new(5001, "字符串转义不完整".to_string()));
                }
                match self.chars[self.pos] {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    c => result.push(c),
                }
            } else {
                result.push(self.chars[self.pos]);
            }
            self.pos += 1;
        }

        if self.pos >= self.chars.len() {
            return Err(ErrorInfo::new(5001, "字符串未闭合".to_string()));
        }
        self.pos += 1; // Skip closing quote

        Ok(SerializableValue::String(result))
    }

    fn parse_number(&mut self) -> Result<SerializableValue> {
        let start = self.pos;
        
        if self.chars[self.pos] == '-' {
            self.pos += 1;
        }

        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        let is_float = if self.pos < self.chars.len() && self.chars[self.pos] == '.' {
            self.pos += 1;
            while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            true
        } else {
            false
        };

        let num_str: String = self.chars[start..self.pos].iter().collect();

        if is_float {
            let f = num_str.parse::<f64>().map_err(|e| {
                ErrorInfo::new(5001, format!("无效的浮点数: {}", e))
            })?;
            Ok(SerializableValue::Float(f))
        } else {
            let i = num_str.parse::<i64>().map_err(|e| {
                ErrorInfo::new(5001, format!("无效的整数: {}", e))
            })?;
            Ok(SerializableValue::Int(i))
        }
    }

    fn parse_array(&mut self) -> Result<SerializableValue> {
        if self.chars[self.pos] != '[' {
            return Err(ErrorInfo::new(5001, "期望数组开始括号".to_string()));
        }
        self.pos += 1;
        self.skip_whitespace();

        let mut arr = Vec::new();

        if self.pos < self.chars.len() && self.chars[self.pos] == ']' {
            self.pos += 1;
            return Ok(SerializableValue::Array(arr));
        }

        loop {
            arr.push(self.parse_value()?);
            self.skip_whitespace();

            if self.pos >= self.chars.len() {
                return Err(ErrorInfo::new(5001, "数组未闭合".to_string()));
            }

            if self.chars[self.pos] == ']' {
                self.pos += 1;
                break;
            } else if self.chars[self.pos] == ',' {
                self.pos += 1;
                self.skip_whitespace();
            } else {
                return Err(ErrorInfo::new(
                    5001,
                    format!("数组中期望 ',' 或 ']'，得到 '{}'", self.chars[self.pos]),
                ));
            }
        }

        Ok(SerializableValue::Array(arr))
    }

    fn parse_object(&mut self) -> Result<SerializableValue> {
        if self.chars[self.pos] != '{' {
            return Err(ErrorInfo::new(5001, "期望对象开始括号".to_string()));
        }
        self.pos += 1;
        self.skip_whitespace();

        let mut obj = HashMap::new();

        if self.pos < self.chars.len() && self.chars[self.pos] == '}' {
            self.pos += 1;
            return Ok(SerializableValue::Object(obj));
        }

        loop {
            // Parse key
            self.skip_whitespace();
            let key = match self.parse_string()? {
                SerializableValue::String(s) => s,
                _ => return Err(ErrorInfo::new(5001, "对象键必须是字符串".to_string())),
            };

            self.skip_whitespace();
            if self.pos >= self.chars.len() || self.chars[self.pos] != ':' {
                return Err(ErrorInfo::new(5001, "期望 ':'".to_string()));
            }
            self.pos += 1;

            // Parse value
            let value = self.parse_value()?;
            obj.insert(key, value);

            self.skip_whitespace();
            if self.pos >= self.chars.len() {
                return Err(ErrorInfo::new(5001, "对象未闭合".to_string()));
            }

            if self.chars[self.pos] == '}' {
                self.pos += 1;
                break;
            } else if self.chars[self.pos] == ',' {
                self.pos += 1;
            } else {
                return Err(ErrorInfo::new(
                    5001,
                    format!("对象中期望 ',' 或 '}}'，得到 '{}'", self.chars[self.pos]),
                ));
            }
        }

        Ok(SerializableValue::Object(obj))
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn match_str(&mut self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        if self.pos + chars.len() > self.chars.len() {
            return false;
        }

        for (i, c) in chars.iter().enumerate() {
            if self.chars[self.pos + i] != *c {
                return false;
            }
        }

        self.pos += chars.len();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_null() {
        let value = SerializableValue::Null;
        assert_eq!(value.to_json().expect("序列化失败"), "null");
    }

    #[test]
    fn test_serialize_bool() {
        assert_eq!(
            SerializableValue::Bool(true)
                .to_json()
                .expect("序列化失败"),
            "true"
        );
        assert_eq!(
            SerializableValue::Bool(false)
                .to_json()
                .expect("序列化失败"),
            "false"
        );
    }

    #[test]
    fn test_serialize_int() {
        assert_eq!(
            SerializableValue::Int(42).to_json().expect("序列化失败"),
            "42"
        );
        assert_eq!(
            SerializableValue::Int(-100)
                .to_json()
                .expect("序列化失败"),
            "-100"
        );
    }

    #[test]
    fn test_serialize_float() {
        assert_eq!(
            SerializableValue::Float(3.14)
                .to_json()
                .expect("序列化失败"),
            "3.14"
        );
    }

    #[test]
    fn test_serialize_string() {
        assert_eq!(
            SerializableValue::String("hello".to_string())
                .to_json()
                .expect("序列化失败"),
            "\"hello\""
        );
    }

    #[test]
    fn test_serialize_array() {
        let arr = SerializableValue::Array(vec![
            SerializableValue::Int(1),
            SerializableValue::Int(2),
            SerializableValue::Int(3),
        ]);
        assert_eq!(arr.to_json().expect("序列化失败"), "[1,2,3]");
    }

    #[test]
    fn test_serialize_object() {
        let mut obj = HashMap::new();
        obj.insert("name".to_string(), SerializableValue::String("Alice".to_string()));
        obj.insert("age".to_string(), SerializableValue::Int(30));

        let value = SerializableValue::Object(obj);
        let json = value.to_json().expect("序列化失败");
        
        // Order may vary, so check both possibilities
        assert!(json.contains("\"name\":\"Alice\""));
        assert!(json.contains("\"age\":30"));
    }

    #[test]
    fn test_deserialize_null() {
        let value = SerializableValue::from_json("null").expect("反序列化失败");
        assert_eq!(value, SerializableValue::Null);
    }

    #[test]
    fn test_deserialize_bool() {
        assert_eq!(
            SerializableValue::from_json("true").expect("反序列化失败"),
            SerializableValue::Bool(true)
        );
        assert_eq!(
            SerializableValue::from_json("false").expect("反序列化失败"),
            SerializableValue::Bool(false)
        );
    }

    #[test]
    fn test_deserialize_int() {
        assert_eq!(
            SerializableValue::from_json("42").expect("反序列化失败"),
            SerializableValue::Int(42)
        );
        assert_eq!(
            SerializableValue::from_json("-100").expect("反序列化失败"),
            SerializableValue::Int(-100)
        );
    }

    #[test]
    fn test_deserialize_float() {
        assert_eq!(
            SerializableValue::from_json("3.14").expect("反序列化失败"),
            SerializableValue::Float(3.14)
        );
    }

    #[test]
    fn test_deserialize_string() {
        assert_eq!(
            SerializableValue::from_json("\"hello\"").expect("反序列化失败"),
            SerializableValue::String("hello".to_string())
        );
    }

    #[test]
    fn test_deserialize_array() {
        let value = SerializableValue::from_json("[1,2,3]").expect("反序列化失败");
        match value {
            SerializableValue::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], SerializableValue::Int(1));
                assert_eq!(arr[1], SerializableValue::Int(2));
                assert_eq!(arr[2], SerializableValue::Int(3));
            }
            _ => panic!("期望数组类型"),
        }
    }

    #[test]
    fn test_deserialize_object() {
        let value = SerializableValue::from_json("{\"name\":\"Alice\",\"age\":30}")
            .expect("反序列化失败");
        match value {
            SerializableValue::Object(obj) => {
                assert_eq!(obj.len(), 2);
                assert_eq!(
                    obj.get("name"),
                    Some(&SerializableValue::String("Alice".to_string()))
                );
                assert_eq!(obj.get("age"), Some(&SerializableValue::Int(30)));
            }
            _ => panic!("期望对象类型"),
        }
    }

    #[test]
    fn test_roundtrip_json() {
        let original = SerializableValue::Array(vec![
            SerializableValue::Int(1),
            SerializableValue::String("test".to_string()),
            SerializableValue::Bool(true),
        ]);

        let json = original.to_json().expect("序列化失败");
        let deserialized = SerializableValue::from_json(&json).expect("反序列化失败");

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_binary_null() {
        let value = SerializableValue::Null;
        let bytes = value.to_binary().expect("序列化失败");
        let deserialized = SerializableValue::from_binary(&bytes).expect("反序列化失败");
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_binary_int() {
        let value = SerializableValue::Int(42);
        let bytes = value.to_binary().expect("序列化失败");
        let deserialized = SerializableValue::from_binary(&bytes).expect("反序列化失败");
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_binary_string() {
        let value = SerializableValue::String("hello".to_string());
        let bytes = value.to_binary().expect("序列化失败");
        let deserialized = SerializableValue::from_binary(&bytes).expect("反序列化失败");
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_binary_array() {
        let value = SerializableValue::Array(vec![
            SerializableValue::Int(1),
            SerializableValue::Int(2),
            SerializableValue::Int(3),
        ]);
        let bytes = value.to_binary().expect("序列化失败");
        let deserialized = SerializableValue::from_binary(&bytes).expect("反序列化失败");
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_binary_complex() {
        let mut obj = HashMap::new();
        obj.insert("name".to_string(), SerializableValue::String("Alice".to_string()));
        obj.insert("age".to_string(), SerializableValue::Int(30));
        obj.insert("active".to_string(), SerializableValue::Bool(true));

        let value = SerializableValue::Object(obj);
        let bytes = value.to_binary().expect("序列化失败");
        let deserialized = SerializableValue::from_binary(&bytes).expect("反序列化失败");
        assert_eq!(value, deserialized);
    }
}
