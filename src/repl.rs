// src/repl.rs
// Interactive REPL for Zeralang.

use crate::{Environment, Stmt, Value, lex_and_parse};
use std::io::{self, Write};

/// Runs the interactive Read-Eval-Print Loop (REPL).
/// Catches panics so runtime errors don't terminate the session.
pub fn run() {
    // Override the panic hook to suppress default Rust panic output in the REPL.
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut env = Environment::new();
    let mut buffer = String::new();

    println!("Zeralang REPL v1.0");
    println!("Type 'exit' or press Ctrl+D to leave.");
    println!();

    loop {
        let prompt = if buffer.is_empty() {
            "zera> "
        } else {
            "  ...> "
        };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                println!();
                break; // EOF (Ctrl+D)
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        }

        let trimmed = input.trim();

        // Exit commands only apply when at the top level
        if buffer.is_empty() {
            if trimmed == "exit" || trimmed == "quit" {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }
        }

        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&input);

        // If input is syntactically incomplete, wait for the next line
        if is_incomplete(&buffer) {
            continue;
        }

        let source = buffer.clone();
        buffer.clear();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ast = lex_and_parse(&source);

            // Bare expressions evaluate and print their result, mimicking Python's REPL
            if ast.len() == 1 {
                if let Stmt::ExprStmt(expr) = &ast[0] {
                    let val = env.evaluate_expression(expr);
                    // Suppress printing Null for statements like function calls
                    if val != Value::Null {
                        println!("{}", val);
                    }
                } else {
                    env.execute_block(&ast);
                }
            } else {
                env.execute_block(&ast);
            }
        }));

        if let Err(e) = result {
            let msg = e
                .downcast_ref::<&str>()
                .map(|s| *s)
                .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("Unknown error");
            eprintln!("{}", msg);
        }
    }

    std::panic::set_hook(old_hook);
    println!("Goodbye!");
}

/// Heuristic check to determine if more input is needed.
/// Counts unclosed braces and block keywords (`then`/`as` without `end`).
fn is_incomplete(source: &str) -> bool {
    let mut braces = 0i32;
    let mut blocks = 0i32;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("//") || trimmed.starts_with("#") {
            continue;
        }

        for ch in trimmed.chars() {
            match ch {
                '{' => braces += 1,
                '}' => braces -= 1,
                _ => {}
            }
        }

        for word in trimmed.split_whitespace() {
            match word.to_lowercase().as_str() {
                "then" | "as" => blocks += 1,
                "end" => blocks -= 1,
                _ => {}
            }
        }
    }

    if braces > 0 || blocks > 0 {
        return true;
    }

    let last_word = source
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .and_then(|l| l.trim().split_whitespace().last())
        .map(|w| w.to_lowercase())
        .unwrap_or_default();

    matches!(
        last_word.as_str(),
        "then" | "as" | "otherwise" | "else" | "taking" | "and" | "with"
    )
}
