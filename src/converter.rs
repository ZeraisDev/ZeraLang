// src/converter.rs
// The bi-modal converter — walks the AST and emits source code
// in either read mode (English-like) or write mode (C-like).

use crate::{Expr, Stmt, Token};

/// Convert an AST to read-mode (English-like) source code.
pub fn emit_read(stmts: &[Stmt]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        out.push_str(&stmt_read(stmt, 0));
        out.push('\n');
    }
    out
}

/// Convert an AST to write-mode (C-like) source code.
pub fn emit_write(stmts: &[Stmt]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        out.push_str(&stmt_write(stmt, 0));
        out.push('\n');
    }
    out
}

// ===== STATEMENT EMITTERS =====

fn stmt_read(stmt: &Stmt, indent: usize) -> String {
    let ind = indent_str(indent);
    match stmt {
        Stmt::Import(path) => {
            format!("{}import \"{}\"", ind, path)
        }
        Stmt::Set(name, expr) => {
            format!("{}set {} to {}", ind, name, expr_read(expr))
        }
        Stmt::SetField(target, field, value) => {
            format!("{}set {}.{} to {}", ind, expr_read(target), field, expr_read(value))
        }
        Stmt::Show(expr) => {
            format!("{}show {}", ind, expr_read(expr))
        }
        Stmt::If(cond, then_block, else_block) => {
            let mut out = format!("{}if {} then\n", ind, expr_read(cond));
            out.push_str(&block_read(then_block, indent + 1));
            if !else_block.is_empty() {
                out.push_str(&ind);
                out.push_str("otherwise\n");
                out.push_str(&block_read(else_block, indent + 1));
            }
            out.push_str(&ind);
            out.push_str("end");
            out
        }
        Stmt::Try(try_block, catch_var, catch_block) => {
            let mut out = format!("{}try {{\n", ind);
            out.push_str(&block_read(try_block, indent + 1));
            out.push_str(&ind);
            out.push_str(&format!("}} catch {} {{\n", catch_var));
            out.push_str(&block_read(catch_block, indent + 1));
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::Throw(expr) => {
            format!("{}throw {}", ind, expr_read(expr))
        }
        Stmt::While(cond, body) => {
            let mut out = format!("{}while {} then\n", ind, expr_read(cond));
            out.push_str(&block_read(body, indent + 1));
            out.push_str(&ind);
            out.push_str("end");
            out
        }
        Stmt::ForEach(var, iterable, body) => {
            let mut out = format!("{}for each {} in {} then\n", ind, var, expr_read(iterable));
            out.push_str(&block_read(body, indent + 1));
            out.push_str(&ind);
            out.push_str("end");
            out
        }
        Stmt::Function(name, params, body) => {
            let params_str = params.join(" and ");
            let mut out = if params.is_empty() {
                format!("{}define {} as\n", ind, name)
            } else {
                format!("{}define {} taking {} as\n", ind, name, params_str)
            };
            out.push_str(&block_read(body, indent + 1));
            out.push_str(&ind);
            out.push_str("end");
            out
        }
        Stmt::Return(expr) => {
            format!("{}give back {}", ind, expr_read(expr))
        }
        Stmt::ExprStmt(expr) => {
            format!("{}{}", ind, expr_read(expr))
        }
        Stmt::Break => format!("{}break", ind),
        Stmt::Continue => format!("{}continue", ind),

        // ===== OOP: Class =====
        Stmt::Class(name, superclass, fields, constructor, methods) => {
            let mut out = format!("{}class {}", ind, name);
            if let Some(parent) = superclass {
                out.push_str(&format!(" extends {}", parent));
            }
            out.push('\n');
            for fname in fields {
                out.push_str(&indent_str(indent + 1));
                out.push_str(&format!("field {}\n", fname));
            }
            if let Some((params, body)) = constructor {
                let params_str = params.join(" and ");
                out.push_str(&indent_str(indent + 1));
                if params.is_empty() {
                    out.push_str("construct as\n");
                } else {
                    out.push_str(&format!("construct taking {} as\n", params_str));
                }
                out.push_str(&block_read(body, indent + 2));
                out.push_str(&indent_str(indent + 1));
                out.push_str("end\n");
            }
            for (mname, mparams, mbody) in methods {
                let params_str = mparams.join(" and ");
                out.push_str(&indent_str(indent + 1));
                if mparams.is_empty() {
                    out.push_str(&format!("define {} as\n", mname));
                } else {
                    out.push_str(&format!("define {} taking {} as\n", mname, params_str));
                }
                out.push_str(&block_read(mbody, indent + 2));
                out.push_str(&indent_str(indent + 1));
                out.push_str("end\n");
            }
            out.push_str(&ind);
            out.push_str("end");
            out
        }
    }
}

fn stmt_write(stmt: &Stmt, indent: usize) -> String {
    let ind = indent_str(indent);
    match stmt {
        Stmt::Import(path) => {
            format!("{}import \"{}\"", ind, path)
        }
        Stmt::Set(name, expr) => {
            format!("{}{} = {}", ind, name, expr_write(expr))
        }
        Stmt::SetField(target, field, value) => {
            format!("{}{}.{} = {}", ind, expr_write(target), field, expr_write(value))
        }
        Stmt::Show(expr) => {
            format!("{}show {}", ind, expr_write(expr))
        }
        Stmt::If(cond, then_block, else_block) => {
            let mut out = format!("{}if ({}) {{\n", ind, expr_write(cond));
            out.push_str(&block_write(then_block, indent + 1));
            if !else_block.is_empty() {
                out.push_str(&ind);
                out.push_str("} else {\n");
                out.push_str(&block_write(else_block, indent + 1));
            }
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::While(cond, body) => {
            let mut out = format!("{}while ({}) {{\n", ind, expr_write(cond));
            out.push_str(&block_write(body, indent + 1));
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::Try(try_block, catch_var, catch_block) => {
            let mut out = format!("{}try {{\n", ind);
            out.push_str(&block_write(try_block, indent + 1));
            out.push_str(&ind);
            out.push_str(&format!("}} catch {} {{\n", catch_var));
            out.push_str(&block_write(catch_block, indent + 1));
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::Throw(expr) => {
            format!("{}throw {}", ind, expr_write(expr))
        }
        Stmt::ForEach(var, iterable, body) => {
            let mut out = format!("{}for each {} in {} {{\n", ind, var, expr_write(iterable));
            out.push_str(&block_write(body, indent + 1));
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::Function(name, params, body) => {
            let params_str = params.join(", ");
            let mut out = format!("{}func {}({}) {{\n", ind, name, params_str);
            out.push_str(&block_write(body, indent + 1));
            out.push_str(&ind);
            out.push_str("}");
            out
        }
        Stmt::Return(expr) => {
            format!("{}return {}", ind, expr_write(expr))
        }
        Stmt::ExprStmt(expr) => {
            format!("{}{}", ind, expr_write(expr))
        }
        Stmt::Break => format!("{}break", ind),
        Stmt::Continue => format!("{}continue", ind),

        // ===== OOP: Class =====
        Stmt::Class(name, superclass, fields, constructor, methods) => {
            let mut out = format!("{}class {}", ind, name);
            if let Some(parent) = superclass {
                out.push_str(&format!(" extends {}", parent));
            }
            out.push_str(" {\n");
            for fname in fields {
                out.push_str(&indent_str(indent + 1));
                out.push_str(&format!("field {}\n", fname));
            }
            if let Some((params, body)) = constructor {
                let params_str = params.join(", ");
                out.push_str(&indent_str(indent + 1));
                out.push_str(&format!("construct({}) {{\n", params_str));
                out.push_str(&block_write(body, indent + 2));
                out.push_str(&indent_str(indent + 1));
                out.push_str("}\n");
            }
            for (mname, mparams, mbody) in methods {
                let params_str = mparams.join(", ");
                out.push_str(&indent_str(indent + 1));
                out.push_str(&format!("func {}({}) {{\n", mname, params_str));
                out.push_str(&block_write(mbody, indent + 2));
                out.push_str(&indent_str(indent + 1));
                out.push_str("}\n");
            }
            out.push_str(&ind);
            out.push_str("}");
            out
        }
    }
}

fn block_read(stmts: &[Stmt], indent: usize) -> String {
    let mut out = String::new();
    for stmt in stmts {
        out.push_str(&stmt_read(stmt, indent));
        out.push('\n');
    }
    out
}

fn block_write(stmts: &[Stmt], indent: usize) -> String {
    let mut out = String::new();
    for stmt in stmts {
        out.push_str(&stmt_write(stmt, indent));
        out.push('\n');
    }
    out
}

// ===== EXPRESSION EMITTERS =====

fn expr_read(expr: &Expr) -> String {
    match expr {
        Expr::Ternary(cond, then_expr, else_expr) => {
            format!("{} ? {} : {}", expr_read(cond), expr_read(then_expr), expr_read(else_expr))
        }
        Expr::Lambda(params, body) => {
            let params_str = params.join(", ");
            let body_str = block_write(body, 1);
            format!("func({}) {{\n{}}}", params_str, body_str)
        }
        Expr::Number(n) => n.to_string(),
        Expr::String(s) => format!("\"{}\"", escape_string(s)),
        Expr::Boolean(b) => b.to_string(),
        Expr::Null => "null".to_string(),
        Expr::Ident(name) => name.clone(),
        Expr::This => "self".to_string(),

        Expr::BinOp(left, op, right) => {
            let l = wrap_parens_read(left, op, true);
            let r = wrap_parens_read(right, op, false);
            format!("{} {} {}", l, op_read(op), r)
        }

        Expr::Unary(op, expr) => {
            let inner = if matches!(expr.as_ref(), Expr::BinOp(..)) {
                format!("({})", expr_read(expr))
            } else {
                expr_read(expr)
            };
            match op {
                Token::Not => format!("not {}", inner),
                Token::Minus => format!("-{}", inner),
                _ => inner,
            }
        }

        Expr::Call(callee, args) => {
            if let Expr::Ident(name) = callee.as_ref() {
                match args.len() {
                    0 => format!("{}()", name),
                    1 => format!("{} {}", name, expr_read(&args[0])),
                    _ => {
                        let args_str = args.iter()
                            .map(expr_read)
                            .collect::<Vec<_>>()
                            .join(" with ");
                        format!("{} {}", name, args_str)
                    }
                }
            } else {
                let args_str = args.iter().map(expr_read)
                    .collect::<Vec<_>>().join(", ");
                format!("{}({})", expr_read(callee), args_str)
            }
        }

        Expr::Array(elements) => {
            let elems = elements.iter().map(expr_read)
                .collect::<Vec<_>>().join(", ");
            format!("[{}]", elems)
        }

        Expr::Index(obj, index) => {
            if let Expr::String(s) = index.as_ref() {
                format!("{}.{}", expr_read(obj), s)
            } else {
                format!("{}[{}]", expr_read(obj), expr_read(index))
            }
        }

        Expr::Dict(pairs) => {
            let pairs_str = pairs.iter()
                .map(|(k, v)| format!("{}: {}", expr_read(k), expr_read(v)))
                .collect::<Vec<_>>().join(", ");
            format!("{{{}}}", pairs_str)
        }
    }
}

fn expr_write(expr: &Expr) -> String {
    match expr {
        Expr::Ternary(cond, then_expr, else_expr) => {
            format!("{} ? {} : {}", expr_write(cond), expr_write(then_expr), expr_write(else_expr))
        }
        Expr::Lambda(params, body) => {
            let params_str = params.join(", ");
            let body_str = block_write(body, 1);
            format!("func({}) {{\n{}}}", params_str, body_str)
        }
        Expr::Number(n) => n.to_string(),
        Expr::String(s) => format!("\"{}\"", escape_string(s)),
        Expr::Boolean(b) => b.to_string(),
        Expr::Null => "null".to_string(),
        Expr::Ident(name) => name.clone(),
        Expr::This => "self".to_string(),

        Expr::BinOp(left, op, right) => {
            let l = wrap_parens_write(left, op, true);
            let r = wrap_parens_write(right, op, false);
            format!("{} {} {}", l, op_write(op), r)
        }

        Expr::Unary(op, expr) => {
            let inner = if matches!(expr.as_ref(), Expr::BinOp(..)) {
                format!("({})", expr_write(expr))
            } else {
                expr_write(expr)
            };
            match op {
                Token::Not => format!("!{}", inner),
                Token::Minus => format!("-{}", inner),
                _ => inner,
            }
        }

        Expr::Call(callee, args) => {
            let args_str = args.iter().map(expr_write)
                .collect::<Vec<_>>().join(", ");
            format!("{}({})", expr_write(callee), args_str)
        }

        Expr::Array(elements) => {
            let elems = elements.iter().map(expr_write)
                .collect::<Vec<_>>().join(", ");
            format!("[{}]", elems)
        }

        Expr::Index(obj, index) => {
            if let Expr::String(s) = index.as_ref() {
                format!("{}.{}", expr_write(obj), s)
            } else {
                format!("{}[{}]", expr_write(obj), expr_write(index))
            }
        }

        Expr::Dict(pairs) => {
            let pairs_str = pairs.iter()
                .map(|(k, v)| format!("{}: {}", expr_write(k), expr_write(v)))
                .collect::<Vec<_>>().join(", ");
            format!("{{{}}}", pairs_str)
        }
    }
}

// ===== PAREN WRAPPING (for correct precedence) =====

fn wrap_parens_read(child: &Expr, parent_op: &Token, is_left: bool) -> String {
    let needs = if is_left {
        needs_parens_left(child, parent_op)
    } else {
        needs_parens_right(child, parent_op)
    };
    if needs {
        format!("({})", expr_read(child))
    } else {
        expr_read(child)
    }
}

fn wrap_parens_write(child: &Expr, parent_op: &Token, is_left: bool) -> String {
    let needs = if is_left {
        needs_parens_left(child, parent_op)
    } else {
        needs_parens_right(child, parent_op)
    };
    if needs {
        format!("({})", expr_write(child))
    } else {
        expr_write(child)
    }
}

fn needs_parens_left(child: &Expr, parent_op: &Token) -> bool {
    if let Expr::BinOp(_, child_op, _) = child {
        prec(child_op) < prec(parent_op)
    } else {
        false
    }
}

fn needs_parens_right(child: &Expr, parent_op: &Token) -> bool {
    if let Expr::BinOp(_, child_op, _) = child {
        prec(child_op) <= prec(parent_op)
    } else {
        false
    }
}

fn prec(op: &Token) -> u8 {
    match op {
        Token::Or => 1,
        Token::And => 2,
        Token::Greater | Token::GreaterEq | Token::Less
        | Token::LessEq | Token::EqEq | Token::NotEq | Token::Is => 3,
        Token::Plus | Token::Minus => 4,
        Token::Star | Token::Slash | Token::Percent => 5,
        _ => 10,
    }
}

// ===== OPERATOR MAPPING =====
fn op_read(op: &Token) -> &'static str {
    match op {
        Token::Plus => "+",
        Token::Minus => "-",
        Token::Star => "*",
        Token::Slash => "/",
        Token::Percent => "%",
        Token::Greater => "greater than",
        Token::Less => "less than",
        Token::GreaterEq => ">=",
        Token::LessEq => "<=",
        Token::EqEq | Token::Is => "is",
        Token::NotEq => "is not",
        Token::And => "and",
        Token::Or => "or",
        _ => "?",
    }
}

fn op_write(op: &Token) -> &'static str {
    match op {
        Token::Plus => "+",
        Token::Minus => "-",
        Token::Star => "*",
        Token::Slash => "/",
        Token::Percent => "%",
        Token::Greater => ">",
        Token::Less => "<",
        Token::GreaterEq => ">=",
        Token::LessEq => "<=",
        Token::EqEq | Token::Is => "==",
        Token::NotEq => "!=",
        Token::And => "&&",
        Token::Or => "||",
        _ => "?",
    }
}

// ===== HELPERS =====

fn escape_string(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out
}

fn indent_str(level: usize) -> String {
    "    ".repeat(level)
}