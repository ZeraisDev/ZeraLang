
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use cranelift::codegen::ir::{AbiParam, FuncRef, Function, GlobalValue};
use cranelift::codegen::settings;
use cranelift::prelude::*;
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::vm::{Chunk, Compiler, OpCode};

struct FnEntry {
    symbol: String,
    chunk: Rc<Chunk>,

    arity: usize,

    param_count: usize,
    zera_name: Option<String>,
    class: Option<String>,
    method: Option<String>,
}

#[derive(Default)]
struct Plan {
    entries: Vec<FnEntry>,

    by_ptr: HashMap<usize, usize>,

    named: HashMap<String, usize>,
    classes: Vec<Rc<crate::vm::VmClass>>,
    class_ptrs: HashSet<usize>,
    scanned: HashSet<usize>,

    ids: Vec<FuncId>,
}

impl Plan {
    fn add(&mut self, chunk: &Rc<Chunk>, zera_name: Option<String>, class: Option<String>, method: Option<String>) {
        let key = Rc::as_ptr(chunk) as usize;
        if self.by_ptr.contains_key(&key) {
            return;
        }
        let index = self.entries.len();
        let arity = chunk.param_count + usize::from(class.is_some());
        let base = match (&class, &method) {
            (Some(c), Some(m)) => format!("__zera_m_{}_{}", c, m),
            _ => format!("__zera_f{}", index),
        };
        let mut symbol = base.clone();
        let mut n = 1;
        while self.entries.iter().any(|e| e.symbol == symbol) {
            symbol = format!("{}_{}", base, n);
            n += 1;
        }
        if let Some(name) = &zera_name {
            self.named.insert(name.clone(), index);
        }
        self.by_ptr.insert(key, index);
        self.entries.push(FnEntry {
            symbol,
            chunk: chunk.clone(),
            arity,
            param_count: chunk.param_count,
            zera_name,
            class,
            method,
        });
    }

    fn scan(&mut self, chunk: &Rc<Chunk>) {
        if !self.scanned.insert(Rc::as_ptr(chunk) as usize) {
            return;
        }
        for (i, op) in chunk.code.iter().enumerate() {
            let OpCode::Constant(idx) = op else { continue };
            match &chunk.constants[*idx] {
                crate::Value::Bytecode(inner) => {

                    let name = match chunk.code.get(i + 1) {
                        Some(OpCode::SetGlobal(n)) => Some(n.to_string()),
                        _ => None,
                    };
                    self.add(inner, name, None, None);
                    self.scan(inner);
                }
                crate::Value::VmClass(class) => {
                    if self.class_ptrs.insert(Rc::as_ptr(class) as usize) {
                        self.classes.push(class.clone());
                    }
                    if let Some(ctor) = &class.constructor {
                        self.add(ctor, None, Some(class.name.clone()), Some("construct".to_string()));
                        self.scan(ctor);
                    }

                    let mut methods: Vec<(&String, &Rc<Chunk>)> = class.methods.iter().collect();
                    methods.sort_by_key(|(k, _)| (*k).clone());
                    for (mname, mchunk) in methods {
                        let (c, m) = (class.name.clone(), mname.clone());
                        self.add(mchunk, None, Some(c), Some(m));
                        self.scan(mchunk);
                    }
                }
                _ => {}
            }
        }
    }

}

struct Runtime {
    funcs: HashMap<String, FuncId>,
}

struct RuntimeRefs {
    funcs: HashMap<String, FuncRef>,
}

impl Runtime {
    fn declare_in(&self, module: &mut ObjectModule, func: &mut Function) -> RuntimeRefs {
        RuntimeRefs {
            funcs: self
                .funcs
                .iter()
                .map(|(n, id)| (n.clone(), module.declare_func_in_func(*id, func)))
                .collect(),
        }
    }
}

impl RuntimeRefs {
    fn get(&self, name: &str) -> FuncRef {
        *self.funcs.get(name).unwrap_or_else(|| panic!("AOT: runtime helper '{}' was not imported", name))
    }
}

fn call_i64(b: &mut FunctionBuilder, rt: &RuntimeRefs, name: &str, args: &[Value]) -> Value {
    let inst = b.ins().call(rt.get(name), args);
    b.inst_results(inst)[0]
}

fn call_void(b: &mut FunctionBuilder, rt: &RuntimeRefs, name: &str, args: &[Value]) {
    b.ins().call(rt.get(name), args);
}

