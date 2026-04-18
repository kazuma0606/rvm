// forge-cli: ForgeScript CLI
// Phase 2-D 実装

mod forge_toml;
mod new;
mod templates;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::forge_toml::{DependencyValue, ForgeToml};
use bloom_compiler::{collect_bloom_files, compile_generated_forge_to_wasm, generated_forge_path, inline_critical_css, preprocess_render_calls, wasm_output_path};
use forge_compiler::ast::{Stmt, UsePath};
use forge_compiler::deps::DepsManager;
use forge_compiler::parser::parse_source;
use forge_compiler::typechecker::type_check_source;
use forge_goblet::{
    analyze_source as goblet_analyze_source,
    expand_closure_details as goblet_expand_closure_details, render_json as goblet_render_json,
    render_mermaid as goblet_render_mermaid, render_text as goblet_render_text, NodeStatus,
    PipelineGraph,
};
use forge_notebook::{
    export_ipynb, load_output, output_path_for, parse_notebook, save_output, Cell, CellOutput,
    KernelClient, NotebookOutput, OutputItem, PipelineTraceCorruption, PipelineTraceStage,
};
use forge_transpiler::transpile;
use forge_vm::interpreter::{
    Interpreter, PipelineTraceEvent, PipelineTraceNodeRef, PipelineTraceOutcome,
};
use forge_vm::value::Value;

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("new") => {
            if args
                .iter()
                .skip(2)
                .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
            {
                print_new_help();
                return;
            }

            let name = args
                .iter()
                .skip(2)
                .find(|arg| !arg.starts_with('-'))
                .map(|s| s.as_str());
            let template = args
                .iter()
                .position(|a| a == "--template")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str())
                .unwrap_or("script");
            let git = args.iter().skip(2).any(|arg| arg == "--git");

            if let Err(e) = new::run_with_options(name, template, git) {
                eprintln!("エラー: {}", e);
                std::process::exit(1);
            }
        }
        Some("run") => {
            run_entry(args.get(2).map(|s| s.as_str()));
        }
        Some("check") => {
            if let Some(path) = args.get(2) {
                check_file(path);
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge check <file.forge>");
                std::process::exit(1);
            }
        }
        Some("transpile") => {
            if let Some(path) = args.get(2) {
                let output = args
                    .iter()
                    .position(|s| s == "-o")
                    .and_then(|i| args.get(i + 1));
                transpile_file(path, output.map(|s| s.as_str()));
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge transpile <file.forge> [-o output.rs]");
                std::process::exit(1);
            }
        }
        Some("build") => {
            let output = args
                .iter()
                .position(|s| s == "-o")
                .and_then(|i| args.get(i + 1));
            let web = args.iter().any(|arg| arg == "--web");
            let path = build_path_arg(&args);
            if web {
                build_web_entry(path, output.map(|s| s.as_str()));
            } else {
                build_entry(path, output.map(|s| s.as_str()));
            }
        }
        Some("goblet") => {
            goblet_entry(&args);
        }
        Some("notebook") => {
            if args.get(2).map(|s| s.as_str()) == Some("export") {
                let Some(path) = args.get(3).map(|s| s.as_str()) else {
                    eprintln!("繧ｨ繝ｩ繝ｼ: 繝輔ぃ繧､繝ｫ繝代せ繧呈欠螳壹＠縺ｦ縺上□縺輔＞");
                    eprintln!("菴ｿ逕ｨ譁ｹ豕・ forge notebook export <file.fnb> --format ipynb");
                    std::process::exit(1);
                };
                let format = args
                    .iter()
                    .position(|arg| arg == "--format")
                    .and_then(|i| args.get(i + 1))
                    .map(|s| s.as_str())
                    .unwrap_or("ipynb");
                let output = args
                    .iter()
                    .position(|arg| arg == "-o" || arg == "--output")
                    .and_then(|i| args.get(i + 1))
                    .map(|s| s.as_str());
                let code = export_notebook(path, format, output);
                if code != 0 {
                    std::process::exit(code);
                }
            } else {
                notebook_entry(&args);
            }
        }
        Some("lsp") => {
            forge_lsp::run_stdio_blocking();
        }
        Some("test") => {
            let filter = args
                .iter()
                .position(|s| s == "--filter")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            test_entry(args.get(2).map(|s| s.as_str()), filter);
        }
        Some("repl") => {
            run_repl();
        }
        Some("version") | Some("--version") | Some("-V") => {
            println!("forge {}", env!("CARGO_PKG_VERSION"));
        }
        Some("mcp") => match args.get(2).map(|s| s.as_str()) {
            None | Some("--stdio") => forge_mcp::run_stdio(),
            Some("--daemon-inner") => forge_mcp::run_daemon_inner(),
            Some("start") => {
                if let Err(e) = forge_mcp::daemon::start() {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            Some("stop") => {
                if let Err(e) = forge_mcp::daemon::stop() {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            Some("restart") => {
                if let Err(e) = forge_mcp::daemon::restart() {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            Some("status") => {
                if let Err(e) = forge_mcp::daemon::status() {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            Some("connect") => forge_mcp::run_stdio(),
            Some("logs") => {
                let follow = args.iter().any(|a| a == "-f");
                let errors_only = args.iter().any(|a| a == "--errors");
                let clear = args.iter().any(|a| a == "--clear");
                if clear {
                    if let Err(e) = forge_mcp::clear_logs() {
                        eprintln!("{}", e);
                        std::process::exit(1);
                    }
                } else if let Err(e) = forge_mcp::show_logs(follow, errors_only) {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            Some(sub) => {
                eprintln!("エラー: 不明な mcp サブコマンド '{}'", sub);
                eprintln!("使用可能: forge mcp [start|stop|restart|status|connect|logs]");
                std::process::exit(1);
            }
        },
        Some("dev") => {
            dev_entry(&args);
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

fn build_path_arg(args: &[String]) -> Option<&str> {
    let mut skip_next = false;
    for arg in args.iter().skip(2) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "-o" || arg == "--output" {
            skip_next = true;
            continue;
        }
        if arg == "--web" {
            continue;
        }
        if !arg.starts_with('-') {
            return Some(arg.as_str());
        }
    }
    None
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

    let file_path = std::path::Path::new(path);
    let mut interp = Interpreter::with_file_path(file_path);
    if let Err(e) = interp.eval(&module) {
        eprintln!("実行エラー: {}", e);
        std::process::exit(1);
    }
}

fn run_entry(path: Option<&str>) {
    match resolve_project_request(path) {
        Ok(ProjectRequest::File(file_path)) => run_file(&file_path.to_string_lossy()),
        Ok(ProjectRequest::Project {
            project_dir,
            forge_toml,
        }) => {
            let entry = project_dir.join(&forge_toml.package.entry);
            let dep_paths = forge_toml.local_dep_paths(&project_dir);
            if dep_paths.is_empty() {
                run_file(&entry.to_string_lossy());
            } else {
                run_file_with_deps(&entry, dep_paths);
            }
        }
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_file_with_deps(entry: &Path, dep_paths: Vec<(String, PathBuf)>) {
    let source = match fs::read_to_string(entry) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "エラー: ファイル '{}' を読み込めませんでした: {}",
                entry.display(),
                e
            );
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

    let project_root = entry
        .parent()
        .and_then(|p| {
            if p.file_name().and_then(|n| n.to_str()) == Some("src") {
                p.parent()
            } else {
                Some(p)
            }
        })
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut interp = Interpreter::with_project_root_and_deps(project_root, dep_paths);
    if let Err(e) = interp.eval(&module) {
        eprintln!("実行エラー: {}", e);
        std::process::exit(1);
    }
}

fn test_file(path: &str, filter: Option<&str>, project_root: Option<&Path>) {
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

    let file_path = std::path::Path::new(path);
    let mut interp = match project_root {
        Some(root) => Interpreter::with_project_root(root.to_path_buf()),
        None => Interpreter::with_file_path(file_path),
    };
    interp.is_test_mode = true;

    let results = interp.run_tests(&module.stmts, filter);

    let total = results.len();
    println!("running {} tests", total);

    let mut passed = 0usize;
    let mut failed = 0usize;

    for result in &results {
        if result.passed {
            println!("  \u{2705} {}", result.name);
            passed += 1;
        } else {
            println!("  \u{274c} {}", result.name);
            if let Some(ref msg) = result.failure_message {
                println!("       {}", msg);
            }
            failed += 1;
        }
    }

    println!();
    if failed == 0 {
        println!("test result: ok. {} passed; 0 failed", passed);
    } else {
        println!("test result: FAILED. {} passed; {} failed", passed, failed);
        std::process::exit(1);
    }
}

fn run_repl() {
    println!("ForgeScript REPL v0.0.1");
    println!("終了: exit と入力するか Ctrl+C を押してください");
    println!("モジュールコマンド: :modules, :reload <path>, :unload <path>");

    let mut interp = Interpreter::new();
    // REPL ではカレントディレクトリをプロジェクトルートとしてモジュールローダーを初期化する（M-7-A）
    interp.init_module_loader_from_cwd();

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

        // REPL 専用コマンド（M-7-A）
        if let Some(cmd_result) = handle_repl_command(trimmed, &mut interp) {
            match cmd_result {
                Ok(msg) => {
                    if !msg.is_empty() {
                        println!("{}", msg);
                    }
                }
                Err(e) => eprintln!("エラー: {}", e),
            }
            continue;
        }

        // `use ...` 文の場合はモジュールロードとして処理してシンボルを記録する（M-7-A）
        match parse_source(trimmed) {
            Ok(module) => {
                // use 文の実行前に imported_symbols のキーを記録する
                let before_keys: std::collections::HashSet<String> =
                    interp.imported_symbols.keys().cloned().collect();

                match interp.eval(&module) {
                    Ok(Value::Unit) => {
                        // use 文で新たに追加されたシンボルを loaded_modules に記録する
                        let has_use_decl = module
                            .stmts
                            .iter()
                            .any(|s| matches!(s, forge_compiler::ast::Stmt::UseDecl { .. }));
                        if has_use_decl {
                            let after_keys: std::collections::HashSet<String> =
                                interp.imported_symbols.keys().cloned().collect();
                            let new_syms: Vec<String> =
                                after_keys.difference(&before_keys).cloned().collect();

                            if !new_syms.is_empty() {
                                // use 文からモジュールパスを取得する
                                for stmt in &module.stmts {
                                    if let forge_compiler::ast::Stmt::UseDecl { path, .. } = stmt {
                                        let mod_path = match path {
                                            forge_compiler::ast::UsePath::Local(p) => p.clone(),
                                            forge_compiler::ast::UsePath::External(p) => p.clone(),
                                            forge_compiler::ast::UsePath::Stdlib(p) => p.clone(),
                                        };
                                        let entry = interp
                                            .loaded_modules
                                            .entry(mod_path.clone())
                                            .or_insert_with(Vec::new);
                                        for sym in &new_syms {
                                            if !entry.contains(sym) {
                                                entry.push(sym.clone());
                                            }
                                        }
                                        println!("✔ {} をロード済み", mod_path);
                                    }
                                }
                            }
                        }
                    }
                    Ok(val) => println!("{}", val),
                    Err(e) => eprintln!("エラー: {}", e),
                }
            }
            Err(e) => eprintln!("構文エラー: {}", e),
        }
    }
}

/// REPL 専用コマンドを処理する（M-7-A）
/// コマンドでない場合は None を返す
fn handle_repl_command(input: &str, interp: &mut Interpreter) -> Option<Result<String, String>> {
    if input == ":modules" {
        // ロード済みモジュール一覧を表示
        if interp.loaded_modules.is_empty() {
            return Some(Ok("ロード済みモジュール: なし".to_string()));
        }
        let mut output = "ロード済みモジュール:".to_string();
        let mut paths: Vec<&String> = interp.loaded_modules.keys().collect();
        paths.sort();
        for path in paths {
            output.push_str(&format!("\n  - {}", path));
        }
        return Some(Ok(output));
    }

    if let Some(rest) = input.strip_prefix(":reload ") {
        let path = rest.trim();
        if path.is_empty() {
            return Some(Err(
                ":reload にはモジュールパスを指定してください".to_string()
            ));
        }
        // キャッシュから削除してアンロード
        interp.unload_module(path);
        // モジュールローダーのキャッシュもクリアする
        interp.clear_module_loader_cache(path);
        // 再度ロードする: use パスとして再評価する
        let use_src = format!("use ./{}.{}", path, "*");
        match parse_source(&use_src) {
            Ok(module) => {
                let before_keys: std::collections::HashSet<String> =
                    interp.imported_symbols.keys().cloned().collect();
                match interp.eval(&module) {
                    Ok(_) => {
                        let after_keys: std::collections::HashSet<String> =
                            interp.imported_symbols.keys().cloned().collect();
                        let new_syms: Vec<String> =
                            after_keys.difference(&before_keys).cloned().collect();
                        if !new_syms.is_empty() {
                            let entry = interp
                                .loaded_modules
                                .entry(path.to_string())
                                .or_insert_with(Vec::new);
                            for sym in new_syms {
                                if !entry.contains(&sym) {
                                    entry.push(sym);
                                }
                            }
                        }
                        Some(Ok(format!("✔ {} を再ロードしました", path)))
                    }
                    Err(e) => Some(Err(format!(
                        "モジュール '{}' の再ロードに失敗しました: {}",
                        path, e
                    ))),
                }
            }
            Err(e) => Some(Err(format!("パースエラー: {}", e))),
        }
    } else if let Some(rest) = input.strip_prefix(":unload ") {
        let path = rest.trim();
        if path.is_empty() {
            return Some(Err(
                ":unload にはモジュールパスを指定してください".to_string()
            ));
        }
        if interp.loaded_modules.contains_key(path) {
            interp.unload_module(path);
            Some(Ok(format!("✔ {} をアンロードしました", path)))
        } else {
            Some(Err(format!("モジュール '{}' はロードされていません", path)))
        }
    } else if input.starts_with(':') {
        // 未知のコマンド
        Some(Err(format!(
            "不明なコマンド '{}'\n利用可能: :modules, :reload <path>, :unload <path>",
            input
        )))
    } else {
        None
    }
}

fn check_file(path: &str) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            std::process::exit(1);
        }
    };

    let errors = type_check_source(&source);
    if errors.is_empty() {
        println!("型チェック: エラーなし");
    } else {
        for err in &errors {
            eprintln!("{}", err);
        }
        std::process::exit(1);
    }
}

fn goblet_entry(args: &[String]) {
    let code = match args.get(2).map(|s| s.as_str()) {
        Some("graph") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!(
                    "使用方法: forge goblet graph <file> [--format text|json|mermaid] [--output <file>] [--function <name>] [--include-closures]"
                );
                std::process::exit(1);
            };
            let format = args
                .iter()
                .position(|arg| arg == "--format")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str())
                .unwrap_or("text");
            let output = args
                .iter()
                .position(|arg| arg == "--output")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            let function = args
                .iter()
                .position(|arg| arg == "--function")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            let include_closures = args.iter().any(|arg| arg == "--include-closures");
            run_goblet_graph(path, format, output, function, include_closures)
        }
        Some("explain") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge goblet explain <file> [--function <name>] [--line N]");
                std::process::exit(1);
            };
            let function = args
                .iter()
                .position(|arg| arg == "--function")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            let line = args
                .iter()
                .position(|arg| arg == "--line")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<usize>().ok());
            run_goblet_explain(path, function, line)
        }
        Some("dump") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge goblet dump <file>");
                std::process::exit(1);
            };
            run_goblet_dump(path)
        }
        Some("export") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("繧ｨ繝ｩ繝ｼ: 繝輔ぃ繧､繝ｫ繝代せ繧呈欠螳壹＠縺ｦ縺上□縺輔＞");
                eprintln!("菴ｿ逕ｨ譁ｹ豕・ forge notebook export <file.fnb> --format ipynb");
                std::process::exit(1);
            };
            let format = args
                .iter()
                .position(|arg| arg == "--format")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str())
                .unwrap_or("ipynb");
            let output = args
                .iter()
                .position(|arg| arg == "-o" || arg == "--output")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.as_str());
            export_notebook(path, format, output)
        }
        _ => {
            eprintln!("エラー: 未知の goblet サブコマンドです");
            eprintln!("使用可能: forge goblet graph / explain / dump");
            std::process::exit(1);
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}

