/*
 * 递归下降解析器。
 * 块以 Indent / Dedent 结束，路径用 :: 和 . 组成。
 */

use crate::dsl::ast::*;
use crate::dsl::lexer::{Lexer, Token};
use error::{ErrorCategory, ErrorInfo, ErrorKind, ErrorSeverity};
use std::fmt;

#[derive(Debug)]
pub struct ParseError {
    info: ErrorInfo,
}

impl ParseError {
    pub fn new(code: u32, message: String) -> Self {
        Self {
            info: ErrorInfo::new(code, message.clone())
                .with_category(ErrorCategory::Parse)
                .with_severity(ErrorSeverity::Error)
                .with_hint(crate::dsl::diagnose::hint(code, &message)),
        }
    }

    fn at(mut self, line: u32, column: u32, hint: String) -> Self {
        self.info = ErrorInfo::new(self.info.error_code(), self.info.error_message())
            .with_category(ErrorCategory::Parse)
            .with_severity(ErrorSeverity::Error)
            .with_place("", line, column)
            .with_hint(hint);
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.info)
    }
}

impl std::error::Error for ParseError {}

pub struct Parser {
    tokens: Vec<Token>,
    spans: Vec<(u32, u32)>,
    position: usize,
}

fn name_token(token: &Token) -> Option<String> {
    Some(match token {
        Token::Ident(name) => name.clone(),
        Token::Set => "set".into(),
        Token::Env => "env".into(),
        Token::If => "if".into(),
        Token::Elif => "elif".into(),
        Token::Else => "else".into(),
        Token::Lib => "lib".into(),
        Token::Repo => "repo".into(),
        Token::Use => "use".into(),
        Token::Action => "action".into(),
        Token::Struct => "struct".into(),
        Token::Pipe => "pipe".into(),
        Token::Each => "each".into(),
        Token::As => "as".into(),
        Token::In => "in".into(),
        Token::Print => "print".into(),
        Token::Doing => "doing".into(),
        Token::Await => "await".into(),
        Token::Url => "url".into(),
        Token::Dir => "dir".into(),
        Token::Serve => "serve".into(),
        Token::Share => "share".into(),
        Token::Route => "route".into(),
        Token::Catch => "catch".into(),
        Token::Stop => "stop".into(),
        Token::Error => "error".into(),
        Token::Num => "num".into(),
        Token::Str => "str".into(),
        Token::Bool(true) => "true".into(),
        Token::Bool(false) => "false".into(),
        _ => return None,
    })
}

impl Parser {
    pub fn new(input: &str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(input);
        let spanned = lexer
            .tokenize()
            .map_err(|e| ParseError::new(1000, format!("词法分析失败: {e}")))?;
        let mut tokens = Vec::new();
        let mut spans = Vec::new();
        for (token, line, column) in spanned {
            tokens.push(token);
            spans.push((line, column));
        }
        Ok(Self { tokens, spans, position: 0 })
    }

    pub fn parse(&mut self) -> Result<Ast, ParseError> {
        let statements = self.parse_block_body(false)?;
        Ok(Ast { statements })
    }

