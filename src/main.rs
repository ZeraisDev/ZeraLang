// ====================================================================
// ZERALANG - A Bi-Modal Programming Language
// Phase 1: Compound Assignment (+=) & String Interpolation ({var})
// Phase 2: Try/Catch/Throw, Ternary, Lambdas
// Phase 3: OOP (Classes, Instances, Fields, Methods, Inheritance)
// ====================================================================
mod converter;
mod repl;
mod vm; // <-- ADD THIS
use libloading::{Library, Symbol};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// --------------------------------------------------------------------
// TOKENS
// --------------------------------------------------------------------

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    // Literals
    Number(f64),
    String(String),
    Ident(String),

    // Keywords
    Set, To, Is, Greater, Less, Than, Then, Otherwise, Else,
    Show, Function, Taking, And, Or, Not, Gives, Back,
    If, End, While, Break, Continue,
    Define, Give, As,
    For, Each, In,
    True, False, Null,
    With,
    Try, Catch, Throw,
    // OOP keywords
    Class, Field, Construct, Extends,

    // Operators
    Plus, Minus, Star, Slash, Percent,
    GreaterEq, LessEq, NotEq,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    Question,
    // Punctuation
    Comma, LParen, RParen, LBracket, RBracket,
    LBrace, RBrace, Colon, Equals, EqEq, Dot,

    // Special
    NewLine,
    EOF,
    Import,
}

// --------------------------------------------------------------------
// LEXER
// --------------------------------------------------------------------
pub struct Lexer {
    chars: Vec<char>,
    position: usize,
    line: usize,
    col: usize,
    token_line: usize,
    token_col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Lexer {
        Lexer {
            chars: source.chars().collect(),
            position: 0,
            line: 1,
            col: 1,
            token_line: 1,
            token_col: 1,
        }
    }
    fn error(&self, msg: &str) -> ! {
        panic!("Error at line {}, column {}: {}", self.token_line, self.token_col, msg);
    }
    fn peek(&self) -> Option<&char> {
        self.chars.get(self.position)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.position).copied();
        if ch.is_some() {
            if ch == Some('\n') {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.position += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(&ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    pub fn last_pos(&self) -> (usize, usize) {
        (self.token_line, self.token_col)
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        self.token_line = self.line;
        self.token_col = self.col;

        let Some(&ch) = self.peek() else {
            return Token::EOF;
        };

        // Comments
        if ch == '/' {
            if let Some(&next) = self.chars.get(self.position + 1) {
                if next == '/' {
                    while let Some(&c) = self.peek() {
                        if c == '\n' { break; }
                        self.advance();
                    }
                    return self.next_token();
                }
                if next == '*' {
                    self.advance();
                    self.advance();
                    while let Some(&c) = self.peek() {
                        if c == '*' {
                            if let Some(&'/') = self.chars.get(self.position + 1) {
                                self.advance();
                                self.advance();
                                break;
                            }
                        }
                        self.advance();
                    }
                    return self.next_token();
                }
            }
        }
        if ch == '#' {
            while let Some(&c) = self.peek() {
                if c == '\n' { break; }
                self.advance();
            }
            return self.next_token();
        }

        if ch == '\n' {
            self.advance();
            return Token::NewLine;
        }

        if ch.is_ascii_digit() {
            let mut number_str = String::new();
            while let Some(&c) = self.peek() {
                if c.is_ascii_digit() || c == '.' {
                    number_str.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            return Token::Number(number_str.parse::<f64>().unwrap());
        }

        if ch == '"' {
            self.advance();
            let mut string_val = String::new();
            while let Some(&c) = self.peek() {
                if c == '"' {
                    self.advance();
                    break;
                }
                if c == '\\' {
                    self.advance();
                    if let Some(&next) = self.peek() {
                        let escaped = match next {
                            'n' => Some('\n'),
                            't' => Some('\t'),
                            'r' => Some('\r'),
                            '"' => Some('"'),
                            '\\' => Some('\\'),
                            '0' => Some('\0'),
                            '{' => Some('{'),
                            '}' => Some('}'),
                            _ => None,
                        };
                        if let Some(esc_ch) = escaped {
                            string_val.push(esc_ch);
                        } else {
                            string_val.push('\\');
                            string_val.push(next);
                        }
                        self.advance();
                    }
                } else {
                    string_val.push(c);
                    self.advance();
                }
            }
            return Token::String(string_val);
        }

        if ch.is_alphabetic() {
            let mut word = String::new();
            while let Some(&c) = self.peek() {
                if c.is_alphanumeric() || c == '_' {
                    word.push(c);
                    self.advance();
                } else {
                    break;
                }
            }

            return match word.to_lowercase().as_str() {
                "set"       => Token::Set,
                "to"        => Token::To,
                "is"        => Token::Is,
                "greater"   => Token::Greater,
                "less"      => Token::Less,
                "than"      => Token::Than,
                "then"      => Token::Then,
                "import"    => Token::Import,
                "otherwise" => Token::Otherwise,
                "else"      => Token::Else,
                "show"      => Token::Show,
                "function"  => Token::Function,
                "taking"    => Token::Taking,
                "and"       => Token::And,
                "or"        => Token::Or,
                "not"       => Token::Not,
                "gives"     => Token::Back,
                "if"        => Token::If,
                "while"     => Token::While,
                "end"       => Token::End,
                "break"     => Token::Break,
                "continue"  => Token::Continue,
                "define"    => Token::Define,
                "give"      => Token::Give,
                "back"      => Token::Back,
                "as"        => Token::As,
                "for"       => Token::For,
                "each"      => Token::Each,
                "in"        => Token::In,
                "true"      => Token::True,
                "false"     => Token::False,
                "null"      => Token::Null,
                "with"      => Token::With,
                "try"       => Token::Try,
                "catch"     => Token::Catch,
                "throw"     => Token::Throw,
                // OOP keywords
                "class"     => Token::Class,
                "field"     => Token::Field,
                "construct" => Token::Construct,
                "extends"   => Token::Extends,
                _           => Token::Ident(word),
            };
        }

        self.advance();
        match ch {
            '+' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::PlusEq
                } else {
                    Token::Plus
                }
            }
            '-' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::MinusEq
                } else {
                    Token::Minus
                }
            }
            '*' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::StarEq
                } else {
                    Token::Star
                }
            }
            '/' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::SlashEq
                } else {
                    Token::Slash
                }
            }
            '%' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::PercentEq
                } else {
                    Token::Percent
                }
            }
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            ':' => Token::Colon,
            ',' => Token::Comma,
            '.' => Token::Dot,
            '?' => Token::Question,
            '!' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::NotEq
                } else {
                    Token::Not
                }
            }
            '&' => {
                if let Some(&'&') = self.chars.get(self.position) {
                    self.advance();
                    Token::And
                } else {
                    self.error("Unexpected character: & (did you mean '&&'?)");
                }
            }
            '|' => {
                if let Some(&'|') = self.chars.get(self.position) {
                    self.advance();
                    Token::Or
                } else {
                    self.error("Unexpected character: | (did you mean '||'?)");
                }
            }
            '=' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Equals
                }
            }
            '>' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::GreaterEq
                } else {
                    Token::Greater
                }
            }
            '<' => {
                if let Some(&'=') = self.chars.get(self.position) {
                    self.advance();
                    Token::LessEq
                } else {
                    Token::Less
                }
            }
            _ => self.error(&format!("Unexpected character: {}", ch)),
        }
    }
}

// --------------------------------------------------------------------
// AST
// --------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Ident(String),
    BinOp(Box<Expr>, Token, Box<Expr>),
    Unary(Token, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Array(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Lambda(Vec<String>, Vec<Stmt>),
    This, // OOP: 'self' / 'this' keyword
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Set(String, Expr),
    SetField(Box<Expr>, String, Expr), // OOP: target.field = value
    Show(Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    Function(String, Vec<String>, Vec<Stmt>),
    Return(Expr),
    ForEach(String, Expr, Vec<Stmt>),
    ExprStmt(Expr),
    Break,
    Continue,
    Import(String),
    Try(Vec<Stmt>, String, Vec<Stmt>),
    Throw(Expr),
    // OOP: Class definition
    // (name, superclass_name, fields, constructor, methods)
    Class(
        String,
        Option<String>,
        Vec<String>,
        Option<(Vec<String>, Vec<Stmt>)>,
        Vec<(String, Vec<String>, Vec<Stmt>)>,
    ),
}

#[derive(Debug, Clone)]
pub enum BlockResult {
    Normal,
    Break,
    Continue,
    Return(Value),
}

// --------------------------------------------------------------------
// OOP: Class & Instance definitions
// --------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub superclass: Option<Rc<ClassDef>>,
    pub fields: Vec<String>,
    pub constructor: Option<(Vec<String>, Vec<Stmt>)>,
    pub methods: HashMap<String, Rc<FunctionDef>>,
}
// C-FFI Native Module
pub struct NativeModule {
    pub name: String,
    pub lib: Library,
}

// We have to implement Debug manually because Library doesn't derive it
impl std::fmt::Debug for NativeModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[native module '{}']", self.name)
    }
}
#[derive(Debug, Clone)]
pub struct InstanceData {
    pub class: Rc<ClassDef>,
    pub fields: HashMap<String, Value>,
}

