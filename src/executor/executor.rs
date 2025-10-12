/*
 * 执行器 (Executor)
 *
 * 执行解析后的 AST。
 * 处理变量解析、控制流、数据操作等。
 */

use crate::dsl::ast::*;
use crate::executor::context::ExecutionContext;
use crate::executor::crypto::CryptoOperations;
use crate::data::{RegexCache, Validator, SerializableValue, Compressor, CompressionLevel};
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
    
    /// 数据验证器
    validator: Validator,
    
    /// 数据压缩器
    compressor: Compressor,
    
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

/// 并发任务类型
///
/// 用于在 AWAIT 并发执行中标识不同类型的任务
#[derive(Debug, Clone)]
enum TaskType {
    /// 数据管道任务
    Pipe(DataPipeDef),
    /// 命令任务
    Command(CommandDef),
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
            validator: Validator::new(),
            compressor: Compressor::new(),
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
                // 使用 smol 实现真正的并发执行
                self.execute_await(references)
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
                // 实际执行子进程命令
                use std::process::Command;
                
                let resolved_exec = self.context.interpolate(exec);
                let resolved_args: Vec<String> = args.iter()
                    .map(|arg| self.context.interpolate(arg))
                    .collect();
                
                let result = Command::new(&resolved_exec)
                    .args(&resolved_args)
                    .output();
                
                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let status = output.status;
                        
                        // 存储命令输出到上下文
                        let cmd_output_key = format!("_cmd_output_{}", exec);
                        let cmd_status_key = format!("_cmd_status_{}", exec);
                        self.context.set_local(cmd_output_key, Value::String(stdout.to_string()));
                        self.context.set_local(cmd_status_key, Value::Number(status.code().unwrap_or(-1) as i64));
                        
                        if !status.success() {
                            self.output_buffer.push(format!(
                                "命令执行失败: {} {:?}, 状态码: {}, stderr: {}",
                                resolved_exec, resolved_args, status.code().unwrap_or(-1), stderr
                            ));
                            return Err(ExecutionError::new(
                                3001,
                                format!("命令执行失败，状态码: {}", status.code().unwrap_or(-1))
                            ));
                        }
                        
