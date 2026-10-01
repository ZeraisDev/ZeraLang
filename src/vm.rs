
use crate::{Expr, Stmt, Token, Value, NativeModule};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use libloading::Library;
use std::cell::RefCell;
#[derive(Debug, Clone)]
pub struct VmClass {
    pub name: String,
    pub fields: Vec<String>,
    pub constructor: Option<Rc<Chunk>>,
    pub constructor_params: Vec<String>,
    pub methods: HashMap<String, Rc<Chunk>>,
    pub method_params: HashMap<String, Vec<String>>,
    pub superclass: Option<Rc<VmClass>>,
    pub superclass_name: Option<String>,

}

#[derive(Debug, Clone)]
pub struct VmInstance {
    pub class: Rc<VmClass>,
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpCode {
    Constant(usize),
    Pop,
    Add, Sub, Mul, Div, Mod,
    Negate, Not,
    GetLocal(usize),
    SetLocal(usize),
    GetGlobal(Rc<str>),
    SetGlobal(Rc<str>),
    Print,
    Return,

    Jump(usize),
    JumpIfFalse(usize),

    BuiltinCall(Rc<str>, usize),
    Call(usize),
    CallMethod(Rc<str>, usize),

    Greater, Less, GreaterEq, LessEq, Equal, NotEqual,
    BuildArray(usize),
    BuildDict(usize),
    IndexGet,
    IndexSet,
    Try(usize),
    PopTry,
    Throw,
}
struct TryFrame {
    catch_ip: usize,
    stack_len: usize,
    locals_len: usize,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
    pub param_count: usize,
}

impl Chunk {
    pub fn new() -> Chunk {
        Chunk {
            code: Vec::new(),
            constants: Vec::new(),
            param_count: 0,
        }
    }

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
            OpCode::Try(_) => OpCode::Try(target),
            _ => panic!("Cannot patch non-jump opcode"),
        };
        self.code[offset] = op;
    }
}

pub struct Compiler {
    chunk: Chunk,
    locals: Vec<String>,
    depth: usize,
    loop_stack: Vec<(usize, Vec<usize>)>,
    imported_files: Rc<RefCell<HashSet<String>>>,
}

#[inline]
fn intern(s: &str) -> Rc<str> {
    Rc::from(s)
}

fn collect_free_idents_block(stmts: &[Stmt], params: &HashSet<String>, free: &mut HashSet<String>) {
    let mut bound = params.clone();
    for stmt in stmts {
        collect_free_idents_stmt(stmt, &mut bound, free);
    }
}

fn collect_free_idents_stmt(stmt: &Stmt, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    match stmt {
        Stmt::Set(name, expr) => {
            collect_free_idents_expr(expr, bound, free);
            bound.insert(name.clone());
        }
        Stmt::SetField(target, _field, value) => {
            collect_free_idents_expr(target, bound, free);
            collect_free_idents_expr(value, bound, free);
        }
        Stmt::Show(e) | Stmt::Return(e) | Stmt::Throw(e) | Stmt::ExprStmt(e) => {
            collect_free_idents_expr(e, bound, free);
        }
        Stmt::If(cond, then_block, else_block) => {
            collect_free_idents_expr(cond, bound, free);
            let mut b1 = bound.clone();
            for s in then_block { collect_free_idents_stmt(s, &mut b1, free); }
            let mut b2 = bound.clone();
            for s in else_block { collect_free_idents_stmt(s, &mut b2, free); }
        }
        Stmt::While(cond, body) => {
            collect_free_idents_expr(cond, bound, free);
            let mut b = bound.clone();
            for s in body { collect_free_idents_stmt(s, &mut b, free); }
        }
        Stmt::ForEach(var, iterable, body) => {
            collect_free_idents_expr(iterable, bound, free);
            let mut b = bound.clone();
            b.insert(var.clone());
            for s in body { collect_free_idents_stmt(s, &mut b, free); }
        }
        Stmt::Try(try_block, catch_var, catch_block) => {
            let mut b1 = bound.clone();
            for s in try_block { collect_free_idents_stmt(s, &mut b1, free); }
            let mut b2 = bound.clone();
            b2.insert(catch_var.clone());
            for s in catch_block { collect_free_idents_stmt(s, &mut b2, free); }
        }

        Stmt::Function(..) | Stmt::Class(..) => {}
        Stmt::Break | Stmt::Continue | Stmt::Import(_) => {}
    }
}