    fn parse_block_body(&mut self, indented: bool) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();
        if indented {
            self.skip_newlines();
            self.expect(&Token::Indent)?;
        }
        loop {
            self.skip_newlines();
            if self.is_at_end() || self.current() == &Token::Eof {
                break;
            }
            if self.current() == &Token::Dedent {
                if indented {
                    self.advance();
                }
                break;
            }
            if !indented && self.current() == &Token::Dedent {
                return Err(self.fail(1002, "顶层出现多余的缩进结束"));
            }
            let (line, column) = self.place();
            statements.push(Statement::At { line, column });
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.current().clone() {
            Token::Set => self.parse_set(),
            Token::Use => self.parse_use(),
            Token::Action => self.parse_action(),
            Token::Struct => self.parse_struct(),
            Token::Pipe => self.parse_pipe(),
            Token::If => self.parse_if(),
            Token::Lib => self.parse_lib(),
            Token::Repo if self.peek() == Some(&Token::Dot) => {
                let path = self.parse_path()?;
                let args = if self.current() == &Token::LParen { self.parse_arg_list()? } else { Vec::new() };
                Ok(Statement::Call { path, args })
            }
            Token::Repo => self.parse_repo(),
            Token::Print => self.parse_print(),
            Token::Doing => self.parse_doing(),
            Token::Await => self.parse_await(),
            Token::Url => self.parse_url(),
            Token::Dir => self.parse_dir(),
            Token::Serve => self.parse_serve(),
            Token::Share => self.parse_share(),
            Token::Route => self.parse_route_stmt(),
            Token::Catch => self.parse_catch(),
            Token::Stop => {
                self.advance();
                Ok(Statement::Stop)
            }
            Token::Each => self.parse_each(),
            Token::Error => self.parse_error(),
            Token::Ident(_) => {
                let path = self.parse_path()?;
                let args = if self.current() == &Token::LParen {
                    self.parse_arg_list()?
                } else {
                    Vec::new()
                };
                Ok(Statement::Call { path, args })
            }
            other => Err(self.fail(1001, format!("意外的记号: {other}"))),
        }
    }