/// 終了コード: 0=正常, 1=型エラーあり, 2=解析失敗
fn notebook_entry(args: &[String]) {
    match args.get(2).map(|s| s.as_str()) {
        Some("--kernel") => {
            forge_notebook::run_kernel_stdio();
        }
        Some("run") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!(
                    "使用方法: forge notebook run <file.fnb> [--cell <name>] [--stop-on-error]"
                );
                std::process::exit(1);
            };

            let cell_filter = args
                .iter()
                .position(|arg| arg == "--cell")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.to_string());
            let stop_on_error = args.iter().any(|arg| arg == "--stop-on-error");

            let code = run_notebook_file(path, cell_filter, stop_on_error);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("reset") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge notebook reset <file.fnb>");
                std::process::exit(1);
            };
            let code = run_notebook_file(path, None, false);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("clear") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge notebook clear <file.fnb>");
                std::process::exit(1);
            };
            let code = clear_notebook_output(path);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("show") => {
            let Some(path) = args.get(3).map(|s| s.as_str()) else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge notebook show <file.fnb>");
                std::process::exit(1);
            };
            let code = show_notebook(path);
            if code != 0 {
                std::process::exit(code);
            }
        }
        _ => {
            eprintln!("エラー: 未対応の notebook サブコマンドです");
            eprintln!("使用可能: forge notebook run / reset / clear / show / --kernel");
            std::process::exit(1);
        }
    }
}

