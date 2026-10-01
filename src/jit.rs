
use std::collections::HashMap;
use cranelift::prelude::*;
use cranelift::codegen::settings;
use cranelift::codegen::ir::{AbiParam, FuncRef};
use cranelift_module::{Linkage, Module, FuncId};
use cranelift_jit::{JITBuilder, JITModule};
use crate::vm::{Compiler, OpCode, Chunk};

pub fn compile_and_run(statements: &[crate::Stmt]) {
    let chunk = Compiler::new().compile(statements);

    let target = target_lexicon::Triple::host();
    let mut isa_builder = settings::builder();
    isa_builder.set("is_pic", "false").unwrap();
    isa_builder.set("opt_level", "speed").unwrap();
    let flags = settings::Flags::new(isa_builder);
    let isa = cranelift::codegen::isa::lookup(target)
        .expect("Failed to look up ISA")
        .finish(flags)
        .expect("Failed to finish ISA");

    let builder = JITBuilder::with_isa(isa, Box::new(cranelift_module::default_libcall_names()));
    let mut module = JITModule::new(builder);

    let mut func_map: HashMap<String, FuncId> = HashMap::new();
    let mut method_map: HashMap<(String, String), FuncId> = HashMap::new();

    let mut class_map: HashMap<String, std::rc::Rc<crate::vm::VmClass>> = HashMap::new();
    let mut func_values: HashMap<String, i64> = HashMap::new();

    for op in &chunk.code {
        if let OpCode::Constant(idx) = op {
            match &chunk.constants[*idx] {

                crate::Value::Bytecode(func_chunk) => {
                    let mut name = String::new();
                    for i in 0..chunk.code.len() {
                        if i > 0 && chunk.code[i - 1] == OpCode::Constant(*idx) {
                            if let OpCode::SetGlobal(n) = &chunk.code[i] {
                                name = n.to_string();
                                break;
                            }
                        }
                    }
                    if !name.is_empty() {
                        let mut sig = module.make_signature();
                        for _ in 0..func_chunk.param_count {
                            sig.params.push(AbiParam::new(types::I64));
                        }
                        sig.returns.push(AbiParam::new(types::I64));
                        let func_id = module.declare_function(&name, Linkage::Local, &sig).expect("Failed to declare function");
                        func_map.insert(name.clone(), func_id);

                        let arena_idx = crate::alloc_value(chunk.constants[*idx].clone());
                        func_values.insert(name, arena_idx);
                    }
                }

                crate::Value::VmClass(vm_class) => {
                    class_map.insert(vm_class.name.clone(), vm_class.clone());

                    if let Some(cons_chunk) = &vm_class.constructor {
                        let mut sig = module.make_signature();
                        for _ in 0..(cons_chunk.param_count + 1) {
                            sig.params.push(AbiParam::new(types::I64));
                        }
                        sig.returns.push(AbiParam::new(types::I64));
                        let full_name = format!("__method_{}_construct", vm_class.name);
                        let func_id = module.declare_function(&full_name, Linkage::Local, &sig).expect("Failed to declare constructor");
                        method_map.insert((vm_class.name.clone(), "construct".to_string()), func_id);
                    }

                    for (mname, mchunk) in &vm_class.methods {
                        let mut sig = module.make_signature();
                        for _ in 0..(mchunk.param_count + 1) {
                            sig.params.push(AbiParam::new(types::I64));
                        }
                        sig.returns.push(AbiParam::new(types::I64));
                        let full_name = format!("__method_{}_{}", vm_class.name, mname);
                        let func_id = module.declare_function(&full_name, Linkage::Local, &sig).expect("Failed to declare method");
                        method_map.insert((vm_class.name.clone(), mname.clone()), func_id);
                    }
                }
                _ => {}
            }
        }
    }

    for op in &chunk.code {
        if let OpCode::Constant(idx) = op {
            match &chunk.constants[*idx] {
                crate::Value::Bytecode(func_chunk) => {
                    let mut name = String::new();
                    for i in 0..chunk.code.len() {
                        if i > 0 && chunk.code[i - 1] == OpCode::Constant(*idx) {
                            if let OpCode::SetGlobal(n) = &chunk.code[i] {
                                name = n.to_string();
                                break;
                            }
                        }
                    }
                    if !name.is_empty() {
                        compile_chunk(&mut module, &name, func_chunk, &func_map, &func_values, false);
                    }
                }
                crate::Value::VmClass(vm_class) => {

                    if let Some(cons_chunk) = &vm_class.constructor {
                        let full_name = format!("__method_{}_construct", vm_class.name);
                        compile_chunk(&mut module, &full_name, cons_chunk, &func_map, &func_values, true);
                    }

                    for (mname, mchunk) in &vm_class.methods {
                        let full_name = format!("__method_{}_{}", vm_class.name, mname);
                        compile_chunk(&mut module, &full_name, mchunk, &func_map, &func_values, true);
                    }
                }
                _ => {}
            }
        }
    }

    let main_id = compile_chunk(&mut module, "main", &chunk, &func_map, &func_values,false);

    module.finalize_definitions();

    for ((class_name, method_name), &func_id) in &method_map {
        let ptr = module.get_finalized_function(func_id);
        crate::METHOD_PTRS.with(|m| {
            m.borrow_mut().insert((class_name.clone(), method_name.clone()), ptr as i64);
        });
    }

    crate::METHOD_PTRS.with(|m| {
        let mut map = m.borrow_mut();
        let mut inserts: Vec<((String, String), i64)> = Vec::new();

        for (cname, class) in &class_map {

            let mut chain: Vec<std::rc::Rc<crate::vm::VmClass>> = Vec::new();
            let mut cur = Some(class.clone());
            let mut guard = 0;
            while let Some(c) = cur {
                cur = c
                    .superclass_name
                    .as_ref()
                    .and_then(|n| class_map.get(n).cloned());
                chain.push(c);
                guard += 1;
                if guard > 10_000 {
                    break;
                }
            }

            let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
            for c in &chain {
                for k in c.methods.keys() {
                    names.insert(k.clone());
                }
            }

            for mname in names {
                let key = (cname.clone(), mname.clone());
                if map.contains_key(&key) {
                    continue;
                }

                for a in &chain {
                    if let Some(&ptr) = map.get(&(a.name.clone(), mname.clone())) {
                        inserts.push((key, ptr));
                        break;
                    }
                }
            }
        }

        for (k, v) in inserts {
            map.entry(k).or_insert(v);
        }
    });

    let main_fn_ptr = module.get_finalized_function(main_id);
    let main_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(main_fn_ptr) };
    let result = main_fn();
    println!("JIT Exit Code: {}", result);
}

