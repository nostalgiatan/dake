/*
 * 执行器 (Executor)
 *
 * 执行解析后的 AST。
 * 处理变量解析、控制流、数据操作等。
 */

use crate::dsl::ast::*;
use crate::executor::context::ExecutionContext;
use crate::executor::crypto::CryptoOperations;
use crate::data::RegexCache;
use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::fmt;
use std::collections::HashMap;

/// 执行错误
#[derive(Debug)]
pub struct ExecutionError {
    info: ErrorInfo,
}

impl ExecutionError {
    /// 创建新的执行错误
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error),
        }
    }
    
    /// 带上下文的执行错误
    #[allow(dead_code)]
    pub fn with_context(code: u32, message: String, context: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::System)
                .with_severity(ErrorSeverity::Error)
                .with_context(context),
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for ExecutionError {}

/// 执行器
pub struct Executor {
    /// 执行上下文
    context: ExecutionContext,
    
    /// 数据操作定义
    data_ops: HashMap<String, DataOpDef>,
    
    /// 数据管道定义
    data_pipes: HashMap<String, DataPipeDef>,
    
    /// 命令定义
    commands: HashMap<String, CommandDef>,
    
    /// 错误定义
    errors: HashMap<String, ErrorDef>,
    
    /// 加密操作
    crypto: Option<CryptoOperations>,
    
    /// 正则表达式缓存
    regex_cache: RegexCache,
    
    /// 输出缓冲区（用于测试）
    output_buffer: Vec<String>,
}

/// 数据操作定义
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DataOpDef {
    name: String,
    file: String,
    action: String,
}

/// 数据管道定义
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DataPipeDef {
    name: String,
    operations: Vec<PipeOperation>,
}

/// 命令定义
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CommandDef {
    name: String,
    statements: Vec<Statement>,
}

/// 错误定义
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ErrorDef {
    name: String,
    print: String,
}

impl Executor {
    /// 创建新的执行器
    pub fn new() -> Self {
        Self {
            context: ExecutionContext::new(),
            data_ops: HashMap::new(),
            data_pipes: HashMap::new(),
            commands: HashMap::new(),
            errors: HashMap::new(),
            crypto: None,
            regex_cache: RegexCache::new(),
            output_buffer: Vec::new(),
        }
    }
    
    /// 执行 AST
    pub fn execute(&mut self, ast: &Ast) -> Result<(), ExecutionError> {
        for statement in &ast.statements {
            self.execute_statement(statement)?;
        }
        Ok(())
    }
    
