// src/repl.rs
// Interactive REPL for Zeralang.
// Type expressions to see their values. Multi-line blocks supported.

use crate::{Environment, Stmt, Value, lex_and_parse};
use std::io::{self, Write};

pub fn run() {
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut env = Environment::new();
    let mut buffer = String::new();

    println!("Zeralang REPL v0.1");
    println!("Type 'exit' or press Ctrl+D to leave.");
    println!();

    loop {
        // Show different prompt when waiting for more input
        let prompt = if buffer.is_empty() { "zera> " } else { "  ...> " };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        // Read a line
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => { println!(); break; }  // Ctrl+D / EOF
            Ok(_) => {}
            Err(e) => { eprintln!("Input error: {}", e); break; }
        }

        let trimmed = input.trim();

        // Exit commands (only at top level, not mid-block)
        if buffer.is_empty() {
            if trimmed == "exit" || trimmed == "quit" {
                break;
            }
            // Skip empty lines at top level
            if trimmed.is_empty() {
                continue;
            }
        }

        // Accumulate input
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&input);

        // Check if we need more lines (unclosed braces/blocks)
        if is_incomplete(&buffer) {
            continue;
        }

        // We have a complete input — parse and execute
        let source = buffer.clone();
        buffer.clear();

        // Catch panics so the REPL doesn't die on errors
        let result = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| {
                let ast = lex_and_parse(&source);

                // If it's a single bare expression, print its value
                if ast.len() == 1 {
                    if let Stmt::ExprStmt(expr) = &ast[0] {
                        let val = env.evaluate_expression(expr);
                        // Don't print null (e.g. from function calls returning nothing)
                        if val != Value::Null {
                            println!("{}", val);
                        }
                    } else {
                        env.execute_block(&ast);
                    }
                } else {
                    env.execute_block(&ast);
                }
            })
        );

        if let Err(e) = result {
            let msg = e.downcast_ref::<&str>()
                .map(|s| *s)
                .or_else(|| e.downcast_ref::<String>()
                    .map(|s| s.as_str()))
                .unwrap_or("Unknown error");
            eprintln!("{}", msg);    // <-- was: eprintln!("Error: {}", msg);
        }
    }
    std::panic::set_hook(old_hook);
    println!("Goodbye!");
}

/// Heuristic: check if the input has unclosed braces or blocks.
/// Returns true if more input is needed.
fn is_incomplete(source: &str) -> bool {
    let mut braces = 0i32;
    let mut blocks = 0i32;

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip comment lines
        if trimmed.starts_with("//") || trimmed.starts_with("#") {
            continue;
        }

        // Count braces
        for ch in trimmed.chars() {
            match ch {
                '{' => braces += 1,
                '}' => braces -= 1,
                _ => {}
            }
        }

        // Count block keywords (then/as open, end closes)
        for word in trimmed.split_whitespace() {
            match word.to_lowercase().as_str() {
                "then" | "as" => blocks += 1,
                "end" => blocks -= 1,
                _ => {}
            }
        }
    }

    // Unclosed braces or blocks
    if braces > 0 || blocks > 0 {
        return true;
    }

    // Check if the last word suggests more input is needed
    let last_word = source.lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .and_then(|l| l.trim().split_whitespace().last())
        .map(|w| w.to_lowercase())
        .unwrap_or_default();

    matches!(last_word.as_str(),
        "then" | "as" | "otherwise" | "else" | "taking" | "and" | "with"
    )
}