/// Look up a method by walking the superclass chain.
fn lookup_method(class: &Rc<ClassDef>, name: &str) -> Option<Rc<FunctionDef>> {
    if let Some(m) = class.methods.get(name) {
        return Some(m.clone());
    }
    if let Some(parent) = &class.superclass {
        return lookup_method(parent, name);
    }
    None
}

// --------------------------------------------------------------------
// PARSER
// --------------------------------------------------------------------
pub struct Parser {
    tokens: Vec<Token>,
    lines: Vec<usize>,
    cols: Vec<usize>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, lines: Vec<usize>, cols: Vec<usize>) -> Parser {
        Parser { tokens, lines, cols, pos: 0 }
    }

    fn error(&self, msg: &str) -> ! {
        let (line, col) = if self.pos < self.lines.len() {
            (self.lines[self.pos], self.cols[self.pos])
        } else if !self.lines.is_empty() {
            (*self.lines.last().unwrap(), *self.cols.last().unwrap())
        } else {
            (1, 1)
        };
        panic!("Error at line {}, column {}: {}", line, col, msg);
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_next(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let t = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn is_no_paren_arg_starter(&self) -> bool {
        matches!(self.peek(),
            Some(Token::Number(_)) |
            Some(Token::String(_)) |
            Some(Token::True) |
            Some(Token::False) |
            Some(Token::Null) |
            Some(Token::Ident(_)) |
            Some(Token::LBrace)
        )
    }

    pub fn parse_program(&mut self) -> Vec<Stmt> {
        self.parse_block()
    }

    fn parse_block(&mut self) -> Vec<Stmt> {
        let mut statements = Vec::new();
        while let Some(t) = self.peek() {
            match t {
                Token::EOF | Token::End | Token::Otherwise | Token::Else | Token::RBrace => break,
                Token::NewLine => { self.advance(); }
                _ => statements.push(self.parse_statement()),
            }
        }
        statements
    }

    fn parse_function(&mut self) -> Stmt {
        self.advance(); // consume keyword

        let name = match self.advance() {
            Some(Token::Ident(n)) => n,
            _ => { self.error("Expected function name"); }
        };

        let mut params = Vec::new();

        if let Some(Token::Taking) = self.peek() {
            self.advance();
            while let Some(t) = self.peek() {
                if t == &Token::As { break; }
                if let Some(Token::Ident(p)) = self.advance() { params.push(p); }
                if let Some(Token::And) = self.peek() { self.advance(); }
            }
        }
        else if let Some(Token::LParen) = self.peek() {
            self.advance();
            while let Some(t) = self.peek() {
                if t == &Token::RParen { break; }
                if let Some(Token::Ident(p)) = self.advance() { params.push(p); }
                if let Some(Token::Comma) = self.peek() { self.advance(); }
            }
            self.advance();
        }

        match self.advance() {
            Some(Token::As) | Some(Token::LBrace) => {},
            _ => self.error("Expected 'as' or '{{' before function body"),
        }

        while let Some(Token::NewLine) = self.peek() { self.advance(); }
        let body = self.parse_block();

        match self.advance() {
            Some(Token::End) | Some(Token::RBrace) => {},
            _ => self.error("Expected 'end' or '}}' after function body"),
        }
        Stmt::Function(name, params, body)
    }

    // ================================================================
    // OOP: Class parsing
    // ================================================================
    fn parse_class(&mut self) -> Stmt {
        self.advance(); // consume 'class'

        let name = match self.advance() {
            Some(Token::Ident(n)) => n,
            _ => self.error("Expected class name after 'class'"),
        };

        let superclass = if matches!(self.peek(), Some(Token::Extends)) {
            self.advance();
            match self.advance() {
                Some(Token::Ident(n)) => Some(n),
                _ => self.error("Expected superclass name after 'extends'"),
            }
        } else {
            None
        };

        // Accept '{' or newline
        let uses_braces = match self.advance() {
            Some(Token::LBrace) => true,
            Some(Token::NewLine) => false,
            _ => self.error("Expected '{' or newline after class name"),
        };

        let mut fields = Vec::new();
        let mut constructor: Option<(Vec<String>, Vec<Stmt>)> = None;
        let mut methods: Vec<(String, Vec<String>, Vec<Stmt>)> = Vec::new();

        loop {
            match self.peek() {
                None | Some(Token::EOF) => break,
                Some(Token::End) | Some(Token::RBrace) => break,
                Some(Token::NewLine) => { self.advance(); }
                Some(Token::Field) => {
                    self.advance();
                    match self.advance() {
                        Some(Token::Ident(n)) => fields.push(n),
                        _ => self.error("Expected field name after 'field'"),
                    }
                }
                Some(Token::Construct) => {
                    self.advance();
                    let (params, body) = self.parse_construct_body();
                    constructor = Some((params, body));
                }
                Some(Token::Function) | Some(Token::Define) => {
                    let func_stmt = self.parse_function();
                    if let Stmt::Function(mname, mparams, mbody) = func_stmt {
                        methods.push((mname, mparams, mbody));
                    }
                }
                Some(t) => {
                    if let Token::Ident(name) = t {
                        if name == "func" {
                            let func_stmt = self.parse_function();
                            if let Stmt::Function(mname, mparams, mbody) = func_stmt {
                                methods.push((mname, mparams, mbody));
                            }
                            continue;
                        }
                    }
                    self.advance();
                }
            }
        }

        if uses_braces {
            if let Some(Token::RBrace) = self.peek() { self.advance(); }
        } else {
            match self.advance() {
                Some(Token::End) | Some(Token::RBrace) => {},
                _ => self.error("Expected 'end' after class body"),
            }
        }

        Stmt::Class(name, superclass, fields, constructor, methods)
    }

    fn parse_construct_body(&mut self) -> (Vec<String>, Vec<Stmt>) {
        let mut params = Vec::new();

        if matches!(self.peek(), Some(Token::LParen)) {
            self.advance();
            while let Some(t) = self.peek() {
                if t == &Token::RParen { break; }
                if let Some(Token::Ident(p)) = self.advance() { params.push(p); }
                if matches!(self.peek(), Some(Token::Comma)) { self.advance(); }
            }
            match self.advance() {
                Some(Token::RParen) => {},
                _ => self.error("Expected ')' after constructor parameters"),
            }
            match self.advance() {
                Some(Token::LBrace) => {},
                _ => self.error("Expected '{' before constructor body"),
            }
        } else if matches!(self.peek(), Some(Token::Taking)) {
            self.advance();
            while let Some(t) = self.peek() {
                if t == &Token::As { break; }
                if let Some(Token::Ident(p)) = self.advance() { params.push(p); }
                if matches!(self.peek(), Some(Token::And)) { self.advance(); }
            }
            match self.advance() {
                Some(Token::As) => {},
                _ => self.error("Expected 'as' after constructor parameters"),
            }
        }

        while let Some(Token::NewLine) = self.peek() { self.advance(); }
        let body = self.parse_block();

        match self.advance() {
            Some(Token::End) | Some(Token::RBrace) => {},
            _ => self.error("Expected 'end' or '}' after constructor body"),
        }

        (params, body)
    }

    // ================================================================
    // Statement parsing
    // ================================================================

    fn parse_statement(&mut self) -> Stmt {
        if let Some(Token::Ident(name)) = self.peek() {
            if name == "func" { return self.parse_function(); }
            if name == "return" {
                self.advance();
                let expr = self.parse_expression();
                return Stmt::Return(expr);
            }
        }

        match self.peek().unwrap().clone() {
            Token::Class => self.parse_class(),

            Token::Set => {
                self.advance();
                let var_name = match self.advance() {
                    Some(Token::Ident(name)) => name,
                    _ => self.error("Expected variable name after 'set'"),
                };
                match self.advance() {
                    Some(Token::To) => {},
                    _ => self.error("Expected 'to' after variable name"),
                }
                let value = self.parse_expression();
                Stmt::Set(var_name, value)
            }

            Token::Ident(_) => {
                let name = if let Some(Token::Ident(n)) = self.peek() { n.clone() } else { String::new() };

                // Compound assignment (+=, -=, etc.)
                if matches!(self.peek_next(),
                    Some(Token::PlusEq) | Some(Token::MinusEq) |
                    Some(Token::StarEq) | Some(Token::SlashEq) |
                    Some(Token::PercentEq)
                ) {
                    self.advance();
                    let op_token = self.advance().unwrap();
                    let value_expr = self.parse_expression();

                    let bin_op = match op_token {
                        Token::PlusEq => Token::Plus,
                        Token::MinusEq => Token::Minus,
                        Token::StarEq => Token::Star,
                        Token::SlashEq => Token::Slash,
                        Token::PercentEq => Token::Percent,
                        _ => unreachable!(),
                    };

                    let expr = Expr::BinOp(
                        Box::new(Expr::Ident(name.clone())),
                        bin_op,
                        Box::new(value_expr)
                    );
                    return Stmt::Set(name, expr);
                }

                // Standard assignment: x = value
                if self.peek_next() == Some(&Token::Equals) {
                    self.advance();
                    self.advance();
                    let value = self.parse_expression();
                    return Stmt::Set(name, value);
                }

                // Fall through: expression or field assignment
                self.parse_expr_statement()
            }

            Token::Show => {
                self.advance();
                let value = self.parse_expression();
                Stmt::Show(value)
            }

            Token::If => {
                self.advance();
                let condition = self.parse_expression();

                let uses_braces = match self.advance() {
                    Some(Token::Then) => false,
                    Some(Token::LBrace) => true,
                    _ => self.error("Expected 'then' or '{' after condition"),
                };

                while let Some(Token::NewLine) = self.peek() { self.advance(); }
                let then_block = self.parse_block();

                if uses_braces {
                    if let Some(Token::RBrace) = self.peek() { self.advance(); }
                }

                let otherwise_block = if matches!(self.peek(), Some(Token::Otherwise) | Some(Token::Else)) {
                    self.advance();
                    while let Some(Token::NewLine) = self.peek() { self.advance(); }
                    if let Some(Token::LBrace) = self.peek() { self.advance(); }
                    while let Some(Token::NewLine) = self.peek() { self.advance(); }
                    let block = self.parse_block();
                    if uses_braces {
                        if let Some(Token::RBrace) = self.peek() { self.advance(); }
                    }
                    block
                } else {
                    Vec::new()
                };

                if !uses_braces {
                    match self.advance() {
                        Some(Token::End) | Some(Token::RBrace) => {},
                        _ => self.error("Expected 'end' or '}' at the end of if statement"),
                    }
                }

                Stmt::If(condition, then_block, otherwise_block)
            }

            Token::While => {
                self.advance();
                let condition = self.parse_expression();

                match self.advance() {
                    Some(Token::Then) | Some(Token::LBrace) => {},
                    _ => self.error("Expected 'then' or '{' after while condition"),
                }

                while let Some(Token::NewLine) = self.peek() { self.advance(); }
                let body = self.parse_block();

                match self.advance() {
                    Some(Token::End) | Some(Token::RBrace) => {},
                    _ => self.error("Expected 'end' or '}' at the end of while loop"),
                }
                Stmt::While(condition, body)
            }

            Token::For => {
                self.advance();
                if let Some(Token::Each) = self.peek() { self.advance(); }

                let var_name = match self.advance() {
                    Some(Token::Ident(n)) => n,
                    _ => self.error("Expected variable name"),
                };
                match self.advance() {
                    Some(Token::In) => {},
                    _ => self.error("Expected 'in' after variable name"),
                }
                let iterable = self.parse_expression();

                match self.advance() {
                    Some(Token::Then) | Some(Token::LBrace) => {},
                    _ => self.error("Expected 'then' or '{' after iterable"),
                }

                while let Some(Token::NewLine) = self.peek() { self.advance(); }
                let body = self.parse_block();

                match self.advance() {
                    Some(Token::End) | Some(Token::RBrace) => {},
                    _ => self.error("Expected 'end' or '}' at the end of for loop"),
                }
                Stmt::ForEach(var_name, iterable, body)
            }
            Token::Import => {
                self.advance();
                let path = match self.advance() {
                    Some(Token::String(s)) => s,
                    _ => self.error("Expected string path after 'import'"),
                };
                Stmt::Import(path)
            }
            Token::Define => self.parse_function(),

            Token::Give => {
                self.advance();
                match self.advance() {
                    Some(Token::Back) => {},
                    _ => self.error("Expected 'back' after 'give'"),
                }
                let expr = self.parse_expression();
                Stmt::Return(expr)
            }
            Token::Try => {
                self.advance();
                match self.advance() {
                    Some(Token::LBrace) => {},
                    _ => self.error("Expected '{' after 'try'"),
                }
                while let Some(Token::NewLine) = self.peek() { self.advance(); }
                let try_block = self.parse_block();
                match self.advance() {
                    Some(Token::RBrace) => {},
                    _ => self.error("Expected '}' after try block"),
                }

                let catch_var = match self.advance() {
                    Some(Token::Catch) => {
                        match self.advance() {
                            Some(Token::Ident(v)) => v,
                            _ => self.error("Expected variable name after 'catch'"),
                        }
                    }
                    _ => self.error("Expected 'catch' after try block"),
                };

                match self.advance() {
                    Some(Token::LBrace) => {},
                    _ => self.error("Expected '{' after catch variable"),
                }
                while let Some(Token::NewLine) = self.peek() { self.advance(); }
                let catch_block = self.parse_block();
                match self.advance() {
                    Some(Token::RBrace) => {},
                    _ => self.error("Expected '}' after catch block"),
                }

                Stmt::Try(try_block, catch_var, catch_block)
            }

            Token::Throw => {
                self.advance();
                let expr = self.parse_expression();
                Stmt::Throw(expr)
            }

            Token::Break => {
                self.advance();
                Stmt::Break
            }

            Token::Continue => {
                self.advance();
                Stmt::Continue
            }

            _ => self.parse_expr_statement(),
        }
    }

    /// Parse an expression, then check if it's followed by `=` for field assignment.
    fn parse_expr_statement(&mut self) -> Stmt {
        let expr = self.parse_expression();
        if let Some(Token::Equals) = self.peek() {
            self.advance();
            let value = self.parse_expression();
            if let Expr::Index(target, key) = &expr {
                if let Expr::String(field_name) = key.as_ref() {
                    return Stmt::SetField(target.clone(), field_name.clone(), value);
                }
            }
            self.error("Invalid assignment target (expected obj.field = value)");
        }
        Stmt::ExprStmt(expr)
    }

    // ================================================================
    // EXPRESSION PARSING
    // ================================================================

    fn parse_expression(&mut self) -> Expr {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Expr {
        let cond = self.parse_or();
        if matches!(self.peek(), Some(Token::Question)) {
            self.advance();
            let then_expr = self.parse_ternary();
            match self.advance() {
                Some(Token::Colon) => {},
                _ => self.error("Expected ':' in ternary expression"),
            }
            let else_expr = self.parse_ternary();
            return Expr::Ternary(Box::new(cond), Box::new(then_expr), Box::new(else_expr));
        }
        cond
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();
        while let Some(Token::Or) = self.peek() {
            self.advance();
            let right = self.parse_and();
            left = Expr::BinOp(Box::new(left), Token::Or, Box::new(right));
        }
        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_comparison();
        while let Some(Token::And) = self.peek() {
            self.advance();
            let right = self.parse_comparison();
            left = Expr::BinOp(Box::new(left), Token::And, Box::new(right));
        }
        left
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut left = self.parse_additive();
        while let Some(t) = self.peek() {
            match t {
                Token::Greater | Token::GreaterEq | Token::Less
                | Token::LessEq | Token::EqEq | Token::NotEq | Token::Is => {
                    let op = self.advance().unwrap();
                    if op == Token::Greater || op == Token::Less {
                        if let Some(Token::Than) = self.peek() { self.advance(); }
                    }
                    let actual_op = if op == Token::Is {
                        if let Some(Token::Not) = self.peek() {
                            self.advance();
                            Token::NotEq
                        } else {
                            Token::EqEq
                        }
                    } else { op };
                    let right = self.parse_additive();
                    left = Expr::BinOp(Box::new(left), actual_op, Box::new(right));
                }
                _ => break,
            }
        }
        left
    }

    fn parse_additive(&mut self) -> Expr {
        let mut left = self.parse_multiplicative();
        while let Some(t) = self.peek() {
            match t {
                Token::Plus | Token::Minus => {
                    let op = self.advance().unwrap();
                    let right = self.parse_multiplicative();
                    left = Expr::BinOp(Box::new(left), op, Box::new(right));
                }
                _ => break,
            }
        }
        left
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut left = self.parse_unary();
        while let Some(t) = self.peek() {
            match t {
                Token::Star | Token::Slash | Token::Percent => {
                    let op = self.advance().unwrap();
                    let right = self.parse_unary();
                    left = Expr::BinOp(Box::new(left), op, Box::new(right));
                }
                _ => break,
            }
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        if matches!(self.peek(), Some(Token::Minus)) {
            self.advance();
            let expr = self.parse_unary();
            return Expr::Unary(Token::Minus, Box::new(expr));
        }
        if matches!(self.peek(), Some(Token::Not)) {
            self.advance();
            let expr = self.parse_unary();
            return Expr::Unary(Token::Not, Box::new(expr));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Expr {
        let mut expr = self.parse_atom();
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.advance();
                    let key = match self.advance() {
                        Some(Token::Ident(n)) => n,
                        _ => self.error("Expected identifier after '.'"),
                    };
                    expr = Expr::Index(Box::new(expr), Box::new(Expr::String(key)));
                }
                Some(Token::LBracket) => {
                    self.advance();
                    let index = self.parse_expression();
                    match self.advance() {
                        Some(Token::RBracket) => {},
                        _ => self.error("Expected ']' after index"),
                    }
                    expr = Expr::Index(Box::new(expr), Box::new(index));
                }
                Some(Token::LParen) => {
                    self.advance();
                    let mut args = Vec::new();
                    while let Some(t) = self.peek() {
                        if t == &Token::RParen { break; }
                        args.push(self.parse_expression());
                        if let Some(Token::Comma) = self.peek() { self.advance(); }
                    }
                    match self.advance() {
                        Some(Token::RParen) => {},
                        _ => self.error("Expected ')' after arguments"),
                    }
                    expr = Expr::Call(Box::new(expr), args);
                }
                _ => break,
            }
        }
        expr
    }

    fn build_interpolated_string(&mut self, raw: &str) -> Expr {
        if !raw.contains('{') {
            return Expr::String(raw.to_string());
        }

        let mut parts: Vec<Expr> = Vec::new();
        let mut chars = raw.chars().peekable();
        let mut current_str = String::new();

        while let Some(c) = chars.next() {
            if c == '{' {
                if !current_str.is_empty() {
                    parts.push(Expr::String(current_str.clone()));
                    current_str.clear();
                }
                let mut expr_str = String::new();
                let mut depth = 1;
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        depth -= 1;
                        if depth == 0 {
                            chars.next();
                            break;
                        }
                    } else if next == '{' {
                        depth += 1;
                    }
                    expr_str.push(next);
                    chars.next();
                }

                if !expr_str.is_empty() {
                    let mut lexer = Lexer::new(&expr_str);
                    let mut tokens = Vec::new();
                    let mut lines = Vec::new();
                    let mut cols = Vec::new();
                    loop {
                        let token = lexer.next_token();
                        let (l, c) = lexer.last_pos();
                        if token == Token::EOF { break; }
                        tokens.push(token);
                        lines.push(l);
                        cols.push(c);
                    }
                    let mut parser = Parser::new(tokens, lines, cols);
                    let expr = parser.parse_expression();
                    parts.push(Expr::Call(
                        Box::new(Expr::Ident("str".to_string())),
                        vec![expr]
                    ));
                }
            } else {
                current_str.push(c);
            }
        }
        if !current_str.is_empty() {
            parts.push(Expr::String(current_str));
        }

        if parts.is_empty() {
            return Expr::String(String::new());
        }

        let mut result = parts[0].clone();
        for p in parts.iter().skip(1) {
            result = Expr::BinOp(Box::new(result), Token::Plus, Box::new(p.clone()));
        }
        result
    }

    fn parse_atom(&mut self) -> Expr {
        match self.advance() {
            Some(Token::Number(n))  => Expr::Number(n),
            Some(Token::String(s))  => self.build_interpolated_string(&s),
            Some(Token::True)       => Expr::Boolean(true),
            Some(Token::False)      => Expr::Boolean(false),
            Some(Token::Null)       => Expr::Null,

            Some(Token::LBracket) => {
                let mut elements = Vec::new();
                while let Some(t) = self.peek() {
                    if t == &Token::RBracket { break; }
                    elements.push(self.parse_expression());
                    if let Some(Token::Comma) = self.peek() { self.advance(); }
                }
                match self.advance() {
                    Some(Token::RBracket) => {},
                    _ => self.error("Expected ']' after array elements"),
                }
                Expr::Array(elements)
            }

            Some(Token::LBrace) => {
                let mut pairs = Vec::new();
                while let Some(t) = self.peek() {
                    if t == &Token::RBrace { break; }
                    let key = self.parse_expression();
                    match self.advance() {
                        Some(Token::Colon) => {},
                        _ => self.error("Expected ':' after dictionary key"),
                    }
                    let value = self.parse_expression();
                    pairs.push((key, value));
                    if let Some(Token::Comma) = self.peek() { self.advance(); }
                }
                match self.advance() {
                    Some(Token::RBrace) => {},
                    _ => self.error("Expected '}}' after dictionary pairs"),
                }
                Expr::Dict(pairs)
            }

            Some(Token::Ident(name)) => {
                // Lambda parsing
                if name == "func" {
                    let mut params = Vec::new();
                    if let Some(Token::LParen) = self.peek() {
                        self.advance();
                        while let Some(t) = self.peek() {
                            if t == &Token::RParen { break; }
                            if let Some(Token::Ident(p)) = self.advance() { params.push(p); }
                            if let Some(Token::Comma) = self.peek() { self.advance(); }
                        }
                        match self.advance() {
                            Some(Token::RParen) => {},
                            _ => self.error("Expected ')' after lambda params"),
                        }
                    }
                    match self.advance() {
                        Some(Token::LBrace) => {},
                        _ => self.error("Expected '{' before lambda body"),
                    }
                    while let Some(Token::NewLine) = self.peek() { self.advance(); }
                    let body = self.parse_block();
                    match self.advance() {
                        Some(Token::RBrace) => {},
                        _ => self.error("Expected '}' after lambda body"),
                    }
                    return Expr::Lambda(params, body);
                }

                // OOP: self / this keyword
                if name == "self" || name == "this" {
                    return Expr::This;
                }

                if self.is_no_paren_arg_starter() {
                    let mut args = vec![self.parse_primary()];
                    while let Some(Token::With) = self.peek() {
                        self.advance();
                        args.push(self.parse_primary());
                    }
                    Expr::Call(Box::new(Expr::Ident(name)), args)
                } else {
                    Expr::Ident(name)
                }
            }

            Some(Token::LParen) => {
                let expr = self.parse_expression();
                match self.advance() {
                    Some(Token::RParen) => {},
                    _ => self.error("Expected ')' after expression"),
                }
                expr
            }
            _ => self.error("Unexpected token in expression"),
        }
    }
}