    /// 执行单个语句
    fn execute_statement(&mut self, stmt: &Statement) -> Result<(), ExecutionError> {
        match stmt {
            Statement::Set { key, value } => {
                self.context.set_local(key.clone(), value.clone());
                Ok(())
            }
            
            Statement::SetEnv { key, value } => {
                self.context.set_env(key.clone(), value.clone());
                Ok(())
            }
            
            Statement::ControlFlow(cf) => {
                self.execute_control_flow(cf)
            }
            
            Statement::Lib(lib_def) => {
                // 记录 lib 定义（实际打包功能需要进一步实现）
                self.output_buffer.push(format!("定义数据包: {}", lib_def.name));
                Ok(())
            }
            
            Statement::Repo(repo_def) => {
                // 记录 repo 定义
                self.output_buffer.push(format!("定义仓库: {}", repo_def.name));
                Ok(())
            }
            
            Statement::LogInit { dir, print } => {
                self.output_buffer.push(format!("初始化日志: dir={}, print={}", dir, print));
                Ok(())
            }
            
            Statement::ErrorDef { name, print } => {
                self.errors.insert(
                    name.clone(),
                    ErrorDef {
                        name: name.clone(),
                        print: print.clone(),
                    },
                );
                Ok(())
            }
            
            Statement::DataDo { name, file, action } => {
                self.data_ops.insert(
                    name.clone(),
                    DataOpDef {
                        name: name.clone(),
                        file: file.clone(),
                        action: action.clone(),
                    },
                );
                Ok(())
            }
            
            Statement::DataPipe { name, operations } => {
                self.data_pipes.insert(
                    name.clone(),
                    DataPipeDef {
                        name: name.clone(),
                        operations: operations.clone(),
                    },
                );
                Ok(())
            }
            
            Statement::DataRe { pattern } => {
                // 编译并缓存正则表达式
                let regex = self.regex_cache.get_or_compile(pattern)
                    .map_err(|e| ExecutionError::new(e.code(), e.message().to_string()))?;
                
                self.output_buffer.push(format!("编译正则表达式: {}", pattern));
                
                // 验证正则表达式可用
                let _ = regex.is_match("");
                
                Ok(())
            }
            
            Statement::Comm { name, action_name } => {
                self.output_buffer.push(format!("定义命令: {} -> {}", name, action_name));
                Ok(())
            }
            
            Statement::CommAction { name, statements } => {
                let cmd_name = name.clone().unwrap_or_else(|| "anonymous".to_string());
                self.commands.insert(
                    cmd_name.clone(),
                    CommandDef {
                        name: cmd_name,
                        statements: statements.clone(),
                    },
                );
                Ok(())
            }
            
            Statement::Print(msg) => {
                let interpolated = self.context.interpolate(msg);
                println!("{}", interpolated);
                self.output_buffer.push(interpolated);
                Ok(())
            }
            
            Statement::Doing(reference) => {
                self.execute_doing(reference)
            }
            
            Statement::Await(references) => {
                // 简化实现：顺序执行
                // 完整实现应使用 tokio 并发执行
                for reference in references {
                    self.execute_doing(reference)?;
                }
                Ok(())
            }
            
            Statement::Files(paths) => {
                self.output_buffer.push(format!("处理 {} 个文件", paths.len()));
                for path in paths {
                    let path_str = self.resolve_file_path(path)?;
                    self.output_buffer.push(format!("  - {}", path_str));
                }
                Ok(())
            }
            
            Statement::FilesAll { exclude } => {
                self.output_buffer.push(format!("递归处理文件，排除: {:?}", exclude));
                Ok(())
            }
            
            Statement::FilesEncry => {
                // 初始化加密操作
                let (crypto, key) = CryptoOperations::new(true); // deterministic 模式
                self.output_buffer.push(format!("启用文件加密，密钥长度: {}", key.len()));
                self.crypto = Some(crypto);
                Ok(())
            }
            
            Statement::LogInfo(msg) => {
                let interpolated = self.context.interpolate(msg);
                self.output_buffer.push(format!("[INFO] {}", interpolated));
                Ok(())
            }
            
            Statement::LogError(error_ref) => {
                self.output_buffer.push(format!("[ERROR] {}", error_ref));
                Ok(())
            }
            
            Statement::CommCmd { exec, args } => {
                self.output_buffer.push(format!("执行命令: {} {:?}", exec, args));
                Ok(())
            }
            
            Statement::Catch { error_name, statements } => {
                // 简化实现：直接执行 statements
                // 完整实现应处理错误捕获
                for stmt in statements {
                    if let Err(e) = self.execute_statement(stmt) {
                        self.output_buffer.push(format!("捕获错误 {}: {}", error_name, e));
                    }
                }
                Ok(())
            }
            
            Statement::DataVali { validator, key, value: _value } => {
                // 记录验证操作
                self.output_buffer.push(format!("验证: {} 使用 {} 验证器", key, validator));
                // 实际实现中应该根据 validator 名称调用相应的验证方法
                Ok(())
            }
            
            Statement::DataSeria { format, value } => {
                // 记录序列化操作
                let format_str = match format {
                    SerializationFormat::Json => "JSON",
                    SerializationFormat::Bin => "BIN",
                };
                self.output_buffer.push(format!("序列化: {} 使用 {} 格式", value, format_str));
                Ok(())
            }
            
            Statement::DataDeseria { format, data } => {
                // 记录反序列化操作
                let format_str = match format {
                    SerializationFormat::Json => "JSON",
                    SerializationFormat::Bin => "BIN",
                };
                self.output_buffer.push(format!("反序列化: {} 从 {} 格式", data, format_str));
                Ok(())
            }
            
            Statement::DataComp { level, data } => {
                // 记录压缩操作
                let level_str = level.map(|l| l.to_string()).unwrap_or_else(|| "default".to_string());
                self.output_buffer.push(format!("压缩: {} 使用级别 {}", data, level_str));
                Ok(())
            }
            
            Statement::DataDecomp { data } => {
                // 记录解压缩操作
                self.output_buffer.push(format!("解压缩: {}", data));
                Ok(())
            }
            
            Statement::Comment(_) => {
                // 忽略注释
                Ok(())
            }
        }
    }
    