    fn parse_set(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Set)?;
        let env = if self.current() == &Token::Dot {
            self.advance();
            self.expect(&Token::Env)?;
            true
        } else {
            false
        };
        self.expect(&Token::LParen)?;
        let key = self.parse_ident()?;
        self.expect(&Token::Comma)?;
        let value = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        if env {
            Ok(Statement::SetEnv { key, value })
        } else {
            Ok(Statement::Set { key, value })
        }
    }

    fn parse_use(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Use)?;
        let file = self.expect_string()?;
        self.expect(&Token::As)?;
        let alias = self.parse_module_path()?;
        Ok(Statement::Use { file, alias })
    }

    fn parse_action(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Action)?;
        let name = self.parse_ident()?;
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if self.current() != &Token::RParen {
            params.push(self.parse_ident()?);
            while self.current() == &Token::Comma {
                self.advance();
                params.push(self.parse_ident()?);
            }
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::Colon)?;
        let body = self.parse_block_body(true)?;
        Ok(Statement::Action { name, params, body })
    }

    fn parse_struct(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Struct)?;
        let name = self.parse_ident()?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut from = None;
        let mut layout = None;
        let mut sep = None;
        let mut order = Endian::Be;
        let mut replaces = None;
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.current() == &Token::Dedent {
                self.advance();
                break;
            }
            if self.is_at_end() {
                break;
            }
            let key = self.parse_ident()?;
            self.expect(&Token::Colon)?;
            match key.as_str() {
                "from" => {
                    let carrier = self.parse_name()?;
                    from = Some(match carrier.as_str() {
                        "str" => Carrier::Str,
                        "bytes" => Carrier::Bytes,
                        other => return Err(self.fail(1004, format!("from 只能是 str 或 bytes，得到 {other}"))),
                    });
                }
                "layout" => {
                    let kind = self.parse_ident()?;
                    layout = Some(match kind.as_str() {
                        "whole" => LayoutKind::Whole,
                        "lines" => LayoutKind::Lines,
                        "split" => LayoutKind::Split,
                        "json" => LayoutKind::Json,
                        "width" => LayoutKind::Width,
                        "block" => LayoutKind::Block,
                        other => return Err(self.fail(1004, format!("layout 只能是 whole、lines、split、json、width 或 block，得到 {other}"))),
                    });
                }
                "sep" => {
                    let Token::String(value) = self.current().clone() else {
                        return Err(self.fail(1004, "sep 必须是字符串"));
                    };
                    self.advance();
                    if value.is_empty() {
                        return Err(self.fail(1004, "sep 不能为空"));
                    }
                    sep = Some(value);
                }
                "order" => {
                    let kind = self.parse_ident()?;
                    order = match kind.as_str() {
                        "be" => Endian::Be,
                        "le" => Endian::Le,
                        other => return Err(self.fail(1004, format!("order 只能是 be 或 le，得到 {other}"))),
                    };
                }
                "replaces" => {
                    if replaces.is_some() {
                        return Err(self.fail(1004, "replaces 只能写一次"));
                    }
                    replaces = Some(self.parse_path()?);
                }
                _ => {
                    let ty = self.parse_field_type()?;
                    let width = if let Token::Number(n) = *self.current() {
                        let n = n;
                        self.advance();
                        if n <= 0 {
                            return Err(self.fail(1004, format!("字段 {key} 的宽度必须大于 0")));
                        }
                        Some(n as u64)
                    } else {
                        None
                    };
                    let (check, default, take) = self.parse_field_clauses(&key)?;
                    fields.push(StructField { name: key, ty, link: None, width, check, default, take });
                }
            }
        }
        let from = from.ok_or_else(|| self.fail(1004, format!("结构 {name} 缺少 from")))?;
        let layout = layout.ok_or_else(|| self.fail(1004, format!("结构 {name} 缺少 layout")))?;
        if fields.is_empty() {
            return Err(self.fail(1004, format!("结构 {name} 没有字段")));
        }
        Ok(Statement::Struct { name, from, layout, sep, order, replaces, fields })
    }

    fn parse_field_clauses(&mut self, field: &str) -> Result<(Option<String>, Option<Value>, Option<String>), ParseError> {
        let mut check = None;
        let mut default = None;
        let mut take = None;
        while matches!(self.current(), Token::Ident(_)) {
            let Token::Ident(clause) = self.current().clone() else { break };
            if !matches!(clause.as_str(), "check" | "default" | "take") {
                break;
            }
            self.advance();
            self.expect(&Token::Colon)?;
            match clause.as_str() {
                "check" => {
                    if check.is_some() {
                        return Err(self.fail(1004, format!("字段 {field} 的 check 只能写一次")));
                    }
                    let name = self.parse_ident()?;
                    if !matches!(name.as_str(), "not_empty" | "email" | "url" | "numeric" | "alpha" | "alphanumeric") {
                        return Err(self.fail(1004, format!("未知的验证器: {name}")));
                    }
                    check = Some(name);
                }
                "default" => {
                    if default.is_some() {
                        return Err(self.fail(1004, format!("字段 {field} 的 default 只能写一次")));
                    }
                    default = Some(self.parse_literal()?);
                }
                "take" => {
                    if take.is_some() {
                        return Err(self.fail(1004, format!("字段 {field} 的 take 只能写一次")));
                    }
                    take = Some(self.parse_ident()?);
                }
                _ => unreachable!(),
            }
        }
        Ok((check, default, take))
    }

    fn parse_literal(&mut self) -> Result<Value, ParseError> {
        match self.current().clone() {
            Token::String(text) => {
                self.advance();
                Ok(Value::String(text))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Value::Number(n))
            }
            Token::Float(n) => {
                self.advance();
                Ok(Value::Float(n))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Value::Bool(b))
            }
            other => Err(self.fail(1004, format!("default 需要字面量，得到 {other}"))),
        }
    }

    fn parse_field_type(&mut self) -> Result<FieldType, ParseError> {
        if self.current() == &Token::Str {
            self.advance();
            return Ok(FieldType::Str);
        }
        if let Token::Ident(name) = self.current().clone() {
            if name == "list" {
                self.advance();
                return Ok(FieldType::List(Box::new(self.parse_field_type()?)));
            }
            if matches!(name.as_str(), "bytes" | "int" | "float" | "bool") {
                self.advance();
                return Ok(match name.as_str() {
                    "bytes" => FieldType::Bytes,
                    "int" => FieldType::Int,
                    "float" => FieldType::Float,
                    "bool" => FieldType::Bool,
                    _ => unreachable!(),
                });
            }
        }
        Ok(FieldType::Struct(self.parse_path()?))
    }

    fn parse_pipe(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Pipe)?;
        let name = self.parse_ident()?;
        let params = if self.current() == &Token::LParen {
            self.advance();
            let mut params = Vec::new();
            if self.current() != &Token::RParen {
                params.push(self.parse_ident()?);
                while self.current() == &Token::Comma {
                    self.advance();
                    params.push(self.parse_ident()?);
                }
            }
            self.expect(&Token::RParen)?;
            params
        } else {
            Vec::new()
        };
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut steps = Vec::new();
        loop {
            self.skip_newlines();
            if self.current() == &Token::Dedent {
                self.advance();
                break;
            }
            if self.is_at_end() {
                break;
            }
            let path = self.parse_path()?;
            if self.current() == &Token::LParen {
                let args = self.parse_arg_list()?;
                steps.push(Statement::Call { path, args });
            } else {
                steps.push(Statement::Doing(path));
            }
        }
        if steps.is_empty() {
            return Err(self.fail(1003, format!("管道 {name} 没有步骤")));
        }
        Ok(Statement::Pipe { name, params, steps })
    }

    fn parse_if(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Colon)?;
        let then_body = self.parse_block_body(true)?;
        let mut elifs = Vec::new();
        while self.current() == &Token::Elif {
            self.advance();
            let elif_cond = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            elifs.push((elif_cond, self.parse_block_body(true)?));
        }
        let else_body = if self.current() == &Token::Else {
            self.advance();
            self.expect(&Token::Colon)?;
            Some(self.parse_block_body(true)?)
        } else {
            None
        };
        Ok(Statement::If { cond, then_body, elifs, else_body })
    }

    fn parse_lib(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Lib)?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut name = None;
        let mut version = None;
        let mut desc = None;
        let mut repo = None;
        let mut keywords = Vec::new();
        let mut readme = None;
        let mut mods = Vec::new();
        let mut out_dir = None;
        let mut depends = Vec::new();
        let mut replaces = None;
        let mut sign = None;
        loop {
            self.skip_newlines();
            if self.current() == &Token::Dedent {
                self.advance();
                break;
            }
            let field = self.parse_name()?;
            self.expect(&Token::Colon)?;
            match field.as_str() {
                "name" => name = Some(self.expect_string()?),
                "version" => version = Some(self.expect_string()?),
                "desc" => desc = Some(self.expect_string()?),
                "repo" => repo = Some(self.expect_string()?),
                "readme" => readme = Some(self.expect_string()?),
                "out_dir" => out_dir = Some(self.expect_string()?),
                "keywords" => keywords = self.parse_string_list()?,
                "mods" => mods = self.parse_string_list()?,
                "depends" => depends = self.parse_string_list()?,
                "replaces" => replaces = Some(self.expect_string()?),
                "sign" => sign = Some(self.expect_string()?),
                other => return Err(self.fail(1004, format!("未知的 lib 字段: {other}"))),
            }
        }
        let version_text = version.ok_or_else(|| self.fail(1005, "lib 缺少 version"))?;
        let version = semver::Version::parse(&version_text).map_err(|e| self.fail(1005, format!("版本号无效: {e}")))?;
        let name = name.ok_or_else(|| self.fail(1004, "lib 缺少 name"))?;
        check_segment("包名", &name).map_err(|message| self.fail(1004, message))?;
        check_segment("版本", &version.to_string()).map_err(|message| self.fail(1004, message))?;
        Ok(Statement::Lib(LibDefinition {
            name,
            version,
            desc: desc.unwrap_or_default(),
            repo: repo.unwrap_or_default(),
            keywords,
            readme: readme.unwrap_or_default(),
            mods,
            out_dir: out_dir.unwrap_or_else(|| "output".into()),
            depends,
            replaces: replaces.unwrap_or_default(),
            sign: sign.unwrap_or_default(),
        }))
    }

    fn parse_repo(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Repo)?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut name = None;
        let mut dir = None;
        let mut capacity = None;
        let mut max_pkgs = None;
        loop {
            self.skip_newlines();
            if self.current() == &Token::Dedent {
                self.advance();
                break;
            }
            let field = self.parse_ident()?;
            self.expect(&Token::Colon)?;
            match field.as_str() {
                "name" => name = Some(self.expect_string()?),
                "dir" => dir = Some(self.expect_string()?),
                "capacity" => capacity = Some(self.expect_number()? as u64),
                "max_pkgs" => max_pkgs = Some(self.expect_number()? as u64),
                other => return Err(self.fail(1004, format!("未知的 repo 字段: {other}"))),
            }
        }
        let name = name.ok_or_else(|| self.fail(1004, "repo 缺少 name"))?;
        check_segment("仓库名", &name).map_err(|message| self.fail(1004, message))?;
        Ok(Statement::Repo(RepoDefinition {
            name,
            dir: dir.ok_or_else(|| self.fail(1004, "repo 缺少 dir"))?,
            capacity,
            max_pkgs,
        }))
    }

    fn parse_print(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Print)?;
        self.expect(&Token::LParen)?;
        let expr = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        Ok(Statement::Print(expr))
    }

    fn parse_doing(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Doing)?;
        Ok(Statement::Doing(self.parse_path()?))
    }

    fn parse_await(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Await)?;
        let mut paths = vec![self.parse_path()?];
        while self.current() == &Token::Comma {
            self.advance();
            paths.push(self.parse_path()?);
        }
        Ok(Statement::Await(paths))
    }

    fn parse_url(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Url)?;
        let name = self.parse_ident()?;
        let address = self.expect_string()?;
        let mut cert = None;
        let mut key = None;
        while self.peek() == Some(&Token::Colon) {
            let Some(field) = name_token(self.current()) else { break };
            self.advance();
            self.advance();
            let value = self.parse_expr()?;
            match field.as_str() {
                "cert" if cert.is_none() => cert = Some(value),
                "key" if key.is_none() => key = Some(value),
                "cert" | "key" => return Err(self.fail(1004, format!("{field} 只能写一次"))),
                _ => return Err(self.fail(1004, format!("url 没有 {field}"))),
            }
        }
        Ok(Statement::Url { name, address, cert, key })
    }

    fn parse_dir(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Dir)?;
        let name = self.parse_ident()?;
        let path = self.expect_string()?;
        let mut suffix = None;
        let mut deep = None;
        let mut exclude = None;
        while self.peek() == Some(&Token::Colon) {
            let Some(key) = name_token(self.current()) else { break };
            self.advance();
            self.advance();
            let value = self.parse_expr()?;
            match key.as_str() {
                "suffix" if suffix.is_none() => suffix = Some(value),
                "deep" if deep.is_none() => deep = Some(value),
                "exclude" if exclude.is_none() => exclude = Some(value),
                "suffix" | "deep" | "exclude" => return Err(self.fail(1004, format!("{key} 只能写一次"))),
                _ => return Err(self.fail(1004, format!("dir 没有 {key}"))),
            }
        }
        Ok(Statement::Dir { name, path, suffix, deep, exclude })
    }

    fn parse_share(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Share)?;
        let name = self.parse_ident()?;
        Ok(Statement::Share { name })
    }

    fn parse_serve(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Serve)?;
        let mut workers = None;
        if name_token(self.current()).as_deref() == Some("workers") {
            self.advance();
            self.expect(&Token::Colon)?;
            workers = Some(self.parse_expr()?);
        }
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        let mut routes = Vec::new();
        let mut repo = None;
        loop {
            self.skip_newlines();
            if self.current() == &Token::Dedent {
                self.advance();
                break;
            }
            match self.current() {
                Token::Route => routes.push(match self.parse_route_stmt()? {
                    Statement::Route(route) => route,
                    _ => unreachable!(),
                }),
                Token::Repo => {
                    self.advance();
                    if repo.is_some() {
                        return Err(self.fail(1001, "serve 里只能写一个 repo"));
                    }
                    repo = Some(self.parse_ident()?);
                }
                _ => return Err(self.fail(1001, "serve 里只能写 route 或 repo 地址名")),
            }
        }
        if routes.is_empty() && repo.is_none() {
            return Err(self.fail(1001, "serve 至少要有一条 route"));
        }
        Ok(Statement::Serve { workers, routes, repo })
    }

    fn parse_route_stmt(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Route)?;
        let source = self.parse_ident()?;
        let pattern = if matches!(self.current(), Token::String(_)) {
            Some(self.expect_string()?)
        } else {
            None
        };
        let struct_name = self.parse_path()?;
        let action = self.parse_path()?;
        Ok(Statement::Route(RouteDecl { source, pattern, struct_name, action }))
    }

    fn parse_catch(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Catch)?;
        let error_name = if self.current() == &Token::As {
            None
        } else {
            Some(self.parse_ident()?)
        };
        self.expect(&Token::As)?;
        let var = match self.current().clone() {
            Token::Var(name) => {
                self.advance();
                name
            }
            Token::Ident(name) => {
                self.advance();
                name
            }
            _ => return Err(self.fail(1006, "catch 需要 as 变量")),
        };
        self.expect(&Token::Colon)?;
        let body = self.parse_block_body(true)?;
        let fail = if self.current() == &Token::Else {
            self.advance();
            self.expect(&Token::Colon)?;
            self.parse_block_body(true)?
        } else {
            Vec::new()
        };
        Ok(Statement::Catch { error_name, var, body, fail })
    }

    fn parse_each(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Each)?;
        let var = match self.current().clone() {
            Token::Var(name) => {
                self.advance();
                name
            }
            _ => return Err(self.fail(1006, "each 需要 ${变量}")),
        };
        let index = if matches!(self.current(), Token::Ident(name) if name == "at") {
            self.advance();
            match self.current().clone() {
                Token::Var(name) => {
                    self.advance();
                    Some(name)
                }
                _ => return Err(self.fail(1006, "each 的下标需要 ${变量}")),
            }
        } else {
            None
        };
        self.expect(&Token::In)?;
        let source = if matches!(self.current(), Token::Ident(name) if name == "files")
            && self.peek() == Some(&Token::Colon)
        {
            self.advance();
            EachSource::Files
        } else {
            EachSource::Expr(self.parse_expr()?)
        };
        self.expect(&Token::Colon)?;
        let body = self.parse_block_body(true)?;
        Ok(Statement::Each { var, index, source, body })
    }

    fn parse_error(&mut self) -> Result<Statement, ParseError> {
        self.expect(&Token::Error)?;
        let name = self.parse_ident()?;
        self.expect(&Token::Colon)?;
        self.skip_newlines();
        self.expect(&Token::Indent)?;
        self.skip_newlines();
        let message = self.expect_string()?;
        self.skip_newlines();
        self.expect(&Token::Dedent)?;
        Ok(Statement::ErrorDef { name, message })
    }

    fn parse_path(&mut self) -> Result<NamePath, ParseError> {
        let mut parts = vec![self.parse_ident()?];
        while self.current() == &Token::PathSep {
            self.advance();
            parts.push(self.parse_ident()?);
        }
        while self.current() == &Token::Dot {
            self.advance();
            parts.push(self.parse_ident()?);
        }
        let behavior = parts.pop().unwrap();
        Ok(NamePath { modules: parts, behavior })
    }

    fn parse_module_path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = vec![self.parse_ident()?];
        while self.current() == &Token::PathSep {
            self.advance();
            parts.push(self.parse_ident()?);
        }
        Ok(parts)
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.expect(&Token::LParen)?;
        let mut args = Vec::new();
        if self.current() != &Token::RParen {
            args.push(self.parse_arg()?);
            while self.current() == &Token::Comma {
                self.advance();
                if self.current() == &Token::RParen {
                    break;
                }
                args.push(self.parse_arg()?);
            }
        }
        self.expect(&Token::RParen)?;
        Ok(args)
    }

    fn parse_arg(&mut self) -> Result<Arg, ParseError> {
        if self.peek() == Some(&Token::Colon) {
            if let Some(name) = name_token(self.current()) {
                self.advance();
                self.advance();
                let value = self.parse_expr()?;
                return Ok(Arg::Named { name, value });
            }
        }
        Ok(Arg::Pos(self.parse_expr()?))
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.current() == &Token::Or {
            self.advance();
            left = Expr::Binary { op: BinaryOp::Or, left: Box::new(left), right: Box::new(self.parse_and()?) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_cmp()?;
        while self.current() == &Token::And {
            self.advance();
            left = Expr::Binary { op: BinaryOp::And, left: Box::new(left), right: Box::new(self.parse_cmp()?) };
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add()?;
        if let Some(op) = self.cmp_op() {
            self.advance();
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_add()?) };
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul()?;
        while matches!(self.current(), Token::Plus | Token::Minus) {
            let op = if self.current() == &Token::Plus { BinaryOp::Add } else { BinaryOp::Sub };
            self.advance();
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_mul()?) };
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        while matches!(self.current(), Token::Star | Token::Slash) {
            let op = if self.current() == &Token::Star { BinaryOp::Mul } else { BinaryOp::Div };
            self.advance();
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(self.parse_unary()?) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.current() == &Token::Not {
            self.advance();
            return Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(self.parse_unary()?) });
        }
        if self.current() == &Token::Minus {
            self.advance();
            return Ok(Expr::Binary {
                op: BinaryOp::Sub,
                left: Box::new(Expr::Literal(Value::Number(0))),
                right: Box::new(self.parse_unary()?),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_atom()?;
        self.parse_postfix(expr)
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            if self.current() == &Token::Dot {
                self.advance();
                let field = self.parse_ident()?;
                expr = Expr::Field { record: Box::new(expr), field };
            } else if self.current() == &Token::LBracket {
                self.advance();
                let start = self.parse_expr()?;
                if self.current() == &Token::Colon {
                    self.advance();
                    let end = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Slice { base: Box::new(expr), start: Box::new(start), end: Box::new(end) };
                } else {
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index { base: Box::new(expr), index: Box::new(start) };
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        match self.current().clone() {
            Token::String(s) => {
                self.advance();
                Ok(Expr::Literal(Value::String(self.interpolate_string_literal(&s))))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Literal(Value::Number(n)))
            }
            Token::Float(n) => {
                self.advance();
                Ok(Expr::Literal(Value::Float(n)))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Expr::Literal(Value::Bool(b)))
            }
            Token::Var(name) => {
                self.advance();
                Ok(Expr::Var(name))
            }
            Token::Num => {
                self.advance();
                self.expect(&Token::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Num(Box::new(inner)))
            }
            Token::Str => {
                self.advance();
                self.expect(&Token::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::Str(Box::new(inner)))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Repo if self.peek() == Some(&Token::Dot) => {
                let path = self.parse_path()?;
                if self.current() != &Token::LParen {
                    return Ok(Expr::Name(path));
                }
                let args = self.parse_arg_list()?;
                Ok(Expr::Call { path, args })
            }
            Token::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if self.current() != &Token::RBracket {
                    items.push(self.parse_expr()?);
                    while self.current() == &Token::Comma {
                        self.advance();
                        if self.current() == &Token::RBracket {
                            break;
                        }
                        items.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Call {
                    path: NamePath { modules: vec!["list".into()], behavior: "of".into() },
                    args: items.into_iter().map(Arg::Pos).collect(),
                })
            }
            Token::Ident(_) => {
                let path = self.parse_path()?;
                if self.current() != &Token::LParen {
                    return Ok(Expr::Name(path));
                }
                self.expect(&Token::LParen)?;
                let mut args = Vec::new();
                if self.current() != &Token::RParen {
                    args.push(self.parse_arg()?);
                    while self.current() == &Token::Comma {
                        self.advance();
                        if self.current() == &Token::RParen {
                            break;
                        }
                        args.push(self.parse_arg()?);
                    }
                }
                self.expect(&Token::RParen)?;
                Ok(Expr::Call { path, args })
            }
            other => Err(self.fail(1013, format!("意外的表达式: {other}"))),
        }
    }

    fn interpolate_string_literal(&self, raw: &str) -> String {
        raw.to_string()
    }

    fn cmp_op(&self) -> Option<BinaryOp> {
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

    fn parse_string_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&Token::LBracket)?;
        let mut items = Vec::new();
        if self.current() != &Token::RBracket {
            items.push(self.expect_string()?);
            while self.current() == &Token::Comma {
                self.advance();
                if self.current() == &Token::RBracket {
                    break;
                }
                items.push(self.expect_string()?);
            }
        }
        self.expect(&Token::RBracket)?;
        Ok(items)
    }

    fn parse_name(&mut self) -> Result<String, ParseError> {
        if self.current() == &Token::Repo {
            self.advance();
            return Ok("repo".into());
        }
        if self.current() == &Token::Str {
            self.advance();
            return Ok("str".into());
        }
        self.parse_ident()
    }

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        if let Some(name) = name_token(self.current()) {
            self.advance();
            return Ok(name);
        }
        Err(self.fail(1014, format!("期望标识符，得到: {}", self.current())))
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.current().clone() {
            Token::String(s) => {
                self.advance();
                Ok(s)
            }
            other => Err(self.fail(1015, format!("期望字符串，得到: {other}"))),
        }
    }

    fn expect_number(&mut self) -> Result<i64, ParseError> {
        match self.current().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(n)
            }
            other => Err(self.fail(1016, format!("期望数字，得到: {other}"))),
        }
    }

    fn expect(&mut self, token: &Token) -> Result<(), ParseError> {
        if self.current() == token {
            self.advance();
            Ok(())
        } else {
            Err(self.fail(1017, format!("期望 {token:?}，得到 {}", self.current())))
        }
    }

    fn place(&self) -> (u32, u32) {
        self.spans.get(self.position).copied().unwrap_or((1, 1))
    }

    fn fail(&self, code: u32, message: impl Into<String>) -> ParseError {
        let message = message.into();
        let (line, column) = self.place();
        let hint = crate::dsl::diagnose::hint(code, &message);
        ParseError::new(code, message).at(line, column, hint)
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::Eof)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position + 1)
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current(), Token::Newline | Token::Comment(_)) {
            self.advance();
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

fn check_segment(kind: &str, text: &str) -> Result<(), String> {
    if text.is_empty() || text == "." || text == ".." || text.contains('/') || text.contains('\\') || text.contains('\0') {
        return Err(format!("{kind}不能作为路径段: {text}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_action_and_pipe() {
        let src = "action clean(file):\n    print(${file})\n\npipe process:\n    clean\n";
        let mut parser = Parser::new(src).unwrap();
        let ast = parser.parse().unwrap();
        let stmts: Vec<_> = ast.statements.iter().filter(|stmt| !matches!(stmt, Statement::At { .. })).collect();
        assert!(matches!(stmts[0], Statement::Action { .. }));
        assert!(matches!(stmts[1], Statement::Pipe { .. }));
    }

    #[test]
    fn parses_use_and_qualified_path() {
        let src = "use \"rules/csv.dake\" as tools::pack\ndoing tools::pack.seal\n";
        let mut parser = Parser::new(src).unwrap();
        let ast = parser.parse().unwrap();
        let stmts: Vec<_> = ast.statements.iter().filter(|stmt| !matches!(stmt, Statement::At { .. })).collect();
        assert!(matches!(stmts[0], Statement::Use { .. }));
        match stmts[1] {
            Statement::Doing(path) => {
                assert_eq!(path.modules, vec!["tools", "pack"]);
                assert_eq!(path.behavior, "seal");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn if_block_stops_at_dedent() {
        let src = "if ${ok}:\n    print(\"yes\")\nprint(\"after\")\n";
        let mut parser = Parser::new(src).unwrap();
        let ast = parser.parse().unwrap();
        let stmts: Vec<_> = ast.statements.iter().filter(|stmt| !matches!(stmt, Statement::At { .. })).collect();
        assert_eq!(stmts.len(), 2);
    }
}