// ====================================================================
// EVALUATOR
// ====================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    params: Vec<String>,
    body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Function(Rc<FunctionDef>),
    Array(Rc<Vec<Value>>),
    Dict(Rc<HashMap<String, Value>>),
    Class(Rc<ClassDef>),
    Instance(Rc<RefCell<InstanceData>>),
    NativeModule(Rc<NativeModule>),
    Pointer(usize), // <-- NEW: Holds a raw memory address
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Array(a), Value::Array(b)) => {
                if a.len() != b.len() { return false; }
                a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (Value::Dict(a), Value::Dict(b)) => {
                if a.len() != b.len() { return false; }
                a.iter().all(|(k, v)| b.get(k).is_some_and(|bv| v == bv))
            }
            (Value::Class(a), Value::Class(b)) => Rc::ptr_eq(a, b),
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::NativeModule(a), Value::NativeModule(b)) => Rc::ptr_eq(a, b), // <-- NEW

            (Value::Pointer(a), Value::Pointer(b)) => a == b,
            _ => false,

        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Function(_) => write!(f, "[function]"),
            Value::Array(arr) => {
                let strs: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", strs.join(", "))
            }
            Value::Dict(map) => {
                let strs: Vec<String> = map.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                write!(f, "{{{}}}", strs.join(", "))
            }
            Value::Class(c) => write!(f, "[class {}]", c.name),
            Value::Instance(i) => write!(f, "[{} instance]", i.borrow().class.name),
            Value::NativeModule(m) => write!(f, "[native module '{}']", m.name), // <-- NEW
            Value::Pointer(p) => write!(f, "[pointer 0x{:x}]", p), // <-- NEW
        }
    }
}