fn collect_free_idents_expr(expr: &Expr, bound: &HashSet<String>, free: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name) => {
            if !bound.contains(name) {
                free.insert(name.clone());
            }
        }
        Expr::This | Expr::Number(_) | Expr::String(_) | Expr::Boolean(_) | Expr::Null => {}
        Expr::BinOp(l, _, r) => {
            collect_free_idents_expr(l, bound, free);
            collect_free_idents_expr(r, bound, free);
        }
        Expr::Unary(_, e) => collect_free_idents_expr(e, bound, free),
        Expr::Call(callee, args) => {
            collect_free_idents_expr(callee, bound, free);
            for a in args { collect_free_idents_expr(a, bound, free); }
        }
        Expr::Array(elements) => {
            for e in elements { collect_free_idents_expr(e, bound, free); }
        }
        Expr::Index(obj, idx) => {
            collect_free_idents_expr(obj, bound, free);
            collect_free_idents_expr(idx, bound, free);
        }
        Expr::Dict(pairs) => {
            for (k, v) in pairs {
                collect_free_idents_expr(k, bound, free);
                collect_free_idents_expr(v, bound, free);
            }
        }
        Expr::Ternary(c, t, e) => {
            collect_free_idents_expr(c, bound, free);
            collect_free_idents_expr(t, bound, free);
            collect_free_idents_expr(e, bound, free);
        }
        Expr::Lambda(params, body) => {

            let mut b = bound.clone();
            for p in params { b.insert(p.clone()); }
            for s in body { collect_free_idents_stmt(s, &mut b, free); }
        }
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "str" |"type" | "number" | "ask" | "input" | "print" | "time" |
        "read_file" | "write_file" | "append_file" | "exec" |
        "load_library" | "sleep" | "length" | "push" | "pop" |
        "random" | "abs" | "floor" | "ceil" | "round" | "sum" |
        "min" | "max" | "upper" | "lower" | "split" | "join" |
        "contains" | "keys" | "values" | "range" | "json_parse" | "json_dump"
    )
}
impl Compiler {
    pub fn new() -> Compiler {
        Compiler {
            chunk: Chunk::new(),
            locals: Vec::new(),
            depth: 0,
            loop_stack: Vec::new(),
            imported_files: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    fn child(&self, depth: usize) -> Compiler {
        Compiler {
            chunk: Chunk::new(),
            locals: Vec::new(),
            depth,
            loop_stack: Vec::new(),
            imported_files: self.imported_files.clone(),
        }
    }

    pub fn compile(mut self, statements: &[Stmt]) -> Chunk {
        for stmt in statements {
            self.compile_stmt(stmt);
        }
        self.chunk.write(OpCode::Return);
        self.chunk
    }

    fn resolve_local(&mut self, name: &str) -> usize {
        for (i, local) in self.locals.iter().enumerate().rev() {
            if local == name {
                return i;
            }
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
                if self.depth == 0 {
                    self.chunk.write(OpCode::SetGlobal(intern(name)));
                } else {
                    let idx = self.resolve_local(name);
                    self.chunk.write(OpCode::SetLocal(idx));
                }
            }
            Stmt::Try(try_block, catch_var, catch_block) => {

                let try_op_idx = self.chunk.write(OpCode::Try(0));

                for s in try_block { self.compile_stmt(s); }

                self.chunk.write(OpCode::PopTry);
                let jump_over_catch = self.chunk.write(OpCode::Jump(0));

                let catch_start = self.chunk.code.len();
                self.chunk.patch_jump(try_op_idx, catch_start);

                if self.depth == 0 {
                    self.chunk.write(OpCode::SetGlobal(intern(catch_var)));
                } else {
                    let idx = self.resolve_local(catch_var);
                    self.chunk.write(OpCode::SetLocal(idx));
                }

                for s in catch_block { self.compile_stmt(s); }

                let end_try = self.chunk.code.len();
                self.chunk.patch_jump(jump_over_catch, end_try);
            }
            Stmt::Throw(expr) => {
                self.compile_expr(expr);
                self.chunk.write(OpCode::Throw);
            }
            Stmt::ForEach(var_name, iterable, body) => {

                self.compile_expr(iterable);
                let iter_idx = self.resolve_local("__iter__");
                self.chunk.write(OpCode::SetLocal(iter_idx));

                self.chunk.write(OpCode::GetLocal(iter_idx));
                self.chunk.write(OpCode::BuiltinCall(intern("length"), 1));
                let len_idx = self.resolve_local("__len__");
                self.chunk.write(OpCode::SetLocal(len_idx));

                let idx_idx = self.resolve_local("__idx__");
                let zero_idx = self.chunk.add_constant(Value::Number(0.0));
                self.chunk.write(OpCode::Constant(zero_idx));
                self.chunk.write(OpCode::SetLocal(idx_idx));

                let loop_start = self.chunk.code.len();
                self.loop_stack.push((loop_start, Vec::new()));

                self.chunk.write(OpCode::GetLocal(idx_idx));
                self.chunk.write(OpCode::GetLocal(len_idx));
                self.chunk.write(OpCode::Less);

                let exit_jump = self.chunk.write(OpCode::JumpIfFalse(0));
                self.chunk.write(OpCode::Pop);

                let var_idx = self.resolve_local(var_name);
                self.chunk.write(OpCode::GetLocal(iter_idx));
                self.chunk.write(OpCode::GetLocal(idx_idx));
                self.chunk.write(OpCode::IndexGet);
                self.chunk.write(OpCode::SetLocal(var_idx));

                for s in body { self.compile_stmt(s); }

                self.chunk.write(OpCode::GetLocal(idx_idx));
                let one_idx = self.chunk.add_constant(Value::Number(1.0));
                self.chunk.write(OpCode::Constant(one_idx));
                self.chunk.write(OpCode::Add);
                self.chunk.write(OpCode::SetLocal(idx_idx));

                self.chunk.write(OpCode::Jump(loop_start));

                let loop_end = self.chunk.code.len();
                self.chunk.patch_jump(exit_jump, loop_end);
                self.chunk.write(OpCode::Pop);

                let (_, break_jumps) = self.loop_stack.pop().unwrap();
                for break_addr in break_jumps {
                    self.chunk.patch_jump(break_addr, loop_end);
                }
            }
            Stmt::If(cond, then_block, else_block) => {
                self.compile_expr(cond);

                let jump_to_else = self.chunk.write(OpCode::JumpIfFalse(0));
                self.chunk.write(OpCode::Pop);

                for s in then_block {
                    self.compile_stmt(s);
                }

                let jump_over_else = self.chunk.write(OpCode::Jump(0));

                let else_start = self.chunk.code.len();
                self.chunk.patch_jump(jump_to_else, else_start);
                self.chunk.write(OpCode::Pop);

                for s in else_block {
                    self.compile_stmt(s);
                }

                let end_if = self.chunk.code.len();
                self.chunk.patch_jump(jump_over_else, end_if);
            }
            Stmt::While(cond, body) => {
                let loop_start = self.chunk.code.len();

                self.loop_stack.push((loop_start, Vec::new()));

                self.compile_expr(cond);
                let exit_jump = self.chunk.write(OpCode::JumpIfFalse(0));
                self.chunk.write(OpCode::Pop);

                for s in body {
                    self.compile_stmt(s);
                }

                self.chunk.write(OpCode::Jump(loop_start));

                let loop_end = self.chunk.code.len();
                self.chunk.patch_jump(exit_jump, loop_end);
                self.chunk.write(OpCode::Pop);

                let (_, break_jumps) = self.loop_stack.pop().unwrap();
                for break_addr in break_jumps {
                    self.chunk.patch_jump(break_addr, loop_end);
                }
            }
            Stmt::Break => {
                let jump = self.chunk.write(OpCode::Jump(0));
                if let Some((_, break_jumps)) = self.loop_stack.last_mut() {
                    break_jumps.push(jump);
                }
            }
            Stmt::Continue => {
                if let Some((loop_start, _)) = self.loop_stack.last() {
                    self.chunk.write(OpCode::Jump(*loop_start));
                }
            }
            Stmt::Function(name, params, body) => {
                let func_chunk = self.compile_function(params, body);
                let idx = self.chunk.add_constant(Value::Bytecode(Rc::new(func_chunk)));
                self.chunk.write(OpCode::Constant(idx));

                self.chunk.write(OpCode::SetGlobal(intern(name)));
            }
            Stmt::Return(expr) => {
                self.compile_expr(expr);
                self.chunk.write(OpCode::Return);
            }
            Stmt::Class(name, _superclass, fields, constructor, methods) => {
                let mut vm_class = VmClass {
                    name: name.clone(),
                    fields: fields.clone(),
                    constructor: None,
                    constructor_params: Vec::new(),
                    methods: HashMap::new(),
                    method_params: HashMap::new(),
                    superclass: None,
                    superclass_name: _superclass.clone(),
                };

                if let Some((params, body)) = constructor {
                    let mut func_compiler = self.child(1);
                    func_compiler.resolve_local("self");
                    for p in params { func_compiler.resolve_local(p); }
                    for stmt in body { func_compiler.compile_stmt(stmt); }

                    func_compiler.chunk.write(OpCode::GetLocal(0));
                    func_compiler.chunk.write(OpCode::Return);
                    func_compiler.chunk.param_count = params.len();
                    vm_class.constructor = Some(Rc::new(func_compiler.chunk));
                    vm_class.constructor_params = params.clone();
                }

                for (mname, mparams, mbody) in methods {
                    let mut func_compiler = self.child(1);
                    func_compiler.resolve_local("self");
                    for p in mparams { func_compiler.resolve_local(p); }
                    for stmt in mbody { func_compiler.compile_stmt(stmt); }
                    func_compiler.chunk.write(OpCode::Return);
                    func_compiler.chunk.param_count = mparams.len();
                    vm_class.methods.insert(mname.clone(), Rc::new(func_compiler.chunk));
                    vm_class.method_params.insert(mname.clone(), mparams.clone());
                }

                let idx = self.chunk.add_constant(Value::VmClass(Rc::new(vm_class)));
                self.chunk.write(OpCode::Constant(idx));
                self.chunk.write(OpCode::SetGlobal(intern(name)));
            }
            Stmt::SetField(target, field, value) => {
                self.compile_expr(target);
                let key_idx = self.chunk.add_constant(Value::String(field.clone()));
                self.chunk.write(OpCode::Constant(key_idx));
                self.compile_expr(value);
                self.chunk.write(OpCode::IndexSet);
                self.chunk.write(OpCode::Pop);
            }
            Stmt::Import(path) => {
                let abs_path = std::fs::canonicalize(path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.clone());

                if self.imported_files.borrow().contains(&abs_path) {
                    return;
                }
                self.imported_files.borrow_mut().insert(abs_path);

                let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
                    panic!("VM Compile Error: Failed to import '{}': {}", path, e)
                });
                let ast = crate::lex_and_parse(&source);
                for stmt in &ast {
                    self.compile_stmt(stmt);
                }
            }
        }
    }

    fn compile_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(n) => {
                let idx = self.chunk.add_constant(Value::Number(*n));
                self.chunk.write(OpCode::Constant(idx));
            }
            Expr::String(s) => {
                let idx = self.chunk.add_constant(Value::String(s.clone()));
                self.chunk.write(OpCode::Constant(idx));
            }
            Expr::Boolean(b) => {
                let idx = self.chunk.add_constant(Value::Boolean(*b));
                self.chunk.write(OpCode::Constant(idx));
            }
            Expr::Null => {
                let idx = self.chunk.add_constant(Value::Null);
                self.chunk.write(OpCode::Constant(idx));
            }
            Expr::This => {

                if self.depth > 0 {
                    self.chunk.write(OpCode::GetLocal(0));
                } else {
                    let idx = self.chunk.add_constant(Value::Null);
                    self.chunk.write(OpCode::Constant(idx));
                }
            }
            Expr::Ident(name) => {

                let local_idx = self.locals.iter().rposition(|l| l == name);
                if let Some(idx) = local_idx {
                    self.chunk.write(OpCode::GetLocal(idx));
                    return;
                }

                self.chunk.write(OpCode::GetGlobal(intern(name)));
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

            Expr::Call(callee, args) => {
                if let Expr::Ident(name) = callee.as_ref() {

                    if is_builtin(name) {
                        for arg in args { self.compile_expr(arg); }
                        self.chunk.write(OpCode::BuiltinCall(intern(name), args.len()));
                        return;
                    }

                    let local_idx = self.locals.iter().rposition(|l| l == name);
                    if let Some(idx) = local_idx {
                        for arg in args { self.compile_expr(arg); }
                        self.chunk.write(OpCode::GetLocal(idx));
                        self.chunk.write(OpCode::Call(args.len()));
                        return;
                    }

                    for arg in args { self.compile_expr(arg); }
                    self.chunk.write(OpCode::GetGlobal(intern(name)));
                    self.chunk.write(OpCode::Call(args.len()));
                } else if let Expr::Index(obj, key) = callee.as_ref() {

                    if let Expr::String(method_name) = key.as_ref() {
                        self.compile_expr(obj);
                        for arg in args { self.compile_expr(arg); }
                        self.chunk.write(OpCode::CallMethod(intern(method_name), args.len()));
                    } else {
                        self.compile_expr(callee);
                        for arg in args { self.compile_expr(arg); }
                        self.chunk.write(OpCode::Call(args.len()));
                    }
                } else {
                    self.compile_expr(callee);
                    for arg in args { self.compile_expr(arg); }
                    self.chunk.write(OpCode::Call(args.len()));
                }
            }
            Expr::Lambda(params, body) => {

                let param_set: HashSet<String> = params.iter().cloned().collect();
                let mut free_names: HashSet<String> = HashSet::new();
                collect_free_idents_block(body, &param_set, &mut free_names);

                let mut captured: Vec<String> = free_names.into_iter()
                    .filter(|n| !is_builtin(n))
                    .collect();
                captured.sort();

                let mut func_compiler = self.child(1);
                for cname in &captured { func_compiler.resolve_local(cname); }
                for p in params { func_compiler.resolve_local(p); }
                for stmt in body { func_compiler.compile_stmt(stmt); }
                func_compiler.chunk.write(OpCode::Return);
                func_compiler.chunk.param_count = captured.len() + params.len();

                let chunk_idx = self.chunk.add_constant(Value::Bytecode(Rc::new(func_compiler.chunk)));
                self.chunk.write(OpCode::Constant(chunk_idx));

                if captured.is_empty() {

                    return;
                }

                for cname in &captured {
                    if let Some(idx) = self.locals.iter().rposition(|l| l == cname) {
                        self.chunk.write(OpCode::GetLocal(idx));
                    } else {
                        self.chunk.write(OpCode::GetGlobal(intern(cname)));
                    }
                }

                self.chunk.write(OpCode::BuildArray(1 + captured.len()));
            }
            Expr::Array(elements) => {
                for elem in elements {
                    self.compile_expr(elem);
                }
                self.chunk.write(OpCode::BuildArray(elements.len()));
            }
            Expr::Dict(pairs) => {
                for (k, v) in pairs {

                    if let Expr::Ident(name) = k {
                        let idx = self.chunk.add_constant(Value::String(name.clone()));
                        self.chunk.write(OpCode::Constant(idx));
                    } else {
                        self.compile_expr(k);
                    }
                    self.compile_expr(v);
                }
                self.chunk.write(OpCode::BuildDict(pairs.len()));
            }
            Expr::Index(obj, idx) => {
                self.compile_expr(obj);
                self.compile_expr(idx);
                self.chunk.write(OpCode::IndexGet);
            }

            _ => {
                let idx = self.chunk.add_constant(Value::Null);
                self.chunk.write(OpCode::Constant(idx));
            }
        }
    }

    fn compile_function(&mut self, params: &[String], body: &[Stmt]) -> Chunk {
        let mut func_compiler = self.child(1);
        for p in params {
            func_compiler.resolve_local(p);
        }
        for stmt in body {
            func_compiler.compile_stmt(stmt);
        }
        func_compiler.chunk.write(OpCode::Return);
        func_compiler.chunk.param_count = params.len();
        func_compiler.chunk
    }
}

