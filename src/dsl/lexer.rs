/*
 * 词法分析器
 *
 * 关键字一律小写。`::` 与 `:` 分开。
 * 行首空格变成 Indent / Dedent，空行和整行注释不产生缩进。
 */

use std::collections::VecDeque;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Set,
    Env,
    If,
    Elif,
    Else,
    Lib,
    Repo,
    Use,
    Action,
    Struct,
    Pipe,
    Each,
    As,
    In,
    Print,
    Doing,
    Await,
    Url,
    Dir,
    Serve,
    Route,
    Catch,
    Stop,
    Error,
    Num,
    Str,
    String(String),
    Number(i64),
    Float(f64),
    Bool(bool),
    Ident(String),
    Var(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    PathSep,
    Comma,
    Dot,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Not,
    Plus,
    Minus,
    Star,
    Slash,
    Newline,
    Indent,
    Dedent,
    Eof,
    Comment(String),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Ident(s) => write!(f, "{s}"),
            Token::String(s) => write!(f, "\"{s}\""),
            Token::Var(s) => write!(f, "${{{s}}}"),
            Token::Number(n) => write!(f, "{n}"),
            Token::Float(n) => write!(f, "{n}"),
            Token::Bool(b) => write!(f, "{b}"),
            other => write!(f, "{other:?}"),
        }
    }
}

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
    at_bol: bool,
    indent_stack: Vec<usize>,
    pending: VecDeque<(Token, u32, u32)>,
    span: (u32, u32),
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
            at_bol: true,
            indent_stack: vec![0],
            pending: VecDeque::new(),
            span: (1, 1),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<(Token, u32, u32)>, String> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let span = self.span;
            let done = token == Token::Eof;
            tokens.push((token, span.0, span.1));
            if done {
                break;
            }
        }
        Ok(tokens)
    }

    pub fn next_token(&mut self) -> Result<Token, String> {
        if let Some((token, line, column)) = self.pending.pop_front() {
            self.span = (line, column);
            return Ok(token);
        }
        if self.at_bol {
            self.scan_indent()?;
            self.at_bol = false;
            if let Some((token, line, column)) = self.pending.pop_front() {
                self.span = (line, column);
                return Ok(token);
            }
        }
        self.skip_spaces();
        self.span = (self.line as u32, self.column as u32);
        if self.is_at_end() {
            self.emit_closing_dedents();
            return Ok(self.pending.pop_front().map(|(token, line, column)| {
                self.span = (line, column);
                token
            }).unwrap_or(Token::Eof));
        }
        let ch = self.current_char();
        if ch == '#' {
            return self.scan_comment();
        }
        if ch == '\n' {
            self.advance();
            self.line += 1;
            self.column = 1;
            self.at_bol = true;
            return Ok(Token::Newline);
        }
        if ch == '"' || ch == '\'' {
            return self.scan_string(ch);
        }
        if ch == '$' {
            return self.scan_var();
        }
        if ch.is_ascii_digit() {
            return self.scan_number(false);
        }
        if ch.is_alphabetic() || ch == '_' {
            return self.scan_identifier();
        }
        self.scan_symbol(ch)
    }

    fn scan_indent(&mut self) -> Result<(), String> {
        let mut spaces = 0usize;
        loop {
            if self.is_at_end() {
                self.emit_closing_dedents();
                return Ok(());
            }
            match self.current_char() {
                ' ' => {
                    spaces += 1;
                    self.advance();
                }
                '\t' => {
                    return Err(format!("第 {} 行: 不允许用制表符缩进", self.line));
                }
                '\n' => {
                    self.advance();
                    self.line += 1;
                    self.column = 1;
                    spaces = 0;
                }
                '#' => {
                    while !self.is_at_end() && self.current_char() != '\n' {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
        let current = *self.indent_stack.last().unwrap_or(&0);
        if spaces == current {
            return Ok(());
        }
        if spaces > current {
            self.indent_stack.push(spaces);
            self.pending.push_back((Token::Indent, self.line as u32, 1));
            return Ok(());
        }
        while self.indent_stack.last().copied().unwrap_or(0) > spaces {
            self.indent_stack.pop();
            self.pending.push_back((Token::Dedent, self.line as u32, 1));
        }
        if self.indent_stack.last().copied() != Some(spaces) {
            return Err(format!("第 {} 行: 缩进与外层不一致", self.line));
        }
        Ok(())
    }

    fn emit_closing_dedents(&mut self) {
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.pending.push_back((Token::Dedent, self.line as u32, 1));
        }
        self.pending.push_back((Token::Eof, self.line as u32, self.column as u32));
    }

    fn scan_symbol(&mut self, ch: char) -> Result<Token, String> {
        self.advance();
        let token = match ch {
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            ',' => Token::Comma,
            '.' => Token::Dot,
            '+' => Token::Plus,
            '*' => Token::Star,
            '/' => Token::Slash,
            ':' => {
                if self.current_char() == ':' {
                    self.advance();
                    Token::PathSep
                } else {
                    Token::Colon
                }
            }
            '-' => {
                if self.current_char().is_ascii_digit() {
                    return self.scan_number(true);
                }
                Token::Minus
            }
            '=' => {
                if self.current_char() == '=' {
                    self.advance();
                    Token::Eq
                } else {
                    return Err(format!("第 {} 行第 {} 列: 不支持单独的 =", self.line, self.column));
                }
            }
            '!' => {
                if self.current_char() == '=' {
                    self.advance();
                    Token::Ne
                } else {
                    Token::Not
                }
            }
            '>' => {
                if self.current_char() == '=' {
                    self.advance();
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            '<' => {
                if self.current_char() == '=' {
                    self.advance();
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            '&' => {
                if self.current_char() == '&' {
                    self.advance();
                    Token::And
                } else {
                    return Err(format!("第 {} 行: 意外的字符 '&'", self.line));
                }
            }
            '|' => {
                if self.current_char() == '|' {
                    self.advance();
                    Token::Or
                } else {
                    return Err(format!("第 {} 行: 意外的字符 '|'", self.line));
                }
            }
            _ => return Err(format!("第 {} 行第 {} 列: 未知字符 '{ch}'", self.line, self.column)),
        };
        Ok(token)
    }

    fn scan_var(&mut self) -> Result<Token, String> {
        self.advance();
        if self.current_char() != '{' {
            return Err(format!("第 {} 行: 变量必须以 ${{ 开始", self.line));
        }
        self.advance();
        let mut name = String::new();
        while !self.is_at_end() && (self.current_char().is_alphanumeric() || self.current_char() == '_') {
            name.push(self.advance());
        }
        if name.is_empty() || self.current_char() != '}' {
            return Err(format!("第 {} 行: 变量未闭合", self.line));
        }
        self.advance();
        Ok(Token::Var(name))
    }

    fn scan_string(&mut self, quote: char) -> Result<Token, String> {
        self.advance();
        let mut value = String::new();
        while !self.is_at_end() && self.current_char() != quote {
            let ch = self.advance();
            if ch == '\\' {
                if self.is_at_end() {
                    break;
                }
                let escaped = self.advance();
                match escaped {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    '$' => value.push('$'),
                    _ => {
                        value.push('\\');
                        value.push(escaped);
                    }
                }
            } else {
                value.push(ch);
            }
        }
        if self.is_at_end() {
            return Err(format!("第 {} 行: 字符串未闭合", self.line));
        }
        self.advance();
        Ok(Token::String(value))
    }

    fn scan_number(&mut self, negative: bool) -> Result<Token, String> {
        let mut number = String::new();
        if negative {
            number.push('-');
        }
        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            number.push(self.advance());
        }
        if self.current_char() == '.' && self.peek_char().is_ascii_digit() {
            number.push(self.advance());
            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                number.push(self.advance());
            }
            return number
                .parse::<f64>()
                .map(Token::Float)
                .map_err(|_| format!("第 {} 行: 无效的数字 '{number}'", self.line));
        }
        number
            .parse::<i64>()
            .map(Token::Number)
            .map_err(|_| format!("第 {} 行: 无效的数字 '{number}'", self.line))
    }

    fn scan_identifier(&mut self) -> Result<Token, String> {
        let mut ident = String::new();
        while !self.is_at_end() {
            let ch = self.current_char();
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(self.advance());
            } else {
                break;
            }
        }
        let token = match ident.as_str() {
            "set" => Token::Set,
            "env" => Token::Env,
            "if" => Token::If,
            "elif" => Token::Elif,
            "else" => Token::Else,
            "lib" => Token::Lib,
            "repo" => Token::Repo,
            "use" => Token::Use,
            "action" => Token::Action,
            "struct" => Token::Struct,
            "pipe" => Token::Pipe,
            "each" => Token::Each,
            "as" => Token::As,
            "in" => Token::In,
            "print" => Token::Print,
            "doing" => Token::Doing,
            "await" => Token::Await,
            "url" => Token::Url,
            "dir" => Token::Dir,
            "serve" => Token::Serve,
            "route" => Token::Route,
            "catch" => Token::Catch,
            "stop" => Token::Stop,
            "error" => Token::Error,
            "num" => Token::Num,
            "str" => Token::Str,
            "true" => Token::Bool(true),
            "false" => Token::Bool(false),
            "files" => Token::Ident("files".into()),
            _ => Token::Ident(ident),
        };
        Ok(token)
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    fn current_char(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.input[self.position]
        }
    }

    fn peek_char(&self) -> char {
        self.input.get(self.position + 1).copied().unwrap_or('\0')
    }

    fn advance(&mut self) -> char {
        if self.is_at_end() {
            return '\0';
        }
        let ch = self.input[self.position];
        self.position += 1;
        self.column += 1;
        ch
    }

    fn skip_spaces(&mut self) {
        while self.current_char() == ' ' || self.current_char() == '\r' {
            self.advance();
        }
    }

    fn scan_comment(&mut self) -> Result<Token, String> {
        self.advance();
        let mut comment = String::new();
        while !self.is_at_end() && self.current_char() != '\n' {
            comment.push(self.advance());
        }
        Ok(Token::Comment(comment.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new("set(key, \"value\")");
        assert_eq!(lexer.next_token().unwrap(), Token::Set);
        assert_eq!(lexer.next_token().unwrap(), Token::LParen);
        assert_eq!(lexer.next_token().unwrap(), Token::Ident("key".into()));
    }

    #[test]
    fn test_path_sep_and_indent() {
        let mut lexer = Lexer::new("action clean(file):\n    print(${file})\n");
        assert_eq!(lexer.next_token().unwrap(), Token::Action);
        let tokens = lexer.tokenize().unwrap();
        assert!(tokens.iter().any(|(token, _, _)| *token == Token::Indent));
        assert!(tokens.iter().any(|(token, _, _)| *token == Token::Dedent));
    }

    #[test]
    fn test_keywords_lowercase() {
        let mut lexer = Lexer::new("if elif else");
        assert_eq!(lexer.next_token().unwrap(), Token::If);
        assert_eq!(lexer.next_token().unwrap(), Token::Elif);
        assert_eq!(lexer.next_token().unwrap(), Token::Else);
    }

    #[test]
    fn test_path_sep() {
        let mut lexer = Lexer::new("tools::pack");
        assert_eq!(lexer.next_token().unwrap(), Token::Ident("tools".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::PathSep);
        assert_eq!(lexer.next_token().unwrap(), Token::Ident("pack".into()));
    }
}