pub struct Environment {
    variables: HashMap<String, Value>,
    imported_files: HashSet<String>,
}

impl Environment {
    pub fn new() -> Environment {
        Environment {
            variables: HashMap::new(),
            imported_files: HashSet::new(),
        }
    }
    fn error(&self, msg: &str) -> ! {
        panic!("Runtime error: {}", msg);
    }
    pub fn execute_block(&mut self, statements: &[Stmt]) -> BlockResult {
        for stmt in statements {
            let result = self.execute_statement(stmt);
            match result {
                BlockResult::Normal => continue,
                _ => return result,
            }
        }
        BlockResult::Normal
    }

    fn is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::Boolean(b) => *b,
            Value::Number(n)  => *n != 0.0,
            Value::String(s)  => !s.is_empty(),
            Value::Null       => false,
            Value::Array(a)   => !a.is_empty(),
            Value::Dict(d)    => !d.is_empty(),
            Value::Function(_) => true,
            Value::Class(_) => true,
            Value::Instance(_) => true,
            Value::NativeModule(_) => true, // <-- ADD THIS
            Value::Pointer(p) => *p != 0, // <-- NEW: Non-null pointers are truthy
        }
    }

    fn execute_statement(&mut self, stmt: &Stmt) -> BlockResult {
        match stmt {
            Stmt::Set(name, expr) => {
                let value = self.evaluate_expression(expr);
                self.variables.insert(name.clone(), value);
                BlockResult::Normal
            }

            // OOP: Field assignment — self.field = value  or  obj.field = value
            Stmt::SetField(target, field_name, value_expr) => {
                let target_val = self.evaluate_expression(target);
                let value = self.evaluate_expression(value_expr);
                if let Value::Instance(inst) = target_val {
                    inst.borrow_mut().fields.insert(field_name.clone(), value);
                } else {
                    self.error("Can only set fields on instances!");
                }
                BlockResult::Normal
            }

            // OOP: Class definition
            Stmt::Class(name, superclass, fields, constructor, methods) => {
                let super_def = if let Some(parent_name) = superclass {
                    match self.variables.get(parent_name) {
                        Some(Value::Class(c)) => Some(c.clone()),
                        _ => self.error(&format!("Superclass '{}' not found", parent_name)),
                    }
                } else {
                    None
                };

                // Merge superclass fields
                let mut all_fields = Vec::new();
                if let Some(parent) = &super_def {
                    all_fields.extend(parent.fields.clone());
                }
                all_fields.extend(fields.clone());

                // Build methods map
                let mut methods_map = HashMap::new();
                for (mname, mparams, mbody) in methods {
                    let func_def = Rc::new(FunctionDef {
                        params: mparams.clone(),
                        body: mbody.clone(),
                    });
                    methods_map.insert(mname.clone(), func_def);
                }

                let class_def = Rc::new(ClassDef {
                    name: name.clone(),
                    superclass: super_def,
                    fields: all_fields,
                    constructor: constructor.clone(),
                    methods: methods_map,
                });

                self.variables.insert(name.clone(), Value::Class(class_def));
                BlockResult::Normal
            }

            Stmt::Show(expr) => {
                let value = self.evaluate_expression(expr);
                println!("{}", value);
                BlockResult::Normal
            }
            Stmt::If(condition, then_block, otherwise_block) => {
                let cond_value = self.evaluate_expression(condition);
                if self.is_truthy(&cond_value) {
                    self.execute_block(then_block)
                } else {
                    self.execute_block(otherwise_block)
                }
            }
            Stmt::While(condition, body) => {
                loop {
                    let cond_value = self.evaluate_expression(condition);
                    if self.is_truthy(&cond_value) {
                        match self.execute_block(body) {
                            BlockResult::Return(v) => return BlockResult::Return(v),
                            BlockResult::Break => break,
                            BlockResult::Continue => continue,
                            BlockResult::Normal => {}
                        }
                    } else { break; }
                }
                BlockResult::Normal
            }
            Stmt::Try(try_block, catch_var, catch_block) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.execute_block(try_block)
                }));

                match result {
                    Ok(block_res) => match block_res {
                        BlockResult::Normal => BlockResult::Normal,
                        other => other,
                    },
                    Err(panic_payload) => {
                        let err_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "Unknown runtime error".to_string()
                        };

                        let clean_msg = if err_msg.starts_with("ZERA_THROW: ") {
                            err_msg.strip_prefix("ZERA_THROW: ").unwrap().to_string()
                        } else if err_msg.starts_with("Runtime error: ") {
                            err_msg.strip_prefix("Runtime error: ").unwrap().to_string()
                        } else {
                            err_msg
                        };

                        self.variables.insert(catch_var.clone(), Value::String(clean_msg));
                        self.execute_block(catch_block)
                    }
                }
            }

            Stmt::Throw(expr) => {
                let val = self.evaluate_expression(expr);
                panic!("ZERA_THROW: {}", val.to_string());
            }

            Stmt::ForEach(var_name, iterable, body) => {
                let iter_val = self.evaluate_expression(iterable);
                if let Value::Array(arr) = iter_val {
                    for item in arr.iter() {
                        self.variables.insert(var_name.clone(), item.clone());
                        match self.execute_block(body) {
                            BlockResult::Return(v) => return BlockResult::Return(v),
                            BlockResult::Break => break,
                            BlockResult::Continue => continue,
                            BlockResult::Normal => {}
                        }
                    }
                } else { self.error("Can only loop over arrays!"); }
                BlockResult::Normal
            }
            Stmt::Function(name, params, body) => {
                let func_def = FunctionDef { params: params.clone(), body: body.clone() };
                self.variables.insert(name.clone(), Value::Function(Rc::new(func_def)));
                BlockResult::Normal
            }
            Stmt::Return(expr) => {
                let value = self.evaluate_expression(expr);
                BlockResult::Return(value)
            }
            Stmt::ExprStmt(expr) => {
                self.evaluate_expression(expr);
                BlockResult::Normal
            }
            Stmt::Import(path) => {
                let abs_path = match std::fs::canonicalize(&path) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => path.clone(),
                };
                if self.imported_files.contains(&abs_path) {
                    return BlockResult::Normal;
                }
                self.imported_files.insert(abs_path);
                let source = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => self.error(&format!("Failed to import '{}': {}", path, e)),
                };
                let ast = crate::lex_and_parse(&source);
                match self.execute_block(&ast) {
                    BlockResult::Normal => BlockResult::Normal,
                    other => other,
                }
            }
            Stmt::Break => BlockResult::Break,
            Stmt::Continue => BlockResult::Continue,
        }
    }

    pub fn evaluate_expression(&mut self, expr: &Expr) -> Value {
        match expr {
            Expr::Number(n)  => Value::Number(*n),
            Expr::String(s)  => Value::String(s.clone()),
            Expr::Boolean(b) => Value::Boolean(*b),
            Expr::Null       => Value::Null,

            // OOP: self / this
            Expr::This => {
                self.variables.get("self").cloned().unwrap_or(Value::Null)
            }

            Expr::Ident(name) => self.variables.get(name).cloned().unwrap_or(Value::Null),

            Expr::Unary(op, expr) => {
                let val = self.evaluate_expression(expr);
                match op {
                    Token::Minus => {
                        if let Value::Number(n) = val { Value::Number(-n) }
                        else { self.error("Cannot negate a non-number!"); }
                    }
                    Token::Not => Value::Boolean(!self.is_truthy(&val)),
                    _ => self.error(&format!("Unknown unary operator: {:?}", op)),
                }
            }

            Expr::Array(elements) => {
                let mut vals = Vec::new();
                for e in elements { vals.push(self.evaluate_expression(e)); }
                Value::Array(Rc::new(vals))
            }

            Expr::Dict(pairs) => {
                let mut map = HashMap::new();
                for (key_expr, val_expr) in pairs {
                    let key_val = self.evaluate_expression(key_expr);
                    let val = self.evaluate_expression(val_expr);
                    let key_str = match key_val {
                        Value::String(s) => s,
                        Value::Number(n) => n.to_string(),
                        _ => self.error("Dictionary keys must be strings or numbers!"),
                    };
                    map.insert(key_str, val);
                }
                Value::Dict(Rc::new(map))
            }

            Expr::Index(array_expr, index_expr) => {
                let collection = self.evaluate_expression(array_expr);
                let index_val = self.evaluate_expression(index_expr);

                if let (Value::Array(arr), Value::Number(i)) = (&collection, &index_val) {
                    return arr.get(*i as usize).cloned().unwrap_or(Value::Null);
                }
                if let (Value::Dict(map), Value::String(s)) = (&collection, &index_val) {
                    return map.get(s).cloned().unwrap_or(Value::Null);
                }

                // OOP: Instance field access
                if let Value::Instance(inst) = &collection {
                    if let Value::String(field_name) = &index_val {
                        let inst_ref = inst.borrow();
                        if let Some(val) = inst_ref.fields.get(field_name) {
                            return val.clone();
                        }
                        return Value::Null;
                    }
                }

                self.error("Can only index arrays, dicts, or instances!")
            }

            Expr::Ternary(cond, then_expr, else_expr) => {
                let cond_val = self.evaluate_expression(cond);
                if self.is_truthy(&cond_val) {
                    self.evaluate_expression(then_expr)
                } else {
                    self.evaluate_expression(else_expr)
                }
            }

            Expr::Lambda(params, body) => {
                let func_def = FunctionDef { params: params.clone(), body: body.clone() };
                Value::Function(Rc::new(func_def))
            }

            Expr::BinOp(left, op, right) => {
                if *op == Token::And {
                    let left_val = self.evaluate_expression(left);
                    if !self.is_truthy(&left_val) { return Value::Boolean(false); }
                    let right_val = self.evaluate_expression(right);
                    return Value::Boolean(self.is_truthy(&right_val));
                }
                if *op == Token::Or {
                    let left_val = self.evaluate_expression(left);
                    if self.is_truthy(&left_val) { return Value::Boolean(true); }
                    let right_val = self.evaluate_expression(right);
                    return Value::Boolean(self.is_truthy(&right_val));
                }

                let left_val = self.evaluate_expression(left);
                let right_val = self.evaluate_expression(right);

                match op {
                    Token::Plus => {
                        if let (Value::Number(l), Value::Number(r)) = (&left_val, &right_val) { Value::Number(l + r) }
                        else if let (Value::String(l), Value::String(r)) = (&left_val, &right_val) { Value::String(l.clone() + r) }
                        else { self.error("Can only add numbers or strings!"); }
                    }
                    Token::Minus => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Number(l - r) }
                        else { self.error("Math operations require numbers!"); }
                    }
                    Token::Star => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Number(l * r) }
                        else { self.error("Math operations require numbers!"); }
                    }
                    Token::Slash => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Number(l / r) }
                        else { self.error("Math operations require numbers!"); }
                    }
                    Token::Percent => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Number(l % r) }
                        else { self.error("Math operations require numbers!"); }
                    }
                    Token::Greater => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Boolean(l > r) }
                        else { self.error("Comparisons require numbers!"); }
                    }
                    Token::GreaterEq => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Boolean(l >= r) }
                        else { self.error("Comparisons require numbers!"); }
                    }
                    Token::Less => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Boolean(l < r) }
                        else { self.error("Comparisons require numbers!"); }
                    }
                    Token::LessEq => {
                        if let (Value::Number(l), Value::Number(r)) = (left_val, right_val) { Value::Boolean(l <= r) }
                        else { self.error("Comparisons require numbers!"); }
                    }
                    Token::Is | Token::EqEq => Value::Boolean(left_val == right_val),
                    Token::NotEq => Value::Boolean(left_val != right_val),
                    _ => self.error(&format!("Unsupported operator: {:?}", op)),
                }
            }

            Expr::Call(callee, args) => {

                // ============================================================
                // OOP: Method call on instance — obj.method(args)
                // ============================================================
                if let Expr::Index(obj_expr, key_expr) = callee.as_ref() {
                    if let Expr::String(method_name) = key_expr.as_ref() {
                        let obj_val = self.evaluate_expression(obj_expr);
                        // C-FFI Call: native_module.function_name(args)
                        if let Expr::Index(obj_expr, key_expr) = callee.as_ref() {
                            let obj_val = self.evaluate_expression(obj_expr);
                            if let Value::NativeModule(module) = &obj_val {
                                if let Expr::String(func_name) = key_expr.as_ref() {
                                    let ffi_args: Vec<Value> = args.iter().map(|a| self.evaluate_expression(a)).collect();

                                    // Heuristic: If function name ends with _ptr, treat return as a pointer
                                    let returns_ptr = func_name.ends_with("_ptr");

                                    let result = unsafe {
                                        // 0 args
                                        if ffi_args.is_empty() {
                                            if returns_ptr {
                                                let func: Symbol<extern "C" fn() -> *mut std::ffi::c_void> =
                                                    module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                Value::Pointer(func() as usize)
                                            } else {
                                                let func: Symbol<extern "C" fn() -> f64> =
                                                    module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                Value::Number(func())
                                            }
                                        }
                                        // 1 arg
                                        else if ffi_args.len() == 1 {
                                            match &ffi_args[0] {
                                                Value::Number(n) => {
                                                    let func: Symbol<extern "C" fn(f64) -> f64> =
                                                        module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                    Value::Number(func(*n))
                                                }
                                                Value::String(s) => {
                                                    let func: Symbol<extern "C" fn(*const std::os::raw::c_char) -> *const std::os::raw::c_char> =
                                                        module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                    let c_arg = std::ffi::CString::new(s.as_str()).unwrap();
                                                    let ptr = func(c_arg.as_ptr());
                                                    if ptr.is_null() { Value::Null } else { Value::String(std::ffi::CStr::from_ptr(ptr).to_string_lossy().to_string()) }
                                                }
                                                // Pointer -> Null (e.g., free(ptr))
                                                Value::Pointer(p) => {
                                                    let func: Symbol<extern "C" fn(*mut std::ffi::c_void)> =
                                                        module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                    func(*p as *mut std::ffi::c_void);
                                                    Value::Null
                                                }
                                                _ => self.error("FFI 1-arg: Unsupported type")
                                            }
                                        }
                                        // 2 args
                                        else if ffi_args.len() == 2 {
                                            match (&ffi_args[0], &ffi_args[1]) {
                                                (Value::Number(a), Value::Number(b)) => {
                                                    if returns_ptr {
                                                        let func: Symbol<extern "C" fn(f64, f64) -> *mut std::ffi::c_void> =
                                                            module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                        Value::Pointer(func(*a, *b) as usize)
                                                    } else {
                                                        let func: Symbol<extern "C" fn(f64, f64) -> f64> =
                                                            module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                        Value::Number(func(*a, *b))
                                                    }
                                                }
                                                // Pointer, Number -> Number
                                                (Value::Pointer(p), Value::Number(n)) => {
                                                    let func: Symbol<extern "C" fn(*mut std::ffi::c_void, f64) -> f64> =
                                                        module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                    Value::Number(func(*p as *mut std::ffi::c_void, *n))
                                                }
                                                _ => self.error("FFI 2-arg: Unsupported types")
                                            }
                                        }
                                        // 3 args
                                        else if ffi_args.len() == 3 {
                                            match (&ffi_args[0], &ffi_args[1], &ffi_args[2]) {
                                                (Value::Pointer(p), Value::Number(a), Value::Number(b)) => {
                                                    // Heuristic: get_ functions return a Number, set_/free_ return Null
                                                    if func_name.starts_with("zera_get_") {
                                                        let func: Symbol<extern "C" fn(*mut std::ffi::c_void, f64, f64) -> f64> =
                                                            module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                        Value::Number(func(*p as *mut std::ffi::c_void, *a, *b))
                                                    } else {
                                                        let func: Symbol<extern "C" fn(*mut std::ffi::c_void, f64, f64)> =
                                                            module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                        func(*p as *mut std::ffi::c_void, *a, *b);
                                                        Value::Null
                                                    }
                                                }
                                                _ => self.error("FFI 3-arg: Unsupported types")
                                            }
                                        }
                                        // 4 args
                                        else if ffi_args.len() == 4 {
                                            match (&ffi_args[0], &ffi_args[1], &ffi_args[2], &ffi_args[3]) {
                                                // Pointer, Number, Number, Number -> Null (e.g., set_pixel with color)
                                                (Value::Pointer(p), Value::Number(a), Value::Number(b), Value::Number(c)) => {
                                                    let func: Symbol<extern "C" fn(*mut std::ffi::c_void, f64, f64, f64)> =
                                                        module.lib.get(func_name.as_bytes()).unwrap_or_else(|e| self.error(&format!("Function '{}' not found: {}", func_name, e)));
                                                    func(*p as *mut std::ffi::c_void, *a, *b, *c);
                                                    Value::Null
                                                }
                                                _ => self.error("FFI 4-arg: Unsupported types")
                                            }
                                        }
                                        else {
                                            self.error("FFI currently supports 0 to 4 arguments maximum");
                                        }
                                    };
                                    return result;
                                }
                            }
                        }
                        if let Value::Instance(inst) = &obj_val {
                            // Clone the Rc<ClassDef> so we don't hold a borrow on the RefCell!
                            let class_def = inst.borrow().class.clone();

                            // Try method lookup in class hierarchy
                            if let Some(func_def) = lookup_method(&class_def, method_name) {
                                let arg_vals: Vec<Value> = args.iter()
                                    .map(|a| self.evaluate_expression(a))
                                    .collect();
                                let mut func_env = Environment {
                                    variables: self.variables.clone(),
                                    imported_files: self.imported_files.clone(),
                                };
                                // Bind self
                                func_env.variables.insert("self".to_string(), Value::Instance(inst.clone()));
                                for (i, param) in func_def.params.iter().enumerate() {
                                    let val = arg_vals.get(i).unwrap_or(&Value::Null).clone();
                                    func_env.variables.insert(param.clone(), val);
                                }
                                return match func_env.execute_block(&func_def.body) {
                                    BlockResult::Return(v) => v,
                                    _ => Value::Null,
                                };
                            }

                            // Try field that contains a function (lambda)
                            let field_val = inst.borrow().fields.get(method_name).cloned();
                            if let Some(Value::Function(func_def)) = field_val {
                                let arg_vals: Vec<Value> = args.iter()
                                    .map(|a| self.evaluate_expression(a))
                                    .collect();
                                let mut func_env = Environment {
                                    variables: self.variables.clone(),
                                    imported_files: self.imported_files.clone(),
                                };
                                for (i, param) in func_def.params.iter().enumerate() {
                                    let val = arg_vals.get(i).unwrap_or(&Value::Null).clone();
                                    func_env.variables.insert(param.clone(), val);
                                }
                                return match func_env.execute_block(&func_def.body) {
                                    BlockResult::Return(v) => v,
                                    _ => Value::Null,
                                };
                            }
                            self.error(&format!("No method or field '{}' on instance", method_name));
                        }
                    }
                }

                // Evaluate callee for built-in / function / class calls
                let func_val = self.evaluate_expression(callee);
                let arg_vals: Vec<Value> = args.iter().map(|a| self.evaluate_expression(a)).collect();

                // Built-in functions
                if let Expr::Ident(name) = callee.as_ref() {
                    match name.as_str() {
                        "ask" => {
                            let prompt = match arg_vals.get(0) { Some(Value::String(s)) => s.clone(), _ => String::new() };
                            print!("{}", prompt); io::stdout().flush().unwrap();
                            let mut input = String::new(); io::stdin().read_line(&mut input).unwrap();
                            return Value::String(input.trim().to_string());
                        }
                        "input" => {
                            let prompt = match arg_vals.get(0) { Some(Value::String(s)) => s.clone(), _ => String::new() };
                            print!("{}", prompt); io::stdout().flush().unwrap();
                            let mut input = String::new(); io::stdin().read_line(&mut input).unwrap();
                            return Value::String(input.trim().to_string());
                        }
                        "print" => {
                            if let Some(v) = arg_vals.get(0) { print!("{}", v); io::stdout().flush().unwrap(); }
                            return Value::Null;
                        }
                        "time" => {
                            let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                            return Value::Number(duration.as_secs_f64());
                        }
                        "number" => {
                            if let Some(val) = arg_vals.get(0) {
                                match val {
                                    Value::String(s) => { if let Ok(n) = s.parse::<f64>() { return Value::Number(n); } return Value::Null; }
                                    Value::Boolean(b) => return Value::Number(if *b { 1.0 } else { 0.0 }),
                                    Value::Number(n) => return Value::Number(*n),
                                    _ => self.error("number() requires a string, bool, or number"),
                                }
                            }
                            self.error("number() requires an argument");
                        }
                        "str" => {
                            if let Some(v) = arg_vals.get(0) { return Value::String(v.to_string()); }
                            self.error("str() requires an argument");
                        }
                        "type" => {
                            if let Some(v) = arg_vals.get(0) {
                                let type_name = match v {
                                    Value::Number(_)  => "number", Value::String(_)  => "string",
                                    Value::Boolean(_) => "boolean", Value::Null       => "null",
                                    Value::Function(_) => "function", Value::Array(_)   => "array",
                                    Value::Dict(_)    => "dict",
                                    Value::Class(_)    => "class",
                                    Value::Instance(_) => "instance",
                                    Value::NativeModule(_) => "module",
                                    Value::Pointer(_) => "pointer", // <-- ADD THIS LINE
                                };
                                return Value::String(type_name.to_string());
                            }
                            self.error("type() requires an argument");
                        }
                        "length" => {
                            if let Some(val) = arg_vals.get(0) {
                                match val {
                                    Value::Array(arr)  => return Value::Number(arr.len() as f64),
                                    Value::String(s)   => return Value::Number(s.chars().count() as f64),
                                    Value::Dict(map)   => return Value::Number(map.len() as f64),
                                    _ => self.error("length() requires an array, string, or dict"),
                                }
                            }
                            self.error("length() requires an argument");
                        }
                        "push" => {
                            if let (Some(Value::Array(arr)), Some(val)) = (arg_vals.get(0), arg_vals.get(1)) {
                                let mut new_arr = (**arr).clone(); new_arr.push(val.clone());
                                return Value::Array(Rc::new(new_arr));
                            }
                            self.error("push(array, value) requires an array and a value");
                        }
                        "pop" => {
                            if let Some(Value::Array(arr)) = arg_vals.get(0) { return arr.last().cloned().unwrap_or(Value::Null); }
                            self.error("pop(array) requires an array");
                        }
                        "random" => {
                            if let Some(Value::Number(max)) = arg_vals.get(0) {
                                let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
                                let mixed = (nanos.wrapping_mul(1103515245).wrapping_add(12345) >> 16) % (*max as u128);
                                return Value::Number(mixed as f64);
                            }
                            self.error("random(max) requires a number argument");
                        }
                        "abs" => { if let Some(Value::Number(n)) = arg_vals.get(0) { return Value::Number(n.abs()); } self.error("abs(n) requires a number argument"); }
                        "floor" => { if let Some(Value::Number(n)) = arg_vals.get(0) { return Value::Number(n.floor()); } self.error("floor(n) requires a number argument"); }
                        "ceil" => { if let Some(Value::Number(n)) = arg_vals.get(0) { return Value::Number(n.ceil()); } self.error("ceil(n) requires a number argument"); }
                        "round" => { if let Some(Value::Number(n)) = arg_vals.get(0) { return Value::Number(n.round()); } self.error("round(n) requires a number argument"); }
                        "sum" => {
                            if let Some(Value::Array(arr)) = arg_vals.get(0) {
                                let mut total = 0.0;
                                for v in arr.iter() { if let Value::Number(n) = v { total += *n; } else { self.error("sum() requires an array of numbers"); } }
                                return Value::Number(total);
                            }
                            self.error("sum(array) requires an array argument");
                        }
                        "min" => {
                            if let Some(Value::Array(arr)) = arg_vals.get(0) {
                                let mut result = f64::INFINITY;
                                for v in arr.iter() { if let Value::Number(n) = v { if *n < result { result = *n; } } else { self.error("min() requires an array of numbers"); } }
                                return Value::Number(result);
                            }
                            self.error("min(array) requires an array argument");
                        }
                        "max" => {
                            if let Some(Value::Array(arr)) = arg_vals.get(0) {
                                let mut result = f64::NEG_INFINITY;
                                for v in arr.iter() { if let Value::Number(n) = v { if *n > result { result = *n; } } else { self.error("max() requires an array of numbers"); } }
                                return Value::Number(result);
                            }
                            self.error("max(array) requires an array argument");
                        }
                        "upper" => { if let Some(Value::String(s)) = arg_vals.get(0) { return Value::String(s.to_uppercase()); } self.error("upper(s) requires a string argument"); }
                        "lower" => { if let Some(Value::String(s)) = arg_vals.get(0) { return Value::String(s.to_lowercase()); } self.error("lower(s) requires a string argument"); }
                        "split" => {
                            if let (Some(Value::String(s)), Some(Value::String(d))) = (arg_vals.get(0), arg_vals.get(1)) {
                                let parts: Vec<Value> = s.split(d).map(|p| Value::String(p.to_string())).collect();
                                return Value::Array(Rc::new(parts));
                            }
                            self.error("split(string, delimiter) requires two strings");
                        }
                        "join" => {
                            if let (Some(Value::Array(arr)), Some(Value::String(d))) = (arg_vals.get(0), arg_vals.get(1)) {
                                let strs: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                                return Value::String(strs.join(d));
                            }
                            self.error("join(array, delimiter) requires an array and a string");
                        }
                        "contains" => {
                            if let (Some(collection), Some(target)) = (arg_vals.get(0), arg_vals.get(1)) {
                                match (collection, target) {
                                    (Value::Array(arr), _) => return Value::Boolean(arr.iter().any(|v| v == target)),
                                    (Value::String(haystack), Value::String(needle)) => return Value::Boolean(haystack.contains(needle)),
                                    (Value::Dict(map), Value::String(key)) => return Value::Boolean(map.contains_key(key)),
                                    _ => self.error("contains() requires array+value, string+string, or dict+string"),
                                }
                            }
                            self.error("contains() requires two arguments");
                        }
                        "keys" => {
                            if let Some(Value::Dict(map)) = arg_vals.get(0) {
                                let keys: Vec<Value> = map.keys().map(|k| Value::String(k.clone())).collect();
                                return Value::Array(Rc::new(keys));
                            }
                            self.error("keys(dict) requires a dict argument");
                        }
                        "values" => {
                            if let Some(Value::Dict(map)) = arg_vals.get(0) {
                                let vals: Vec<Value> = map.values().cloned().collect();
                                return Value::Array(Rc::new(vals));
                            }
                            self.error("values(dict) requires a dict argument");
                        }
                        "range" => {
                            match arg_vals.len() {
                                1 => {
                                    if let Some(Value::Number(n)) = arg_vals.get(0) {
                                        let vals: Vec<Value> = (0..(*n as i64)).map(|i| Value::Number(i as f64)).collect();
                                        return Value::Array(Rc::new(vals));
                                    }
                                }
                                2 => {
                                    if let (Some(Value::Number(s)), Some(Value::Number(e))) = (arg_vals.get(0), arg_vals.get(1)) {
                                        let vals: Vec<Value> = ((*s as i64)..(*e as i64)).map(|i| Value::Number(i as f64)).collect();
                                        return Value::Array(Rc::new(vals));
                                    }
                                }
                                _ => {}
                            }
                            self.error("range(n) or range(start, end) requires number argument(s)");
                        }
                        "load_library" => {
                            if let Some(Value::String(path)) = arg_vals.get(0) {
                                let lib = unsafe {
                                    Library::new(path).unwrap_or_else(|e| self.error(&format!("Failed to load library '{}': {}", path, e)))
                                };
                                return Value::NativeModule(Rc::new(NativeModule {
                                    name: path.clone(),
                                    lib,
                                }));
                            }
                            self.error("load_library(path) requires a string path to a .dll/.so/.dylib");
                        }
                        "sleep" => {
                            if let Some(Value::Number(ms)) = arg_vals.get(0) {
                                std::thread::sleep(std::time::Duration::from_millis(*ms as u64));
                                return Value::Null;
                            }
                            self.error("sleep(ms) requires a number argument");
                        }
                        _ => {}
                    }
                }

                // ============================================================
                // OOP: Class instantiation — ClassName(args)
                // ============================================================
                if let Value::Class(class_def) = &func_val {
                    // Initialize all fields (including inherited) to Null
                    let mut inst_fields = HashMap::new();
                    let mut current = Some(class_def.clone());
                    while let Some(c) = current {
                        for fname in &c.fields {
                            inst_fields.entry(fname.clone()).or_insert(Value::Null);
                        }
                        current = c.superclass.clone();
                    }

                    let inst = Rc::new(RefCell::new(InstanceData {
                        class: class_def.clone(),
                        fields: inst_fields,
                    }));

                    // Run constructor if present
                    if let Some((params, body)) = &class_def.constructor {
                        let mut func_env = Environment {
                            variables: self.variables.clone(),
                            imported_files: self.imported_files.clone(),
                        };
                        func_env.variables.insert("self".to_string(), Value::Instance(inst.clone()));
                        for (i, param) in params.iter().enumerate() {
                            let val = arg_vals.get(i).unwrap_or(&Value::Null).clone();
                            func_env.variables.insert(param.clone(), val);
                        }
                        func_env.execute_block(body);
                    }

                    return Value::Instance(inst);
                }

                // Regular function call
                if let Value::Function(func_def) = func_val {
                    let mut func_env = Environment {
                        variables: self.variables.clone(),
                        imported_files: self.imported_files.clone(),
                    };
                    for (i, param) in func_def.params.iter().enumerate() {
                        let val = arg_vals.get(i).unwrap_or(&Value::Null).clone();
                        func_env.variables.insert(param.clone(), val);
                    }
                    match func_env.execute_block(&func_def.body) {
                        BlockResult::Return(v) => v,
                        _ => Value::Null,
                    }
                } else {
                    self.error("Tried to call something that is not a function or class!");
                }
            }
        }
    }
}