fn run_notebook_file(path: &str, cell_filter: Option<String>, stop_on_error: bool) -> i32 {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            return 1;
        }
    };

    let cells = parse_notebook(&source);
    let mut client = match KernelClient::spawn() {
        Ok(client) => client,
        Err(error) => {
            eprintln!("エラー: notebook kernel を起動できませんでした: {}", error);
            return 1;
        }
    };

    let mut cell_outputs = Vec::new();
    let mut has_error = false;
    for cell in &cells {
        let Cell::Code(code) = cell else {
            continue;
        };
        if cell_filter
            .as_deref()
            .is_some_and(|filter| filter != code.name.as_str())
        {
            continue;
        }
        if code.skip {
            println!("[skipped] {}", code.name);
            cell_outputs.push(CellOutput {
                index: code.index,
                name: code.name.clone(),
                status: "skipped".to_string(),
                outputs: Vec::new(),
                duration_ms: 0,
            });
            continue;
        }

        let response = match client.execute(&code.source) {
            Ok(response) => response,
            Err(error) => {
                eprintln!("エラー: kernel execute に失敗しました: {}", error);
                let _ = client.shutdown();
                return 1;
            }
        };

        match response.status.as_str() {
            "ok" => {
                println!(
                    "[ok] {} ({} ms)",
                    code.name,
                    response.duration_ms.unwrap_or_default()
                );
            }
            "error" => {
                has_error = true;
                println!(
                    "[error] {} ({} ms)",
                    code.name,
                    response.duration_ms.unwrap_or_default()
                );
            }
            other => println!("[{}] {}", other, code.name),
        }

        print_outputs(&response.outputs);

        cell_outputs.push(CellOutput {
            index: code.index,
            name: code.name.clone(),
            status: response.status.clone(),
            outputs: response.outputs.clone(),
            duration_ms: response.duration_ms.unwrap_or_default(),
        });

        if has_error && stop_on_error {
            break;
        }
    }
    let _ = client.shutdown();

    let notebook_output = NotebookOutput {
        version: 1,
        file: Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string(),
        executed_at: chrono::Utc::now(),
        cells: cell_outputs,
    };
    let output_path = output_path_for(Path::new(path));
    if let Err(error) = save_output(&output_path, &notebook_output) {
        eprintln!("エラー: 出力ファイルを書き込めませんでした: {}", error);
        return 1;
    }

    if has_error {
        1
    } else {
        0
    }
}

fn clear_notebook_output(path: &str) -> i32 {
    let output_path = output_path_for(Path::new(path));
    match fs::remove_file(&output_path) {
        Ok(_) => {
            println!("{}", output_path.display());
            0
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => {
            eprintln!(
                "エラー: 出力ファイル '{}' を削除できませんでした: {}",
                output_path.display(),
                error
            );
            1
        }
    }
}

fn show_notebook(path: &str) -> i32 {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(error) => {
            eprintln!(
                "エラー: ファイル '{}' を読み込めませんでした: {}",
                path, error
            );
            return 1;
        }
    };
    let cells = parse_notebook(&source);
    let output_path = output_path_for(Path::new(path));
    let output = load_output(&output_path).ok();
    print!("{}", format_notebook_show(&cells, output.as_ref()));
    0
}

fn export_notebook(path: &str, format: &str, output_override: Option<&str>) -> i32 {
    if format != "ipynb" {
        eprintln!("繧ｨ繝ｩ繝ｼ: 譛ｪ蟇ｾ蠢懊・ export format '{}'", format);
        return 1;
    }

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(error) => {
            eprintln!(
                "繧ｨ繝ｩ繝ｼ: 繝輔ぃ繧､繝ｫ '{}' 繧定ｪｭ縺ｿ霎ｼ繧√∪縺帙ｓ縺ｧ縺励◆: {}",
                path, error
            );
            return 1;
        }
    };

    let cells = parse_notebook(&source);
    let output_path = output_path_for(Path::new(path));
    let notebook_output = load_output(&output_path).ok();
    let exported = export_ipynb(&cells, notebook_output.as_ref());
    let export_path = output_override
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(path).with_extension("ipynb"));
    let content = match serde_json::to_string_pretty(&exported) {
        Ok(content) => content,
        Err(error) => {
            eprintln!("繧ｨ繝ｩ繝ｼ: ipynb JSON 縺ｮ逕滓・縺ｫ螟ｱ謨励＠縺ｾ縺励◆: {}", error);
            return 1;
        }
    };

    match fs::write(&export_path, format!("{}\n", content)) {
        Ok(_) => {
            println!("{}", export_path.display());
            0
        }
        Err(error) => {
            eprintln!(
                "繧ｨ繝ｩ繝ｼ: export 繝輔ぃ繧､繝ｫ '{}' 縺ｮ譖ｸ縺崎ｾｼ縺ｿ縺ｫ螟ｱ謨励＠縺ｾ縺励◆: {}",
                export_path.display(),
                error
            );
            1
        }
    }
}

