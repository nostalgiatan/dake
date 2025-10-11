/*
 * 执行上下文 (Execution Context)
 *
 * 管理变量、环境变量和执行状态。
 * 确保局部变量 > set.env > 进程环境的优先级。
 */

use std::collections::HashMap;
use crate::dsl::ast::Value;

/// 执行上下文
///
/// 维护执行时的变量状态，支持作用域和优先级。
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 局部变量（最高优先级）
    local_vars: HashMap<String, Value>,
    
    /// 环境变量（次优先级）
    env_vars: HashMap<String, Value>,
    
    /// 父级上下文（用于作用域链）
    parent: Option<Box<ExecutionContext>>,
}

impl ExecutionContext {
    /// 创建新的执行上下文
    pub fn new() -> Self {
        Self {
            local_vars: HashMap::new(),
            env_vars: HashMap::new(),
            parent: None,
        }
    }
    
    /// 创建带父级的子上下文
    #[allow(dead_code)]
    pub fn with_parent(parent: ExecutionContext) -> Self {
        Self {
            local_vars: HashMap::new(),
            env_vars: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }
    
    /// 设置局部变量
    pub fn set_local(&mut self, key: String, value: Value) {
        self.local_vars.insert(key, value);
    }
    
    /// 设置环境变量
    pub fn set_env(&mut self, key: String, value: Value) {
        self.env_vars.insert(key, value);
    }
    
    /// 获取变量值
    ///
    /// 按照优先级：局部变量 > 环境变量 > 父级上下文 > 进程环境
    pub fn get(&self, key: &str) -> Option<Value> {
        // 1. 检查局部变量
        if let Some(value) = self.local_vars.get(key) {
            return Some(value.clone());
        }
        
        // 2. 检查环境变量
        if let Some(value) = self.env_vars.get(key) {
            return Some(value.clone());
        }
        
        // 3. 检查父级上下文
        if let Some(parent) = &self.parent {
            if let Some(value) = parent.get(key) {
                return Some(value);
            }
        }
        
        // 4. 检查进程环境（仅当在 env_vars 中明确声明过时）
        // 这确保了不会隐式读取进程环境
        if self.env_vars.contains_key(key) || self.has_env_in_parent(key) {
            if let Ok(env_val) = std::env::var(key) {
                return Some(Value::String(env_val));
            }
        }
        
        None
    }
    
    /// 检查父级是否声明过环境变量
    fn has_env_in_parent(&self, key: &str) -> bool {
        if let Some(parent) = &self.parent {
            parent.env_vars.contains_key(key) || parent.has_env_in_parent(key)
        } else {
            false
        }
    }
    
    /// 解析字符串中的变量插值
    ///
    /// 将 ${VAR} 形式的变量替换为实际值
    pub fn interpolate(&self, input: &str) -> String {
        let mut result = String::new();
        let mut chars = input.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '$' && chars.peek() == Some(&'{') {
                // 跳过 '{'
                chars.next();
                
                // 读取变量名
                let mut var_name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '}' {
                        chars.next(); // 跳过 '}'
                        break;
                    }
                    var_name.push(chars.next().unwrap());
                }
                
                // 替换变量
                if let Some(value) = self.get(&var_name) {
                    result.push_str(&value.to_string().trim_matches('"'));
                } else {
                    // 如果变量不存在，保持原样
                    result.push_str(&format!("${{{}}}", var_name));
                }
            } else {
                result.push(ch);
            }
        }
        
        result
    }
    
    /// 检查变量是否存在
    #[allow(dead_code)]
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_variable_priority() {
        let mut ctx = ExecutionContext::new();
        ctx.set_local("VAR".to_string(), Value::String("local".to_string()));
        ctx.set_env("VAR".to_string(), Value::String("env".to_string()));
        
        assert_eq!(
            ctx.get("VAR"),
            Some(Value::String("local".to_string()))
        );
    }

    #[test]
    fn test_env_variable_fallback() {
        let mut ctx = ExecutionContext::new();
        ctx.set_env("VAR".to_string(), Value::String("env".to_string()));
        
        assert_eq!(
            ctx.get("VAR"),
            Some(Value::String("env".to_string()))
        );
    }

    #[test]
    fn test_interpolation() {
        let mut ctx = ExecutionContext::new();
        ctx.set_local("USER".to_string(), Value::String("alice".to_string()));
        ctx.set_local("PATH".to_string(), Value::String("/home/alice".to_string()));
        
        let result = ctx.interpolate("Hello ${USER}, your path is ${PATH}");
        assert_eq!(result, "Hello alice, your path is /home/alice");
    }

    #[test]
    fn test_interpolation_missing_var() {
        let ctx = ExecutionContext::new();
        let result = ctx.interpolate("Value: ${MISSING}");
        assert_eq!(result, "Value: ${MISSING}");
    }

    #[test]
    fn test_parent_context() {
        let mut parent = ExecutionContext::new();
        parent.set_local("PARENT_VAR".to_string(), Value::String("parent".to_string()));
        
        let child = ExecutionContext::with_parent(parent);
        
        assert_eq!(
            child.get("PARENT_VAR"),
            Some(Value::String("parent".to_string()))
        );
    }
}
