// src/vm.rs
// Zeralang Bytecode Virtual Machine - Phase 2 (Builtins & Control Flow)

use crate::{Expr, Stmt, Token, Value};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

// ==========================================
// 1. OPCODES
// ==========================================
#[derive(Debug, Clone, PartialEq)]
pub enum OpCode {
    Constant(usize),
    Pop,
    Add, Sub, Mul, Div, Mod,
    Negate, Not,
    GetLocal(usize),
    SetLocal(usize),
    Print,
    Return,
    // NEW: Control Flow
    Jump(usize),
    JumpIfFalse(usize),
    // NEW: Function Calls
    BuiltinCall(String, usize), // name, arg_count
    // NEW: Comparisons
    Greater, Less, GreaterEq, LessEq, Equal, NotEqual,
}

// ==========================================
// 2. CHUNK
// ==========================================
#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
}

impl Chunk {
    pub fn new() -> Chunk { Chunk { code: Vec::new(), constants: Vec::new() } }
    pub fn write(&mut self, op: OpCode) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }
    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.push(value);
        self.constants.len() - 1
    }
    pub fn patch_jump(&mut self, offset: usize, target: usize) {
        let op = match &self.code[offset] {
            OpCode::Jump(_) => OpCode::Jump(target),
            OpCode::JumpIfFalse(_) => OpCode::JumpIfFalse(target),
            _ => panic!("Cannot patch non-jump opcode"),
        };
        self.code[offset] = op;
    }
}

// ==========================================
// 3. COMPILER
// ==========================================
pub struct Compiler {
    chunk: Chunk,
    locals: Vec<String>,
}

impl Compiler {
    pub fn new() -> Compiler { Compiler { chunk: Chunk::new(), locals: Vec::new() } }

    pub fn compile(mut self, statements: &[Stmt]) -> Chunk {
        for stmt in statements { self.compile_stmt(stmt); }
        self.chunk.write(OpCode::Return);
        self.chunk
    }