fn declare_runtime(module: &mut ObjectModule) -> Runtime {
    let i64t = types::I64;
    let f64t = types::F64;
    let mut funcs: HashMap<String, FuncId> = HashMap::new();
    let mut add = |name: &str, params: Vec<Type>, ret: Option<Type>| {
        let mut sig = module.make_signature();
        for p in params {
            sig.params.push(AbiParam::new(p));
        }
        if let Some(r) = ret {
            sig.returns.push(AbiParam::new(r));
        }
        let id = module
            .declare_function(name, Linkage::Import, &sig)
            .unwrap_or_else(|e| panic!("AOT: cannot import {}: {}", name, e));
        funcs.insert(name.to_string(), id);
    };

    add("zera_add", vec![i64t, i64t], Some(i64t));
    add("zera_sub", vec![i64t, i64t], Some(i64t));
    add("zera_mul", vec![i64t, i64t], Some(i64t));
    add("zera_div", vec![i64t, i64t], Some(i64t));
    add("zera_mod", vec![i64t, i64t], Some(i64t));
    add("zera_neg", vec![i64t], Some(i64t));

    add("zera_cmp_lt", vec![i64t, i64t], Some(i64t));
    add("zera_cmp_gt", vec![i64t, i64t], Some(i64t));
    add("zera_cmp_le", vec![i64t, i64t], Some(i64t));
    add("zera_cmp_ge", vec![i64t, i64t], Some(i64t));
    add("zera_cmp_eq", vec![i64t, i64t], Some(i64t));
    add("zera_cmp_ne", vec![i64t, i64t], Some(i64t));
    add("zera_is_truthy", vec![i64t], Some(i64t));
    add("zera_bool", vec![i64t], Some(i64t));
    add("zera_null", vec![], Some(i64t));

    add("zera_static_str", vec![i64t, i64t], Some(i64t));
    add("zera_make_number", vec![f64t], Some(i64t));
    add("zera_make_native_fn", vec![i64t, i64t], Some(i64t));
    add("zera_print", vec![i64t], None);

    add("zera_define_class", vec![i64t, i64t, i64t, i64t], None);
    add("zera_class_add_field", vec![i64t, i64t, i64t, i64t], None);
    add("zera_register_method", vec![i64t, i64t, i64t, i64t, i64t, i64t], None);
    add("zera_register_function", vec![i64t, i64t, i64t, i64t], Some(i64t));
    add("zera_link_classes", vec![], None);
    add("zera_class_ref", vec![i64t, i64t], Some(i64t));
    add("zera_function_ref", vec![i64t, i64t], Some(i64t));

    add("zera_push_arg", vec![i64t], None);
    add("zera_build_array", vec![i64t], Some(i64t));
    add("zera_build_dict", vec![i64t], Some(i64t));
    add("zera_index_get", vec![i64t, i64t], Some(i64t));
    add("zera_index_set", vec![i64t, i64t, i64t], Some(i64t));

    add("zera_builtin_call", vec![i64t, i64t], Some(i64t));
    add("zera_call_function", vec![i64t, i64t], Some(i64t));
    add("zera_get_method_ptr", vec![i64t, i64t], Some(i64t));
    add("zera_call_method", vec![i64t, i64t, i64t], Some(i64t));
    add("zera_instantiate", vec![i64t, i64t], Some(i64t));

    add("zera_alloc_jmpbuf", vec![], Some(i64t));
    add("zera_free_jmpbuf", vec![i64t], None);
    add("zera_push_jmpbuf", vec![i64t], None);
    add("zera_pop_jmpbuf", vec![], None);
    add("zera_throw", vec![i64t], Some(i64t));
    add("zera_get_exception", vec![], Some(i64t));

    add("setjmp", vec![i64t], Some(types::I32));

    Runtime { funcs }
}

pub fn compile_to_object(statements: &[crate::Stmt]) {
    let main_chunk: Rc<Chunk> = Rc::new(Compiler::new().compile(statements));

    let target = target_lexicon::Triple::host();
    let mut isa_builder = settings::builder();
    isa_builder.set("is_pic", "true").unwrap();
    let flags = settings::Flags::new(isa_builder);
    let isa = cranelift::codegen::isa::lookup(target)
        .expect("AOT: failed to look up ISA")
        .finish(flags)
        .expect("AOT: failed to build ISA");

    let obj_builder = ObjectBuilder::new(isa, "zeralang_module", cranelift_module::default_libcall_names())
        .expect("AOT: failed to create object builder");
    let mut module = ObjectModule::new(obj_builder);

    let mut plan = Plan::default();
    plan.scan(&main_chunk);

    let mut names = Vec::new();
    collect_names(&main_chunk, &plan, &mut names);
    let string_data = declare_string_data(&mut module, &names);
    let runtime = declare_runtime(&mut module);

    plan.ids = plan
        .entries
        .iter()
        .map(|e| {
            let mut sig = module.make_signature();
            for _ in 0..e.arity {
                sig.params.push(AbiParam::new(types::I64));
            }
            sig.returns.push(AbiParam::new(types::I64));
            module
                .declare_function(&e.symbol, Linkage::Local, &sig)
                .unwrap_or_else(|err| panic!("AOT: cannot declare {}: {}", e.symbol, err))
        })
        .collect();

    for (i, entry) in plan.entries.iter().enumerate() {
        let ctx = FuncCtx {
            plan: &plan,
            string_data: &string_data,
            runtime: &runtime,
            symbol: entry.symbol.clone(),
            is_method: entry.class.is_some(),
            is_entry: false,
            init_id: None,
        };
        let chunk = entry.chunk.clone();
        compile_chunk(&mut module, &chunk, &ctx);
    }

    let init_id = compile_init(&mut module, &plan, &string_data, &runtime);

    let ctx = FuncCtx {
        plan: &plan,
        string_data: &string_data,
        runtime: &runtime,
        symbol: "main".to_string(),
        is_method: false,
        is_entry: true,
        init_id: Some(init_id),
    };
    compile_chunk(&mut module, &main_chunk, &ctx);

    let bytes = module.finish().emit().expect("AOT: failed to emit object");
    std::fs::write("zeralang_output.o", bytes).expect("AOT: failed to write zeralang_output.o");
    println!(
        "AOT: wrote zeralang_output.o ({} functions, {} classes)",
        plan.entries.len(),
        plan.classes.len()
    );
    println!("Link with:  cc zeralang_output.o target/release/libzera_lang.a -o program");
}