struct CallFrame {
    return_ip: usize,
    return_chunk: Rc<Chunk>,
    locals_offset: usize,
}

pub struct VM {
    chunk: Rc<Chunk>,
    ip: usize,
    stack: Vec<Value>,
    locals: Vec<Value>,
    globals: HashMap<Rc<str>, Value>,
    call_stack: Vec<CallFrame>,
    try_stack: Vec<TryFrame>,
}

impl VM {
    pub fn new(chunk: Chunk) -> VM {
        VM {
            chunk: Rc::new(chunk),
            ip: 0,
            stack: Vec::new(),
            locals: Vec::new(),
            globals: HashMap::new(),
            call_stack: Vec::new(),
            try_stack: Vec::new(),
        }
    }

    pub fn run(mut self) -> Value {
        while self.ip < self.chunk.code.len() {
            let op = self.chunk.code[self.ip].clone();
            self.ip += 1;

            match op {
                OpCode::Constant(idx) => self.stack.push(self.chunk.constants[idx].clone()),
                OpCode::Pop => {
                    self.stack.pop();
                }

                OpCode::Add => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    self.stack.push(self.add(a, b));
                }
                OpCode::Sub => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Number(n1 - n2));
                    } else {
                        panic!("VM Error: Math requires numbers");
                    }
                }

                OpCode::Try(catch_ip) => {
                    self.try_stack.push(TryFrame {
                        catch_ip,
                        stack_len: self.stack.len(),
                        locals_len: self.locals.len(),
                    });
                }
                OpCode::PopTry => {
                    self.try_stack.pop();
                }
                OpCode::Throw => {
                    let err_val = self.stack.pop().unwrap_or(Value::Null);
                    let err_msg = err_val.to_string();

                    if let Some(frame) = self.try_stack.pop() {

                        self.stack.truncate(frame.stack_len);
                        self.locals.truncate(frame.locals_len);

                        self.stack.push(Value::String(err_msg));

                        self.ip = frame.catch_ip;
                    } else {

                        panic!("ZERA_THROW: {}", err_msg);
                    }
                }
                OpCode::Mul => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Number(n1 * n2));
                    } else {
                        panic!("VM Error: Math requires numbers");
                    }
                }
                OpCode::Div => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Number(n1 / n2));
                    } else {
                        panic!("VM Error: Math requires numbers");
                    }
                }
                OpCode::Mod => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Number(n1 % n2));
                    } else {
                        panic!("VM Error: Math requires numbers");
                    }
                }
                OpCode::Negate => {
                    let a = self.stack.pop().unwrap();
                    if let Value::Number(n) = a {
                        self.stack.push(Value::Number(-n));
                    } else {
                        panic!("VM Error: Cannot negate non-number");
                    }
                }
                OpCode::Not => {
                    let a = self.stack.pop().unwrap();
                    self.stack.push(Value::Boolean(!self.is_truthy(&a)));
                }

                OpCode::Greater => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Boolean(n1 > n2));
                    } else {
                        panic!("VM Error: > requires numbers");
                    }
                }
                OpCode::Less => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Boolean(n1 < n2));
                    } else {
                        panic!("VM Error: < requires numbers");
                    }
                }
                OpCode::GreaterEq => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Boolean(n1 >= n2));
                    } else {
                        panic!("VM Error: >= requires numbers");
                    }
                }
                OpCode::LessEq => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
                        self.stack.push(Value::Boolean(n1 <= n2));
                    } else {
                        panic!("VM Error: <= requires numbers");
                    }
                }
                OpCode::Equal => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    self.stack.push(Value::Boolean(a == b));
                }
                OpCode::NotEqual => {
                    let b = self.stack.pop().unwrap();
                    let a = self.stack.pop().unwrap();
                    self.stack.push(Value::Boolean(a != b));
                }

                OpCode::BuildArray(count) => {
                    let mut elements = Vec::with_capacity(count);
                    for _ in 0..count {
                        elements.push(self.stack.pop().unwrap_or(Value::Null));
                    }
                    elements.reverse();
                    self.stack.push(Value::Array(Rc::new(elements)));
                }
                OpCode::BuildDict(count) => {
                    let mut map = HashMap::new();
                    for _ in 0..count {
                        let val = self.stack.pop().unwrap_or(Value::Null);
                        let key_val = self.stack.pop().unwrap_or(Value::Null);
                        let key = match key_val {
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            _ => format!("{}", key_val),
                        };
                        map.insert(key, val);
                    }
                    self.stack.push(Value::Dict(Rc::new(map)));
                }

                OpCode::IndexGet => {
                    let idx_val = self.stack.pop().unwrap_or(Value::Null);
                    let collection = self.stack.pop().unwrap_or(Value::Null);

                    let result = match (&collection, &idx_val) {
                        (Value::Array(arr), Value::Number(i)) => {
                            arr.get(*i as usize).cloned().unwrap_or(Value::Null)
                        }
                        (Value::Dict(map), Value::String(s)) => {
                            map.get(s).cloned().unwrap_or(Value::Null)
                        }
                        (Value::VmInstance(inst), Value::String(s)) => {
                            inst.borrow().fields.get(s).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null,
                    };
                    self.stack.push(result);
                }
                OpCode::IndexSet => {
                    let val = self.stack.pop().unwrap_or(Value::Null);
                    let idx_val = self.stack.pop().unwrap_or(Value::Null);
                    let collection = self.stack.pop().unwrap_or(Value::Null);

                    match (&collection, &idx_val) {
                        (Value::Dict(map), Value::String(s)) => {
                            let mut new_map = (**map).clone();
                            new_map.insert(s.clone(), val.clone());
                            self.stack.push(Value::Dict(Rc::new(new_map)));
                        }
                        (Value::VmInstance(inst), Value::String(s)) => {
                            inst.borrow_mut().fields.insert(s.clone(), val.clone());
                            self.stack.push(Value::VmInstance(inst.clone()));
                        }
                        _ => { self.stack.push(val); }
                    }
                }

                OpCode::GetLocal(idx) => {
                    let offset = self.call_stack.last().map(|f| f.locals_offset).unwrap_or(0);
                    let val = self.locals.get(offset + idx).cloned().unwrap_or(Value::Null);
                    self.stack.push(val);
                }
                OpCode::SetLocal(idx) => {
                    let offset = self.call_stack.last().map(|f| f.locals_offset).unwrap_or(0);
                    let val = self.stack.pop().unwrap_or(Value::Null);
                    while self.locals.len() <= offset + idx {
                        self.locals.push(Value::Null);
                    }
                    self.locals[offset + idx] = val;
                }

                OpCode::GetGlobal(name) => {
                    let val = self.globals.get(&name).cloned().unwrap_or(Value::Null);
                    self.stack.push(val);
                }
                OpCode::SetGlobal(name) => {
                    let val = self.stack.pop().unwrap_or(Value::Null);
                    self.globals.insert(name, val);
                }

                OpCode::Print => {
                    let val = self.stack.pop().unwrap();
                    println!("{}", val);
                }

                OpCode::Jump(target) => {
                    self.ip = target;
                }
                OpCode::JumpIfFalse(target) => {
                    if !self.is_truthy(self.stack.last().unwrap()) {
                        self.ip = target;
                    }
                }

                OpCode::BuiltinCall(name, arg_count) => {
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.stack.pop().unwrap());
                    }
                    args.reverse();

                    let result = self.call_builtin(&name, &args);
                    self.stack.push(result);
                }

                OpCode::Call(arg_count) => {
                    let callee = self.stack.pop().unwrap_or(Value::Null);
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.stack.pop().unwrap_or(Value::Null));
                    }
                    args.reverse();

                    match callee {
                        Value::Bytecode(func_chunk) => {
                            let frame = CallFrame {
                                return_ip: self.ip,
                                return_chunk: self.chunk.clone(),
                                locals_offset: self.locals.len(),
                            };
                            self.call_stack.push(frame);
                            for arg in args { self.locals.push(arg); }
                            self.chunk = func_chunk;
                            self.ip = 0;
                        }
                        Value::Array(arr) if matches!(arr.get(0), Some(Value::Bytecode(_))) => {

                            let func_chunk = match &arr[0] {
                                Value::Bytecode(c) => c.clone(),
                                _ => unreachable!(),
                            };
                            let frame = CallFrame {
                                return_ip: self.ip,
                                return_chunk: self.chunk.clone(),
                                locals_offset: self.locals.len(),
                            };
                            self.call_stack.push(frame);
                            for upval in arr[1..].iter() { self.locals.push(upval.clone()); }
                            for arg in args { self.locals.push(arg); }
                            self.chunk = func_chunk;
                            self.ip = 0;
                        }
                        Value::VmClass(class) => {

                            let mut fields_map = HashMap::new();
                            for f in &class.fields {
                                fields_map.insert(f.clone(), Value::Null);
                            }

                            let inst = Rc::new(RefCell::new(VmInstance {
                                class: class.clone(),
                                fields: fields_map,
                            }));

                            if let Some(cons_chunk) = &class.constructor {
                                let frame = CallFrame {
                                    return_ip: self.ip,
                                    return_chunk: self.chunk.clone(),
                                    locals_offset: self.locals.len(),
                                };
                                self.call_stack.push(frame);

                                self.locals.push(Value::VmInstance(inst.clone()));
                                for arg in args { self.locals.push(arg); }

                                self.chunk = cons_chunk.clone();
                                self.ip = 0;
                            } else {
                                self.stack.push(Value::VmInstance(inst));
                            }
                        }
                        _ => panic!("VM Error: Cannot call non-bytecode value"),
                    }
                }

                OpCode::Return => {
                    if self.call_stack.is_empty() {
                        break;
                    } else {
                        let frame = self.call_stack.pop().unwrap();
                        self.ip = frame.return_ip;
                        self.chunk = frame.return_chunk;
                        self.locals.truncate(frame.locals_offset);
                    }
                }

                OpCode::CallMethod(method_name, arg_count) => {
                    let mut args = Vec::new();
                    for _ in 0..arg_count {
                        args.push(self.stack.pop().unwrap_or(Value::Null));
                    }
                    args.reverse();
                    let obj = self.stack.pop().unwrap_or(Value::Null);

                    if let Value::NativeModule(module) = &obj {
                        let result = crate::call_ffi(module, &method_name, &args);
                        self.stack.push(result);
                    } else if let Value::VmInstance(inst) = &obj {
                        let class = inst.borrow().class.clone();

                        let mut current = Some(class.clone());
                        let mut found_chunk = None;
                        while let Some(c) = current.take() {
                            if let Some(chunk) = c.methods.get(&*method_name) {
                                found_chunk = Some(chunk.clone());
                                break;
                            }

                            if let Some(super_class) = &c.superclass {
                                current = Some(super_class.clone());
                            } else if let Some(super_name) = &c.superclass_name {
                                if let Some(Value::VmClass(super_class)) = self.globals.get(super_name.as_str()) {
                                    current = Some(super_class.clone());
                                }
                            }
                        }

                        if let Some(func_chunk) = found_chunk {
                            let frame = CallFrame {
                                return_ip: self.ip,
                                return_chunk: self.chunk.clone(),
                                locals_offset: self.locals.len(),
                            };
                            self.call_stack.push(frame);

                            self.locals.push(Value::VmInstance(inst.clone()));
                            for arg in args { self.locals.push(arg); }

                            self.chunk = func_chunk.clone();
                            self.ip = 0;
                        } else {
                            panic!("VM Error: Method '{}' not found on instance", method_name);
                        }
                    } else {
                        panic!("VM Error: Cannot call method '{}' on {:?}", method_name, obj);
                    }
                }
            }
        }
        self.stack.pop().unwrap_or(Value::Null)
    }

    pub fn run_method(chunk: Rc<Chunk>, self_val: Value, args: &[Value]) -> Value {
        let mut vm = VM::new((*chunk).clone());
        vm.locals.push(self_val);
        for arg in args { vm.locals.push(arg.clone()); }
        vm.run()
    }

    pub fn run_function(chunk: Rc<Chunk>, args: &[Value]) -> Value {
        let mut vm = VM::new((*chunk).clone());
        for arg in args { vm.locals.push(arg.clone()); }
        vm.run()
    }

    fn add(&self, a: Value, b: Value) -> Value {
        if let (Value::Number(n1), Value::Number(n2)) = (&a, &b) {
            return Value::Number(n1 + n2);
        }
        if let (Value::String(s1), Value::String(s2)) = (&a, &b) {
            return Value::String(s1.clone() + s2);
        }
        if let (Value::String(s), Value::Number(n)) = (&a, &b) {
            return Value::String(s.clone() + &n.to_string());
        }
        if let (Value::Number(n), Value::String(s)) = (&a, &b) {
            return Value::String(n.to_string() + s);
        }
        panic!("VM Error: Cannot add {:?} and {:?}", a, b);
    }

    fn is_truthy(&self, val: &Value) -> bool {
        match val {
            Value::Boolean(b) => *b,
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Null => false,
            _ => true,
        }
    }

    fn call_builtin(&mut self, name: &str, args: &[Value]) -> Value {
        match name {
            "str" => Value::String(args.get(0).unwrap_or(&Value::Null).to_string()),
            "number" => {
                if let Some(val) = args.get(0) {
                    match val {
                        Value::String(s) => {
                            if let Ok(n) = s.parse::<f64>() {
                                return Value::Number(n);
                            }
                            return Value::Null;
                        }
                        Value::Boolean(b) => return Value::Number(if *b { 1.0 } else { 0.0 }),
                        Value::Number(n) => return Value::Number(*n),
                        _ => panic!("VM Error: number() requires string/bool/number"),
                    }
                }
                panic!("VM Error: number() requires argument");
            }
            "ask" | "input" => {
                let prompt = match args.get(0) {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                print!("{}", prompt);
                io::stdout().flush().unwrap();
                let mut input = String::new();
                io::stdin().read_line(&mut input).unwrap();
                Value::String(input.trim().to_string())
            }
            "print" => {
                if let Some(v) = args.get(0) {
                    print!("{}", v);
                    io::stdout().flush().unwrap();
                }
                Value::Null
            }
            "time" => {
                let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                Value::Number(duration.as_secs_f64())
            }
            "read_file" => {
                if let Some(Value::String(path)) = args.get(0) {
                    return std::fs::read_to_string(path).map(Value::String).unwrap_or_else(|e| {
                        panic!("VM Error: Failed to read file '{}': {}", path, e)
                    });
                }
                Value::Null
            }
            "write_file" => {
                if let (Some(Value::String(path)), Some(Value::String(content))) = (args.get(0), args.get(1)) {
                    std::fs::write(path, content).unwrap_or_else(|e| panic!("VM Error: Failed to write to '{}': {}", path, e));
                    return Value::Null;
                }
                Value::Null
            }
            "exec" => {
                if let Some(Value::String(cmd)) = args.get(0) {
                    let output = if cfg!(target_os = "windows") {
                        std::process::Command::new("cmd").args(["/C", cmd]).output()
                    } else {
                        std::process::Command::new("sh").arg("-c").arg(cmd).output()
                    };
                    return match output {
                        Ok(o) => Value::String(String::from_utf8_lossy(&o.stdout).to_string()),
                        Err(e) => panic!("VM Error: Failed to execute command: {}", e),
                    };
                }
                Value::Null
            }
            "load_library" => {
                if let Some(Value::String(path)) = args.get(0) {
                    let lib = unsafe {
                        Library::new(path).unwrap_or_else(|e| {
                            panic!("VM Error: Failed to load library '{}': {}", path, e)
                        })
                    };
                    return Value::NativeModule(Rc::new(NativeModule {
                        name: path.clone(),
                        lib,
                    }));
                }
                Value::Null
            }
            "sleep" => {
                if let Some(Value::Number(ms)) = args.get(0) {
                    std::thread::sleep(std::time::Duration::from_millis(*ms as u64));
                    return Value::Null;
                }
                Value::Null
            }
            "length" => match args.get(0) {
                Some(Value::Array(arr)) => Value::Number(arr.len() as f64),
                Some(Value::String(s)) => Value::Number(s.chars().count() as f64),
                Some(Value::Dict(map)) => Value::Number(map.len() as f64),
                _ => Value::Null,
            },
            "push" => match (args.get(0), args.get(1)) {
                (Some(Value::Array(arr)), Some(val)) => {
                    let mut new_arr = (**arr).clone();
                    new_arr.push(val.clone());
                    Value::Array(Rc::new(new_arr))
                }
                _ => Value::Null,
            },
            "pop" => match args.get(0) {
                Some(Value::Array(arr)) => arr.last().cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            },
            "range" => {
                let n = match args.get(0) {
                    Some(Value::Number(n)) => *n as i64,
                    _ => 0,
                };
                let vals: Vec<Value> = (0..n).map(|i| Value::Number(i as f64)).collect();
                Value::Array(Rc::new(vals))
            }
            "upper" => match args.get(0) {
                Some(Value::String(s)) => Value::String(s.to_uppercase()),
                _ => Value::Null,
            },
            "lower" => match args.get(0) {
                Some(Value::String(s)) => Value::String(s.to_lowercase()),
                _ => Value::Null,
            },
            "contains" => match (args.get(0), args.get(1)) {
                (Some(Value::Array(arr)), Some(target)) => {
                    Value::Boolean(arr.iter().any(|v| v == target))
                }
                (Some(Value::String(haystack)), Some(Value::String(needle))) => {
                    Value::Boolean(haystack.contains(needle))
                }
                _ => Value::Null,
            },
            _ => panic!("VM Error: Unknown built-in function '{}'", name),
        }
    }
}

pub fn execute_bytecode(statements: &[Stmt]) {
    let compiler = Compiler::new();
    let chunk = compiler.compile(statements);
    let vm = VM::new(chunk);
    let _ = vm.run();
}