fn format_notebook_show(cells: &[Cell], output: Option<&NotebookOutput>) -> String {
    let mut lines = Vec::new();
    for cell in cells {
        let Cell::Code(code) = cell else {
            continue;
        };
        let status = output
            .and_then(|out| out.cells.iter().find(|cell| cell.index == code.index))
            .map(|cell| cell.status.as_str())
            .unwrap_or("pending");
        lines.push(format!(
            "{}\tline {}\t{}\t{}",
            code.name, code.start_line, status, code.index
        ));
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn print_outputs(outputs: &[OutputItem]) {
    for output in outputs {
        match output {
            OutputItem::Text { value } => {
                print!("{}", value);
            }
            OutputItem::PipelineTrace {
                pipeline_name,
                stages,
                total_corrupted,
                corruptions,
                ..
            } => {
                print!(
                    "{}",
                    format_pipeline_trace(
                        pipeline_name,
                        stages,
                        *total_corrupted,
                        corruptions
                    )
                );
            }
            OutputItem::Error { message, .. } => {
                eprintln!("{}", message);
            }
            other => {
                println!("{}", serde_json::to_string(other).unwrap_or_default());
            }
        }
    }
}

fn format_pipeline_trace(
    pipeline_name: &str,
    stages: &[PipelineTraceStage],
    total_corrupted: usize,
    corruptions: &[PipelineTraceCorruption],
) -> String {
    let flow = stages
        .iter()
        .map(|stage| {
            if stage.corrupted > 0 {
                format!("{}({}) !{}", stage.name, stage.out, stage.corrupted)
            } else {
                format!("{}({})", stage.name, stage.out)
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut lines = vec![format!("[pipeline: {}] {}", pipeline_name, flow)];
    if total_corrupted > 0 {
        lines.push(format!("! {} corrupted records detected", total_corrupted));
        for corruption in corruptions {
            lines.push(format!("  #{} [{}] {}", corruption.index, corruption.stage, corruption.reason));
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn run_goblet_graph(
    path: &str,
    format: &str,
    output: Option<&str>,
    function: Option<&str>,
    include_closures: bool,
) -> i32 {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            return 2;
        }
    };
    let graphs = match goblet_analyze_source(&source) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("解析エラー: {}", e);
            return 2;
        }
    };
    if graphs.is_empty() {
        eprintln!("pipeline が見つかりませんでした");
        return 2;
    }

    let graphs: Vec<_> = graphs
        .into_iter()
        .filter(|graph| graph_matches_function(graph, function))
        .map(|graph| {
            if include_closures {
                goblet_expand_closure_details(&graph)
            } else {
                graph
            }
        })
        .collect();

    if graphs.is_empty() {
        eprintln!("条件に一致する pipeline が見つかりませんでした");
        return 2;
    }

    let has_type_errors = graphs
        .iter()
        .any(|g| g.nodes.iter().any(|n| n.status == NodeStatus::Error));

    let rendered = match format {
        "text" => render_graph_collection_text(&graphs),
        "json" => format!(
            "[\n{}\n]",
            graphs
                .iter()
                .map(goblet_render_json)
                .collect::<Vec<_>>()
                .join(",\n")
        ),
        "mermaid" => render_graph_collection_mermaid(&graphs),
        other => {
            eprintln!("未対応の format です: {} (text|json|mermaid)", other);
            return 2;
        }
    };

    if let Some(out) = output {
        let content = if format == "mermaid" {
            render_graph_collection_mermaid_markdown(&graphs)
        } else {
            rendered
        };
        if let Err(e) = fs::write(out, content) {
            eprintln!("ファイル書き込みエラー: {}", e);
            return 2;
        }
    } else {
        print!("{rendered}");
    }

    if has_type_errors {
        1
    } else {
        0
    }
}

fn graph_matches_function(graph: &PipelineGraph, function: Option<&str>) -> bool {
    let Some(function) = function else {
        return true;
    };

    graph.function_name.as_deref() == Some(function)
        || graph.roots.iter().any(|rid| {
            graph
                .nodes
                .iter()
                .find(|n| n.id == *rid)
                .is_some_and(|n| n.label == function)
        })
}

fn pipeline_heading(graph: &PipelineGraph, idx: usize) -> String {
    match graph.function_name.as_deref() {
        Some(name) => format!(
            "=== Pipeline {} [{}] ({} nodes) ===",
            idx + 1,
            name,
            graph.nodes.len()
        ),
        None => format!("=== Pipeline {} ({} nodes) ===", idx + 1, graph.nodes.len()),
    }
}

fn render_graph_collection_text(graphs: &[PipelineGraph]) -> String {
    if graphs.len() == 1 {
        return goblet_render_text(&graphs[0]);
    }

    graphs
        .iter()
        .enumerate()
        .map(|(idx, graph)| {
            format!(
                "{}\n{}",
                pipeline_heading(graph, idx),
                goblet_render_text(graph)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_graph_collection_mermaid(graphs: &[PipelineGraph]) -> String {
    if graphs.len() == 1 {
        return goblet_render_mermaid(&graphs[0]);
    }

    graphs
        .iter()
        .enumerate()
        .map(|(idx, graph)| {
            format!(
                "%% {}\n{}",
                pipeline_heading(graph, idx),
                goblet_render_mermaid(graph)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_graph_collection_mermaid_markdown(graphs: &[PipelineGraph]) -> String {
    graphs
        .iter()
        .enumerate()
        .map(|(idx, graph)| {
            if graphs.len() == 1 {
                format!("```mermaid\n{}```\n", goblet_render_mermaid(graph))
            } else {
                format!(
                    "## {}\n```mermaid\n{}```\n",
                    pipeline_heading(graph, idx),
                    goblet_render_mermaid(graph)
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// パイプラインの詳細を人間向けテキストで表示する。
/// --function <name> で特定の root ラベルを持つグラフに絞り込める。
/// --line N でそのノードのスパンに行 N を含むグラフに絞り込める。
/// 終了コード: 0=正常, 1=型エラーあり, 2=解析失敗
fn run_goblet_explain(path: &str, function: Option<&str>, line: Option<usize>) -> i32 {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            return 2;
        }
    };
    let graphs = match goblet_analyze_source(&source) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("解析エラー: {}", e);
            return 2;
        }
    };

    let filtered: Vec<_> = graphs
        .iter()
        .filter(|g| graph_matches_function(g, function))
        .filter(|g| {
            if let Some(ln) = line {
                g.nodes.iter().any(|n| {
                    n.span
                        .as_ref()
                        .map_or(false, |s| s.line <= ln && ln <= s.line)
                })
            } else {
                true
            }
        })
        .collect();

    if filtered.is_empty() {
        eprintln!("条件に一致する pipeline が見つかりませんでした");
        return 2;
    }

    let trace_events = collect_goblet_runtime_trace(path, &source, &filtered);
    let mut has_type_errors = false;
    for (idx, graph) in filtered.iter().enumerate() {
        println!("=== Pipeline {} ({} nodes) ===", idx + 1, graph.nodes.len());
        for node in &graph.nodes {
            let status_str = match node.status {
                NodeStatus::Ok => "ok",
                NodeStatus::Warning => "warning",
                NodeStatus::Error => {
                    has_type_errors = true;
                    "ERROR"
                }
                NodeStatus::Unknown => "unknown",
            };
            println!(
                "[N{}] {:<28} {:?}  [{}]",
                node.id.0, node.label, node.kind, status_str
            );
            if let Some(it) = &node.input_type {
                println!("     input:  {}", it.display);
            }
            if let Some(ot) = &node.output_type {
                println!("     output: {}", ot.display);
            }
            if let Some(di) = &node.data_info {
                let shape_str = format!("{:?}", di.shape);
                println!("     shape:  {}  state: {:?}", shape_str, di.state);
                if let Some(pname) = &di.param_name {
                    println!("     param:  {}", pname);
                }
            }
            for note in &node.notes {
                println!("     note:   {}", note);
            }
            if let Some(span) = &node.span {
                println!("     span:   line {}, col {}", span.line, span.col);
            }
            for event in trace_events
                .iter()
                .filter(|event| event.node_id == Some(node.id.0))
            {
                println!(
                    "     trace:  outcome={} items={}",
                    trace_outcome_label(&event.outcome),
                    event
                        .item_count
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "n/a".to_string())
                );
                if let Some(message) = &event.message {
                    println!("     trace:  message={}", message);
                }
            }
            println!();
        }
        if graph.diagnostics.is_empty() {
            println!("  Diagnostics: none\n");
        } else {
            println!("  Diagnostics:");
            for d in &graph.diagnostics {
                let node_ref = d.node_id.map_or("—".to_string(), |id| format!("N{}", id.0));
                println!("  [{}] {} at {}: {}", d.code, d.code, node_ref, d.message);
                if let Some(exp) = &d.expected {
                    println!("      expected: {}", exp);
                }
                if let Some(act) = &d.actual {
                    println!("      actual:   {}", act);
                }
            }
            println!();
        }
    }

    if has_type_errors {
        1
    } else {
        0
    }
}

fn collect_goblet_runtime_trace(
    path: &str,
    source: &str,
    graphs: &[&PipelineGraph],
) -> Vec<PipelineTraceEvent> {
    let module = match parse_source(source) {
        Ok(module) => module,
        Err(_) => return Vec::new(),
    };

    let node_refs = graphs
        .iter()
        .flat_map(|graph| {
            graph.nodes.iter().filter_map(|node| {
                node.span.as_ref().map(|span| PipelineTraceNodeRef {
                    node_id: node.id.0,
                    start: span.start,
                    end: span.end,
                    line: span.line,
                    col: span.col,
                })
            })
        })
        .collect::<Vec<_>>();

    let (mut interp, _) = Interpreter::with_file_path_and_output_capture(Path::new(path));
    interp.set_pipeline_trace_nodes(node_refs);
    if interp.eval(&module).is_err() {
        return interp.take_pipeline_trace_events();
    }
    interp.take_pipeline_trace_events()
}

fn trace_outcome_label(outcome: &PipelineTraceOutcome) -> &'static str {
    match outcome {
        PipelineTraceOutcome::Ok => "ok",
        PipelineTraceOutcome::FindNone => "find_none",
        PipelineTraceOutcome::ResultErr => "result_err",
    }
}

/// パイプライングラフを JSON で生ダンプする（デバッグ用）。
/// 終了コード: 0=正常, 2=解析失敗
fn run_goblet_dump(path: &str) -> i32 {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            return 2;
        }
    };
    let graphs = match goblet_analyze_source(&source) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("解析エラー: {}", e);
            return 2;
        }
    };

    let json = format!(
        "[\n{}\n]",
        graphs
            .iter()
            .map(goblet_render_json)
            .collect::<Vec<_>>()
            .join(",\n")
    );
    println!("{json}");
    0
}

fn transpile_file(path: &str, output: Option<&str>) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            std::process::exit(1);
        }
    };

    let rust_code = match transpile(&source) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("トランスパイルエラー: {}", e);
            std::process::exit(1);
        }
    };

    match output {
        Some(out_path) => {
            if let Err(e) = fs::write(out_path, &rust_code) {
                eprintln!(
                    "エラー: ファイル '{}' への書き込みに失敗しました: {}",
                    out_path, e
                );
                std::process::exit(1);
            }
            println!("Rust コードを '{}' に書き込みました", out_path);
        }
        None => {
            print!("{}", rust_code);
        }
    }
}

fn build_entry(path: Option<&str>, output: Option<&str>) {
    match resolve_build_request(path) {
        Ok(BuildRequest::File(file_path)) => build_file(&file_path, output),
        Ok(BuildRequest::Project {
            project_dir,
            forge_toml,
        }) => build_project_with_forge_toml(&project_dir, &forge_toml, output),
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn build_web_entry(path: Option<&str>, output: Option<&str>) {
    match resolve_build_request(path) {
        Ok(BuildRequest::File(file_path)) => build_web_file(&file_path, output),
        Ok(BuildRequest::Project { project_dir, .. }) => build_web_project(&project_dir, output),
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn test_entry(path: Option<&str>, filter: Option<&str>) {
    match resolve_project_request(path) {
        Ok(ProjectRequest::File(file_path)) => {
            test_file(&file_path.to_string_lossy(), filter, None)
        }
        Ok(ProjectRequest::Project { project_dir, .. }) => {
            let tests_dir = project_dir.join("tests");
            let test_files = collect_project_test_files(&tests_dir);
            if test_files.is_empty() {
                eprintln!("エラー: tests/*.test.forge が見つかりません");
                std::process::exit(1);
            }

            for test_file_path in test_files {
                test_file(
                    &test_file_path.to_string_lossy(),
                    filter,
                    Some(&project_dir),
                );
            }
        }
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn build_web_file(path: &Path, output: Option<&str>) {
    let project_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let source_root = project_dir;
    let rel_path = path.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("app.bloom"));
    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join("dist"));
    if let Err(e) = emit_bloom_web_artifacts(source_root, &[(path.to_path_buf(), rel_path)], &out_dir) {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
}

fn build_web_project(project_dir: &Path, output: Option<&str>) {
    let source_root = project_dir.join("src");
    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join("dist"));

    // Anvil の .forge ルートファイルがあれば render(<Component />) 構文を前処理する
    if let Err(e) = preprocess_forge_files_in_dir(&source_root, project_dir) {
        eprintln!("警告: render(<...>) 前処理に失敗: {}", e);
    }

    let files = match collect_bloom_files(&source_root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    };
    let file_pairs = files
        .into_iter()
        .map(|file| (file.abs_path, file.rel_path))
        .collect::<Vec<_>>();
    if let Err(e) = emit_bloom_web_artifacts(&source_root, &file_pairs, &out_dir) {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
}

/// src/ 内の .forge ファイルをスキャンし render(<Component />) 構文を前処理する。
/// 変換後の内容でファイルを上書きする（プロジェクトコピー先の dist/generated/ に書く）。
fn preprocess_forge_files_in_dir(dir: &Path, project_root: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("{}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            preprocess_forge_files_in_dir(&path, project_root)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("forge") {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|e| format!("{}: {}", path.display(), e))?;
        if source.contains("render(<") {
            let processed = preprocess_render_calls(&source, project_root)?;
            fs::write(&path, processed)
                .map_err(|e| format!("{}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

fn emit_bloom_web_artifacts(
    source_root: &Path,
    files: &[(PathBuf, PathBuf)],
    out_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("{}: {}", out_dir.display(), e))?;
    for (abs_path, rel_path) in files {
        let bloom_source = fs::read_to_string(abs_path)
            .map_err(|e| format!("{}: {}", abs_path.display(), e))?;
        let generated = compile_bloom_with_compiler_forge(source_root, &bloom_source)?;
        let out_path = out_dir.join("generated").join(generated_forge_path(rel_path));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
        }
        fs::write(&out_path, generated).map_err(|e| format!("{}: {}", out_path.display(), e))?;
        let wasm_path = out_dir.join(wasm_output_path(rel_path));
        compile_generated_forge_to_wasm(&fs::read_to_string(&out_path).map_err(|e| format!("{}: {}", out_path.display(), e))?, &wasm_path)?;
    }

    let bloom_pkg = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packages")
        .join("bloom");
    let runtime_src = bloom_pkg.join("forge.min.js");
    let runtime_dst = out_dir.join("forge.min.js");

    let js_source = fs::read_to_string(&runtime_src)
        .map_err(|e| format!("{}: {}", runtime_src.display(), e))?;

    let critical_css_path = bloom_pkg.join("src").join("critical.css");
    let css = if critical_css_path.exists() {
        fs::read_to_string(&critical_css_path)
            .map_err(|e| format!("{}: {}", critical_css_path.display(), e))?
    } else {
        String::new()
    };

    let js_with_css = inline_critical_css(&js_source, css.trim());
    fs::write(&runtime_dst, js_with_css)
        .map_err(|e| format!("{}: {}", runtime_dst.display(), e))?;

    Ok(())
}

fn compile_bloom_with_compiler_forge(source_root: &Path, bloom_source: &str) -> Result<String, String> {
    let compiler_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packages")
        .join("bloom")
        .join("src")
        .join("compiler.forge");
    let compiler_source =
        fs::read_to_string(&compiler_path).map_err(|e| format!("{}: {}", compiler_path.display(), e))?;

    let mut temp_dir = env::temp_dir();
    temp_dir.push(format!("forge_bloom_web_{}", unique_suffix()));
    fs::create_dir_all(&temp_dir).map_err(|e| format!("{}: {}", temp_dir.display(), e))?;

    let local_source_root = source_root.to_path_buf();
    let result = (|| {
        let compiler_copy = temp_dir.join("compiler.forge");
        fs::write(&compiler_copy, compiler_source)
            .map_err(|e| format!("{}: {}", compiler_copy.display(), e))?;

        let runner_path = temp_dir.join("main.forge");
        let bytes = bloom_source
            .as_bytes()
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let runner = format!(
            "use ./compiler.{{ parse_bloom, bloom_generated }}\nlet source = bytes_to_str([{}])\nprintln(bloom_generated(parse_bloom(source)))\n",
            bytes
        );
        fs::write(&runner_path, runner).map_err(|e| format!("{}: {}", runner_path.display(), e))?;

        let cli = env::current_exe().map_err(|e| e.to_string())?;
        let output = Command::new(cli)
            .arg("run")
            .arg(&runner_path)
            .current_dir(&local_source_root)
            .output()
            .map_err(|e| format!("compiler.forge invocation failed: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "compiler.forge invocation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn build_file(path: &Path, output: Option<&str>) {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s.to_string(),
        None => {
            eprintln!(
                "エラー: ファイル名を解決できませんでした: {}",
                path.display()
            );
            std::process::exit(1);
        }
    };
    let output_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/forge").join(&stem));

    build_generated_project(path, &stem, "0.1.0", "2021", output_path, None);
}

fn build_project_with_forge_toml(project_dir: &Path, forge_toml: &ForgeToml, output: Option<&str>) {
    let entry_path = project_dir.join(&forge_toml.package.entry);
    let edition = forge_toml
        .build
        .as_ref()
        .map(|build| build.edition.as_str())
        .unwrap_or("2021");
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => forge_toml
            .build
            .as_ref()
            .and_then(|build| build.output.as_ref())
            .map(|p| project_dir.join(p))
            .unwrap_or_else(|| project_dir.join("target").join(&forge_toml.package.name)),
    };

    build_generated_project(
        &entry_path,
        &forge_toml.package.name,
        &forge_toml.package.version,
        edition,
        output_path,
        Some(forge_toml),
    );
}

fn build_generated_project(
    entry_path: &Path,
    package_name: &str,
    package_version: &str,
    edition: &str,
    output_path: PathBuf,
    forge_toml: Option<&ForgeToml>,
) {
    use std::process::Command;

    let mut proj_dir = std::env::temp_dir();
    proj_dir.push(format!("forge_build_{}_{}", package_name, unique_suffix()));

    if let Err(e) = fs::create_dir_all(proj_dir.join("src")) {
        eprintln!(
            "エラー: 一時プロジェクトディレクトリを作成できませんでした: {}",
            e
        );
        std::process::exit(1);
    }

    let cargo_toml = build_generated_cargo_toml(
        package_name,
        package_version,
        edition,
        forge_toml.map(|toml| &toml.dependencies),
    );
    if let Err(e) = fs::write(proj_dir.join("Cargo.toml"), cargo_toml) {
        eprintln!("エラー: Cargo.toml の書き込みに失敗しました: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = write_transpiled_project(entry_path, &proj_dir.join("src")) {
        eprintln!(
            "エラー: トランスパイル済みプロジェクトの生成に失敗しました: {}",
            e
        );
        std::process::exit(1);
    }

    format_generated_project(&proj_dir);

    if let Some(parent) = output_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("エラー: 出力ディレクトリを作成できませんでした: {}", e);
            std::process::exit(1);
        }
    }
    let output_abs = if output_path.is_absolute() {
        output_path
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(output_path),
            Err(e) => {
                eprintln!("エラー: カレントディレクトリを取得できませんでした: {}", e);
                let _ = fs::remove_dir_all(&proj_dir);
                std::process::exit(1);
            }
        }
    };

    let manifest_path = match proj_dir.join("Cargo.toml").to_str() {
        Some(p) => p.to_string(),
        None => {
            eprintln!("エラー: 一時ディレクトリのパスが UTF-8 ではありません");
            let _ = fs::remove_dir_all(&proj_dir);
            std::process::exit(1);
        }
    };

    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path", &manifest_path])
        .status();

    match status {
        Ok(s) if s.success() => {
            let built_bin = proj_dir.join(format!("target/release/{}", package_name));
            let built_bin_exe = proj_dir.join(format!("target/release/{}.exe", package_name));
            let src_bin = if built_bin_exe.exists() {
                built_bin_exe
            } else {
                built_bin
            };
            let output_abs = normalized_binary_output_path(output_abs, &src_bin);

            if let Err(e) = fs::copy(&src_bin, &output_abs) {
                eprintln!("エラー: バイナリのコピーに失敗しました: {}", e);
                eprintln!("  コピー元: {}", src_bin.display());
                eprintln!("  コピー先: {}", output_abs.display());
                let _ = fs::remove_dir_all(&proj_dir);
                std::process::exit(1);
            }
            let _ = fs::remove_dir_all(&proj_dir);
            println!("ビルド成功: {}", output_abs.display());
        }
        Ok(s) => {
            let _ = fs::remove_dir_all(&proj_dir);
            eprintln!(
                "cargo build がエラーを返しました (exit code: {:?})",
                s.code()
            );
            std::process::exit(1);
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&proj_dir);
            eprintln!("エラー: cargo を呼び出せませんでした: {}", e);
            eprintln!("ヒント: Rust ツールチェーンがインストールされているか確認してください");
            std::process::exit(1);
        }
    }
}

fn normalized_binary_output_path(mut output_path: PathBuf, src_bin: &Path) -> PathBuf {
    let src_is_windows_exe = src_bin
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    let has_extension = output_path.extension().is_some();

    if src_is_windows_exe && !has_extension {
        output_path.set_extension("exe");
    }

    output_path
}

enum BuildRequest {
    File(PathBuf),
    Project {
        project_dir: PathBuf,
        forge_toml: ForgeToml,
    },
}

enum ProjectRequest {
    File(PathBuf),
    Project {
        project_dir: PathBuf,
        forge_toml: ForgeToml,
    },
}

fn resolve_project_request(path: Option<&str>) -> Result<ProjectRequest, String> {
    let target = match path {
        Some(path) => PathBuf::from(path),
        None => env::current_dir().map_err(|e| e.to_string())?,
    };

    if target.is_file() {
        return Ok(ProjectRequest::File(target));
    }

    let start = if target.exists() {
        target
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(target)
    };

    let forge_toml_path = ForgeToml::find(&start)
        .ok_or_else(|| format!("forge.toml が見つかりません: {}", start.display()))?;
    let project_dir = forge_toml_path
        .parent()
        .ok_or_else(|| "forge.toml の親ディレクトリを解決できません".to_string())?
        .to_path_buf();
    let forge_toml = ForgeToml::load(&project_dir)?;

    Ok(ProjectRequest::Project {
        project_dir,
        forge_toml,
    })
}

fn resolve_build_request(path: Option<&str>) -> Result<BuildRequest, String> {
    match resolve_project_request(path)? {
        ProjectRequest::File(file_path) => Ok(BuildRequest::File(file_path)),
        ProjectRequest::Project {
            project_dir,
            forge_toml,
        } => Ok(BuildRequest::Project {
            project_dir,
            forge_toml,
        }),
    }
}

fn collect_project_test_files(tests_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !tests_dir.is_dir() {
        return files;
    }

    if let Ok(entries) = fs::read_dir(tests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.ends_with(".test.forge"))
                    .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

fn build_generated_cargo_toml(
    package_name: &str,
    package_version: &str,
    edition: &str,
    manifest_deps: Option<&std::collections::BTreeMap<String, DependencyValue>>,
) -> String {
    let mut cargo_toml = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"{package_version}\"\nedition = \"{edition}\"\n\n[dependencies]\n"
    );

    let has_anyhow = manifest_deps
        .map(|deps| deps.contains_key("anyhow"))
        .unwrap_or(false);
    if !has_anyhow {
        cargo_toml.push_str("anyhow = \"1\"\n");
    }

    let has_forge_std = manifest_deps
        .map(|deps| deps.contains_key("forge_std"))
        .unwrap_or(false);
    if !has_forge_std {
        cargo_toml.push_str(&format!("{}\n", forge_std_dependency_line()));
    }

    if let Some(deps) = manifest_deps {
        for (name, dep) in deps {
            let line = match dep {
                DependencyValue::Version(version) => format!("{name} = \"{version}\""),
                DependencyValue::Detailed { version, features } => {
                    if features.is_empty() {
                        format!("{name} = {{ version = \"{version}\" }}")
                    } else {
                        let features = features
                            .iter()
                            .map(|feature| format!("\"{}\"", feature))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "{name} = {{ version = \"{version}\", features = [{}] }}",
                            features
                        )
                    }
                }
                // ローカルパス依存は forge build では Cargo.toml に含めない
                DependencyValue::LocalPath(_) => continue,
            };
            cargo_toml.push_str(&line);
            cargo_toml.push('\n');
        }
    }

    cargo_toml
}

fn forge_std_dependency_line() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("forge-stdlib");
    let path = path.to_string_lossy().replace('\\', "/");
    format!(
        "forge_std = {{ package = \"forge-stdlib\", path = \"{}\" }}",
        path
    )
}

#[derive(Debug, Clone)]
struct ForgeSourceFile {
    rel_path: PathBuf,
    source: String,
}

fn write_transpiled_project(entry_path: &Path, out_src_dir: &Path) -> Result<(), String> {
    let source_root = detect_source_root(entry_path)?;
    let files = collect_forge_files(&source_root, entry_path)?;
    let entry_rel = entry_path
        .strip_prefix(&source_root)
        .map_err(|_| {
            format!(
                "entry path '{}' is outside source root",
                entry_path.display()
            )
        })?
        .to_path_buf();

    let mut deps = DepsManager::new();
    let module_index = build_module_index(&files, &entry_rel);

    for file in &files {
        collect_external_deps(&source_root, file, &mut deps)?;

        let mut rust_code =
            transpile(&file.source).map_err(|e| format!("{}: {}", file.rel_path.display(), e))?;

        let rel_dir = file
            .rel_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let prelude = module_prelude(&rel_dir, &file.rel_path, &entry_rel, &module_index);
        if !prelude.is_empty() {
            rust_code = format!("{}{}", prelude, rust_code);
        }

        collect_codegen_deps(&rust_code, &mut deps);

        let out_rel = forge_rel_to_rust_rel(&file.rel_path, &entry_rel);
        let out_path = out_src_dir.join(out_rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&out_path, rust_code).map_err(|e| e.to_string())?;
    }

    write_synthesized_mod_files(out_src_dir, &module_index, &files)?;

    let cargo_toml_path = out_src_dir
        .parent()
        .ok_or_else(|| {
            format!(
                "src ディレクトリの親を解決できません: {}",
                out_src_dir.display()
            )
        })?
        .join("Cargo.toml");
    update_generated_cargo_toml(&cargo_toml_path, &deps)?;

    Ok(())
}

fn detect_source_root(entry_path: &Path) -> Result<PathBuf, String> {
    let parent = entry_path.parent().ok_or_else(|| {
        format!(
            "cannot determine parent directory for '{}'",
            entry_path.display()
        )
    })?;

    Ok(parent.to_path_buf())
}

fn collect_forge_files(
    source_root: &Path,
    entry_path: &Path,
) -> Result<Vec<ForgeSourceFile>, String> {
    if entry_path.file_name().and_then(|s| s.to_str()) != Some("main.forge") {
        let rel_path = entry_path
            .strip_prefix(source_root)
            .map_err(|_| {
                format!(
                    "{} is outside {}",
                    entry_path.display(),
                    source_root.display()
                )
            })?
            .to_path_buf();
        let source = fs::read_to_string(entry_path)
            .map_err(|e| format!("{}: {}", entry_path.display(), e))?;
        return Ok(vec![ForgeSourceFile { rel_path, source }]);
    }

    fn walk(
        dir: &Path,
        source_root: &Path,
        files: &mut Vec<ForgeSourceFile>,
    ) -> Result<(), String> {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| format!("{}: {}", dir.display(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, source_root, files)?;
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("forge") {
                continue;
            }

            let rel_path = path
                .strip_prefix(source_root)
                .map_err(|_| format!("{} is outside {}", path.display(), source_root.display()))?
                .to_path_buf();
            let source =
                fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
            files.push(ForgeSourceFile { rel_path, source });
        }

        Ok(())
    }

    let mut files = Vec::new();
    walk(source_root, source_root, &mut files)?;
    Ok(files)
}

fn build_module_index(
    files: &[ForgeSourceFile],
    entry_rel: &Path,
) -> std::collections::BTreeMap<PathBuf, Vec<String>> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut index: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    for file in files {
        let rel_dir = file
            .rel_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        if file.rel_path != entry_rel
            && file.rel_path.file_name().and_then(|s| s.to_str()) != Some("main.forge")
        {
            if let Some(stem) = file.rel_path.file_stem().and_then(|s| s.to_str()) {
                if stem != "mod" {
                    index
                        .entry(rel_dir.clone())
                        .or_default()
                        .insert(stem.to_string());
                }
            }
        }

        if !rel_dir.as_os_str().is_empty() {
            let parent = rel_dir.parent().map(Path::to_path_buf).unwrap_or_default();
            if let Some(dir_name) = rel_dir.file_name().and_then(|s| s.to_str()) {
                index
                    .entry(parent)
                    .or_default()
                    .insert(dir_name.to_string());
            }
        }
    }

    index
        .into_iter()
        .map(|(dir, names)| (dir, names.into_iter().collect()))
        .collect()
}

fn module_prelude(
    rel_dir: &Path,
    rel_path: &Path,
    entry_rel: &Path,
    module_index: &std::collections::BTreeMap<PathBuf, Vec<String>>,
) -> String {
    let mut prelude = String::new();
    let decl_dir = if rel_path == entry_rel {
        Some(PathBuf::new())
    } else if rel_path.file_name().and_then(|s| s.to_str()) == Some("mod.forge") {
        Some(rel_dir.to_path_buf())
    } else {
        None
    };

    if let Some(decl_dir) = decl_dir {
        if let Some(children) = module_index.get(&decl_dir) {
            for name in children {
                prelude.push_str(&format!("pub mod {};\n", name));
            }
            if !children.is_empty() {
                prelude.push('\n');
            }
        }
    }

    prelude
}

fn forge_rel_to_rust_rel(rel_path: &Path, entry_rel: &Path) -> PathBuf {
    if rel_path == entry_rel {
        return PathBuf::from("main.rs");
    }

    if rel_path.file_name().and_then(|s| s.to_str()) == Some("mod.forge") {
        return rel_path.with_file_name("mod.rs");
    }

    rel_path.with_extension("rs")
}

fn write_synthesized_mod_files(
    out_src_dir: &Path,
    module_index: &std::collections::BTreeMap<PathBuf, Vec<String>>,
    files: &[ForgeSourceFile],
) -> Result<(), String> {
    use std::collections::HashSet;

    let existing_mods: HashSet<PathBuf> = files
        .iter()
        .filter(|file| file.rel_path.file_name().and_then(|s| s.to_str()) == Some("mod.forge"))
        .filter_map(|file| file.rel_path.parent().map(Path::to_path_buf))
        .collect();

    for (dir, children) in module_index {
        if dir.as_os_str().is_empty() || existing_mods.contains(dir) {
            continue;
        }

        let mut content = String::new();
        for child in children {
            content.push_str(&format!("pub mod {};\n", child));
        }

        let out_path = out_src_dir.join(dir).join("mod.rs");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(out_path, content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn collect_external_deps(
    source_root: &Path,
    file: &ForgeSourceFile,
    deps: &mut DepsManager,
) -> Result<(), String> {
    let module = parse_source(&file.source).map_err(|e| e.to_string())?;
    for stmt in module.stmts {
        if let Stmt::UseDecl {
            path: UsePath::External(crate_name),
            ..
        } = stmt
        {
            let current_dir = source_root.join(
                file.rel_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
            );
            let first_segment = match crate_name.split('/').next() {
                Some(s) => s,
                None => &crate_name,
            };
            let local_file = current_dir.join(format!("{}.forge", first_segment));
            let local_dir = current_dir.join(first_segment);
            if local_file.exists() || local_dir.exists() {
                continue;
            }
            deps.add(&crate_name);
        }
    }
    Ok(())
}

fn collect_codegen_deps(rust_code: &str, deps: &mut DepsManager) {
    if rust_code.contains("once_cell::") {
        deps.add("once_cell");
    }
    if rust_code.contains("serde::Serialize") || rust_code.contains("serde::Deserialize") {
        deps.add("serde");
    }
    if rust_code.contains("tokio::") || rust_code.contains(".await") {
        deps.add("tokio");
    }
    if rust_code.contains("scopeguard::defer") {
        deps.add("scopeguard");
    }
}

fn update_generated_cargo_toml(cargo_toml_path: &Path, deps: &DepsManager) -> Result<(), String> {
    let mut content = fs::read_to_string(cargo_toml_path).map_err(|e| e.to_string())?;

    let mut extra_lines = Vec::new();
    let crates = deps.crates();

    if crates.contains("once_cell") && !content.contains("\nonce_cell = ") {
        extra_lines.push("once_cell = \"1.21.4\"".to_string());
    }
    if crates.contains("serde") && !content.contains("\nserde = ") {
        extra_lines.push("serde = { version = \"1.0.228\", features = [\"derive\"] }".to_string());
    }
    if crates.contains("tokio") && !content.contains("\ntokio = ") {
        extra_lines.push("tokio = { version = \"1\", features = [\"full\"] }".to_string());
    }
    if crates.contains("scopeguard") && !content.contains("\nscopeguard = ") {
        extra_lines.push("scopeguard = \"1\"".to_string());
    }
    if crates.contains("reqwest") && !content.contains("\nreqwest = ") {
        extra_lines.push("reqwest = { version = \"0.12\", features = [\"json\"] }".to_string());
    }

    let mut others = crates
        .iter()
        .filter(|name| {
            name.as_str() != "once_cell" && name.as_str() != "serde" && name.as_str() != "tokio"
        })
        .cloned()
        .collect::<Vec<_>>();
    others.sort();
    for name in others {
        if !content.contains(&format!("\n{} = ", name)) {
            extra_lines.push(format!("{name} = \"*\""));
        }
    }

    if !extra_lines.is_empty() {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        for line in extra_lines {
            content.push_str(&line);
            content.push('\n');
        }
        fs::write(cargo_toml_path, content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn format_generated_project(proj_dir: &Path) {
    use std::process::Command;

    let manifest_path = match proj_dir.join("Cargo.toml").to_str() {
        Some(p) => p.to_string(),
        None => {
            eprintln!("警告: 一時ディレクトリのパスが UTF-8 ではないため整形をスキップします");
            return;
        }
    };

    let status = Command::new("cargo")
        .args(["fmt", "--all", "--manifest-path", &manifest_path])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => {
            eprintln!("警告: 生成 Rust の整形に失敗しましたが、ビルドは続行します");
        }
        Err(_) => {
            eprintln!("警告: cargo fmt を実行できませんでしたが、ビルドは続行します");
        }
    }
}

fn unique_suffix() -> String {
    let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}", std::process::id(), seq)
}

fn dev_entry(args: &[String]) {
    let port = args
        .iter()
        .position(|s| s == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(5173);

    let addr = format!("127.0.0.1:{}", port);
    println!("forge dev: listening on http://{}", addr);
    println!("DevTools endpoints:");
    println!("  GET  http://{}/devtools/snapshots", addr);
    println!("  POST http://{}/devtools/time-travel  {{\"index\": N}}", addr);

    use std::net::{TcpListener, TcpStream};
    use std::io::{Read as _, Write as _};

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("エラー: {} に bind できませんでした: {}", addr, e);
            std::process::exit(1);
        }
    };

    // スナップショットの簡易インメモリストレージ（開発サーバー用）
    let snapshots: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    fn send_response(mut stream: TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    }

    for stream in listener.incoming() {
        let snapshots = snapshots.clone();
        match stream {
            Ok(mut stream) => {
                let mut buf = [0u8; 4096];
                let n = match stream.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");

                if first_line.contains("GET /devtools/snapshots") {
                    let snaps = snapshots.lock().unwrap();
                    let body = format!("[{}]", snaps.join(","));
                    send_response(stream, "200 OK", &body);
                } else if first_line.contains("POST /devtools/time-travel") {
                    // ボディから index を抽出
                    let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                    let body = &req[body_start..];
                    let index: i64 = body
                        .find("\"index\"")
                        .and_then(|i| body[i + 7..].trim_start_matches(|c: char| !c.is_ascii_digit() && c != '-').parse().ok())
                        .unwrap_or(0);
                    let snaps = snapshots.lock().unwrap();
                    let len = snaps.len() as i64;
                    let clamped = if len == 0 {
                        None
                    } else {
                        let idx = index.max(0).min(len - 1) as usize;
                        snaps.get(idx).cloned()
                    };
                    let resp = match clamped {
                        Some(s) => s,
                        None => "{}".to_string(),
                    };
                    send_response(stream, "200 OK", &resp);
                } else if first_line.contains("POST /devtools/snapshot") {
                    // 内部: スナップショットを記録
                    let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                    let body = req[body_start..].to_string();
                    let mut snaps = snapshots.lock().unwrap();
                    snaps.push(body);
                    let count = snaps.len();
                    send_response(stream, "200 OK", &format!("{{\"count\":{}}}", count));
                } else {
                    send_response(stream, "404 Not Found", "{\"error\":\"not found\"}");
                }
            }
            Err(_) => continue,
        }
    }
}

fn print_help() {
    println!("ForgeScript CLI");
    println!();
    println!("使用方法:");
    println!("  forge new [name] [--template <name>]  新しいプロジェクトを作成");
    println!("  forge run <file.forge>              ファイルを読み込んで実行");
    println!("  forge test <file.forge>             インラインテストを実行");
    println!("  forge test <file.forge> --filter <pattern>  テスト名で絞り込み");
    println!("  forge check <file.forge>            型チェックのみ（実行しない）");
    println!("  forge transpile <file.forge>        Rust コードを stdout に出力");
    println!("  forge transpile <file.forge> -o out.rs  Rust コードをファイルに出力");
    println!("  forge goblet graph <file.forge>     パイプライン可視化を出力");
    println!("  forge notebook run <file.fnb>       Notebook を実行");
    println!("  forge notebook reset <file.fnb>     Notebook を再実行");
    println!("  forge notebook clear <file.fnb>     Notebook 出力を削除");
    println!("  forge notebook show <file.fnb>      Notebook セル一覧を表示");
    println!("  forge lsp                           LSP サーバをstdioモードで起動");
    println!("  forge build                         forge.toml からバイナリを生成");
    println!("  forge build --web                   .bloom を dist/ に Web 出力");
    println!("  forge build <dir/>                  指定ディレクトリの forge.toml を使用");
    println!("  forge build <file.forge>            単一ファイルからバイナリを生成");
    println!("  forge build <file.forge> -o myapp   出力バイナリ名を指定");
    println!("  forge repl                          対話型 REPL を起動");
    println!("  forge mcp                           MCP サーバをstdioモードで起動");
    println!("  forge mcp start|stop|restart        MCP デーモンを管理");
    println!("  forge mcp status                    MCP デーモンの状態を表示");
    println!("  forge mcp logs [-f] [--errors]      MCP ログを表示");
    println!("  forge version                       バージョンを表示");
    println!("  forge help                          このヘルプを表示");
}

fn print_new_help() {
    println!("forge new");
    println!();
    println!("使用方法:");
    println!("  forge new [name] [--template <name>]");
    println!();
    println!("オプション:");
    println!("  --template <name>   使用するテンプレート名（デフォルト: script）");
    println!("  --git               生成後に git init を実行");
    println!("  -h, --help          このヘルプを表示");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn build_generated_cargo_toml_includes_manifest_dependencies() {
        let mut deps = BTreeMap::new();
        deps.insert(
            "serde".to_string(),
            DependencyValue::Version("1".to_string()),
        );
        deps.insert(
            "tokio".to_string(),
            DependencyValue::Detailed {
                version: "1".to_string(),
                features: vec!["full".to_string()],
            },
        );

        let cargo_toml = build_generated_cargo_toml("demo", "0.2.0", "2024", Some(&deps));
        assert!(cargo_toml.contains("name = \"demo\""));
        assert!(cargo_toml.contains("version = \"0.2.0\""));
        assert!(cargo_toml.contains("edition = \"2024\""));
        assert!(cargo_toml.contains("serde = \"1\""));
        assert!(cargo_toml.contains("tokio = { version = \"1\", features = [\"full\"] }"));
        assert!(cargo_toml.contains("forge_std = { package = \"forge-stdlib\", path = "));
    }

    #[test]
    fn build_generated_cargo_toml_does_not_duplicate_forge_std() {
        let mut deps = BTreeMap::new();
        deps.insert(
            "forge_std".to_string(),
            DependencyValue::Detailed {
                version: "0.1".to_string(),
                features: vec!["custom".to_string()],
            },
        );

        let cargo_toml = build_generated_cargo_toml("demo", "0.2.0", "2024", Some(&deps));
        assert_eq!(cargo_toml.matches("forge_std =").count(), 1);
        assert!(cargo_toml.contains("forge_std = { version = \"0.1\", features = [\"custom\"] }"));
    }

    #[test]
    fn normalized_binary_output_path_adds_exe_for_windows_binary() {
        let output = PathBuf::from("packages/anvil/target/anvil");
        let src = PathBuf::from("tmp/target/release/anvil.exe");
        assert_eq!(
            normalized_binary_output_path(output, &src),
            PathBuf::from("packages/anvil/target/anvil.exe")
        );
    }

    #[test]
    fn normalized_binary_output_path_preserves_existing_extension() {
        let output = PathBuf::from("packages/anvil/target/anvil.bin");
        let src = PathBuf::from("tmp/target/release/anvil.exe");
        assert_eq!(normalized_binary_output_path(output.clone(), &src), output);
    }

    #[test]
    fn normalized_binary_output_path_keeps_unix_binary_name() {
        let output = PathBuf::from("target/demo");
        let src = PathBuf::from("tmp/target/release/demo");
        assert_eq!(normalized_binary_output_path(output.clone(), &src), output);
    }

    #[test]
    fn test_notebook_show() {
        let cells =
            parse_notebook("```forge name=\"setup\"\nlet x = 42\n```\n```forge\nprintln(x)\n```");
        let output = NotebookOutput {
            version: 1,
            file: "demo.fnb".to_string(),
            executed_at: chrono::Utc::now(),
            cells: vec![CellOutput {
                index: 0,
                name: "setup".to_string(),
                status: "ok".to_string(),
                outputs: vec![],
                duration_ms: 1,
            }],
        };

        let rendered = format_notebook_show(&cells, Some(&output));
        assert!(rendered.contains("setup"));
        assert!(rendered.contains("line 1"));
        assert!(rendered.contains("ok"));
        assert!(rendered.contains("cell_1"));
    }

    #[test]
    fn test_fallback_text() {
        let rendered = format_pipeline_trace(
            "names",
            &[
                PipelineTraceStage {
                    name: "source".to_string(),
                    r#in: 3,
                    out: 3,
                    corrupted: 0,
                    line: Some(1),
                },
                PipelineTraceStage {
                    name: "map".to_string(),
                    r#in: 3,
                    out: 2,
                    corrupted: 1,
                    line: Some(2),
                },
            ],
            1,
            &[PipelineTraceCorruption {
                stage: "map".to_string(),
                index: 4,
                reason: "name was none".to_string(),
            }],
        );

        assert!(rendered.contains("[pipeline: names] source(3) -> map(2) !1"));
        assert!(rendered.contains("! 1 corrupted records detected"));
        assert!(rendered.contains("#4 [map] name was none"));
    }

    #[test]
    fn test_collect_codegen_deps_scopeguard() {
        let mut deps = DepsManager::new();
        collect_codegen_deps("scopeguard::defer(|| {});", &mut deps);
        assert!(deps.crates().contains("scopeguard"));
    }

    #[test]
    fn test_update_generated_cargo_toml_scopeguard() {
        let content = r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
"#;
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(content.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();

        let mut deps = DepsManager::new();
        deps.add("scopeguard");

        update_generated_cargo_toml(&path, &deps).expect("update");

        let updated = std::fs::read_to_string(&path).expect("read");
        assert!(
            updated.contains("scopeguard = \"1\""),
            "scopeguard not added: {}",
            updated
        );
    }
}