    /// 执行控制流
    fn execute_control_flow(&mut self, cf: &ControlFlow) -> Result<(), ExecutionError> {
        // 评估 IF 条件
        if self.evaluate_expression(&cf.if_branch.0)? {
            for stmt in &cf.if_branch.1 {
                self.execute_statement(stmt)?;
            }
            return Ok(());
        }
        
        // 评估 ELIF 分支
        for (condition, statements) in &cf.elif_branches {
            if self.evaluate_expression(condition)? {
                for stmt in statements {
                    self.execute_statement(stmt)?;
                }
                return Ok(());
            }
        }
        
        // 执行 ELSE 分支
        if let Some(else_statements) = &cf.else_branch {
            for stmt in else_statements {
                self.execute_statement(stmt)?;
            }
        }
        
        Ok(())
    }
    
    /// 评估表达式
    fn evaluate_expression(&self, expr: &Expression) -> Result<bool, ExecutionError> {
        match expr {
            Expression::Literal(Value::Bool(b)) => Ok(*b),
            
            Expression::Variable(name) => {
                match self.context.get(name) {
                    Some(Value::Bool(b)) => Ok(b),
                    Some(_) => Err(ExecutionError::new(4001, format!("变量 {} 不是布尔值", name))),
                    None => Err(ExecutionError::new(4002, format!("未定义的变量: {}", name))),
                }
            }
            
            Expression::Binary { op, left, right } => {
                self.evaluate_binary_op(op, left, right)
            }
            
            Expression::Unary { op, expr } => {
                match op {
                    UnaryOp::Not => Ok(!self.evaluate_expression(expr)?),
                }
            }
            
            _ => Err(ExecutionError::new(4003, "不支持的表达式类型".to_string())),
        }
    }
    