    fn resolve_local(&mut self, name: &str) -> usize {
        for (i, local) in self.locals.iter().enumerate().rev() {
            if local == name { return i; }
        }
        self.locals.push(name.to_string());
        self.locals.len() - 1
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ExprStmt(expr) => {
                self.compile_expr(expr);
                self.chunk.write(OpCode::Pop);
            }
            Stmt::Show(expr) => {
                self.compile_expr(expr);
                self.chunk.write(OpCode::Print);
            }
            Stmt::Set(name, expr) => {
                self.compile_expr(expr);
                let idx = self.resolve_local(name);
                self.chunk.write(OpCode::SetLocal(idx));
            }
            // NEW: If statement compilation
            Stmt::If(cond, then_block, else_block) => {
                self.compile_expr(cond);

                // Jump to else block if false
                let jump_to_else = self.chunk.write(OpCode::JumpIfFalse(0));
                self.chunk.write(OpCode::Pop); // Pop the condition off the stack

                // Compile then block
                for s in then_block { self.compile_stmt(s); }

                // After then block, jump over the else block
                let jump_over_else = self.chunk.write(OpCode::Jump(0));

                // Patch the false jump to land here (start of else)
                let else_start = self.chunk.code.len();
                self.chunk.patch_jump(jump_to_else, else_start);
                self.chunk.write(OpCode::Pop); // Pop condition (in the false path)

                // Compile else block
                for s in else_block { self.compile_stmt(s); }

                // Patch the end-of-then jump to land here
                let end_if = self.chunk.code.len();
                self.chunk.patch_jump(jump_over_else, end_if);
            }
            // NEW: While loop compilation
            Stmt::While(cond, body) => {
                let loop_start = self.chunk.code.len();

                self.compile_expr(cond);
                let exit_jump = self.chunk.write(OpCode::JumpIfFalse(0));
                self.chunk.write(OpCode::Pop); // Pop condition

                // Compile loop body
                for s in body { self.compile_stmt(s); }

                // Jump back to the start to re-evaluate condition
                self.chunk.write(OpCode::Jump(loop_start));

                // Patch the exit jump
                let loop_end = self.chunk.code.len();
                self.chunk.patch_jump(exit_jump, loop_end);
                self.chunk.write(OpCode::Pop); // Pop condition on exit
            }
            // We ignore things we can't compile yet, but we must not break the stack
            Stmt::Function(_, _, _) => { /* TODO: Function definitions in VM */ }
            _ => {}
        }
    }

    fn compile_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(n) => { let idx = self.chunk.add_constant(Value::Number(*n)); self.chunk.write(OpCode::Constant(idx)); }
            Expr::String(s) => { let idx = self.chunk.add_constant(Value::String(s.clone())); self.chunk.write(OpCode::Constant(idx)); }
            Expr::Boolean(b) => { let idx = self.chunk.add_constant(Value::Boolean(*b)); self.chunk.write(OpCode::Constant(idx)); }
            Expr::Null => { let idx = self.chunk.add_constant(Value::Null); self.chunk.write(OpCode::Constant(idx)); }

            Expr::Ident(name) => {
                let idx = self.resolve_local(name);
                self.chunk.write(OpCode::GetLocal(idx));
            }

            Expr::BinOp(left, op, right) => {
                self.compile_expr(left);
                self.compile_expr(right);
                match op {
                    Token::Plus => self.chunk.write(OpCode::Add),
                    Token::Minus => self.chunk.write(OpCode::Sub),
                    Token::Star => self.chunk.write(OpCode::Mul),
                    Token::Slash => self.chunk.write(OpCode::Div),
                    Token::Percent => self.chunk.write(OpCode::Mod),
                    // NEW: Comparisons
                    Token::Greater => self.chunk.write(OpCode::Greater),
                    Token::Less => self.chunk.write(OpCode::Less),
                    Token::GreaterEq => self.chunk.write(OpCode::GreaterEq),
                    Token::LessEq => self.chunk.write(OpCode::LessEq),
                    Token::EqEq | Token::Is => self.chunk.write(OpCode::Equal),
                    Token::NotEq => self.chunk.write(OpCode::NotEqual),
                    _ => self.chunk.write(OpCode::Pop),
                };
            }

            Expr::Unary(op, expr) => {
                self.compile_expr(expr);
                match op {
                    Token::Minus => self.chunk.write(OpCode::Negate),
                    Token::Not => self.chunk.write(OpCode::Not),
                    _ => self.chunk.write(OpCode::Pop),
                };
            }

            // NEW: Function Calls (Only supports built-ins for now)
            Expr::Call(callee, args) => {
                if let Expr::Ident(name) = callee.as_ref() {
                    // Compile arguments left-to-right
                    for arg in args { self.compile_expr(arg); }

                    // Emit BuiltinCall opcode
                    self.chunk.write(OpCode::BuiltinCall(name.clone(), args.len()));
                } else {
                    // If it's not a direct builtin name, push Null for now
                    let idx = self.chunk.add_constant(Value::Null);
                    self.chunk.write(OpCode::Constant(idx));
                }
            }

            _ => {
                let idx = self.chunk.add_constant(Value::Null);
                self.chunk.write(OpCode::Constant(idx));
            }
        }
    }
}

// ==========================================
// 4. VIRTUAL MACHINE
// ==========================================
pub struct VM {
    chunk: Chunk,
    ip: usize,
    stack: Vec<Value>,
}

impl VM {
    pub fn new(chunk: Chunk) -> VM { VM { chunk, ip: 0, stack: Vec::new() } }