fn collect_names(chunk: &Chunk, plan: &Plan, out: &mut Vec<String>) {
    fn push(out: &mut Vec<String>, s: &str) {
        if !out.iter().any(|e| e == s) {
            out.push(s.to_string());
        }
    }
    fn walk(chunk: &Chunk, out: &mut Vec<String>, seen: &mut HashSet<usize>) {
        if !seen.insert(chunk as *const Chunk as usize) {
            return;
        }
        for op in &chunk.code {
            if let OpCode::BuiltinCall(n, _) | OpCode::CallMethod(n, _) = op {
                push(out, n);
            }
        }
        for value in &chunk.constants {
            match value {
                crate::Value::String(s) => push(out, s),
                crate::Value::Bytecode(c) => walk(c, out, seen),
                crate::Value::VmClass(class) => {
                    push(out, &class.name);
                    push(out, "construct");
                    for f in &class.fields {
                        push(out, f);
                    }
                    if let Some(s) = &class.superclass_name {
                        push(out, s);
                    }
                    for m in class.methods.keys() {
                        push(out, m);
                    }
                    if let Some(c) = &class.constructor {
                        walk(c, out, seen);
                    }
                    for c in class.methods.values() {
                        walk(c, out, seen);
                    }
                }
                _ => {}
            }
        }
    }
    let mut seen = HashSet::new();
    walk(chunk, out, &mut seen);
    for entry in &plan.entries {
        walk(&entry.chunk, out, &mut seen);
        if let Some(n) = &entry.zera_name {
            push(out, n);
        }
    }

    push(out, "construct");
    for class in &plan.classes {
        push(out, &class.name);
        if let Some(s) = &class.superclass_name {
            push(out, s);
        }
        for f in &class.fields {
            push(out, f);
        }
        for m in class.methods.keys() {
            push(out, m);
        }
    }
    for name in plan.named.keys() {
        push(out, name);
    }
}

fn declare_string_data(module: &mut ObjectModule, strings: &[String]) -> HashMap<String, DataId> {
    let mut map = HashMap::new();
    for (i, s) in strings.iter().enumerate() {
        let id = module
            .declare_data(&format!("__zera_str_{}", i), Linkage::Local, false, false)
            .expect("AOT: failed to declare string data");
        let mut desc = DataDescription::new();
        desc.define(s.clone().into_bytes().into_boxed_slice());
        module.define_data(id, &desc).expect("AOT: failed to define string data");
        map.insert(s.clone(), id);
    }
    map
}

struct FuncCtx<'a> {
    plan: &'a Plan,
    string_data: &'a HashMap<String, DataId>,
    runtime: &'a Runtime,
    symbol: String,
    is_method: bool,

    is_entry: bool,
    init_id: Option<FuncId>,
}

fn ensure_block(
    b: &mut FunctionBuilder,
    blocks: &mut HashMap<usize, Block>,
    order: &mut Vec<Block>,
    ip: usize,
) -> Block {
    if let Some(&existing) = blocks.get(&ip) {
        return existing;
    }
    let created = b.create_block();
    order.push(created);
    blocks.insert(ip, created);
    created
}

