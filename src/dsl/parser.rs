/*
 * 语法分析器 (Parser)
 *
 * 使用 winnow 解析器组合子库将 Token 流转换为 AST。
 * 严格遵循 DSL 语法规则。
 */

use crate::dsl::ast::*;
use crate::dsl::lexer::{Token, Lexer};
use error::{ErrorInfo, ErrorCategory, ErrorSeverity};
use std::fmt;

/// 解析错误
#[derive(Debug)]
pub struct ParseError {
    info: ErrorInfo,
}

impl ParseError {
    /// 创建新的解析错误
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error),
        }
    }
    
    /// 带上下文的解析错误
    #[allow(dead_code)]
    pub fn with_context(code: u32, message: String, context: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message)
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
                .with_context(context),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for ParseError {}

/// 语法分析器
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    /// 创建新的语法分析器
    ///
    /// # 参数
    /// - `input`: DSL 源代码字符串
    ///
    /// # 返回值
    /// - `Ok(Parser)`: 成功创建的解析器
    /// - `Err(ParseError)`: 词法分析错误
    pub fn new(input: &str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()
            .map_err(|e| ParseError::new(1000, format!("词法分析失败: {}", e)))?;
        
        Ok(Self {
            tokens,
            position: 0,
        })
    }
    
    /// 解析完整的 DSL 程序
    ///
    /// # 返回值
    /// - `Ok(Ast)`: 解析成功的抽象语法树
    /// - `Err(ParseError)`: 语法分析错误
    pub fn parse(&mut self) -> Result<Ast, ParseError> {
        let mut statements = Vec::new();
        
        while !self.is_at_end() {
            // 跳过注释和换行
            if self.match_token(&Token::Newline) || matches!(self.current(), Token::Comment(_)) {
                self.advance();
                continue;
            }
            
            if self.current() == &Token::Eof {
                break;
            }
            
            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }
        
        Ok(Ast { statements })
    }
    
    /// 解析单个语句
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let token = self.current().clone();
        
        match &token {
            Token::Set => self.parse_set(),
            Token::If => self.parse_control_flow(),
            Token::Lib => self.parse_lib(),
            Token::Repo => self.parse_repo(),
            Token::Log => self.parse_log_statement(),
            Token::Error => self.parse_error_def(),
            Token::Data => self.parse_data_statement(),
            Token::Comm => self.parse_comm_statement(),
            Token::Print => self.parse_print(),
            Token::Doing => self.parse_doing(),
            Token::Await => self.parse_await(),
            Token::Files => self.parse_files_statement(),
            Token::Catch => self.parse_catch(),
            Token::Comment(c) => {
                let comment = c.clone();
                self.advance();
                Ok(Statement::Comment(comment))
            }
            _ => Err(ParseError::new(
                1001,
                format!("意外的 Token: {}", token),
            )),
        }
    }
    
    /// 解析 set 语句
    fn parse_set(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Set)?;
        
        // 检查是否是 set.env
        if self.match_token(&Token::Dot) {
            self.advance(); // 消费 .
            self.expect(&Token::Env)?;
            self.expect(&Token::LParen)?;
            
            let key = self.parse_identifier()?;
            self.expect(&Token::Comma)?;
            let value = self.parse_value()?;
            
            self.expect(&Token::RParen)?;
            
            return Ok(Statement::SetEnv { key, value });
        }
        
        // 普通 set
        self.expect(&Token::LParen)?;
        let key = self.parse_identifier()?;
        self.expect(&Token::Comma)?;
        let value = self.parse_value()?;
        self.expect(&Token::RParen)?;
        
        Ok(Statement::Set { key, value })
    }
    
    /// 解析控制流语句
    fn parse_control_flow(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::If)?;
        let if_condition = self.parse_expression()?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        
        let if_body = self.parse_block()?;
        
        let mut elif_branches = Vec::new();
        while self.match_token(&Token::Elif) {
            self.advance();
            let condition = self.parse_expression()?;
            self.expect(&Token::Colon)?;
            self.skip_newlines();
            let body = self.parse_block()?;
            elif_branches.push((condition, body));
        }
        
        let else_branch = if self.match_token(&Token::Else) {
            self.advance();
            self.expect(&Token::Colon)?;
            self.skip_newlines();
            Some(self.parse_block()?)
        } else {
            None
        };
        
        Ok(Statement::ControlFlow(ControlFlow {
            if_branch: (if_condition, if_body),
            elif_branches,
            else_branch,
        }))
    }
    
    /// 解析代码块（缩进敏感）
    fn parse_block(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();
        
        // 简化实现：解析直到遇到 ELIF, ELSE 或下一个顶层语句
        while !self.is_at_end() 
            && !self.match_token(&Token::Elif) 
            && !self.match_token(&Token::Else)
            && !self.is_block_terminator()
        {
            if self.match_token(&Token::Newline) {
                self.advance();
                continue;
            }
            
            if matches!(self.current(), Token::Comment(_)) {
                self.advance();
                continue;
            }
            
            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }
        
        Ok(statements)
    }
    
    /// 检查是否是控制流块的终止符
    fn is_block_terminator(&self) -> bool {
        matches!(
            self.current(),
            Token::Set | Token::If | Token::Lib | Token::Repo
        )
    }
    
    /// 检查是否是顶层关键字（用于确定语句块结束）
    fn is_top_level_keyword(&self) -> bool {
        matches!(
            self.current(),
            Token::Set | Token::If | Token::Print | 
            Token::Data | Token::Comm | Token::Log | Token::Error | 
            Token::Doing | Token::Await | Token::Files | Token::Catch
        )
    }
    
    /// 解析 lib 定义
    fn parse_lib(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Lib)?;
        self.skip_newlines();
        
        // 解析 lib 块的字段
        let mut name = None;
        let mut version = None;
        let mut desc = None;
        let mut repo = None;
        let mut keywords = None;
        let mut readme = None;
        let mut mods = Vec::new();
        let mut out_dir = None;
        
        while !self.is_at_end() && !self.is_top_level_keyword() {
            if self.match_token(&Token::Newline) {
                self.advance();
                continue;
            }
            
            // 跳过注释
            if matches!(self.current(), Token::Comment(_)) {
                self.advance();
                continue;
            }
            
            let field_name = self.parse_identifier()?;
            self.expect(&Token::Colon)?;
            
            match field_name.as_str() {
                "name" => {
                    name = Some(self.parse_string()?);
                }
                "version" => {
                    let ver_str = self.parse_string()?;
                    version = Some(semver::Version::parse(&ver_str)
                        .map_err(|e| ParseError::new(1002, format!("无效的版本号: {}", e)))?);
                }
                "desc" => {
                    desc = Some(self.parse_string()?);
                }
                "repo" => {
                    repo = Some(self.parse_string()?);
                }
                "keywords" => {
                    keywords = Some(self.parse_string_list()?);
                }
                "readme" => {
                    readme = Some(self.parse_string()?);
                }
                "mods" => {
                    mods = self.parse_string_list()?;
                }
                "out_dir" => {
                    out_dir = Some(self.parse_string()?);
                }
                _ => {
                    return Err(ParseError::new(1003, format!("未知的 lib 字段: {}", field_name)));
                }
            }
            
            self.skip_newlines();
        }
        
        // 验证必填字段
        let name = name.ok_or_else(|| ParseError::new(1004, "lib 缺少 name 字段".to_string()))?;
        let version = version.ok_or_else(|| ParseError::new(1004, "lib 缺少 version 字段".to_string()))?;
        let desc = desc.ok_or_else(|| ParseError::new(1004, "lib 缺少 desc 字段".to_string()))?;
        let repo = repo.ok_or_else(|| ParseError::new(1004, "lib 缺少 repo 字段".to_string()))?;
        let keywords = keywords.ok_or_else(|| ParseError::new(1004, "lib 缺少 keywords 字段".to_string()))?;
        let readme = readme.ok_or_else(|| ParseError::new(1004, "lib 缺少 readme 字段".to_string()))?;
        let out_dir = out_dir.ok_or_else(|| ParseError::new(1004, "lib 缺少 out_dir 字段".to_string()))?;
        
        Ok(Statement::Lib(LibDefinition {
            name,
            version,
            desc,
            repo,
            keywords,
            readme,
            mods,
            out_dir,
        }))
    }
    
    /// 解析 repo 定义
    fn parse_repo(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Repo)?;
        self.skip_newlines();
        
        let mut name = None;
        let mut capacity = None;
        let mut max_pkgs = None;
        
        while !self.is_at_end() && !self.is_top_level_keyword() {
            if self.match_token(&Token::Newline) {
                self.advance();
                continue;
            }
            
            // 跳过注释
            if matches!(self.current(), Token::Comment(_)) {
                self.advance();
                continue;
            }
            
            let field_name = self.parse_identifier()?;
            self.expect(&Token::Colon)?;
            
            match field_name.as_str() {
                "name" => {
                    name = Some(self.parse_string()?);
                }
                "capacity" => {
                    let num = self.parse_number()?;
                    capacity = Some(num as u64);
                    // 期望 MiB 单位
                    let unit = self.parse_identifier()?;
                    if unit != "MiB" {
                        return Err(ParseError::new(1005, format!("容量单位必须是 MiB，得到: {}", unit)));
                    }
                }
                "max_pkgs" => {
                    let num = self.parse_number()?;
                    max_pkgs = Some(num as u64);
                }
                _ => {
                    return Err(ParseError::new(1006, format!("未知的 repo 字段: {}", field_name)));
                }
            }
            
            self.skip_newlines();
        }
        
        let name = name.ok_or_else(|| ParseError::new(1007, "repo 缺少 name 字段".to_string()))?;
        let capacity = capacity.ok_or_else(|| ParseError::new(1007, "repo 缺少 capacity 字段".to_string()))?;
        let max_pkgs = max_pkgs.ok_or_else(|| ParseError::new(1007, "repo 缺少 max_pkgs 字段".to_string()))?;
        
        Ok(Statement::Repo(RepoDefinition {
            name,
            capacity,
            max_pkgs,
        }))
    }
    
    /// 解析 LOG 相关语句
    fn parse_log_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Log)?;
        self.expect(&Token::Dot)?;
        
        let log_type = self.current().clone();
        
        match log_type {
            Token::Init => {
                self.advance();
                self.expect(&Token::LParen)?;
                let dir = self.parse_string()?;
                self.expect(&Token::Comma)?;
                let print = self.parse_bool()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::LogInit { dir, print })
            }
            Token::Info => {
                self.advance();
                self.expect(&Token::LParen)?;
                let msg = self.parse_string()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::LogInfo(msg))
            }
            Token::Error => {
                self.advance();
                self.expect(&Token::LParen)?;
                let error_ref = self.parse_identifier()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::LogError(error_ref))
            }
            _ => Err(ParseError::new(1008, format!("无效的 LOG 类型: {}", log_type))),
        }
    }
    
    /// 解析 ERROR 定义
    fn parse_error_def(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Error)?;
        let name = self.parse_identifier()?;
        self.expect(&Token::LParen)?;
        
        // 简化实现：只解析 print 参数
        let _print_label = self.parse_identifier()?; // "print"
        self.expect(&Token::Colon)?;
        let print_msg = self.parse_string()?;
        
        // 可选的其他参数
        while self.match_token(&Token::Comma) {
            self.advance();
            let _param = self.parse_identifier()?;
            self.expect(&Token::Colon)?;
            let _type = self.parse_identifier()?;
        }
        
        self.expect(&Token::RParen)?;
        
        Ok(Statement::ErrorDef { name, print: print_msg })
    }
    
    /// 解析 DATA 相关语句
    fn parse_data_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Data)?;
        self.expect(&Token::Dot)?;
        
        let data_type = self.current().clone();
        
        match data_type {
            Token::Do => {
                self.advance();
                self.expect(&Token::LParen)?;
                let name = self.parse_identifier()?;
                self.expect(&Token::Comma)?;
                let file = self.parse_string()?;
                self.expect(&Token::Comma)?;
                let action = self.parse_identifier()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::DataDo { name, file, action })
            }
            Token::Pipe => {
                self.advance();
                self.expect(&Token::Dot)?;
                let name = self.parse_identifier()?;
                self.expect(&Token::LParen)?;
                
                let mut operations = Vec::new();
                while !self.match_token(&Token::RParen) {
                    let op_ref = self.parse_identifier()?;
                    
                    // 判断是 do 还是 pipe 引用
                    let operation = if op_ref.starts_with("do.") {
                        PipeOperation::DoRef(op_ref)
                    } else if op_ref.starts_with("pipe.") {
                        PipeOperation::PipeRef(op_ref)
                    } else {
                        // 默认假设是 do 引用
                        PipeOperation::DoRef(op_ref)
                    };
                    
                    operations.push(operation);
                    
                    if !self.match_token(&Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                
                self.expect(&Token::RParen)?;
                Ok(Statement::DataPipe { name, operations })
            }
            Token::Re => {
                self.advance();
                self.expect(&Token::LParen)?;
                let pattern = self.parse_string()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::DataRe { pattern })
            }
            _ => Err(ParseError::new(1009, format!("无效的 DATA 类型: {}", data_type))),
        }
    }
    
    /// 解析 COMM 相关语句
    fn parse_comm_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Comm)?;
        self.expect(&Token::Dot)?;
        
        let comm_type = self.current().clone();
        
        match comm_type {
            Token::Action => {
                self.advance();
                
                // 可选的命名
                let name = if self.match_token(&Token::Dot) {
                    self.advance();
                    Some(self.parse_identifier()?)
                } else {
                    None
                };
                
                self.expect(&Token::LParen)?;
                self.skip_newlines();
                
                let mut statements = Vec::new();
                while !self.match_token(&Token::RParen) {
                    if self.match_token(&Token::Newline) {
                        self.advance();
                        continue;
                    }
                    
                    let stmt = self.parse_statement()?;
                    statements.push(stmt);
                    self.skip_newlines();
                }
                
                self.expect(&Token::RParen)?;
                Ok(Statement::CommAction { name, statements })
            }
            Token::Cmd => {
                self.advance();
                self.expect(&Token::LParen)?;
                let exec = self.parse_string()?;
                
                let mut args = Vec::new();
                while self.match_token(&Token::Comma) {
                    self.advance();
                    args.push(self.parse_string()?);
                }
                
                self.expect(&Token::RParen)?;
                Ok(Statement::CommCmd { exec, args })
            }
            Token::Ident(name) => {
                let comm_name = name.clone();
                self.advance();
                self.expect(&Token::LParen)?;
                let action_name = self.parse_identifier()?;
                self.expect(&Token::RParen)?;
                Ok(Statement::Comm { name: comm_name, action_name })
            }
            _ => Err(ParseError::new(1010, format!("无效的 COMM 类型: {}", comm_type))),
        }
    }
    
    /// 解析 PRINT 语句
    fn parse_print(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Print)?;
        self.expect(&Token::LParen)?;
        let msg = self.parse_string()?;
        self.expect(&Token::RParen)?;
        Ok(Statement::Print(msg))
    }
    
    /// 解析 DOING 语句
    fn parse_doing(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Doing)?;
        self.expect(&Token::LParen)?;
        let reference = self.parse_identifier()?;
        self.expect(&Token::RParen)?;
        Ok(Statement::Doing(reference))
    }
    
    /// 解析 AWAIT 语句
    fn parse_await(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Await)?;
        self.expect(&Token::LParen)?;
        
        let mut references = Vec::new();
        while !self.match_token(&Token::RParen) {
            references.push(self.parse_identifier()?);
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }
        
        self.expect(&Token::RParen)?;
        Ok(Statement::Await(references))
    }
    
    /// 解析 FILES 相关语句
    fn parse_files_statement(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Files)?;
        
        // 检查 FILES.ALL 或 FILES.ENCRY
        if self.match_token(&Token::Dot) {
            self.advance();
            let files_type = self.current().clone();
            
            match files_type {
                Token::All => {
                    self.advance();
                    self.expect(&Token::LParen)?;
                    
                    // 解析 other:[string]
                    let _other_label = self.parse_identifier()?; // "other"
                    self.expect(&Token::Colon)?;
                    let exclude = self.parse_string_list()?;
                    
                    self.expect(&Token::RParen)?;
                    Ok(Statement::FilesAll { exclude })
                }
                Token::Encry => {
                    self.advance();
                    self.expect(&Token::LParen)?;
                    self.expect(&Token::RParen)?;
                    Ok(Statement::FilesEncry)
                }
                _ => Err(ParseError::new(1011, format!("无效的 FILES 类型: {}", files_type))),
            }
        } else {
            // 普通 FILES(paths...)
            self.expect(&Token::LParen)?;
            self.skip_newlines();
            
            let mut paths = Vec::new();
            while !self.match_token(&Token::RParen) {
                if self.match_token(&Token::Newline) {
                    self.advance();
                    continue;
                }
                
                let path_str = self.parse_string()?;
                let file_path = FilePath::parse(&path_str);
                
                // 验证路径
                file_path.validate()
                    .map_err(|e| ParseError::new(1012, e.to_string()))?;
                
                paths.push(file_path);
                
                // 检查逗号分隔符
                if !self.match_token(&Token::Comma) {
                    break;
                }
                self.advance();
                self.skip_newlines();
            }
            
            self.skip_newlines(); // 跳过右括号前的换行
            self.expect(&Token::RParen)?;
            Ok(Statement::Files(paths))
        }
    }
    
    /// 解析 CATCH 语句
    fn parse_catch(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Catch)?;
        let error_name = self.parse_identifier()?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        
        let statements = self.parse_block()?;
        
        Ok(Statement::Catch { error_name, statements })
    }
    
    /// 解析表达式
    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_or_expression()
    }
    
    fn parse_or_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_and_expression()?;
        
        while self.match_token(&Token::Or) {
            self.advance();
            let right = self.parse_and_expression()?;
            left = Expression::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_and_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_comparison_expression()?;
        
        while self.match_token(&Token::And) {
            self.advance();
            let right = self.parse_comparison_expression()?;
            left = Expression::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_comparison_expression(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_unary_expression()?;
        
        if let Some(op) = self.parse_comparison_op() {
            self.advance();
            let right = self.parse_unary_expression()?;
            left = Expression::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_unary_expression(&mut self) -> Result<Expression, ParseError> {
        if self.match_token(&Token::Not) {
            self.advance();
            let expr = self.parse_unary_expression()?;
            return Ok(Expression::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }
        
        self.parse_primary_expression()
    }
    
    fn parse_primary_expression(&mut self) -> Result<Expression, ParseError> {
        let token = self.current().clone();
        
        match token {
            Token::String(s) => {
                self.advance();
                Ok(Expression::Literal(Value::String(s)))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Expression::Literal(Value::Number(n)))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Expression::Literal(Value::Bool(b)))
            }
            Token::Ident(id) => {
                self.advance();
                Ok(Expression::Variable(id))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            _ => Err(ParseError::new(1013, format!("意外的表达式 Token: {}", token))),
        }
    }
    
    fn parse_comparison_op(&self) -> Option<BinaryOp> {
        match self.current() {
            Token::Eq => Some(BinaryOp::Eq),
            Token::Ne => Some(BinaryOp::Ne),
            Token::Gt => Some(BinaryOp::Gt),
            Token::Lt => Some(BinaryOp::Lt),
            Token::Ge => Some(BinaryOp::Ge),
            Token::Le => Some(BinaryOp::Le),
            _ => None,
        }
    }
    
    // 辅助解析方法
    
    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let mut parts = Vec::new();
        
        match self.current() {
            Token::Ident(id) => {
                parts.push(id.clone());
                self.advance();
            }
            // 处理可能作为标识符使用的关键字
            Token::Error => {
                parts.push("ERROR".to_string());
                self.advance();
            }
            Token::Data => {
                parts.push("DATA".to_string());
                self.advance();
            }
            Token::Pipe => {
                parts.push("PIPE".to_string());
                self.advance();
            }
            Token::Do => {
                parts.push("DO".to_string());
                self.advance();
            }
            Token::Comm => {
                parts.push("COMM".to_string());
                self.advance();
            }
            Token::Action => {
                parts.push("ACTION".to_string());
                self.advance();
            }
            Token::Repo => {
                parts.push("repo".to_string());
                self.advance();
            }
            Token::Lib => {
                parts.push("lib".to_string());
                self.advance();
            }
            Token::Set => {
                parts.push("set".to_string());
                self.advance();
            }
            Token::Env => {
                parts.push("env".to_string());
                self.advance();
            }
            _ => {
                return Err(ParseError::new(1014, format!("期望标识符，得到: {}", self.current())));
            }
        }
        
        // 处理限定标识符 (如 DATA.DO.name)
        while self.match_token(&Token::Dot) {
            self.advance(); // 消费 .
            
            match self.current() {
                Token::Ident(id) => {
                    parts.push(id.clone());
                    self.advance();
                }
                Token::Do => {
                    parts.push("DO".to_string());
                    self.advance();
                }
                Token::Pipe => {
                    parts.push("PIPE".to_string());
                    self.advance();
                }
                Token::Action => {
                    parts.push("ACTION".to_string());
                    self.advance();
                }
                Token::Cmd => {
                    parts.push("CMD".to_string());
                    self.advance();
                }
                Token::Info => {
                    parts.push("INFO".to_string());
                    self.advance();
                }
                Token::Error => {
                    parts.push("ERROR".to_string());
                    self.advance();
                }
                _ => {
                    return Err(ParseError::new(1014, format!("期望标识符部分，得到: {}", self.current())));
                }
            }
        }
        
        Ok(parts.join("."))
    }
    
    fn parse_string(&mut self) -> Result<String, ParseError> {
        match self.current() {
            Token::String(s) => {
                let result = s.clone();
                self.advance();
                Ok(result)
            }
            _ => Err(ParseError::new(1015, format!("期望字符串，得到: {}", self.current()))),
        }
    }
    
    fn parse_number(&mut self) -> Result<i64, ParseError> {
        match self.current() {
            Token::Number(n) => {
                let result = *n;
                self.advance();
                Ok(result)
            }
            _ => Err(ParseError::new(1016, format!("期望数字，得到: {}", self.current()))),
        }
    }
    
    fn parse_bool(&mut self) -> Result<bool, ParseError> {
        match self.current() {
            Token::Bool(b) => {
                let result = *b;
                self.advance();
                Ok(result)
            }
            _ => Err(ParseError::new(1017, format!("期望布尔值，得到: {}", self.current()))),
        }
    }
    
    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match self.current() {
            Token::String(s) => {
                let result = Value::String(s.clone());
                self.advance();
                Ok(result)
            }
            Token::Number(n) => {
                let result = Value::Number(*n);
                self.advance();
                Ok(result)
            }
            Token::Bool(b) => {
                let result = Value::Bool(*b);
                self.advance();
                Ok(result)
            }
            Token::LBracket => {
                self.advance();
                let list = self.parse_value_list()?;
                self.expect(&Token::RBracket)?;
                Ok(Value::List(list))
            }
            _ => Err(ParseError::new(1018, format!("期望值，得到: {}", self.current()))),
        }
    }
    
    fn parse_value_list(&mut self) -> Result<Vec<Value>, ParseError> {
        let mut values = Vec::new();
        
        while !self.match_token(&Token::RBracket) {
            values.push(self.parse_value()?);
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }
        
        Ok(values)
    }
    
    fn parse_string_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&Token::LBracket)?;
        let mut strings = Vec::new();
        
        while !self.match_token(&Token::RBracket) {
            strings.push(self.parse_string()?);
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.advance();
        }
        
        self.expect(&Token::RBracket)?;
        Ok(strings)
    }
    
    // Token 流操作
    
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len() || self.current() == &Token::Eof
    }
    
    fn current(&self) -> &Token {
        if self.position < self.tokens.len() {
            &self.tokens[self.position]
        } else {
            &Token::Eof
        }
    }
    
    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.position += 1;
        }
        self.current()
    }
    
    fn match_token(&self, token: &Token) -> bool {
        if self.is_at_end() {
            return false;
        }
        
        // 使用模式匹配来比较 Token
        match (self.current(), token) {
            (Token::Ident(_), Token::Ident(_)) => false, // 标识符不能用 match_token
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }
    
    fn expect(&mut self, token: &Token) -> Result<(), ParseError> {
        if self.match_token(token) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::new(
                1019,
                format!("期望 {}，得到: {}", token, self.current()),
            ))
        }
    }
    
    fn skip_newlines(&mut self) {
        while self.match_token(&Token::Newline) {
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set() {
        let mut parser = Parser::new("set(KEY, \"value\")").expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        assert!(matches!(ast.statements[0], Statement::Set { .. }));
    }

    #[test]
    fn test_parse_set_env() {
        let mut parser = Parser::new("set.env(PATH, \"/usr/bin\")").expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        assert!(matches!(ast.statements[0], Statement::SetEnv { .. }));
    }

    #[test]
    fn test_parse_print() {
        let mut parser = Parser::new("PRINT(\"Hello World\")").expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        assert!(matches!(ast.statements[0], Statement::Print(_)));
    }

    #[test]
    fn test_parse_control_flow() {
        let input = r#"
IF true:
    PRINT("true")
ELSE:
    PRINT("false")
"#;
        let mut parser = Parser::new(input).expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        assert!(matches!(ast.statements[0], Statement::ControlFlow(_)));
    }

    #[test]
    fn test_parse_files() {
        let mut parser = Parser::new(r#"FILES("path/to/file.txt", "${VAR}/file.txt")"#)
            .expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        assert!(matches!(ast.statements[0], Statement::Files(_)));
    }
    
    #[test]
    fn test_parse_lib_complete() {
        let input = r#"lib
    name:"test"
    version:"1.0.0"
    desc:"test desc"
    repo:"https://example.com"
    keywords:["test"]
    readme:"README.md"
    out_dir:"./out"
"#;
        
        let mut parser = Parser::new(input).expect("Failed to create parser");
        let result = parser.parse();
        
        match &result {
            Ok(a) => {
                assert_eq!(a.statements.len(), 1);
                assert!(matches!(a.statements[0], Statement::Lib(_)));
            }
            Err(e) => {
                panic!("Failed to parse lib: {}", e);
            }
        }
    }
    
    #[test]
    fn test_parse_data_re() {
        let input = r#"DATA.RE("\\d+")"#;
        let mut parser = Parser::new(input).expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        
        match &ast.statements[0] {
            Statement::DataRe { pattern } => {
                assert_eq!(pattern, r"\d+");
            }
            _ => panic!("Expected DataRe statement"),
        }
    }
    
    #[test]
    fn test_parse_data_re_complex() {
        let input = r#"DATA.RE("^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$")"#;
        let mut parser = Parser::new(input).expect("Failed to create parser");
        let ast = parser.parse().expect("Failed to parse");
        assert_eq!(ast.statements.len(), 1);
        
        match &ast.statements[0] {
            Statement::DataRe { pattern } => {
                assert_eq!(pattern, r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$");
            }
            _ => panic!("Expected DataRe statement"),
        }
    }
}