// ====================================================================
// MAIN
// ====================================================================
fn run_source(source_code: &str) {
    let ast = lex_and_parse(source_code);
    let mut env = Environment::new();
    env.execute_block(&ast);
}

fn print_usage(prog: &str) {
    eprintln!("Zeralang Interpreter v0.3 (with OOP)");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {}                            Start REPL (interactive mode)", prog);
    eprintln!("  {} <file.zera>               Run a .zera file", prog);
    eprintln!("  {} --test                    Run built-in test", prog);
    eprintln!("  {} --convert-read <file>     Convert to read mode", prog);
    eprintln!("  {} --convert-write <file>    Convert to write mode", prog);
    eprintln!("  {} --roundtrip <file>        Verify round-trip", prog);
    eprintln!("  {} --repl                    Start REPL", prog);
    eprintln!("  {} --help                    Show this help", prog);
}

fn run_test() {
    let source_code = r#"
show "=== OOP FEATURE TEST ==="

// --- Basic class with constructor and methods ---
class Animal {
    field name
    field sound

    construct(name, sound) {
        self.name = name
        self.sound = sound
    }

    func describe() {
        return self.name + " says " + self.sound
    }

    func loud_describe() {
        return upper(self.describe())
    }
}

// --- Inheritance ---
class Dog extends Animal {
    field breed

    construct(name, breed) {
        self.name = name
        self.sound = "Woof"
        self.breed = breed
    }

    func fetch() {
        return self.name + " the " + self.breed + " fetches the ball!"
    }
}

show "--- Creating Animal ---"
a = Animal("Cow", "Moo")
show a.describe()
show a.loud_describe()

show "--- Creating Dog (inherits from Animal) ---"
d = Dog("Rex", "Labrador")
show d.describe()
show d.fetch()

show "--- Field Access ---"
show d.name
show d.breed

show "--- Field Mutation ---"
d.name = "Buddy"
show d.describe()

show "--- Method Chaining ---"
class Calculator {
    field result

    construct() {
        self.result = 0
    }

    func add(n) {
        self.result = self.result + n
        return self
    }

    func multiply(n) {
        self.result = self.result * n
        return self
    }

    func get() {
        return self.result
    }
}

c = Calculator()
c.add(5)
c.multiply(3)
show c.get()

show "--- Instance as Dict-like ---"
show type(a)
show type(d)
show type(Calculator)

show "=== ALL OOP TESTS PASSED ==="
    "#;
    run_source(source_code);
}