fn compile_chunk(module: &mut ObjectModule, chunk: &Chunk, ctx: &FuncCtx) {
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut context = module.make_context();

    let arity = chunk.param_count + usize::from(ctx.is_method);
    for _ in 0..arity {
        context.func.signature.params.push(AbiParam::new(types::I64));
    }
    context.func.signature.returns.push(AbiParam::new(types::I64));

    let linkage = if ctx.symbol == "main" { Linkage::Export } else { Linkage::Local };
    let func_id = module
        .declare_function(&ctx.symbol, linkage, &context.func.signature)
        .unwrap_or_else(|e| panic!("AOT: cannot declare {}: {}", ctx.symbol, e));

    let rt = ctx.runtime.declare_in(module, &mut context.func);
    let globals: HashMap<String, GlobalValue> = ctx
        .string_data
        .iter()
        .map(|(s, id)| (s.clone(), module.declare_data_in_func(*id, &mut context.func)))
        .collect();
    let init_ref = ctx.init_id.map(|id| module.declare_func_in_func(id, &mut context.func));

    let plan = ctx.plan;

    let mut builder = FunctionBuilder::new(&mut context.func, &mut fn_builder_ctx);

    let mut blocks: HashMap<usize, Block> = HashMap::new();
    let mut order: Vec<Block> = Vec::new();
    let entry = builder.create_block();
    order.push(entry);
    blocks.insert(0, entry);

    for (i, op) in chunk.code.iter().enumerate() {
        match op {
            OpCode::Jump(t) | OpCode::JumpIfFalse(t) | OpCode::Try(t) => {
                let _ = ensure_block(&mut builder, &mut blocks, &mut order, *t);
                let _ = ensure_block(&mut builder, &mut blocks, &mut order, i + 1);
            }
            _ => {}
        }
    }
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    if ctx.symbol == "main" {
        if let Some(init_ref) = init_ref {
            builder.ins().call(init_ref, &[]);
        }
    }

    let mut stack: Vec<Value> = Vec::new();
    let mut vars: HashMap<String, Variable> = HashMap::new();
    let mut next_var = 0usize;
    let mut pending_call: Option<String> = None;
    let mut terminated = false;

    let mut catch_stack: HashMap<usize, usize> = HashMap::new();

    for i in 0..arity {
        let param = builder.block_params(entry)[i];
        let v = Variable::new(next_var);
        next_var += 1;
        builder.declare_var(v, types::I64);
        builder.def_var(v, param);
        vars.insert(format!("__local_{}", i), v);
    }

    macro_rules! var_for {
        ($key:expr) => {{
            let key: String = $key;
            match vars.get(&key) {
                Some(existing) => *existing,
                None => {
                    let fresh = Variable::new(next_var);
                    next_var += 1;
                    builder.declare_var(fresh, types::I64);
                    vars.insert(key, fresh);
                    fresh
                }
            }
        }};
    }

    let mut ip = 0usize;
    while ip < chunk.code.len() {
        if let Some(&block) = blocks.get(&ip) {
            if ip > 0 && !terminated {
                builder.ins().jump(block, &[]);
            }
            builder.switch_to_block(block);
            terminated = false;
            if let Some(&depth) = catch_stack.get(&ip) {

                let buf = builder.block_params(block)[0];
                call_void(&mut builder, &rt, "zera_free_jmpbuf", &[buf]);
                let exc = call_i64(&mut builder, &rt, "zera_get_exception", &[]);
                stack.truncate(depth);
                stack.push(exc);
            }
        } else if terminated {
            ip += 1;
            continue;
        }

        let op = &chunk.code[ip];

        if let OpCode::Constant(idx) = op {
            if matches!(chunk.constants[*idx], crate::Value::Bytecode(_))
                && matches!(chunk.code.get(ip + 1), Some(OpCode::SetGlobal(_)))
            {
                ip += 2;
                continue;
            }
        }

        match op {
            OpCode::Try(catch_ip) => {
                let buf = call_i64(&mut builder, &rt, "zera_alloc_jmpbuf", &[]);
                let setjmp_ref = rt.get("setjmp");
                let sj = builder.ins().call(setjmp_ref, &[buf]);
                let returned = builder.inst_results(sj)[0];
                let zero32 = builder.ins().iconst(types::I32, 0);
                let threw = builder.ins().icmp(IntCC::NotEqual, returned, zero32);

                let catch_block = ensure_block(&mut builder, &mut blocks, &mut order, *catch_ip);
                builder.append_block_param(catch_block, types::I64);
                let setup = builder.create_block();
                order.push(setup);
                builder.append_block_param(setup, types::I64);
                catch_stack.insert(*catch_ip, stack.len());
                builder.ins().brif(threw, catch_block, &[buf], setup, &[buf]);
                terminated = true;

                builder.switch_to_block(setup);
                let bp = builder.block_params(setup)[0];
                call_void(&mut builder, &rt, "zera_push_jmpbuf", &[bp]);
                let body = ensure_block(&mut builder, &mut blocks, &mut order, ip + 1);
                builder.ins().jump(body, &[]);
            }
            OpCode::PopTry => {
                call_void(&mut builder, &rt, "zera_pop_jmpbuf", &[]);
            }
            OpCode::Throw => {
                let val = pop_from(&mut stack, "throw");
                call_void(&mut builder, &rt, "zera_throw", &[val]);
                builder.ins().trap(TrapCode::UnreachableCodeReached);
                terminated = true;
            }
            OpCode::Constant(idx) => match &chunk.constants[*idx] {
                crate::Value::Number(n) => {
                    let r = n.round();
                    let pushed = if n.is_finite() && *n == r && n.abs() < (1i64 << 50) as f64 {
                        builder.ins().iconst(types::I64, (r as i64) << 1)
                    } else {
                        let fv = builder.ins().f64const(*n);
                        call_i64(&mut builder, &rt, "zera_make_number", &[fv])
                    };
                    stack.push(pushed);
                }
                crate::Value::String(s) => {
                    let v = load_str(&mut builder, &rt, &globals, s);
                    stack.push(v);
                }
                crate::Value::Boolean(b) => {
                    let t = builder.ins().iconst(types::I64, i64::from(*b));
                    stack.push(tag_bool(&mut builder, &rt, t));
                }
                crate::Value::Null => stack.push(null(&mut builder, &rt)),
                crate::Value::VmClass(c) => {
                    let name = c.name.clone();
                    let (addr, len) = static_str(&mut builder, &globals, &name);
                    let v = call_i64(&mut builder, &rt, "zera_class_ref", &[addr, len]);
                    stack.push(v);
                }
                crate::Value::Bytecode(inner) => {
                    let key = Rc::as_ptr(inner) as usize;
                    let Some(&index) = plan.by_ptr.get(&key) else {
                        panic!("AOT: lambda chunk was never compiled");
                    };
                    let id = plan.ids[index];
                    let params = plan.entries[index].param_count;
                    let fr = module.declare_func_in_func(id, &mut builder.func);
                    let ptr = builder.ins().func_addr(types::I64, fr);
                    let ar = builder.ins().iconst(types::I64, params as i64);
                    let v = call_i64(&mut builder, &rt, "zera_make_native_fn", &[ptr, ar]);
                    stack.push(v);
                }
                other => panic!("AOT: constant {:?} cannot be emitted", other),
            },
            OpCode::Print => {
                let val = pop_from(&mut stack, "print");
                call_void(&mut builder, &rt, "zera_print", &[val]);
            }
            OpCode::Pop => {
                stack.pop();
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul => {
                let r = pop_from(&mut stack, "rhs");
                let l = pop_from(&mut stack, "lhs");
                let b = &mut builder;
                match op {
                    OpCode::Add => stack.push(int_binop(b, &rt, l, r, "zera_add", add_ints)),
                    OpCode::Sub => stack.push(int_binop(b, &rt, l, r, "zera_sub", sub_ints)),
                    _ => stack.push(int_binop(b, &rt, l, r, "zera_mul", mul_ints)),
                }
            }
            OpCode::Div | OpCode::Mod => {

                let helper = if matches!(op, OpCode::Div) { "zera_div" } else { "zera_mod" };
                let r = pop_from(&mut stack, "rhs");
                let l = pop_from(&mut stack, "lhs");
                let res = call_i64(&mut builder, &rt, helper, &[l, r]);
                stack.push(res);
            }
            OpCode::Negate => {
                let val = pop_from(&mut stack, "negate");
                let res = call_i64(&mut builder, &rt, "zera_neg", &[val]);
                stack.push(res);
            }
            OpCode::Not => {
                let val = pop_from(&mut stack, "not");
                let b = &mut builder;
                let t = truthy_flag(b, &rt, val);
                let one = b.ins().iconst(types::I64, 1);
                let negated = b.ins().isub(one, t);
                stack.push(tag_bool(b, &rt, negated));
            }
            OpCode::Less | OpCode::Greater | OpCode::GreaterEq | OpCode::LessEq | OpCode::Equal | OpCode::NotEqual => {
                let helper = match op {
                    OpCode::Less => "zera_cmp_lt",
                    OpCode::Greater => "zera_cmp_gt",
                    OpCode::GreaterEq => "zera_cmp_ge",
                    OpCode::LessEq => "zera_cmp_le",
                    OpCode::Equal => "zera_cmp_eq",
                    _ => "zera_cmp_ne",
                };
                let r = pop_from(&mut stack, "comparison rhs");
                let l = pop_from(&mut stack, "comparison lhs");
                let cc = match op {
                    OpCode::Less => IntCC::SignedLessThan,
                    OpCode::Greater => IntCC::SignedGreaterThan,
                    OpCode::GreaterEq => IntCC::SignedGreaterThanOrEqual,
                    OpCode::LessEq => IntCC::SignedLessThanOrEqual,
                    OpCode::Equal => IntCC::Equal,
                    _ => IntCC::NotEqual,
                };

                let res = cmp_inline(&mut builder, &rt, l, r, cc, helper);
                stack.push(res);
            }
            OpCode::BuildArray(count) => {
                let count = *count;
                push_args(&mut builder, &rt, &mut stack, count);
                let b = &mut builder;
                let n = b.ins().iconst(types::I64, count as i64);
                let v = call_i64(b, &rt, "zera_build_array", &[n]);
                stack.push(v);
            }
            OpCode::BuildDict(count) => {
                let pairs = *count;
                push_args(&mut builder, &rt, &mut stack, pairs * 2);
                let b = &mut builder;
                let n = b.ins().iconst(types::I64, pairs as i64);
                let v = call_i64(b, &rt, "zera_build_dict", &[n]);
                stack.push(v);
            }
            OpCode::IndexGet => {
                let idx = pop_from(&mut stack, "index");
                let obj = pop_from(&mut stack, "collection");
                let res = call_i64(&mut builder, &rt, "zera_index_get", &[obj, idx]);
                stack.push(res);
            }
            OpCode::IndexSet => {
                let val = pop_from(&mut stack, "assigned value");
                let idx = pop_from(&mut stack, "index");
                let obj = pop_from(&mut stack, "collection");
                let res = call_i64(&mut builder, &rt, "zera_index_set", &[obj, idx, val]);
                stack.push(res);
            }
            OpCode::BuiltinCall(name, count) => {
                let count = *count;
                let builtin = name.to_string();
                push_args(&mut builder, &rt, &mut stack, count);
                let b = &mut builder;
                let name_val = load_str(b, &rt, &globals, &builtin);
                let n = b.ins().iconst(types::I64, count as i64);
                let v = call_i64(b, &rt, "zera_builtin_call", &[name_val, n]);
                stack.push(v);
            }
            OpCode::CallMethod(name, arg_count) => {
                let count = *arg_count;
                let method_name = name.to_string();
                let mut args = Vec::new();
                for _ in 0..count {
                    args.insert(0, pop_from(&mut stack, "method argument"));
                }
                let receiver = pop_from(&mut stack, "method receiver");
                let b = &mut builder;
                let name_val = load_str(b, &rt, &globals, &method_name);

                let ptr = call_i64(b, &rt, "zera_get_method_ptr", &[receiver, name_val]);
                let zero = b.ins().iconst(types::I64, 0);
                let is_native = b.ins().icmp(IntCC::NotEqual, ptr, zero);

                let native = b.create_block();
                order.push(native);
                let fallback = b.create_block();
                order.push(fallback);
                let join = b.create_block();
                order.push(join);
                b.append_block_param(join, types::I64);
                b.ins().brif(is_native, native, &[], fallback, &[]);

                b.switch_to_block(native);
                let mut sig = module.make_signature();
                for _ in 0..(count + 1) {
                    sig.params.push(AbiParam::new(types::I64));
                }
                sig.returns.push(AbiParam::new(types::I64));
                let sig_ref = b.import_signature(sig);
                let mut all = vec![receiver];
                all.extend(args.iter().copied());
                let inst = b.ins().call_indirect(sig_ref, ptr, &all);
                let direct = b.inst_results(inst)[0];
                b.ins().jump(join, &[direct]);

                b.switch_to_block(fallback);

                for a in args.iter().rev() {
                    call_void(b, &rt, "zera_push_arg", &[*a]);
                }
                let n = b.ins().iconst(types::I64, count as i64);
                let boxed = call_i64(b, &rt, "zera_call_method", &[receiver, name_val, n]);
                b.ins().jump(join, &[boxed]);

                b.switch_to_block(join);
                b.seal_block(join);
                stack.push(b.block_params(join)[0]);
            }
            OpCode::Call(arg_count) => {
                let count = *arg_count;
                if let Some(name) = pending_call.take() {
                    if let Some(&index) = plan.named.get(&name) {
                        let declared = plan.entries[index].param_count;
                        let id = plan.ids[index];
                        let b = &mut builder;
                        let mut args = Vec::new();
                        for _ in 0..count {
                            args.insert(0, pop_from(&mut stack, "call argument"));
                        }

                        while args.len() < declared {
                            args.push(null(b, &rt));
                        }
                        args.truncate(declared);
                        let fr = module.declare_func_in_func(id, &mut b.func);
                        let inst = b.ins().call(fr, &args);
                        let res = b.inst_results(inst)[0];
                        stack.push(res);
                        ip += 1;
                        continue;
                    }
                }
                let callee = pop_from(&mut stack, "callee");
                push_args(&mut builder, &rt, &mut stack, count);
                let b = &mut builder;
                let n = b.ins().iconst(types::I64, count as i64);
                let v = call_i64(b, &rt, "zera_call_function", &[callee, n]);
                stack.push(v);
            }
            OpCode::Jump(target) => {
                let target = *target;
                let b = &mut builder;
                let block = ensure_block(b, &mut blocks, &mut order, target);
                b.ins().jump(block, &[]);
                terminated = true;
            }
            OpCode::JumpIfFalse(target) => {
                let target = *target;
                let next = ip + 1;
                let b = &mut builder;
                let cond = pop_from(&mut stack, "condition");
                let truthy = truthy_flag(b, &rt, cond);
                let zero = b.ins().iconst(types::I64, 0);
                let is_true = b.ins().icmp(IntCC::NotEqual, truthy, zero);
                let next_block = ensure_block(b, &mut blocks, &mut order, next);
                let target_block = ensure_block(b, &mut blocks, &mut order, target);
                b.ins().brif(is_true, next_block, &[], target_block, &[]);
                terminated = true;
            }
            OpCode::GetGlobal(name) => {
                let name = name.to_string();
                if plan.named.contains_key(&name) {
                    if matches!(chunk.code.get(ip + 1), Some(OpCode::Call(_))) {
                        pending_call = Some(name);
                    } else {

                        let n = name.clone();
                        let b = &mut builder;
                        let (addr, len) = static_str(b, &globals, &n);
                        let v = call_i64(b, &rt, "zera_function_ref", &[addr, len]);
                        stack.push(v);
                    }
                } else {
                    let v = var_for!(name);
                    stack.push(builder.use_var(v));
                }
            }
            OpCode::SetGlobal(name) => {
                let val = pop_from(&mut stack, "assignment");
                let v = var_for!(name.to_string());
                builder.def_var(v, val);
            }
            OpCode::GetLocal(idx) => {
                let v = var_for!(format!("__local_{}", idx));
                stack.push(builder.use_var(v));
            }
            OpCode::SetLocal(idx) => {
                let val = pop_from(&mut stack, "local assignment");
                let v = var_for!(format!("__local_{}", idx));
                builder.def_var(v, val);
            }
            OpCode::Return => {
                let b = &mut builder;
                let val = if stack.is_empty() {
                    if ctx.is_entry { b.ins().iconst(types::I64, 0) } else { null(b, &rt) }
                } else {
                    stack.pop().unwrap()
                };
                b.ins().return_(&[val]);
                terminated = true;
            }
        }
        ip += 1;
    }

    if !terminated {

        let v = if ctx.is_entry { builder.ins().iconst(types::I64, 0) } else { null(&mut builder, &rt) };
        builder.ins().return_(&[v]);
    }

    for &block in &order {
        builder.seal_block(block);
    }
    builder.finalize();

    if let Err(e) = cranelift::codegen::verify_function(&context.func, module.isa()) {
        panic!("AOT: {} failed verification: {}\n{}", ctx.symbol, e, context.func);
    }
    module
        .define_function(func_id, &mut context)
        .unwrap_or_else(|e| panic!("AOT: failed to define {}: {}", ctx.symbol, e));
    module.clear_context(&mut context);
}

fn pop_from(stack: &mut Vec<Value>, what: &str) -> Value {
    stack.pop().unwrap_or_else(|| panic!("AOT: operand stack underflow reading {}", what))
}

fn null(b: &mut FunctionBuilder, rt: &RuntimeRefs) -> Value {
    call_i64(b, rt, "zera_null", &[])
}

fn tag_bool(b: &mut FunctionBuilder, rt: &RuntimeRefs, flag: Value) -> Value {
    call_i64(b, rt, "zera_bool", &[flag])
}

fn static_str(b: &mut FunctionBuilder, globals: &HashMap<String, GlobalValue>, s: &str) -> (Value, Value) {
    let g = *globals
        .get(s)
        .unwrap_or_else(|| panic!("AOT: string '{}' was not interned into the object file", s));
    (b.ins().symbol_value(types::I64, g), b.ins().iconst(types::I64, s.len() as i64))
}

fn load_str(b: &mut FunctionBuilder, rt: &RuntimeRefs, globals: &HashMap<String, GlobalValue>, s: &str) -> Value {
    let (addr, len) = static_str(b, globals, s);
    call_i64(b, rt, "zera_static_str", &[addr, len])
}

fn push_args(b: &mut FunctionBuilder, rt: &RuntimeRefs, stack: &mut Vec<Value>, count: usize) {
    for _ in 0..count {
        let val = pop_from(stack, "argument");
        call_void(b, rt, "zera_push_arg", &[val]);
    }
}

fn both_tagged_ints(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    let zero = b.ins().iconst(types::I64, 0);
    let one = b.ins().iconst(types::I64, 1);
    let both = b.ins().bor(l, r);
    let tag = b.ins().band(both, one);
    b.ins().icmp(IntCC::Equal, tag, zero)
}

fn fast_or_call(
    b: &mut FunctionBuilder,
    rt: &RuntimeRefs,
    l: Value,
    r: Value,
    cond: Value,
    helper: &str,
    fast: impl FnOnce(&mut FunctionBuilder, Value, Value) -> Value,
) -> Value {
    let fast_block = b.create_block();
    let slow_block = b.create_block();
    let join = b.create_block();
    b.append_block_param(join, types::I64);
    b.ins().brif(cond, fast_block, &[], slow_block, &[]);

    b.switch_to_block(fast_block);
    b.seal_block(fast_block);
    let inline_value = fast(b, l, r);
    b.ins().jump(join, &[inline_value]);

    b.switch_to_block(slow_block);
    b.seal_block(slow_block);
    let boxed = call_i64(b, rt, helper, &[l, r]);
    b.ins().jump(join, &[boxed]);

    b.switch_to_block(join);
    b.seal_block(join);
    b.block_params(join)[0]
}

fn int_binop(
    b: &mut FunctionBuilder,
    rt: &RuntimeRefs,
    l: Value,
    r: Value,
    helper: &str,
    fast: impl FnOnce(&mut FunctionBuilder, Value, Value) -> Value,
) -> Value {
    let cond = both_tagged_ints(b, l, r);
    fast_or_call(b, rt, l, r, cond, helper, fast)
}

fn cmp_inline(
    b: &mut FunctionBuilder,
    rt: &RuntimeRefs,
    l: Value,
    r: Value,
    cc: IntCC,
    helper: &str,
) -> Value {
    let rt2 = rt;
    let cond = both_tagged_ints(b, l, r);
    fast_or_call(b, rt, l, r, cond, helper, |b, l, r| {
        let bit = b.ins().icmp(cc, l, r);
        let flag = b.ins().uextend(types::I64, bit);
        call_i64(b, rt2, "zera_bool", &[flag])
    })
}

fn truthy_flag(b: &mut FunctionBuilder, rt: &RuntimeRefs, v: Value) -> Value {
    let zero = b.ins().iconst(types::I64, 0);
    let one = b.ins().iconst(types::I64, 1);
    let tag = b.ins().band(v, one);
    let is_int = b.ins().icmp(IntCC::Equal, tag, zero);
    let fast_block = b.create_block();
    let slow_block = b.create_block();
    let join = b.create_block();
    b.append_block_param(join, types::I64);
    b.ins().brif(is_int, fast_block, &[], slow_block, &[]);

    b.switch_to_block(fast_block);
    b.seal_block(fast_block);
    let bit = b.ins().icmp(IntCC::NotEqual, v, zero);
    let flag = b.ins().uextend(types::I64, bit);
    b.ins().jump(join, &[flag]);

    b.switch_to_block(slow_block);
    b.seal_block(slow_block);
    let boxed = call_i64(b, rt, "zera_is_truthy", &[v]);
    b.ins().jump(join, &[boxed]);

    b.switch_to_block(join);
    b.seal_block(join);
    b.block_params(join)[0]
}

fn add_ints(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    b.ins().iadd(l, r)
}

fn sub_ints(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    b.ins().isub(l, r)
}

fn mul_ints(b: &mut FunctionBuilder, l: Value, r: Value) -> Value {
    let one = b.ins().iconst(types::I64, 1);
    let x = b.ins().sshr(l, one);
    let y = b.ins().sshr(r, one);
    let product = b.ins().imul(x, y);
    b.ins().ishl(product, one)
}

fn compile_init(
    module: &mut ObjectModule,
    plan: &Plan,
    string_data: &HashMap<String, DataId>,
    runtime: &Runtime,
) -> FuncId {
    let mut context = module.make_context();
    let sig = module.make_signature();
    let func_id = module
        .declare_function("__zera_init", Linkage::Local, &sig)
        .expect("AOT: cannot declare __zera_init");

    let rt = runtime.declare_in(module, &mut context.func);
    let globals: HashMap<String, GlobalValue> = string_data
        .iter()
        .map(|(s, id)| (s.clone(), module.declare_data_in_func(*id, &mut context.func)))
        .collect();

    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut fn_builder_ctx);
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    for class in &plan.classes {
        let name = class.name.clone();
        let superclass = class.superclass_name.clone();
        let fields: Vec<String> = class.fields.clone();
        let b = &mut builder;
        let (np, nl) = static_str(b, &globals, &name);
        let (sp, sl) = match &superclass {
            Some(s) => static_str(b, &globals, s),
            None => (b.ins().iconst(types::I64, 0), b.ins().iconst(types::I64, 0)),
        };
        call_void(b, &rt, "zera_define_class", &[np, nl, sp, sl]);
        for f in &fields {
            let b = &mut builder;
            let (cn, cl) = static_str(b, &globals, &name);
            let (fp, fl) = static_str(b, &globals, f);
            call_void(b, &rt, "zera_class_add_field", &[cn, cl, fp, fl]);
        }
    }

    for (i, entry) in plan.entries.iter().enumerate() {
        let (Some(class), Some(method)) = (&entry.class, &entry.method) else { continue };
        let (class, method, arity) = (class.clone(), method.clone(), entry.arity);
        let b = &mut builder;
        let fr = module.declare_func_in_func(plan.ids[i], &mut b.func);
        let ptr = b.ins().func_addr(types::I64, fr);
        let (cn, cl) = static_str(b, &globals, &class);
        let (mn, ml) = static_str(b, &globals, &method);
        let ar = b.ins().iconst(types::I64, arity as i64);
        call_void(b, &rt, "zera_register_method", &[cn, cl, mn, ml, ptr, ar]);
    }

    for (name, &i) in &plan.named {
        let (name, arity) = (name.clone(), plan.entries[i].param_count);
        let b = &mut builder;
        let fr = module.declare_func_in_func(plan.ids[i], &mut b.func);
        let ptr = b.ins().func_addr(types::I64, fr);
        let (np, nl) = static_str(b, &globals, &name);
        let ar = b.ins().iconst(types::I64, arity as i64);
        call_void(b, &rt, "zera_register_function", &[np, nl, ptr, ar]);
    }

    call_void(&mut builder, &rt, "zera_link_classes", &[]);
    builder.ins().return_(&[]);
    builder.finalize();

    module.define_function(func_id, &mut context).expect("AOT: failed to define __zera_init");
    module.clear_context(&mut context);
    func_id
}