    pub fn run(mut self) {
        while self.ip < self.chunk.code.len() {
            let op = self.chunk.code[self.ip].clone();
            self.ip += 1;

            match op {
                OpCode::Constant(idx) => self.stack.push(self.chunk.constants[idx].clone()),
                OpCode::Pop => { self.stack.pop(); }

                // Math & Logic
                OpCode::Add => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); self.stack.push(self.add(a, b)); }
                OpCode::Sub => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Number(n1 - n2)); } else { panic!("VM Error: Math requires numbers"); } }
                OpCode::Mul => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Number(n1 * n2)); } else { panic!("VM Error: Math requires numbers"); } }
                OpCode::Div => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Number(n1 / n2)); } else { panic!("VM Error: Math requires numbers"); } }
                OpCode::Mod => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Number(n1 % n2)); } else { panic!("VM Error: Math requires numbers"); } }
                OpCode::Negate => { let a = self.stack.pop().unwrap(); if let Value::Number(n) = a { self.stack.push(Value::Number(-n)); } else { panic!("VM Error: Cannot negate non-number"); } }
                OpCode::Not => { let a = self.stack.pop().unwrap(); self.stack.push(Value::Boolean(!self.is_truthy(&a))); }

                // Variables
                // Comparisons
                OpCode::Greater => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Boolean(n1 > n2)); } else { panic!("VM Error: > requires numbers"); } }
                OpCode::Less => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Boolean(n1 < n2)); } else { panic!("VM Error: < requires numbers"); } }
                OpCode::GreaterEq => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Boolean(n1 >= n2)); } else { panic!("VM Error: >= requires numbers"); } }
                OpCode::LessEq => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { self.stack.push(Value::Boolean(n1 <= n2)); } else { panic!("VM Error: <= requires numbers"); } }
                OpCode::Equal => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); self.stack.push(Value::Boolean(a == b)); }
                OpCode::NotEqual => { let b = self.stack.pop().unwrap(); let a = self.stack.pop().unwrap(); self.stack.push(Value::Boolean(a != b)); }
                OpCode::GetLocal(idx) => self.stack.push(self.stack[idx].clone()),
                OpCode::SetLocal(idx) => {
                    let val = self.stack.pop().unwrap();
                    while self.stack.len() <= idx { self.stack.push(Value::Null); }
                    self.stack[idx] = val;
                }

                // I/O
                OpCode::Print => { let val = self.stack.pop().unwrap(); println!("{}", val); }

                // Control Flow
                OpCode::Jump(target) => { self.ip = target; }
                OpCode::JumpIfFalse(target) => {
                    let cond = self.stack.last().unwrap().clone();
                    if !self.is_truthy(&cond) { self.ip = target; }
                }

                // NEW: Built-in calls
                OpCode::BuiltinCall(name, arg_count) => {
                    let mut args = Vec::new();
                    for _ in 0..arg_count { args.push(self.stack.pop().unwrap()); }
                    args.reverse(); // They were pushed left-to-right, so popped right-to-left

                    let result = self.call_builtin(&name, &args);
                    self.stack.push(result);
                }

                OpCode::Return => break,
            }
        }
    }

    fn add(&self, a: Value, b: Value) -> Value {
        if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) { return Value::Number(n1 + n2); }
        if let (Value::String(s1), Value::String(s2)) = (&a, &b) { return Value::String(s1.clone() + s2); }
        panic!("VM Error: Cannot add {:?} and {:?}", a, b);
    }

    fn is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::Boolean(b) => *b, Value::Number(n) => *n != 0.0, Value::String(s) => !s.is_empty(),
            Value::Null => false, _ => true,
        }
    }

    // Re-using the built-in logic from the AST evaluator
    fn call_builtin(&self, name: &str, args: &[Value]) -> Value {
        match name {
            "str" => Value::String(args.get(0).unwrap_or(&Value::Null).to_string()),
            "number" => {
                if let Some(val) = args.get(0) {
                    match val {
                        Value::String(s) => { if let Ok(n) = s.parse::<f64>() { return Value::Number(n); } return Value::Null; }
                        Value::Boolean(b) => return Value::Number(if *b { 1.0 } else { 0.0 }),
                        Value::Number(n) => return Value::Number(*n),
                        _ => panic!("VM Error: number() requires string/bool/number"),
                    }
                }
                panic!("VM Error: number() requires argument");
            }
            "ask" | "input" => {
                let prompt = match args.get(0) { Some(Value::String(s)) => s.clone(), _ => String::new() };
                print!("{}", prompt); io::stdout().flush().unwrap();
                let mut input = String::new(); io::stdin().read_line(&mut input).unwrap();
                Value::String(input.trim().to_string())
            }
            "print" => {
                if let Some(v) = args.get(0) { print!("{}", v); io::stdout().flush().unwrap(); }
                Value::Null
            }
            "time" => { let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap(); Value::Number(duration.as_secs_f64()) }
            _ => panic!("VM Error: Unknown built-in function '{}'", name),
        }
    }
}

pub fn execute_bytecode(statements: &[Stmt]) {
    let compiler = Compiler::new();
    let chunk = compiler.compile(statements);
    let vm = VM::new(chunk);
    vm.run();
}