                        self.output_buffer.push(format!(
                            "命令执行成功: {} {:?}, 输出: {}",
                            resolved_exec, resolved_args, stdout.trim()
                        ));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            3002,
                            format!("无法执行命令 {}: {}", resolved_exec, e)
                        ))
                    }
                }
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
            
            Statement::DataVali { validator, key, value } => {
                // 实际执行数据验证
                let resolved_value = self.context.interpolate(value);
                
                // 根据验证器类型执行相应的验证
                let result = match validator.as_str() {
                    "NOT_EMPTY" => self.validator.validate_not_empty(key, &resolved_value),
                    "EMAIL" => self.validator.validate_email(key, &resolved_value),
                    "URL" => self.validator.validate_url(key, &resolved_value),
                    "NUMERIC" => self.validator.validate_numeric(key, &resolved_value),
                    "ALPHA" => self.validator.validate_alpha(key, &resolved_value),
                    "ALPHANUMERIC" => self.validator.validate_alphanumeric(key, &resolved_value),
                    _ => {
                        return Err(ExecutionError::new(
                            4020,
                            format!("未知的验证器: {}", validator)
                        ));
                    }
                };
                
                match result {
                    Ok(_) => {
                        self.output_buffer.push(format!("验证成功: {} 使用 {} 验证器", key, validator));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            e.code(),
                            format!("验证失败: {}", e.message())
                        ))
                    }
                }
            }
            
            Statement::DataSeria { format, value } => {
                // 实际执行序列化操作
                let resolved_value = self.context.interpolate(value);
                
                // 构造 SerializableValue
                // 简单实现：尝试将字符串转换为适当的类型
                let serializable = self.parse_to_serializable(&resolved_value)?;
                
                let format_str = match format {
                    SerializationFormat::Json => "JSON",
                    SerializationFormat::Bin => "BIN",
                };
                
                let result = match format {
                    SerializationFormat::Json => {
                        serializable.to_json().map(|s| s.into_bytes())
                    },
                    SerializationFormat::Bin => serializable.to_binary(),
                };
                
                match result {
                    Ok(data) => {
                        // 将序列化结果存储到上下文（以字节数转十六进制字符串形式）
                        let hex_data = data.iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        let serialized_key = format!("_serialized_{}", value);
                        self.context.set_local(serialized_key, Value::String(hex_data));
                        
                        self.output_buffer.push(format!("序列化成功: {} 使用 {} 格式，大小: {} 字节", value, format_str, data.len()));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            e.code(),
                            format!("序列化失败: {}", e.message())
                        ))
                    }
                }
            }
            
            Statement::DataDeseria { format, data } => {
                // 实际执行反序列化操作
                let resolved_data = self.context.interpolate(data);
                
                // 从十六进制字符串转换回字节数组
                let bytes = self.hex_to_bytes(&resolved_data)?;
                
                let format_str = match format {
                    SerializationFormat::Json => "JSON",
                    SerializationFormat::Bin => "BIN",
                };
                
                let result = match format {
                    SerializationFormat::Json => {
                        // 字节转字符串
                        String::from_utf8(bytes)
                            .map_err(|e| ErrorInfo::new(5002, format!("无效的 UTF-8 数据: {}", e)))
                            .and_then(|s| SerializableValue::from_json(&s))
                    },
                    SerializationFormat::Bin => SerializableValue::from_binary(&bytes),
                };
                
                match result {
                    Ok(value) => {
                        // 将反序列化结果存储到上下文
                        let deserialized_key = format!("_deserialized_{}", data);
                        self.context.set_local(deserialized_key, Value::String(format!("{:?}", value)));
                        
                        self.output_buffer.push(format!("反序列化成功: {} 从 {} 格式", data, format_str));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            e.code(),
                            format!("反序列化失败: {}", e.message())
                        ))
                    }
                }
            }
            
            Statement::DataComp { level, data } => {
                // 实际执行压缩操作
                let resolved_data = self.context.interpolate(data);
                let bytes = resolved_data.as_bytes();
                
                // 确定压缩级别
                let compression_level = if let Some(l) = level {
                    if *l == 1 {
                        CompressionLevel::Fast
                    } else if *l == 3 {
                        CompressionLevel::Default
                    } else if *l == 19 {
                        CompressionLevel::Best
                    } else {
                        CompressionLevel::Custom(*l as i32)
                    }
                } else {
                    CompressionLevel::Default
                };
                
                let result = self.compressor.compress(bytes, compression_level);
                
                match result {
                    Ok(compressed) => {
                        // 将压缩结果存储到上下文（十六进制字符串）
                        let hex_data = compressed.iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<String>();
                        let compressed_key = format!("_compressed_{}", data);
                        self.context.set_local(compressed_key, Value::String(hex_data));
                        
                        let level_str = level.map(|l| l.to_string()).unwrap_or_else(|| "default".to_string());
                        let ratio = Compressor::compression_ratio(bytes.len(), compressed.len());
                        self.output_buffer.push(format!(
                            "压缩成功: {} 使用级别 {}，原始: {} 字节，压缩后: {} 字节，压缩比: {:.2}%",
                            data, level_str, bytes.len(), compressed.len(), ratio
                        ));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            e.code(),
                            format!("压缩失败: {}", e.message())
                        ))
                    }
                }
            }
            
            Statement::DataDecomp { data } => {
                // 实际执行解压缩操作
                let resolved_data = self.context.interpolate(data);
                
                // 从十六进制字符串转换回字节数组
                let bytes = self.hex_to_bytes(&resolved_data)?;
                
                let result = self.compressor.decompress(&bytes);
                
                match result {
                    Ok(decompressed) => {
                        // 尝试将解压缩结果转换为字符串
                        let decompressed_str = String::from_utf8(decompressed.clone())
                            .unwrap_or_else(|_| format!("[{} 字节的二进制数据]", decompressed.len()));
                        
                        // 将解压缩结果存储到上下文
                        let decompressed_key = format!("_decompressed_{}", data);
                        self.context.set_local(decompressed_key, Value::String(decompressed_str.clone()));
                        
                        self.output_buffer.push(format!(
                            "解压缩成功: {}，解压后大小: {} 字节",
                            data, decompressed.len()
                        ));
                        Ok(())
                    }
                    Err(e) => {
                        Err(ExecutionError::new(
                            e.code(),
                            format!("解压缩失败: {}", e.message())
                        ))
                    }
                }
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
    
    /// 规范化引用名称
    /// 
    /// 将 "DATA.PIPE.name"、"DATA.DO.name"、"COMM.ACTION.name" 等完整引用
    /// 转换为简单名称 "name"，以便在 HashMap 中查找
    /// 
    /// # 参数
    /// - `reference`: 引用字符串，可能包含前缀
    /// 
    /// # 返回值
    /// 规范化后的名称
    fn normalize_reference(reference: &str) -> &str {
        // 支持的前缀列表
        const PREFIXES: &[&str] = &[
            "DATA.PIPE.",
            "DATA.DO.",
            "COMM.ACTION.",
            "COMM.",
        ];
        
        // 尝试剥离已知前缀
        for prefix in PREFIXES {
            if let Some(stripped) = reference.strip_prefix(prefix) {
                return stripped;
            }
        }
        
        // 如果没有匹配的前缀，返回原始引用
        reference
    }
    
    /// 执行 DOING
    fn execute_doing(&mut self, reference: &str) -> Result<(), ExecutionError> {
        // 规范化引用名称
        let normalized = Self::normalize_reference(reference);
        
        if let Some(pipe) = self.data_pipes.get(normalized).cloned() {
            self.output_buffer.push(format!("执行管道: {}", pipe.name));
            return Ok(());
        }
        
        if let Some(cmd) = self.commands.get(normalized).cloned() {
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
    
    /// 并发执行多个引用
    ///
    /// 使用 smol 异步运行时实现真正的并发执行。
    /// 所有引用的操作将在独立的任务中并发执行。
    ///
    /// # 参数
    /// * `references` - 要并发执行的引用列表
    ///
    /// # 返回
    /// 执行成功返回 Ok(())，任何一个任务失败则返回错误
    ///
    /// # 错误处理
    /// 如果任何一个任务失败，会立即返回错误，但其他任务会继续执行直到完成。
    fn execute_await(&mut self, references: &[String]) -> Result<(), ExecutionError> {
        use std::sync::{Arc, Mutex};
        
        // 如果只有一个引用，直接顺序执行以优化性能
        if references.len() <= 1 {
            for reference in references {
                self.execute_doing(reference)?;
            }
            return Ok(());
        }
        
        // 为每个引用准备执行数据
        // 我们需要克隆必要的数据以在并发任务间共享
        let mut task_data = Vec::new();
        
        for reference in references {
            let normalized = Self::normalize_reference(reference);
            
            // 检查引用是否存在，收集需要执行的内容
            if let Some(pipe) = self.data_pipes.get(normalized).cloned() {
                task_data.push((reference.clone(), TaskType::Pipe(pipe)));
            } else if let Some(cmd) = self.commands.get(normalized).cloned() {
                task_data.push((reference.clone(), TaskType::Command(cmd)));
            } else {
                return Err(ExecutionError::new(
                    4005,
                    format!("未找到操作或命令: {}", reference),
                ));
            }
        }
        
        // 使用 Arc<Mutex<>> 来共享输出缓冲区
        let outputs = Arc::new(Mutex::new(Vec::<String>::new()));
        let errors = Arc::new(Mutex::new(Vec::<String>::new()));
        
        // 克隆执行上下文，用于命令执行
        let context = self.context.clone();
        
        // 使用 smol 并发执行所有任务
        smol::block_on(async {
            let mut tasks = Vec::new();
            
            for (_reference, task_type) in task_data {
                let outputs = Arc::clone(&outputs);
                let errors = Arc::clone(&errors);
                let context = context.clone();
                
                let task = smol::spawn(async move {
                    match task_type {
                        TaskType::Pipe(pipe) => {
                            // 执行管道
                            let output = format!("执行管道: {}", pipe.name);
                            outputs.lock().unwrap().push(output);
                        }
                        TaskType::Command(cmd) => {
                            // 为每个命令创建一个独立的执行器
                            // 这样可以并发执行不同的命令而不会相互干扰
                            let mut isolated_executor = Executor::new();
                            isolated_executor.context = context;
                            
                            // 执行命令中的所有语句
                            for stmt in &cmd.statements {
                                match isolated_executor.execute_statement(stmt) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        errors.lock().unwrap().push(format!("命令 {} 执行失败: {}", cmd.name, e));
                                        return;
                                    }
                                }
                            }
                            
                            // 收集命令执行的输出
                            for output in isolated_executor.output_buffer {
                                outputs.lock().unwrap().push(output);
                            }
                        }
                    }
                });
                
                tasks.push(task);
            }
            
            // 等待所有任务完成
            for task in tasks {
                task.await;
            }
        });
        
        // 收集所有输出
        let collected_outputs = outputs.lock().unwrap();
        for output in collected_outputs.iter() {
            self.output_buffer.push(output.clone());
        }
        
        // 检查是否有错误
        let collected_errors = errors.lock().unwrap();
        if !collected_errors.is_empty() {
            return Err(ExecutionError::new(
                4007,
                format!("AWAIT 执行失败: {:?}", collected_errors),
            ));
        }
        
        Ok(())
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
    
    /// 解析字符串为 SerializableValue
    ///
    /// # 参数
    /// * `value` - 要解析的字符串
    ///
    /// # 返回
    /// 成功时返回 SerializableValue
    fn parse_to_serializable(&self, value: &str) -> Result<SerializableValue, ExecutionError> {
        // 尝试解析为不同的类型
        if value == "null" {
            return Ok(SerializableValue::Null);
        }
        
        if value == "true" {
            return Ok(SerializableValue::Bool(true));
        }
        
        if value == "false" {
            return Ok(SerializableValue::Bool(false));
        }
        
        // 尝试解析为整数
        if let Ok(i) = value.parse::<i64>() {
            return Ok(SerializableValue::Int(i));
        }
        
        // 尝试解析为浮点数
        if let Ok(f) = value.parse::<f64>() {
            return Ok(SerializableValue::Float(f));
        }
        
        // 默认作为字符串处理
        Ok(SerializableValue::String(value.to_string()))
    }
    
    /// 十六进制字符串转字节数组
    ///
    /// # 参数
    /// * `hex` - 十六进制字符串
    ///
    /// # 返回
    /// 成功时返回字节数组
    fn hex_to_bytes(&self, hex: &str) -> Result<Vec<u8>, ExecutionError> {
        // 移除可能的空格和换行符
        let hex = hex.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        
        if hex.len() % 2 != 0 {
            return Err(ExecutionError::new(
                5003,
                "十六进制字符串长度必须为偶数".to_string()
            ));
        }
        
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for i in (0..hex.len()).step_by(2) {
            let byte_str = &hex[i..i + 2];
            let byte = u8::from_str_radix(byte_str, 16)
                .map_err(|e| ExecutionError::new(
                    5004,
                    format!("无效的十六进制字符串: {}", e)
                ))?;
            bytes.push(byte);
        }
        
        Ok(bytes)
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
    
    #[test]
    fn test_doing_with_data_pipe_reference() {
        let mut executor = Executor::new();
        
        // 定义一个 DATA.PIPE
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "process".to_string(),
                    operations: vec![],
                },
                // 使用完整引用调用管道
                Statement::Doing("DATA.PIPE.process".to_string()),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "DOING(DATA.PIPE.process) 应该能找到名为 'process' 的管道");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("执行管道: process"));
    }
    
    #[test]
    fn test_doing_with_comm_action_reference() {
        let mut executor = Executor::new();
        
        // 定义一个 COMM.ACTION
        let ast = Ast {
            statements: vec![
                Statement::CommAction {
                    name: Some("test_action".to_string()),
                    statements: vec![
                        Statement::Print("Action executed".to_string()),
                    ],
                },
                // 使用完整引用调用动作
                Statement::Doing("COMM.ACTION.test_action".to_string()),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "DOING(COMM.ACTION.test_action) 应该能找到名为 'test_action' 的动作");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("Action executed"));
    }
    
    #[test]
    fn test_doing_with_simple_reference() {
        let mut executor = Executor::new();
        
        // 定义一个管道，使用简单名称调用
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "simple".to_string(),
                    operations: vec![],
                },
                // 使用简单名称调用
                Statement::Doing("simple".to_string()),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "DOING(simple) 应该能找到名为 'simple' 的管道");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("执行管道: simple"));
    }
    
    #[test]
    fn test_normalize_reference() {
        // 测试 DATA.PIPE 前缀
        assert_eq!(Executor::normalize_reference("DATA.PIPE.process"), "process");
        
        // 测试 DATA.DO 前缀
        assert_eq!(Executor::normalize_reference("DATA.DO.operation"), "operation");
        
        // 测试 COMM.ACTION 前缀
        assert_eq!(Executor::normalize_reference("COMM.ACTION.test"), "test");
        
        // 测试 COMM 前缀
        assert_eq!(Executor::normalize_reference("COMM.command"), "command");
        
        // 测试没有前缀的情况
        assert_eq!(Executor::normalize_reference("simple"), "simple");
        
        // 测试包含点的名称（但不匹配前缀）
        assert_eq!(Executor::normalize_reference("my.custom.name"), "my.custom.name");
        
        // 测试空字符串
        assert_eq!(Executor::normalize_reference(""), "");
    }
    
    #[test]
    fn test_await_with_normalized_references() {
        let mut executor = Executor::new();
        
        // 定义两个管道和一个动作
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "pipe1".to_string(),
                    operations: vec![],
                },
                Statement::DataPipe {
                    name: "pipe2".to_string(),
                    operations: vec![],
                },
                Statement::CommAction {
                    name: Some("action1".to_string()),
                    statements: vec![
                        Statement::Print("Action 1 executed".to_string()),
                    ],
                },
                // 使用 AWAIT 并发执行，带有完整引用
                Statement::Await(vec![
                    "DATA.PIPE.pipe1".to_string(),
                    "DATA.PIPE.pipe2".to_string(),
                    "COMM.ACTION.action1".to_string(),
                ]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "AWAIT 应该能处理带前缀的引用");
        
        // 验证所有操作都被执行
        let output = executor.output();
        assert_eq!(output.len(), 3);
        assert!(output[0].contains("执行管道: pipe1"));
        assert!(output[1].contains("执行管道: pipe2"));
        assert!(output[2].contains("Action 1 executed"));
    }
    
    #[test]
    fn test_integration_parser_executor_with_references() {
        use crate::dsl::parser::Parser;
        
        // 测试完整的解析和执行流程
        let dsl = r#"
            DATA.PIPE.testpipe()
            COMM.ACTION.testaction(
                PRINT("Action executed")
            )
            DOING(DATA.PIPE.testpipe)
            DOING(COMM.ACTION.testaction)
        "#;
        
        let mut parser = Parser::new(dsl).expect("解析器创建失败");
        let ast = parser.parse().expect("解析失败");
        
        let mut executor = Executor::new();
        let result = executor.execute(&ast);
        
        assert!(result.is_ok(), "执行应该成功");
        
        // 验证输出
        let output = executor.output();
        assert_eq!(output.len(), 2);
        assert!(output[0].contains("执行管道: testpipe"));
        assert!(output[1].contains("Action executed"));
    }
    
    #[test]
    fn test_data_vali_not_empty_success() {
        let mut executor = Executor::new();
        executor.context.set_local("name".to_string(), Value::String("Alice".to_string()));
        
        let ast = Ast {
            statements: vec![
                Statement::DataVali {
                    validator: "NOT_EMPTY".to_string(),
                    key: "name".to_string(),
                    value: "${name}".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "验证应该成功");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("验证成功"));
        assert!(executor.output()[0].contains("NOT_EMPTY"));
    }
    
    #[test]
    fn test_data_vali_not_empty_failure() {
        let mut executor = Executor::new();
        executor.context.set_local("empty".to_string(), Value::String("".to_string()));
        
        let ast = Ast {
            statements: vec![
                Statement::DataVali {
                    validator: "NOT_EMPTY".to_string(),
                    key: "empty".to_string(),
                    value: "${empty}".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_err(), "验证应该失败");
    }
    
    #[test]
    fn test_data_vali_email_success() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataVali {
                    validator: "EMAIL".to_string(),
                    key: "email".to_string(),
                    value: "test@example.com".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "邮箱验证应该成功");
        assert!(executor.output()[0].contains("验证成功"));
    }
    
    #[test]
    fn test_data_vali_email_failure() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataVali {
                    validator: "EMAIL".to_string(),
                    key: "email".to_string(),
                    value: "invalid-email".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_err(), "邮箱验证应该失败");
    }
    
    #[test]
    fn test_data_seria_json() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataSeria {
                    format: SerializationFormat::Json,
                    value: "42".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "序列化应该成功");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("序列化成功"));
        assert!(executor.output()[0].contains("JSON"));
        
        // 验证数据被存储到上下文
        let serialized = executor.context.get("_serialized_42");
        assert!(serialized.is_some(), "序列化数据应该被存储");
    }
    
    #[test]
    fn test_data_seria_bin() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataSeria {
                    format: SerializationFormat::Bin,
                    value: "Hello".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "二进制序列化应该成功");
        assert!(executor.output()[0].contains("BIN"));
    }
    
    #[test]
    fn test_data_comp_decomp_roundtrip() {
        let mut executor = Executor::new();
        executor.context.set_local("data".to_string(), Value::String("Hello, World! This is a test string for compression.".to_string()));
        
        let ast = Ast {
            statements: vec![
                // 压缩数据
                Statement::DataComp {
                    level: Some(3),
                    data: "${data}".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "压缩应该成功");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("压缩成功"));
        assert!(executor.output()[0].contains("级别 3"));
        
        // 获取压缩数据
        let compressed = executor.context.get("_compressed_${data}").expect("压缩数据应该被存储");
        
        // 解压缩数据
        let mut executor2 = Executor::new();
        executor2.context.set_local("compressed".to_string(), compressed);
        
        let ast2 = Ast {
            statements: vec![
                Statement::DataDecomp {
                    data: "${compressed}".to_string(),
                },
            ],
        };
        
        let result2 = executor2.execute(&ast2);
        assert!(result2.is_ok(), "解压缩应该成功");
        assert!(executor2.output()[0].contains("解压缩成功"));
    }
    
    #[test]
    fn test_data_comp_custom_level() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataComp {
                    level: Some(19),  // Best compression
                    data: "test data".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "自定义级别压缩应该成功");
        assert!(executor.output()[0].contains("级别 19"));
    }
    
    #[test]
    fn test_data_comp_default_level() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::DataComp {
                    level: None,  // Default level
                    data: "test data".to_string(),
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "默认级别压缩应该成功");
        assert!(executor.output()[0].contains("级别 default"));
    }
    
    #[test]
    fn test_comm_cmd_success() {
        let mut executor = Executor::new();
        
        let ast = Ast {
            statements: vec![
                Statement::CommCmd {
                    exec: "echo".to_string(),
                    args: vec!["Hello".to_string(), "World".to_string()],
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "命令执行应该成功");
        assert_eq!(executor.output().len(), 1);
        assert!(executor.output()[0].contains("命令执行成功"));
        assert!(executor.output()[0].contains("echo"));
        
        // 验证输出被存储到上下文
        let cmd_output = executor.context.get("_cmd_output_echo");
        assert!(cmd_output.is_some(), "命令输出应该被存储");
        
        let cmd_status = executor.context.get("_cmd_status_echo");
        assert!(cmd_status.is_some(), "命令状态应该被存储");
        if let Some(Value::Number(status)) = cmd_status {
            assert_eq!(status, 0, "命令状态应该为 0");
        }
    }
    
    #[test]
    fn test_comm_cmd_with_interpolation() {
        let mut executor = Executor::new();
        executor.context.set_local("message".to_string(), Value::String("test".to_string()));
        
        let ast = Ast {
            statements: vec![
                Statement::CommCmd {
                    exec: "echo".to_string(),
                    args: vec!["${message}".to_string()],
                },
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "带变量插值的命令执行应该成功");
        
        let cmd_output = executor.context.get("_cmd_output_echo");
        if let Some(Value::String(output)) = cmd_output {
            assert!(output.contains("test"), "输出应该包含插值后的值");
        }
    }
    
    #[test]
    fn test_parse_to_serializable() {
        let executor = Executor::new();
        
        // 测试 null
        let result = executor.parse_to_serializable("null");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SerializableValue::Null);
        
        // 测试布尔值
        let result = executor.parse_to_serializable("true");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SerializableValue::Bool(true));
        
        let result = executor.parse_to_serializable("false");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SerializableValue::Bool(false));
        
        // 测试整数
        let result = executor.parse_to_serializable("42");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SerializableValue::Int(42));
        
        // 测试浮点数
        let result = executor.parse_to_serializable("3.14");
        assert!(result.is_ok());
        if let SerializableValue::Float(f) = result.unwrap() {
            assert!((f - 3.14).abs() < 0.0001);
        } else {
            panic!("应该解析为浮点数");
        }
        
        // 测试字符串
        let result = executor.parse_to_serializable("Hello World");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SerializableValue::String("Hello World".to_string()));
    }
    
    #[test]
    fn test_hex_to_bytes() {
        let executor = Executor::new();
        
        // 测试简单的十六进制字符串
        let result = executor.hex_to_bytes("48656c6c6f");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"Hello");
        
        // 测试带空格的十六进制字符串
        let result = executor.hex_to_bytes("48 65 6c 6c 6f");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"Hello");
        
        // 测试无效的十六进制字符串（奇数长度）
        let result = executor.hex_to_bytes("48656c6c6");
        assert!(result.is_err());
        
        // 测试无效的十六进制字符
        let result = executor.hex_to_bytes("xyz");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_await_concurrent_execution() {
        let mut executor = Executor::new();
        
        // 定义多个命令动作用于并发测试
        let ast = Ast {
            statements: vec![
                Statement::CommAction {
                    name: Some("task1".to_string()),
                    statements: vec![
                        Statement::Print("Task 1 started".to_string()),
                        Statement::Print("Task 1 completed".to_string()),
                    ],
                },
                Statement::CommAction {
                    name: Some("task2".to_string()),
                    statements: vec![
                        Statement::Print("Task 2 started".to_string()),
                        Statement::Print("Task 2 completed".to_string()),
                    ],
                },
                Statement::CommAction {
                    name: Some("task3".to_string()),
                    statements: vec![
                        Statement::Print("Task 3 started".to_string()),
                        Statement::Print("Task 3 completed".to_string()),
                    ],
                },
                // 并发执行所有任务
                Statement::Await(vec![
                    "COMM.ACTION.task1".to_string(),
                    "COMM.ACTION.task2".to_string(),
                    "COMM.ACTION.task3".to_string(),
                ]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "并发执行应该成功");
        
        // 验证所有任务都被执行
        let output = executor.output();
        assert_eq!(output.len(), 6, "应该有 6 个输出（每个任务 2 个）");
        
        // 验证每个任务的输出都存在
        let output_str = output.join("\n");
        assert!(output_str.contains("Task 1 started"));
        assert!(output_str.contains("Task 1 completed"));
        assert!(output_str.contains("Task 2 started"));
        assert!(output_str.contains("Task 2 completed"));
        assert!(output_str.contains("Task 3 started"));
        assert!(output_str.contains("Task 3 completed"));
    }
    
    #[test]
    fn test_await_with_mixed_types() {
        let mut executor = Executor::new();
        
        // 混合管道和命令的并发执行
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "pipeline_a".to_string(),
                    operations: vec![],
                },
                Statement::DataPipe {
                    name: "pipeline_b".to_string(),
                    operations: vec![],
                },
                Statement::CommAction {
                    name: Some("action_c".to_string()),
                    statements: vec![
                        Statement::Print("Action C executing".to_string()),
                    ],
                },
                // 并发执行混合类型
                Statement::Await(vec![
                    "DATA.PIPE.pipeline_a".to_string(),
                    "DATA.PIPE.pipeline_b".to_string(),
                    "COMM.ACTION.action_c".to_string(),
                ]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "混合类型并发执行应该成功");
        
        let output = executor.output();
        assert_eq!(output.len(), 3);
        assert!(output.iter().any(|s| s.contains("pipeline_a")));
        assert!(output.iter().any(|s| s.contains("pipeline_b")));
        assert!(output.iter().any(|s| s.contains("Action C executing")));
    }
    
    #[test]
    fn test_await_single_reference_optimization() {
        let mut executor = Executor::new();
        
        // 测试单个引用的优化路径（应该顺序执行而非并发）
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "single_pipe".to_string(),
                    operations: vec![],
                },
                Statement::Await(vec![
                    "DATA.PIPE.single_pipe".to_string(),
                ]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "单引用 AWAIT 应该成功");
        
        let output = executor.output();
        assert_eq!(output.len(), 1);
        assert!(output[0].contains("single_pipe"));
    }
    
    #[test]
    fn test_await_with_nonexistent_reference() {
        let mut executor = Executor::new();
        
        // 测试不存在的引用应该返回错误
        let ast = Ast {
            statements: vec![
                Statement::DataPipe {
                    name: "existing_pipe".to_string(),
                    operations: vec![],
                },
                Statement::Await(vec![
                    "DATA.PIPE.existing_pipe".to_string(),
                    "DATA.PIPE.nonexistent_pipe".to_string(),
                ]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_err(), "不存在的引用应该返回错误");
        
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("未找到操作或命令"));
    }
    
    #[test]
    fn test_await_empty_list() {
        let mut executor = Executor::new();
        
        // 测试空的 AWAIT 列表
        let ast = Ast {
            statements: vec![
                Statement::Await(vec![]),
            ],
        };
        
        let result = executor.execute(&ast);
        assert!(result.is_ok(), "空的 AWAIT 列表应该成功（不执行任何操作）");
        
        let output = executor.output();
        assert_eq!(output.len(), 0, "空的 AWAIT 不应产生输出");
    }
}