    /// 评估二元操作
    fn evaluate_binary_op(
        &self,
        op: &BinaryOp,
        left: &Expression,
        right: &Expression,
    ) -> Result<bool, ExecutionError> {
        match op {
            BinaryOp::And => {
                Ok(self.evaluate_expression(left)? && self.evaluate_expression(right)?)
            }
            BinaryOp::Or => {
                Ok(self.evaluate_expression(left)? || self.evaluate_expression(right)?)
            }
            // 比较操作符 - 需要先求值为 Value
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                let left_val = self.evaluate_value(left)?;
                let right_val = self.evaluate_value(right)?;
                self.compare_values(op, &left_val, &right_val)
            }
        }
    }
    
    /// 评估表达式得到 Value
    fn evaluate_value(&self, expr: &Expression) -> Result<Value, ExecutionError> {
        match expr {
            Expression::Literal(val) => Ok(val.clone()),
            Expression::Variable(name) => {
                self.context.get(name)
                    .ok_or_else(|| ExecutionError::new(4002, format!("未定义的变量: {}", name)))
            }
            _ => Err(ExecutionError::new(4005, "表达式类型不支持作为值使用".to_string())),
        }
    }
    
    /// 比较两个值
    fn compare_values(&self, op: &BinaryOp, left: &Value, right: &Value) -> Result<bool, ExecutionError> {
        match (left, right) {
            // 数字比较
            (Value::Number(l), Value::Number(r)) => {
                Ok(match op {
                    BinaryOp::Eq => l == r,
                    BinaryOp::Ne => l != r,
                    BinaryOp::Gt => l > r,
                    BinaryOp::Lt => l < r,
                    BinaryOp::Ge => l >= r,
                    BinaryOp::Le => l <= r,
                    _ => return Err(ExecutionError::new(4004, format!("不支持的操作符: {:?}", op))),
                })
            }
            // 字符串比较
            (Value::String(l), Value::String(r)) => {
                Ok(match op {
                    BinaryOp::Eq => l == r,
                    BinaryOp::Ne => l != r,
                    BinaryOp::Gt => l > r,
                    BinaryOp::Lt => l < r,
                    BinaryOp::Ge => l >= r,
                    BinaryOp::Le => l <= r,
                    _ => return Err(ExecutionError::new(4004, format!("不支持的操作符: {:?}", op))),
                })
            }
            // 布尔值比较（只支持 == 和 !=）
            (Value::Bool(l), Value::Bool(r)) => {
                Ok(match op {
                    BinaryOp::Eq => l == r,
                    BinaryOp::Ne => l != r,
                    _ => return Err(ExecutionError::new(4006, format!("布尔值不支持 {:?} 操作", op))),
                })
            }
            // 类型不匹配
            _ => Err(ExecutionError::new(4007, format!("无法比较不同类型的值: {:?} 和 {:?}", left, right))),
        }
    }
    
    /// 执行 DOING
    fn execute_doing(&mut self, reference: &str) -> Result<(), ExecutionError> {
        if let Some(pipe) = self.data_pipes.get(reference).cloned() {
            self.output_buffer.push(format!("执行管道: {}", pipe.name));
            return Ok(());
        }
        
        if let Some(cmd) = self.commands.get(reference).cloned() {
            for stmt in &cmd.statements {
                self.execute_statement(stmt)?;
            }
            return Ok(());
        }
        
        Err(ExecutionError::new(
            4005,
            format!("未找到操作或命令: {}", reference),
        ))
    }
    
    /// 解析文件路径（处理变量插值）
    fn resolve_file_path(&self, path: &FilePath) -> Result<String, ExecutionError> {
        let mut result = String::new();
        
        for segment in &path.segments {
            match segment {
                PathSegment::Literal(s) => result.push_str(s),
                PathSegment::Variable(var) => {
                    if let Some(value) = self.context.get(var) {
                        result.push_str(&value.to_string().trim_matches('"'));
                    } else {
                        return Err(ExecutionError::new(
                            4006,
                            format!("路径中的变量未定义: {}", var),
                        ));
                    }
                }
            }
        }
        
        Ok(result)
    }
    
    /// 获取输出缓冲区（用于测试）
    #[allow(dead_code)]
    pub fn output(&self) -> &[String] {
        &self.output_buffer
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_print() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::Set {
                    key: "NAME".to_string(),
                    value: Value::String("Alice".to_string()),
                },
                Statement::Print("Hello ${NAME}!".to_string()),
            ],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("Alice"));
    }

    #[test]
    fn test_control_flow() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![Statement::ControlFlow(ControlFlow {
                if_branch: (
                    Expression::Literal(Value::Bool(true)),
                    vec![Statement::Print("条件为真".to_string())],
                ),
                elif_branches: vec![],
                else_branch: Some(vec![Statement::Print("条件为假".to_string())]),
            })],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        assert!(executor.output()[0].contains("条件为真"));
    }

    #[test]
    fn test_file_encryption() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![Statement::FilesEncry],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        assert!(executor.crypto.is_some());
    }
    
    #[test]
    fn test_data_re_basic() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![Statement::DataRe {
                pattern: r"\d+".to_string(),
            }],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        // 验证正则表达式已缓存
        assert_eq!(executor.regex_cache.size().expect("获取大小失败"), 1);
        assert!(executor.regex_cache.contains(r"\d+").expect("检查失败"));
    }
    
    #[test]
    fn test_data_re_multiple() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataRe {
                    pattern: r"\d+".to_string(),
                },
                Statement::DataRe {
                    pattern: r"[a-z]+".to_string(),
                },
                Statement::DataRe {
                    pattern: r"\d+".to_string(),  // 重复的模式
                },
            ],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        // 验证只缓存了2个不同的正则表达式
        assert_eq!(executor.regex_cache.size().expect("获取大小失败"), 2);
    }
    
    #[test]
    fn test_data_re_invalid() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![Statement::DataRe {
                pattern: r"[".to_string(),  // 无效的正则表达式
            }],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_data_re_complex_patterns() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataRe {
                    pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
                },
                Statement::DataRe {
                    pattern: r"^https?://[^\s/$.?#].[^\s]*$".to_string(),
                },
            ],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        assert_eq!(executor.regex_cache.size().expect("获取大小失败"), 2);
    }
    
    #[test]
    fn test_comparison_operators_numbers() {
        let mut executor = Executor::new();
        
        // 设置两个数字变量
        executor.context.set_local("a".to_string(), Value::Number(10));
        executor.context.set_local("b".to_string(), Value::Number(20));
        executor.context.set_local("c".to_string(), Value::Number(10));
        
        let ast = Ast {
            statements: vec![
                // 测试相等
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Eq,
                            left: Box::new(Expression::Variable("a".to_string())),
                            right: Box::new(Expression::Variable("c".to_string())),
                        },
                        vec![Statement::Print("a == c".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
                // 测试不等
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Ne,
                            left: Box::new(Expression::Variable("a".to_string())),
                            right: Box::new(Expression::Variable("b".to_string())),
                        },
                        vec![Statement::Print("a != b".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
                // 测试小于
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Lt,
                            left: Box::new(Expression::Variable("a".to_string())),
                            right: Box::new(Expression::Variable("b".to_string())),
                        },
                        vec![Statement::Print("a < b".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
                // 测试大于等于
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Ge,
                            left: Box::new(Expression::Variable("b".to_string())),
                            right: Box::new(Expression::Variable("a".to_string())),
                        },
                        vec![Statement::Print("b >= a".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
            ],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        // 验证所有条件都执行了
        let output = executor.output();
        assert_eq!(output.len(), 4);
        assert!(output[0].contains("a == c"));
        assert!(output[1].contains("a != b"));
        assert!(output[2].contains("a < b"));
        assert!(output[3].contains("b >= a"));
    }
    
    #[test]
    fn test_comparison_operators_strings() {
        let mut executor = Executor::new();
        
        // 设置字符串变量
        executor.context.set_local("str1".to_string(), Value::String("apple".to_string()));
        executor.context.set_local("str2".to_string(), Value::String("banana".to_string()));
        executor.context.set_local("str3".to_string(), Value::String("apple".to_string()));
        
        let ast = Ast {
            statements: vec![
                // 测试字符串相等
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Eq,
                            left: Box::new(Expression::Variable("str1".to_string())),
                            right: Box::new(Expression::Variable("str3".to_string())),
                        },
                        vec![Statement::Print("str1 == str3".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
                // 测试字符串字典序比较
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Lt,
                            left: Box::new(Expression::Variable("str1".to_string())),
                            right: Box::new(Expression::Variable("str2".to_string())),
                        },
                        vec![Statement::Print("str1 < str2".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
            ],
        };
        
        executor.execute(&ast).expect("执行失败");
        
        let output = executor.output();
        assert_eq!(output.len(), 2);
        assert!(output[0].contains("str1 == str3"));
        assert!(output[1].contains("str1 < str2"));
    }
    
    #[test]
    fn test_comparison_operators_mixed_types_error() {
        let mut executor = Executor::new();
        
        // 设置不同类型的变量
        executor.context.set_local("num".to_string(), Value::Number(10));
        executor.context.set_local("str".to_string(), Value::String("10".to_string()));
        
        let ast = Ast {
            statements: vec![
                Statement::ControlFlow(ControlFlow {
                    if_branch: (
                        Expression::Binary {
                            op: BinaryOp::Eq,
                            left: Box::new(Expression::Variable("num".to_string())),
                            right: Box::new(Expression::Variable("str".to_string())),
                        },
                        vec![Statement::Print("should not execute".to_string())],
                    ),
                    elif_branches: vec![],
                    else_branch: None,
                }),
            ],
        };
        
        // 应该返回错误，因为不能比较不同类型
        let result = executor.execute(&ast);
        assert!(result.is_err());
    }
}
