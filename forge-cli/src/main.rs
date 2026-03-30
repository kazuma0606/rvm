// forge-cli: ForgeScript CLI
// Phase 2-D 実装

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

use forge_compiler::parser::parse_source;
use forge_vm::interpreter::Interpreter;
use forge_vm::value::Value;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("run") => {
            if let Some(path) = args.get(2) {
                run_file(path);
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge run <file.forge>");
                std::process::exit(1);
            }
        }
        Some("repl") => {
            run_repl();
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
        }
        Some(cmd) => {
            eprintln!("エラー: 不明なコマンド '{}'", cmd);
            eprintln!("ヒント: `forge help` でコマンド一覧を確認できます");
            std::process::exit(1);
        }
        None => {
            print_help();
        }
    }
}

fn run_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            std::process::exit(1);
        }
    };

    let module = match parse_source(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("構文エラー: {}", e);
            std::process::exit(1);
        }
    };

    let mut interp = Interpreter::new();
    if let Err(e) = interp.eval(&module) {
        eprintln!("実行エラー: {}", e);
        std::process::exit(1);
    }
}

fn run_repl() {
    println!("ForgeScript REPL v0.0.1");
    println!("終了: exit と入力するか Ctrl+C を押してください");

    let mut interp = Interpreter::new();
    let stdin = io::stdin();

    loop {
        print!("> ");
        if io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("入力エラー: {}", e);
                break;
            }
        }

        let trimmed = line.trim();
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }

        match parse_source(trimmed) {
            Ok(module) => match interp.eval(&module) {
                Ok(Value::Unit) => {}
                Ok(val) => println!("{}", val),
                Err(e) => eprintln!("エラー: {}", e),
            },
            Err(e) => eprintln!("構文エラー: {}", e),
        }
    }
}

fn print_help() {
    println!("ForgeScript CLI");
    println!();
    println!("使用方法:");
    println!("  forge run <file.forge>  ファイルを読み込んで実行");
    println!("  forge repl              対話型 REPL を起動");
    println!("  forge help              このヘルプを表示");
}