fn compile_chunk(
    module: &mut JITModule,
    name: &str,
    chunk: &Chunk,
    func_map: &HashMap<String, FuncId>,
    func_values: &HashMap<String, i64>,
    is_method: bool,
) -> FuncId {
    let mut ctx = module.make_context();
    let mut fn_builder_ctx = FunctionBuilderContext::new();

    let total_params = if is_method { chunk.param_count + 1 } else { chunk.param_count };

    for _ in 0..total_params {
        ctx.func.signature.params.push(AbiParam::new(types::I64));
    }
    ctx.func.signature.returns.push(AbiParam::new(types::I64));

    let func_id = if let Some(id) = func_map.get(name) {
        *id
    } else {
        let linkage = if name == "main" { Linkage::Export } else { Linkage::Local };
        module.declare_function(name, linkage, &ctx.func.signature).expect("Failed to declare function")
    };

    let mut func_refs: HashMap<String, FuncRef> = HashMap::new();
    for (fname, fid) in func_map {
        let func_ref = module.declare_func_in_func(*fid, &mut ctx.func);
        func_refs.insert(fname.clone(), func_ref);
    }

    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fn_builder_ctx);
    let mut catch_block_params: std::collections::HashMap<usize, (Block, usize)> = std::collections::HashMap::new();

    let mut jump_targets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for op in &chunk.code {
        match op {
            OpCode::Jump(t) | OpCode::JumpIfFalse(t) => { jump_targets.insert(*t); }
            _ => {}
        }
    }

    let mut blocks: std::collections::HashMap<usize, Block> = std::collections::HashMap::new();
    for &target in &jump_targets {
        blocks.insert(target, builder.create_block());
    }

    let entry_block = builder.create_block();
    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);
    builder.append_block_params_for_function_params(entry_block);
    blocks.insert(0, entry_block);

    let mut helper_sig = module.make_signature();
    helper_sig.params.push(AbiParam::new(types::I64));
    helper_sig.params.push(AbiParam::new(types::I64));
    helper_sig.returns.push(AbiParam::new(types::I64));
    let helper_sig_ref = builder.import_signature(helper_sig);

    let mut helper1_sig = module.make_signature();
    helper1_sig.params.push(AbiParam::new(types::I64));
    helper1_sig.returns.push(AbiParam::new(types::I64));
    let helper1_sig_ref = builder.import_signature(helper1_sig);

    let mut helper0_ret_sig = module.make_signature();
    helper0_ret_sig.returns.push(AbiParam::new(types::I64));
    let helper0_ret_sig_ref = builder.import_signature(helper0_ret_sig);

    let mut setjmp_sig = module.make_signature();
    setjmp_sig.params.push(AbiParam::new(types::I64));
    setjmp_sig.returns.push(AbiParam::new(types::I32));
    let setjmp_sig_ref = builder.import_signature(setjmp_sig);
    let zera_add_addr = crate::zera_add as *const () as usize as i64;
    let zera_print_addr = crate::zera_print as *const () as usize as i64;

    let mut stack: Vec<Value> = Vec::new();
    let mut var_names: HashMap<String, Variable> = HashMap::new();
    let mut next_var_id: usize = 0;
    let mut pending_call: Option<String> = None;

    for i in 0..total_params {
        let val = builder.block_params(entry_block)[i];
        let var_name = format!("__local_{}", i);
        let var = if let Some(v) = var_names.get(&var_name) {
            *v
        } else {
            let v = Variable::new(next_var_id);
            next_var_id += 1;
            builder.declare_var(v, types::I64);
            var_names.insert(var_name, v);
            v
        };
        builder.def_var(var, val);
    }

    let mut is_terminated = false;
    let mut ip = 0;

    while ip < chunk.code.len() {
        if let Some(&block) = blocks.get(&ip) {
            if ip > 0 && !is_terminated {
                builder.ins().jump(block, &[]);
            }
            builder.switch_to_block(block);
            is_terminated = false;

            if let Some((_, stack_len)) = catch_block_params.get(&ip) {
                let buf_ptr = builder.block_params(block)[0];

                let free_ptr = builder.ins().iconst(types::I64, crate::zera_free_jmpbuf as *const () as usize as i64);
                builder.ins().call_indirect(helper1_sig_ref, free_ptr, &[buf_ptr]);

                let exc_ptr = builder.ins().iconst(types::I64, crate::zera_get_exception as *const () as usize as i64);
                let exc_call = builder.ins().call_indirect(helper0_ret_sig_ref, exc_ptr, &[]);
                let exc_val = builder.inst_results(exc_call)[0];

                stack.truncate(*stack_len);
                stack.push(exc_val);
            }
        } else if is_terminated {
            ip += 1;
            continue;
        }

        let op = &chunk.code[ip];
        match op {
            OpCode::Try(catch_ip) => {
                let stack_len_at_try = stack.len();

                let alloc_ptr = builder.ins().iconst(types::I64, crate::zera_alloc_jmpbuf as *const () as usize as i64);
                let alloc_call = builder.ins().call_indirect(helper0_ret_sig_ref, alloc_ptr, &[]);
                let buf_ptr = builder.inst_results(alloc_call)[0];

                let setjmp_addr = builder.ins().iconst(types::I64, crate::zera_get_setjmp_addr() as i64);
                let setjmp_call = builder.ins().call_indirect(setjmp_sig_ref, setjmp_addr, &[buf_ptr]);
                let ret_val = builder.inst_results(setjmp_call)[0];

                let zero_i32 = builder.ins().iconst(types::I32, 0);
                let is_exc = builder.ins().icmp(IntCC::NotEqual, ret_val, zero_i32);

                let setup_block = builder.create_block();
                builder.append_block_param(setup_block, types::I64);

                let catch_block = blocks.get(catch_ip).copied().unwrap_or_else(|| {
                    let b = builder.create_block();
                    blocks.insert(*catch_ip, b);
                    b
                });
                builder.append_block_param(catch_block, types::I64);
                catch_block_params.insert(*catch_ip, (catch_block, stack_len_at_try));

                builder.ins().brif(is_exc, catch_block, &[buf_ptr], setup_block, &[buf_ptr]);
                is_terminated = true;

                builder.switch_to_block(setup_block);
                builder.seal_block(setup_block);
                let bp = builder.block_params(setup_block)[0];
                let push_ptr = builder.ins().iconst(types::I64, crate::zera_push_jmpbuf as *const () as usize as i64);
                builder.ins().call_indirect(helper1_sig_ref, push_ptr, &[bp]);

                let try_block = blocks.get(&(ip + 1)).copied().unwrap_or_else(|| {
                    let b = builder.create_block();
                    blocks.insert(ip + 1, b);
                    b
                });
                builder.ins().jump(try_block, &[]);
            }
            OpCode::PopTry => {
                let pop_ptr = builder.ins().iconst(types::I64, crate::zera_pop_jmpbuf as *const () as usize as i64);
                builder.ins().call_indirect(helper0_ret_sig_ref, pop_ptr, &[]);
            }
            OpCode::Throw => {
                let val = stack.pop().unwrap();
                let throw_ptr = builder.ins().iconst(types::I64, crate::zera_throw as *const () as usize as i64);
                let _call_inst = builder.ins().call_indirect(helper1_sig_ref, throw_ptr, &[val]);
                builder.ins().trap(TrapCode::UnreachableCodeReached);
                is_terminated = true;
            }
            OpCode::Constant(idx) => {
                match &chunk.constants[*idx] {
                    crate::Value::Number(n) => {
                        let tagged = (*n as i64) << 1;
                        stack.push(builder.ins().iconst(types::I64, tagged));
                    }
                    crate::Value::String(s) => {
                        let arena_idx = crate::alloc_value(crate::Value::String(s.clone()));
                        stack.push(builder.ins().iconst(types::I64, arena_idx));
                    }
                    crate::Value::Boolean(b) => {
                        let tagged = if *b { 2 } else { 0 };
                        stack.push(builder.ins().iconst(types::I64, tagged));
                    }
                    crate::Value::Null => {
                        stack.push(builder.ins().iconst(types::I64, 1));
                    }
                    crate::Value::Bytecode(_) => {

                        let arena_idx = crate::alloc_value(chunk.constants[*idx].clone());
                        stack.push(builder.ins().iconst(types::I64, arena_idx));
                    }
                    _ => {
                        let arena_idx = crate::alloc_value(chunk.constants[*idx].clone());
                        stack.push(builder.ins().iconst(types::I64, arena_idx));
                    }
                }
            }
            OpCode::Print => {
                let val = stack.pop().unwrap();
                let ptr = builder.ins().iconst(types::I64, zera_print_addr);
                builder.ins().call_indirect(helper1_sig_ref, ptr, &[val]);
            }
            OpCode::Add => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();

                let one = builder.ins().iconst(types::I64, 1);
                let zero = builder.ins().iconst(types::I64, 0);
                let or_val = builder.ins().bor(l, r);
                let tag = builder.ins().band(or_val, one);
                let both_num = builder.ins().icmp(IntCC::Equal, tag, zero);

                let fast_block = builder.create_block();
                let slow_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, types::I64);

                builder.ins().brif(both_num, fast_block, &[], slow_block, &[]);

                builder.switch_to_block(fast_block);
                let sum = builder.ins().iadd(l, r);
                builder.ins().jump(merge_block, &[sum]);

                builder.switch_to_block(slow_block);
                let ptr = builder.ins().iconst(types::I64, zera_add_addr);
                let call_inst = builder.ins().call_indirect(helper_sig_ref, ptr, &[l, r]);
                let result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[result]);

                builder.switch_to_block(merge_block);
                builder.seal_block(fast_block);
                builder.seal_block(slow_block);
                builder.seal_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                stack.push(result);
            }
            OpCode::Sub => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let result = builder.ins().isub(l, r);
                stack.push(result);
            }
            OpCode::Mul => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let one = builder.ins().iconst(types::I64, 1);
                let l_val = builder.ins().ushr(l, one);
                let r_val = builder.ins().ushr(r, one);
                let product = builder.ins().imul(l_val, r_val);
                let tagged = builder.ins().ishl(product, one);
                stack.push(tagged);
            }
            OpCode::Div => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let one = builder.ins().iconst(types::I64, 1);
                let l_val = builder.ins().ushr(l, one);
                let r_val = builder.ins().ushr(r, one);
                let quotient = builder.ins().udiv(l_val, r_val);
                let tagged = builder.ins().ishl(quotient, one);
                stack.push(tagged);
            }
            OpCode::Mod => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let one = builder.ins().iconst(types::I64, 1);
                let l_val = builder.ins().ushr(l, one);
                let r_val = builder.ins().ushr(r, one);
                let remainder = builder.ins().urem(l_val, r_val);
                let tagged = builder.ins().ishl(remainder, one);
                stack.push(tagged);
            }
            OpCode::Negate => {
                let val = stack.pop().unwrap();
                let negated = builder.ins().ineg(val);
                stack.push(negated);
            }
            OpCode::Not => {
                let val = stack.pop().unwrap();
                let zero = builder.ins().iconst(types::I64, 0);
                let is_false = builder.ins().icmp(IntCC::Equal, val, zero);
                let is_false_i64 = builder.ins().uextend(types::I64, is_false);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(is_false_i64, one);
                stack.push(tagged);
            }
            OpCode::Less => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::SignedLessThan, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::Greater => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::GreaterEq => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::LessEq => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::Equal => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::Equal, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::NotEqual => {
                let r = stack.pop().unwrap();
                let l = stack.pop().unwrap();
                let cmp = builder.ins().icmp(IntCC::NotEqual, l, r);
                let cmp_i64 = builder.ins().uextend(types::I64, cmp);
                let one = builder.ins().iconst(types::I64, 1);
                let tagged = builder.ins().ishl(cmp_i64, one);
                stack.push(tagged);
            }
            OpCode::BuildArray(count) => {
                for _ in 0..*count {
                    let val = stack.pop().unwrap();
                    let ptr = builder.ins().iconst(types::I64, crate::zera_push_arg as *const () as usize as i64);
                    builder.ins().call_indirect(helper1_sig_ref, ptr, &[val]);
                }
                let count_val = builder.ins().iconst(types::I64, *count as i64);
                let ptr = builder.ins().iconst(types::I64, crate::zera_build_array as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(helper1_sig_ref, ptr, &[count_val]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::BuildDict(count) => {
                for _ in 0..*count * 2 {
                    let val = stack.pop().unwrap();
                    let ptr = builder.ins().iconst(types::I64, crate::zera_push_arg as *const () as usize as i64);
                    builder.ins().call_indirect(helper1_sig_ref, ptr, &[val]);
                }
                let count_val = builder.ins().iconst(types::I64, *count as i64);
                let ptr = builder.ins().iconst(types::I64, crate::zera_build_dict as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(helper1_sig_ref, ptr, &[count_val]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::IndexGet => {
                let idx = stack.pop().unwrap();
                let obj = stack.pop().unwrap();
                let ptr = builder.ins().iconst(types::I64, crate::zera_index_get as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(helper_sig_ref, ptr, &[obj, idx]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::IndexSet => {
                let val = stack.pop().unwrap();
                let idx = stack.pop().unwrap();
                let obj = stack.pop().unwrap();
                let mut sig3 = module.make_signature();
                sig3.params.push(AbiParam::new(types::I64));
                sig3.params.push(AbiParam::new(types::I64));
                sig3.params.push(AbiParam::new(types::I64));
                sig3.returns.push(AbiParam::new(types::I64));
                let sig3_ref = builder.import_signature(sig3);
                let ptr = builder.ins().iconst(types::I64, crate::zera_index_set as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(sig3_ref, ptr, &[obj, idx, val]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::BuiltinCall(name, count) => {
                for _ in 0..*count {
                    let val = stack.pop().unwrap();
                    let ptr = builder.ins().iconst(types::I64, crate::zera_push_arg as *const () as usize as i64);
                    builder.ins().call_indirect(helper1_sig_ref, ptr, &[val]);
                }
                let name_tagged = crate::alloc_value(crate::Value::String(name.to_string()));
                let name_val = builder.ins().iconst(types::I64, name_tagged);
                let count_val = builder.ins().iconst(types::I64, *count as i64);
                let ptr = builder.ins().iconst(types::I64, crate::zera_builtin_call as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(helper_sig_ref, ptr, &[name_val, count_val]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::CallMethod(name, arg_count) => {

                let mut args = Vec::new();
                for _ in 0..*arg_count {
                    args.insert(0, stack.pop().unwrap());
                }

                let self_val = stack.pop().unwrap();

                let name_tagged = crate::alloc_value(crate::Value::String(name.to_string()));
                let name_val = builder.ins().iconst(types::I64, name_tagged);

                let ptr = builder.ins().iconst(types::I64, crate::zera_get_method_ptr as *const () as usize as i64);
                let call_inst = builder.ins().call_indirect(helper_sig_ref, ptr, &[self_val, name_val]);
                let func_ptr = builder.inst_results(call_inst)[0];

                let zero = builder.ins().iconst(types::I64, 0);
                let is_native = builder.ins().icmp(IntCC::NotEqual, func_ptr, zero);

                let native_block = builder.create_block();
                let hybrid_block = builder.create_block();
                let merge_block = builder.create_block();
                builder.append_block_param(merge_block, types::I64);

                builder.ins().brif(is_native, native_block, &[], hybrid_block, &[]);

                builder.switch_to_block(native_block);
                let mut all_args = vec![self_val];
                all_args.extend(args.clone());

                let mut method_sig = module.make_signature();
                for _ in 0..(*arg_count + 1) {
                    method_sig.params.push(AbiParam::new(types::I64));
                }
                method_sig.returns.push(AbiParam::new(types::I64));
                let method_sig_ref = builder.import_signature(method_sig);
                let method_inst = builder.ins().call_indirect(method_sig_ref, func_ptr, &all_args);
                let result = builder.inst_results(method_inst)[0];
                builder.ins().jump(merge_block, &[result]);

                builder.switch_to_block(hybrid_block);

                for arg in &args {
                    let ptr = builder.ins().iconst(types::I64, crate::zera_push_arg as *const () as usize as i64);
                    builder.ins().call_indirect(helper1_sig_ref, ptr, &[*arg]);
                }
                let count_val = builder.ins().iconst(types::I64, *arg_count as i64);
                let ptr = builder.ins().iconst(types::I64, crate::zera_call_method as *const () as usize as i64);

                let mut call_method_sig = module.make_signature();
                call_method_sig.params.push(AbiParam::new(types::I64));
                call_method_sig.params.push(AbiParam::new(types::I64));
                call_method_sig.params.push(AbiParam::new(types::I64));
                call_method_sig.returns.push(AbiParam::new(types::I64));
                let call_method_sig_ref = builder.import_signature(call_method_sig);

                let call_inst = builder.ins().call_indirect(call_method_sig_ref, ptr, &[self_val, name_val, count_val]);
                let result = builder.inst_results(call_inst)[0];
                builder.ins().jump(merge_block, &[result]);

                builder.switch_to_block(merge_block);
                builder.seal_block(native_block);
                builder.seal_block(hybrid_block);
                builder.seal_block(merge_block);
                let result = builder.block_params(merge_block)[0];
                stack.push(result);
            }
            OpCode::Call(arg_count) => {
                let func_name = pending_call.take();

                if let Some(name) = func_name {
                    if let Some(func_ref) = func_refs.get(&name) {
                        let mut args = Vec::new();
                        for _ in 0..*arg_count {
                            args.insert(0, stack.pop().unwrap());
                        }
                        let call_inst = builder.ins().call(*func_ref, &args);
                        let return_val = builder.inst_results(call_inst)[0];
                        stack.push(return_val);
                        ip += 1;
                        continue;
                    }
                }

                let callee = stack.pop().unwrap();
                for _ in 0..*arg_count {
                    let val = stack.pop().unwrap();
                    let ptr = builder.ins().iconst(
                        types::I64,
                        crate::zera_push_arg as *const () as usize as i64,
                    );
                    builder.ins().call_indirect(helper1_sig_ref, ptr, &[val]);
                }
                let count_val = builder.ins().iconst(types::I64, *arg_count as i64);
                let ptr = builder.ins().iconst(
                    types::I64,
                    crate::zera_call_function as *const () as usize as i64,
                );
                let call_inst = builder.ins().call_indirect(helper_sig_ref, ptr, &[callee, count_val]);
                let result = builder.inst_results(call_inst)[0];
                stack.push(result);
            }
            OpCode::Pop => { stack.pop(); }
            OpCode::Jump(target) => {
                let block = blocks.get(target).copied().unwrap_or_else(|| {
                    let b = builder.create_block();
                    blocks.insert(*target, b);
                    b
                });
                builder.ins().jump(block, &[]);
                is_terminated = true;
            }
            OpCode::JumpIfFalse(target) => {
                let cond_val = stack.pop().unwrap();
                let zero = builder.ins().iconst(types::I64, 0);
                let is_true = builder.ins().icmp(IntCC::NotEqual, cond_val, zero);
                let next_block = blocks.get(&(ip + 1)).copied().unwrap_or_else(|| {
                    let b = builder.create_block();
                    blocks.insert(ip + 1, b);
                    b
                });
                let target_block = blocks.get(target).copied().unwrap_or_else(|| {
                    let b = builder.create_block();
                    blocks.insert(*target, b);
                    b
                });
                builder.ins().brif(is_true, next_block, &[], target_block, &[]);
                is_terminated = true;
            }
            OpCode::GetGlobal(name) => {
                if func_map.contains_key(&**name) {

                    let is_call_next = matches!(chunk.code.get(ip + 1), Some(OpCode::Call(_)));
                    if is_call_next {
                        pending_call = Some(name.to_string());
                    } else if let Some(&arena_idx) = func_values.get(&**name) {

                        stack.push(builder.ins().iconst(types::I64, arena_idx));
                    }
                } else {
                    let var = *var_names.get(&**name).unwrap_or_else(|| panic!("JIT: Undefined global '{}'", name));
                    stack.push(builder.use_var(var));
                }
            }
            OpCode::SetGlobal(name) => {
                if !func_map.contains_key(&**name) {
                    let val = stack.pop().unwrap();
                    let var = if let Some(v) = var_names.get(&**name) {
                        *v
                    } else {
                        let v = Variable::new(next_var_id);
                        next_var_id += 1;
                        builder.declare_var(v, types::I64);
                        var_names.insert(name.to_string(), v);
                        v
                    };
                    builder.def_var(var, val);
                }
            }
            OpCode::GetLocal(idx) => {
                let var_name = format!("__local_{}", idx);
                let var = if let Some(v) = var_names.get(&var_name) {
                    *v
                } else {
                    let v = Variable::new(next_var_id);
                    next_var_id += 1;
                    builder.declare_var(v, types::I64);
                    var_names.insert(var_name, v);
                    v
                };
                stack.push(builder.use_var(var));
            }

            OpCode::SetLocal(idx) => {
                let val = stack.pop().unwrap();
                let var_name = format!("__local_{}", idx);
                let var = if let Some(v) = var_names.get(&var_name) {
                    *v
                } else {
                    let v = Variable::new(next_var_id);
                    next_var_id += 1;
                    builder.declare_var(v, types::I64);
                    var_names.insert(var_name, v);
                    v
                };
                builder.def_var(var, val);
            }
            OpCode::Return => {
                let val = stack.pop().unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
                builder.ins().return_(&[val]);
                is_terminated = true;
            }

        }
        ip += 1;
    }

    if !is_terminated {
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[zero]);
    }

    for (_, block) in &blocks {
        builder.seal_block(*block);
    }

    builder.finalize();
    module.define_function(func_id, &mut ctx).expect("Failed to define function");
    module.clear_context(&mut ctx);
    func_id
}
