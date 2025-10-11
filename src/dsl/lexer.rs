/*
 * 词法分析器 (Lexer)
 *
 * 将 DSL 源代码转换为 Token 流。
 * 使用显式状态管理，避免隐式转换。
 */

use std::fmt;

/// Token 类型
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 关键字
    Set,
    Env,
    If,
    Elif,
    Else,
    Lib,
    Repo,
    Log,
    Init,
    Info,
    Error,
    Data,
    Do,
    Pipe,
    Comm,
    Action,
    Cmd,
    Print,
    Doing,
    Await,
    Files,
    All,
    Encry,
    Catch,
    
    // 字面量
    String(String),
    Number(i64),
    Bool(bool),
    Ident(String),
    
    // 符号
    LParen,        // (
    RParen,        // )
    LBrace,        // {
    RBrace,        // }
    LBracket,      // [
    RBracket,      // ]
    Colon,         // :
    Comma,         // ,
    Dot,           // .
    Arrow,         // =>
    
    // 操作符
    Eq,            // ==
    Ne,            // !=
    Gt,            // >
    Lt,            // <
    Ge,            // >=
    Le,            // <=
    And,           // &&
    Or,            // ||
    Not,           // !
    Assign,        // =
    
    // 特殊
    Newline,
    Eof,
    Comment(String),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Set => write!(f, "set"),
            Token::Env => write!(f, "env"),
            Token::If => write!(f, "IF"),
            Token::Elif => write!(f, "ELIF"),
            Token::Else => write!(f, "ELSE"),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Number(n) => write!(f, "{}", n),
            Token::Bool(b) => write!(f, "{}", b),
            Token::Ident(s) => write!(f, "{}", s),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Colon => write!(f, ":"),
            Token::Newline => write!(f, "\\n"),
            Token::Eof => write!(f, "EOF"),
            Token::Comment(s) => write!(f, "# {}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// 词法分析器
pub struct Lexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    /// 创建新的词法分析器
    ///
    /// # 参数
    /// - `input`: DSL 源代码字符串
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// 获取下一个 Token
    ///
    /// # 返回值
    /// - `Ok(Token)`: 成功解析的 Token
    /// - `Err(String)`: 词法错误信息
    pub fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace_except_newline();
        
        if self.is_at_end() {
            return Ok(Token::Eof);
        }
        
        let ch = self.current_char();
        
        // 注释
        if ch == '#' {
            return self.scan_comment();
        }
        
        // 换行符
        if ch == '\n' {
            self.advance();
            self.line += 1;
            self.column = 1;
            return Ok(Token::Newline);
        }
        
        // 字符串字面量
        if ch == '"' || ch == '\'' {
            return self.scan_string(ch);
        }
        
        // 数字
        if ch.is_ascii_digit() || (ch == '-' && self.peek().map_or(false, |c| c.is_ascii_digit())) {
            return self.scan_number();
        }
        
        // 标识符或关键字
        if ch.is_alphabetic() || ch == '_' {
            return self.scan_identifier();
        }
        
        // 符号和操作符
        match ch {
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '{' => {
                self.advance();
                Ok(Token::LBrace)
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            ':' => {
                self.advance();
                Ok(Token::Colon)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            '.' => {
                self.advance();
                Ok(Token::Dot)
            }
            '=' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::Eq)
                } else if self.current_char() == '>' {
                    self.advance();
                    Ok(Token::Arrow)
                } else {
                    Ok(Token::Assign)
                }
            }
            '!' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::Ne)
                } else {
                    Ok(Token::Not)
                }
            }
            '>' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::Ge)
                } else {
                    Ok(Token::Gt)
                }
            }
            '<' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    Ok(Token::Le)
                } else {
                    Ok(Token::Lt)
                }
            }
            '&' => {
                self.advance();
                if self.current_char() == '&' {
                    self.advance();
                    Ok(Token::And)
                } else {
                    Err(format!("第 {} 行第 {} 列: 意外的字符 '&'", self.line, self.column))
                }
            }
            '|' => {
                self.advance();
                if self.current_char() == '|' {
                    self.advance();
                    Ok(Token::Or)
                } else {
                    Err(format!("第 {} 行第 {} 列: 意外的字符 '|'", self.line, self.column))
                }
            }
            _ => Err(format!("第 {} 行第 {} 列: 未知字符 '{}'", self.line, self.column, ch))
        }
    }
    
    /// 扫描所有 Token
    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        
        Ok(tokens)
    }
    
    // 内部辅助方法
    
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
    
    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }
    
    fn advance(&mut self) -> char {
        let ch = self.current_char();
        self.position += 1;
        self.column += 1;
        ch
    }
    
    fn skip_whitespace_except_newline(&mut self) {
        while !self.is_at_end() {
            let ch = self.current_char();
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    fn scan_comment(&mut self) -> Result<Token, String> {
        self.advance(); // 跳过 '#'
        let mut comment = String::new();
        
        while !self.is_at_end() && self.current_char() != '\n' {
            comment.push(self.advance());
        }
        
        Ok(Token::Comment(comment.trim().to_string()))
    }
    
    fn scan_string(&mut self, quote: char) -> Result<Token, String> {
        self.advance(); // 跳过开始引号
        let mut value = String::new();
        
        while !self.is_at_end() && self.current_char() != quote {
            let ch = self.advance();
            if ch == '\\' && !self.is_at_end() {
                // 转义字符
                let escaped = self.advance();
                match escaped {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
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
            return Err(format!("第 {} 行第 {} 列: 字符串未闭合", self.line, self.column));
        }
        
        self.advance(); // 跳过结束引号
        Ok(Token::String(value))
    }
    
    fn scan_number(&mut self) -> Result<Token, String> {
        let mut number = String::new();
        
        // 处理负号
        if self.current_char() == '-' {
            number.push(self.advance());
        }
        
        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            number.push(self.advance());
        }
        
        number.parse::<i64>()
            .map(Token::Number)
            .map_err(|_| format!("第 {} 行第 {} 列: 无效的数字 '{}'", self.line, self.column, number))
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
        
        // 识别关键字
        let token = match ident.as_str() {
            "set" => Token::Set,
            "env" => Token::Env,
            "IF" => Token::If,
            "ELIF" => Token::Elif,
            "ELSE" => Token::Else,
            "lib" => Token::Lib,
            "repo" => Token::Repo,
            "LOG" => Token::Log,
            "INIT" => Token::Init,
            "INFO" => Token::Info,
            "ERROR" => Token::Error,
            "DATA" => Token::Data,
            "DO" => Token::Do,
            "PIPE" => Token::Pipe,
            "COMM" => Token::Comm,
            "ACTION" => Token::Action,
            "CMD" => Token::Cmd,
            "PRINT" => Token::Print,
            "DOING" => Token::Doing,
            "AWAIT" => Token::Await,
            "FILES" => Token::Files,
            "ALL" => Token::All,
            "ENCRY" => Token::Encry,
            "CATCH" => Token::Catch,
            "true" => Token::Bool(true),
            "false" => Token::Bool(false),
            _ => Token::Ident(ident),
        };
        
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new("set(KEY, \"value\")");
        assert_eq!(lexer.next_token().unwrap(), Token::Set);
        assert_eq!(lexer.next_token().unwrap(), Token::LParen);
        assert_eq!(lexer.next_token().unwrap(), Token::Ident("KEY".to_string()));
        assert_eq!(lexer.next_token().unwrap(), Token::Comma);
        assert_eq!(lexer.next_token().unwrap(), Token::String("value".to_string()));
        assert_eq!(lexer.next_token().unwrap(), Token::RParen);
    }

    #[test]
    fn test_lexer_numbers() {
        let mut lexer = Lexer::new("42 -10");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(42));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(-10));
    }

    #[test]
    fn test_lexer_keywords() {
        let mut lexer = Lexer::new("IF ELIF ELSE true false");
        assert_eq!(lexer.next_token().unwrap(), Token::If);
        assert_eq!(lexer.next_token().unwrap(), Token::Elif);
        assert_eq!(lexer.next_token().unwrap(), Token::Else);
        assert_eq!(lexer.next_token().unwrap(), Token::Bool(true));
        assert_eq!(lexer.next_token().unwrap(), Token::Bool(false));
    }

    #[test]
    fn test_lexer_comment() {
        let mut lexer = Lexer::new("# This is a comment\nset");
        assert!(matches!(lexer.next_token().unwrap(), Token::Comment(_)));
        assert_eq!(lexer.next_token().unwrap(), Token::Newline);
        assert_eq!(lexer.next_token().unwrap(), Token::Set);
    }

    #[test]
    fn test_lexer_operators() {
        let mut lexer = Lexer::new("== != > < >= <= && ||");
        assert_eq!(lexer.next_token().unwrap(), Token::Eq);
        assert_eq!(lexer.next_token().unwrap(), Token::Ne);
        assert_eq!(lexer.next_token().unwrap(), Token::Gt);
        assert_eq!(lexer.next_token().unwrap(), Token::Lt);
        assert_eq!(lexer.next_token().unwrap(), Token::Ge);
        assert_eq!(lexer.next_token().unwrap(), Token::Le);
        assert_eq!(lexer.next_token().unwrap(), Token::And);
        assert_eq!(lexer.next_token().unwrap(), Token::Or);
    }
}
