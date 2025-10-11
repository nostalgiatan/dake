/*
 * AST (抽象语法树) 定义
 *
 * 定义 DSL 的所有语法结构的抽象语法树表示。
 * 遵循显式设计原则，所有类型都是显式定义的。
 */

use std::fmt;

/// 完整的 AST 表示
#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    /// 语句列表
    pub statements: Vec<Statement>,
}

/// DSL 语句
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// 变量/常量定义: set(KEY, Value)
    Set { key: String, value: Value },
    
    /// 环境变量定义: set.env(KEY, VALUE)
    SetEnv { key: String, value: Value },
    
    /// 控制流语句
    ControlFlow(ControlFlow),
    
    /// 数据包结构定义
    Lib(LibDefinition),
    
    /// 仓库结构定义
    Repo(RepoDefinition),
    
    /// 日志初始化: LOG.INIT(dir:string, print:bool)
    LogInit { dir: String, print: bool },
    
    /// 错误定义: ERROR EXAMPLEERROR(print:string, e:error)
    ErrorDef { name: String, print: String },
    
    /// 数据操作: DATA.DO(name, file, 行为)
    DataDo { name: String, file: String, action: String },
    
    /// 数据管道: DATA.PIPE.name(do_refs...)
    DataPipe { name: String, operations: Vec<PipeOperation> },
    
    /// 命令定义: COMM.name(action_name)
    Comm { name: String, action_name: String },
    
    /// 命令动作: COMM.ACTION(statements...)
    CommAction { name: Option<String>, statements: Vec<Statement> },
    
    /// 打印语句: PRINT(string)
    Print(String),
    
    /// 执行语句: DOING(reference)
    Doing(String),
    
    /// 并发执行: AWAIT(references...)
    Await(Vec<String>),
    
    /// 文件列表: FILES(paths...)
    Files(Vec<FilePath>),
    
    /// 递归文件列表: FILES.ALL(other:[string])
    FilesAll { exclude: Vec<String> },
    
    /// 文件加密: FILES.ENCRY()
    FilesEncry,
    
    /// 日志信息: LOG.INFO(string)
    LogInfo(String),
    
    /// 日志错误: LOG.ERROR(error)
    LogError(String),
    
    /// 命令执行: COMM.CMD(exec, args...)
    CommCmd { exec: String, args: Vec<String> },
    
    /// 异常捕获: CATCH ERROR.name: statements
    Catch { error_name: String, statements: Vec<Statement> },
    
    /// 注释 (通常在解析时忽略，但可以保留用于文档生成)
    Comment(String),
}

/// 控制流结构
#[derive(Debug, Clone, PartialEq)]
pub struct ControlFlow {
    /// IF 条件和执行块
    pub if_branch: (Expression, Vec<Statement>),
    
    /// ELIF 分支列表
    pub elif_branches: Vec<(Expression, Vec<Statement>)>,
    
    /// ELSE 分支 (可选)
    pub else_branch: Option<Vec<Statement>>,
}

/// 数据包定义
#[derive(Debug, Clone, PartialEq)]
pub struct LibDefinition {
    pub name: String,
    pub version: semver::Version,
    pub desc: String,
    pub repo: String,
    pub keywords: Vec<String>,
    pub readme: String,
    pub mods: Vec<String>, // 可选
    pub out_dir: String,
}

/// 仓库定义
#[derive(Debug, Clone, PartialEq)]
pub struct RepoDefinition {
    pub name: String,
    pub capacity: u64, // MiB
    pub max_pkgs: u64,
}

/// 管道操作引用
#[derive(Debug, Clone, PartialEq)]
pub enum PipeOperation {
    /// 引用 DATA.DO
    DoRef(String),
    /// 引用另一个 DATA.PIPE
    PipeRef(String),
}

/// 文件路径表示
#[derive(Debug, Clone, PartialEq)]
pub struct FilePath {
    /// 路径段，支持变量插值
    pub segments: Vec<PathSegment>,
}

/// 路径段
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// 字面量路径
    Literal(String),
    /// 变量插值 ${KEY}
    Variable(String),
}

/// DSL 表达式
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// 变量引用
    Variable(String),
    
    /// 字面量值
    Literal(Value),
    
    /// 二元操作
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    
    /// 一元操作
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
    },
}

/// 二元操作符
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// 相等 ==
    Eq,
    /// 不等 !=
    Ne,
    /// 大于 >
    Gt,
    /// 小于 <
    Lt,
    /// 大于等于 >=
    Ge,
    /// 小于等于 <=
    Le,
    /// 逻辑与 &&
    And,
    /// 逻辑或 ||
    Or,
}

/// 一元操作符
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// 逻辑非 !
    Not,
}

/// DSL 值类型
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 字符串
    String(String),
    
    /// 整数 (num)
    Number(i64),
    
    /// 布尔值
    Bool(bool),
    
    /// 列表
    List(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Number(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl FilePath {
    /// 验证路径是否合法 (不包含 ../, .../ 或 ./)
    pub fn validate(&self) -> Result<(), &'static str> {
        for segment in &self.segments {
            if let PathSegment::Literal(s) = segment {
                if s.contains("../") || s.contains(".../") || s.starts_with("./") {
                    return Err("路径禁止使用 ../、.../ 和 ./");
                }
            }
        }
        Ok(())
    }
    
    /// 解析路径字符串为 FilePath
    pub fn parse(path: &str) -> Self {
        let mut segments = Vec::new();
        let mut current = String::new();
        let mut chars = path.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '$' {
                if chars.peek() == Some(&'{') {
                    // 保存当前累积的字面量
                    if !current.is_empty() {
                        segments.push(PathSegment::Literal(current.clone()));
                        current.clear();
                    }
                    
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
                    
                    segments.push(PathSegment::Variable(var_name));
                } else {
                    current.push(ch);
                }
            } else {
                current.push(ch);
            }
        }
        
        if !current.is_empty() {
            segments.push(PathSegment::Literal(current));
        }
        
        FilePath { segments }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_display() {
        assert_eq!(Value::String("test".to_string()).to_string(), "\"test\"");
        assert_eq!(Value::Number(42).to_string(), "42");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(
            Value::List(vec![Value::Number(1), Value::Number(2)]).to_string(),
            "[1, 2]"
        );
    }

    #[test]
    fn test_filepath_parse() {
        let path = FilePath::parse("path/to/${VAR}/file.txt");
        assert_eq!(path.segments.len(), 3);
        assert!(matches!(path.segments[0], PathSegment::Literal(_)));
        assert!(matches!(path.segments[1], PathSegment::Variable(_)));
        assert!(matches!(path.segments[2], PathSegment::Literal(_)));
    }

    #[test]
    fn test_filepath_validate() {
        let valid = FilePath::parse("path/to/file.txt");
        assert!(valid.validate().is_ok());
        
        let invalid = FilePath::parse("../path/to/file.txt");
        assert!(invalid.validate().is_err());
    }
}
