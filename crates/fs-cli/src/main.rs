//! ForgeScript CLI tool

use fs_compiler::Compiler;
use fs_parser::Parser;
use rvm_runtime::Vm;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        start_repl();
        return;
    }

    let command = &args[1];

    match command.as_str() {
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: forge run <file>");
                process::exit(1);
            }
            let file_path = &args[2];
            run_file(file_path);
        }
        "repl" => {
            start_repl();
        }
        "help" | "--help" | "-h" => {
            print_help();
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("\nRun 'forge help' for usage information");
            process::exit(1);
        }
    }
}

fn print_help() {
    println!("ForgeScript (forge) v0.1.0");
    println!();
    println!("Usage: forge [command] [args]");
    println!();
    println!("Commands:");
    println!("  (no args)         Start interactive REPL");
    println!("  run <file>        Run a ForgeScript file");
    println!("  repl              Start interactive REPL");
    println!("  help              Show this help message");
    println!();
    println!("Examples:");
    println!("  forge                        # Start REPL");
    println!("  forge run program.fs         # Run a file");
    println!("  forge repl                   # Start REPL explicitly");
}

fn start_repl() {
    let mut repl = fs_repl::Repl::new();
    if let Err(e) = repl.run() {
        eprintln!("REPL error: {}", e);
        process::exit(1);
    }
}

fn run_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", path, e);
            process::exit(1);
        }
    };

    let module = match Parser::parse(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    // Create VM and register native functions first
    let mut vm = Vm::new();
    vm.register_std_natives();

    // Get global names (including native functions) for compiler
    let globals = vm.get_global_names();

    let chunk = match Compiler::compile_with_context(&module, globals) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            process::exit(1);
        }
    };

    if let Err(e) = vm.execute(chunk) {
        eprintln!("Runtime error: {}", e);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test() {
        assert!(true);
    }
}