pub fn lex_and_parse(source: &str) -> Vec<Stmt> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    let mut lines = Vec::new();
    let mut cols = Vec::new();
    loop {
        let token = lexer.next_token();
        let (line, col) = lexer.last_pos();
        if token == Token::EOF { break; }
        tokens.push(token);
        lines.push(line);
        cols.push(col);
    }
    let mut parser = Parser::new(tokens, lines, cols);
    parser.parse_program()
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<&str>()
            .map(|s| *s)
            .or_else(|| info.payload().downcast_ref::<String>()
                .map(|s| s.as_str()))
            .unwrap_or("Unknown error");

        if msg.starts_with("Error at line") {
            eprintln!("{}", msg);
        } else {
            eprintln!("Zeralang Error: {}", msg);
        }
    }));

    let args: Vec<String> = std::env::args().collect();
    let prog = args.get(0).map(|s| s.as_str()).unwrap_or("zera");

    match args.len() {
        1 => { repl::run(); }
        _ => {
            let arg = &args[1];
            match arg.as_str() {
                "--repl" => repl::run(),
                "--help" | "-h" => print_usage(prog),
                "--test" => run_test(),
                "--owner" => {
                    println!("==========================================");
                    println!(" Zeralang Interpreter");
                    println!(" Author: Anant");
                    println!(" Compiled: 26/8/2026");
                    println!(" Built with Rust 🦀");
                    println!("==========================================");
                }
                "--vm" => {
                    if args.len() < 3 { eprintln!("Usage: {} --vm <file.zera>", prog); std::process::exit(1); }
                    let source = std::fs::read_to_string(&args[2]).unwrap_or_else(|e| { eprintln!("Error: Cannot read file '{}': {}", args[2], e); std::process::exit(1); });
                    let ast = lex_and_parse(&source);
                    vm::execute_bytecode(&ast);
                }
                "--convert-read" => {
                    if args.len() < 3 { eprintln!("Usage: {} --convert-read <file.zera>", prog); std::process::exit(1); }
                    let source = std::fs::read_to_string(&args[2]).unwrap_or_else(|e| { eprintln!("Error: Cannot read file '{}': {}", args[2], e); std::process::exit(1); });
                    let ast = lex_and_parse(&source);
                    print!("{}", converter::emit_read(&ast));
                }
                "--convert-write" => {
                    if args.len() < 3 { eprintln!("Usage: {} --convert-write <file.zera>", prog); std::process::exit(1); }
                    let source = std::fs::read_to_string(&args[2]).unwrap_or_else(|e| { eprintln!("Error: Cannot read file '{}': {}", args[2], e); std::process::exit(1); });
                    let ast = lex_and_parse(&source);
                    print!("{}", converter::emit_write(&ast));
                }
                "--roundtrip" => {
                    if args.len() < 3 { eprintln!("Usage: {} --roundtrip <file.zera>", prog); std::process::exit(1); }
                    let source = std::fs::read_to_string(&args[2]).unwrap_or_else(|e| { eprintln!("Error: Cannot read file '{}': {}", args[2], e); std::process::exit(1); });

                    let ast1 = lex_and_parse(&source);
                    let source_read = converter::emit_read(&ast1);
                    let ast2 = lex_and_parse(&source_read);
                    let source_write = converter::emit_write(&ast2);
                    let ast3 = lex_and_parse(&source_write);

                    if ast1 == ast2 && ast1 == ast3 {
                        eprintln!("✓ Round-trip OK! All three ASTs match.");
                    } else {
                        eprintln!("✗ Round-trip FAILED!");
                        std::process::exit(1);
                    }
                }
                _ => {
                    let filename = arg;
                    let source_code = match std::fs::read_to_string(filename) {
                        Ok(content) => content,
                        Err(e) => {
                            eprintln!("Error: Cannot read file '{}': {}", filename, e);
                            std::process::exit(1);
                        }
                    };
                    run_source(&source_code);
                }
            }
        }
    }
}