/*
 * AST
 *
 * 语句只有一种块结构。路径用模块段加行为名表示。
 * 变量读取只出现在 Expr::Var，对应源码里的 ${名字}。
 */

use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub statements: Vec<Statement>,
}

/// 模块路径加行为名。`tools::pack.seal` 的模块是 tools、pack，行为是 seal。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamePath {
    pub modules: Vec<String>,
    pub behavior: String,
}

impl fmt::Display for NamePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modules.is_empty() {
            write!(f, "{}", self.behavior)
        } else {
            write!(f, "{}.{}", self.modules.join("::"), self.behavior)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Set { key: String, value: Expr },
    SetEnv { key: String, value: Expr },
    Use { file: String, alias: Vec<String> },
    Struct {
        name: String,
        from: Carrier,
        layout: LayoutKind,
        sep: Option<String>,
        order: Endian,
        replaces: Option<NamePath>,
        fields: Vec<StructField>,
    },
    Action { name: String, params: Vec<String>, body: Vec<Statement> },
    Pipe { name: String, params: Vec<String>, steps: Vec<Statement> },
    If {
        cond: Expr,
        then_body: Vec<Statement>,
        elifs: Vec<(Expr, Vec<Statement>)>,
        else_body: Option<Vec<Statement>>,
    },
    Lib(LibDefinition),
    Repo(RepoDefinition),
    Print(Expr),
    Doing(NamePath),
    Await(Vec<NamePath>),
    Url { name: String, address: String },
    Dir {
        name: String,
        path: String,
        suffix: Option<Expr>,
        deep: Option<Expr>,
        exclude: Option<Expr>,
    },
    Serve { routes: Vec<RouteDecl> },
    Route(RouteDecl),
    Catch {
        error_name: Option<String>,
        var: String,
        body: Vec<Statement>,
        fail: Vec<Statement>,
    },
    At { line: u32, column: u32 },
    Stop,
    Each { var: String, index: Option<String>, source: EachSource, body: Vec<Statement> },
    Call { path: NamePath, args: Vec<Arg> },
    ErrorDef { name: String, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RouteDecl {
    pub source: String,
    pub pattern: Option<String>,
    pub struct_name: NamePath,
    pub action: NamePath,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Pos(Expr),
    Named { name: String, value: Expr },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibDefinition {
    pub name: String,
    pub version: semver::Version,
    pub desc: String,
    pub repo: String,
    pub keywords: Vec<String>,
    pub readme: String,
    pub mods: Vec<String>,
    pub out_dir: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepoDefinition {
    pub name: String,
    pub capacity: u64,
    pub max_pkgs: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    Var(String),
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Call { path: NamePath, args: Vec<Arg> },
    /// 结构名，只能作为 files.read 的参数；或 update 的字段名
    Name(NamePath),
    Field { record: Box<Expr>, field: String },
    Index { base: Box<Expr>, index: Box<Expr> },
    Slice { base: Box<Expr>, start: Box<Expr>, end: Box<Expr> },
    Num(Box<Expr>),
    Str(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Number(i64),
    Float(f64),
    Bool(bool),
    List(Arc<Vec<Value>>),
    /// 原始字节。不能与字符串隐式转换。复制只增加引用计数。
    Bytes(Arc<Vec<u8>>),
    /// 按结构读出的记录。path 是定义它的文件加结构名。字段列表复制只增加引用计数。
    Record { path: String, fields: Arc<Vec<(String, Value)>> },
    /// 没有结构的对象。键是字符串。复制只增加引用计数。
    Object(Arc<Vec<(String, Value)>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carrier {
    Str,
    Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutKind {
    Whole,
    Lines,
    Split,
    Json,
    Width,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Be,
    Le,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EachSource {
    Files,
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: FieldType,
    /// 嵌套结构解析后的编号
    pub link: Option<String>,
    /// 字节布局中的字段宽度
    pub width: Option<u64>,
    /// 字符串验证器名
    pub check: Option<String>,
    /// 省略时使用的字面量
    pub default: Option<Value>,
    /// 从旧结构的这个字段取值
    pub take: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Bytes,
    Str,
    Int,
    Float,
    Bool,
    List(Box<FieldType>),
    Struct(NamePath),
    /// 降级后的结构编号
    Linked(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Bytes(bytes) => write!(f, "bytes[{}]", bytes.len()),
            Value::Record { fields, .. } => write_pairs(f, fields),
            Value::Object(fields) => write_pairs(f, fields),
        }
    }
}

fn write_pairs(f: &mut fmt::Formatter<'_>, fields: &[(String, Value)]) -> fmt::Result {
    write!(f, "{{")?;
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{name}: {value}")?;
    }
    write!(f, "}}")
}

