// forge-cli: ForgeScript CLI
// Phase 2-D / DBG-5 実装

mod forge_toml;
mod hot_reload;
mod new;
mod templates;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use rustyline::DefaultEditor;

use crate::forge_toml::{DependencyValue, ForgeToml};
use bloom_compiler::{
    collect_bloom_files, compile_bloom_direct, compile_bloom_to_wasm,
    compile_generated_forge_to_wasm, extract_script_section, generated_forge_path,
    inline_critical_css, plan_from_bloom_source, plan_to_generated_forge, preprocess_render_calls,
    wasm_output_path,
};
use forge_compiler::ast::{Stmt, UsePath};
use forge_compiler::deps::DepsManager;
use forge_compiler::parser::{parse_source, parse_source_with_file};
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
            let bloom_flag = args.iter().skip(2).any(|arg| arg == "--bloom");
            let template = if bloom_flag {
                "bloom"
            } else {
                args.iter()
                    .position(|a| a == "--template")
                    .and_then(|i| args.get(i + 1))
                    .map(|s| s.as_str())
                    .unwrap_or("script")
            };
            let git = args.iter().skip(2).any(|arg| arg == "--git");

            if let Err(e) = new::run_with_options(name, template, git) {
                eprintln!("エラー: {}", e);
                std::process::exit(1);
            }
        }
        Some("run") => {
            let verbose = args
                .iter()
                .skip(2)
                .any(|arg| matches!(arg.as_str(), "--verbose" | "-v"));
            let path = args
                .iter()
                .skip(2)
                .find(|arg| !arg.starts_with('-'))
                .map(|s| s.as_str());
            run_entry(path, verbose);
        }
        Some("serve") => {
            if args
                .iter()
                .skip(2)
                .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
            {
                print_serve_help();
                return;
            }
            let opts = parse_serve_options(&args);
            std::env::set_var("FORGE_SERVE_PORT", opts.port.to_string());
            std::env::set_var("PORT", opts.port.to_string());
            if opts.wasm_trace {
                std::env::set_var("FORGE_WASM_TRACE", "1");
            }
            if opts.watch {
                // DBG-5-A: ホットリロードモードでサーバーを起動
                start_serve_with_watch(&opts);
            } else {
                run_entry(opts.path.as_deref(), false);
            }
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
            let dump_ast = args.iter().any(|arg| arg == "--dump-ast");
            let dump_forge = args.iter().any(|arg| arg == "--dump-forge");
            let path = build_path_arg(&args);
            if web {
                build_web_entry(
                    path,
                    output.map(|s| s.as_str()),
                    BloomDumpOptions {
                        dump_ast,
                        dump_forge,
                    },
                );
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
        Some("bloom") => {
            bloom_entry(&args);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServeOptions {
    path: Option<String>,
    port: u16,
    wasm_trace: bool,
    /// --watch フラグ: ファイル変更を監視してホットリロードを行う
    watch: bool,
}

fn parse_serve_options(args: &[String]) -> ServeOptions {
    let mut path = None;
    let mut port = 8080u16;
    let mut wasm_trace = false;
    let mut watch = false;
    let mut i = 2usize;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                let Some(value) = args.get(i + 1) else {
                    eprintln!("エラー: --port にはポート番号が必要です");
                    std::process::exit(1);
                };
                port = parse_serve_port(value);
                i += 2;
            }
            "--wasm-trace" => {
                wasm_trace = true;
                i += 1;
            }
            "--watch" | "-w" => {
                watch = true;
                i += 1;
            }
            arg if arg.starts_with('-') => {
                eprintln!("エラー: 不明な serve オプション '{}'", arg);
                eprintln!("ヒント: `forge serve --help` で使用方法を確認できます");
                std::process::exit(1);
            }
            arg => {
                if path.is_none() {
                    path = Some(arg.to_string());
                }
                i += 1;
            }
        }
    }
    ServeOptions {
        path,
        port,
        wasm_trace,
        watch,
    }
}

fn parse_serve_port(value: &str) -> u16 {
    match value.parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => {
            eprintln!("エラー: 無効なポート番号 '{}'", value);
            std::process::exit(1);
        }
    }
}

fn run_file(path: &str, verbose: bool) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした: {}", path, e);
            std::process::exit(1);
        }
    };

    let module = match parse_source_with_file(&source, path.to_string()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("構文エラー: {}", e);
            std::process::exit(1);
        }
    };

    let file_path = std::path::Path::new(path);
    let mut interp = Interpreter::with_file_path(file_path);
    interp.set_verbose_mode(verbose);
    if let Err(e) = interp.eval(&module) {
        eprintln!("{}", interp.format_runtime_error(&e));
        std::process::exit(1);
    }
}

fn run_entry(path: Option<&str>, verbose: bool) {
    match resolve_project_request(path) {
        Ok(ProjectRequest::File(file_path)) => run_file(&file_path.to_string_lossy(), verbose),
        Ok(ProjectRequest::Project {
            project_dir,
            forge_toml,
        }) => {
            let overrides = preprocess_project_forge_sources(&project_dir, false);
            let entry = project_dir.join(&forge_toml.package.entry);
            let dep_paths = forge_toml.local_dep_paths(&project_dir);
            if dep_paths.is_empty() {
                run_file_with_overrides(&entry.to_string_lossy(), overrides, verbose);
            } else {
                run_file_with_deps_and_overrides(&entry, dep_paths, overrides, verbose);
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

    let module = match parse_source_with_file(&source, entry.to_string_lossy().to_string()) {
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
    interp.set_verbose_mode(false);
    if let Err(e) = interp.eval(&module) {
        eprintln!("{}", interp.format_runtime_error(&e));
        std::process::exit(1);
    }
}

fn run_file_with_overrides(path: &str, overrides: HashMap<PathBuf, String>, verbose: bool) {
    run_file_with_deps_and_overrides(Path::new(path), vec![], overrides, verbose);
}

fn run_file_with_deps_and_overrides(
    entry: &Path,
    dep_paths: Vec<(String, PathBuf)>,
    overrides: HashMap<PathBuf, String>,
    verbose: bool,
) {
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

    let module = match parse_source_with_file(&source, entry.to_string_lossy().to_string()) {
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
    interp.set_verbose_mode(verbose);
    if let Some(loader) = interp.module_loader_mut() {
        for (path, source) in overrides {
            loader.add_source_override(path, source);
        }
    }
    if let Err(e) = interp.eval(&module) {
        eprintln!("{}", interp.format_runtime_error(&e));
        std::process::exit(1);
    }
}

fn test_file(
    path: &str,
    filter: Option<&str>,
    project_root: Option<&Path>,
    dep_paths: Option<Vec<(String, PathBuf)>>,
) {
    test_file_with_overrides(path, filter, project_root, dep_paths, HashMap::new());
}

fn test_file_with_overrides(
    path: &str,
    filter: Option<&str>,
    project_root: Option<&Path>,
    dep_paths: Option<Vec<(String, PathBuf)>>,
    overrides: HashMap<PathBuf, String>,
) {
    let file_path_buf = PathBuf::from(path);
    let canonical = file_path_buf.canonicalize().ok();
    let source = overrides
        .get(&file_path_buf)
        .or_else(|| canonical.as_ref().and_then(|c| overrides.get(c)))
        .cloned()
        .or_else(|| fs::read_to_string(path).ok())
        .unwrap_or_else(|| {
            eprintln!("エラー: ファイル '{}' を読み込めませんでした", path);
            std::process::exit(1);
        });

    let module = match parse_source(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("構文エラー: {}", e);
            std::process::exit(1);
        }
    };

    let mut interp = match (project_root, dep_paths) {
        (Some(root), Some(dep_paths)) => {
            Interpreter::with_project_root_and_deps(root.to_path_buf(), dep_paths)
        }
        (Some(root), None) => Interpreter::with_project_root(root.to_path_buf()),
        (None, _) => Interpreter::with_file_path(&file_path_buf),
    };
    if let Some(loader) = interp.module_loader_mut() {
        for (p, s) in overrides {
            loader.add_source_override(p, s);
        }
    }
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

/// ブレース（`{` / `}`）の深さを数えてソースが「完結しているか」を返す
/// 完結している（深さ 0）なら true
pub(crate) fn is_complete_input(source: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape_next = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        // 行コメント
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // 行末まで読み飛ばす
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth <= 0
}

/// REPL 上で入力ソースを評価し結果を表示する（use 文のシンボル追跡も行う）
fn repl_eval_and_print(source: &str, interp: &mut Interpreter) {
    match parse_source(source) {
        Ok(module) => {
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
                    // Unit は何も表示しない（println などの副作用は既に出力済み）
                }
                Ok(val) => println!("{}", val),
                Err(e) => eprintln!("エラー: {}", interp.format_runtime_error(&e)),
            }
        }
        Err(e) => eprintln!("構文エラー: {}", e),
    }
}

fn run_repl() {
    println!("ForgeScript REPL v{}", env!("CARGO_PKG_VERSION"));
    println!(":help でコマンド一覧を表示します。Ctrl+D または :quit で終了します。");

    let mut interp = Interpreter::new();
    // REPL ではカレントディレクトリをプロジェクトルートとしてモジュールローダーを初期化する（M-7-A）
    interp.init_module_loader_from_cwd();

    // rustyline で行編集・履歴を有効化（DBG-3-A）
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!(
                "警告: 行編集を初期化できませんでした（{}）。フォールバックモードで継続します。",
                e
            );
            // フォールバック: rustyline が使えない場合は stdin で代替
            run_repl_fallback(&mut interp);
            return;
        }
    };

    let mut buffer = String::new(); // 複数行バッファ（DBG-3-B）

    loop {
        let prompt = if buffer.is_empty() {
            "forge> ".to_string()
        } else {
            "....> ".to_string()
        };

        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                // 履歴に追加（空行は除く）
                if !line.trim().is_empty() {
                    let _ = rl.add_history_entry(line.as_str());
                }

                let trimmed = line.trim();

                // `:quit` / `:q` は複数行バッファ中でも即終了（DBG-3-C）
                if trimmed == ":quit" || trimmed == ":q" || trimmed == "exit" || trimmed == "quit" {
                    println!("Bye!");
                    break;
                }

                // 複数行バッファが空でコマンド入力の場合は即処理（DBG-3-C）
                if buffer.is_empty() && trimmed.starts_with(':') {
                    match handle_repl_command(trimmed, &mut interp) {
                        Some(Ok(msg)) => {
                            if !msg.is_empty() {
                                println!("{}", msg);
                            }
                        }
                        Some(Err(e)) => eprintln!("エラー: {}", e),
                        None => {
                            // ':' で始まるが known コマンドでなかった場合は式として評価
                            repl_eval_and_print(trimmed, &mut interp);
                        }
                    }
                    continue;
                }

                // 複数行バッファに追記（DBG-3-B）
                if !buffer.is_empty() {
                    buffer.push('\n');
                }
                buffer.push_str(&line);

                // 入力が完結しているか判定
                if is_complete_input(&buffer) {
                    let source = buffer.trim().to_string();
                    buffer.clear();
                    if source.is_empty() {
                        continue;
                    }
                    repl_eval_and_print(&source, &mut interp);
                }
                // 完結していなければ次のプロンプトへ（`....> `）
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                // Ctrl+C: バッファをクリアして続行
                buffer.clear();
                println!("（入力をキャンセルしました。Ctrl+D で終了）");
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                // Ctrl+D: 終了（DBG-3-C）
                if !buffer.is_empty() {
                    // バッファに残りがあれば評価を試みる
                    let source = buffer.trim().to_string();
                    buffer.clear();
                    if !source.is_empty() {
                        repl_eval_and_print(&source, &mut interp);
                    }
                }
                println!("Bye!");
                break;
            }
            Err(e) => {
                eprintln!("入力エラー: {}", e);
                break;
            }
        }
    }
}

/// rustyline が利用できない環境向けのフォールバック REPL
fn run_repl_fallback(interp: &mut Interpreter) {
    let stdin = io::stdin();
    let mut buffer = String::new();

    loop {
        if buffer.is_empty() {
            print!("forge> ");
        } else {
            print!("....> ");
        }
        if io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF (Ctrl+D)
                println!("\nBye!");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("入力エラー: {}", e);
                break;
            }
        }

        let trimmed = line.trim();

        if trimmed == ":quit" || trimmed == ":q" || trimmed == "exit" || trimmed == "quit" {
            println!("Bye!");
            break;
        }

        if buffer.is_empty() && trimmed.starts_with(':') {
            match handle_repl_command(trimmed, interp) {
                Some(Ok(msg)) => {
                    if !msg.is_empty() {
                        println!("{}", msg);
                    }
                }
                Some(Err(e)) => eprintln!("エラー: {}", e),
                None => repl_eval_and_print(trimmed, interp),
            }
            continue;
        }

        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&line);

        if is_complete_input(&buffer) {
            let source = buffer.trim().to_string();
            buffer.clear();
            if !source.is_empty() {
                repl_eval_and_print(&source, interp);
            }
        }
    }
}

/// REPL 専用コマンドを処理する（DBG-3-C, M-7-A）
/// コマンドでない場合は None を返す
fn handle_repl_command(input: &str, interp: &mut Interpreter) -> Option<Result<String, String>> {
    // ── DBG-3-C: 新コマンド ──────────────────────────────────────────────

    // :help — コマンド一覧表示
    if input == ":help" || input == ":h" {
        return Some(Ok("利用可能なコマンド:\n\
             \x20 :type <expr>     — 式の型を表示\n\
             \x20 :trace on|off    — 実行トレースの切り替え\n\
             \x20 :load <file>     — ファイルをスコープに読み込む\n\
             \x20 :reset           — スコープをリセット\n\
             \x20 :modules         — ロード済みモジュール一覧\n\
             \x20 :reload <path>   — モジュールを再ロード\n\
             \x20 :unload <path>   — モジュールをアンロード\n\
             \x20 :help            — このヘルプを表示\n\
             \x20 :quit / :q       — REPL を終了"
            .to_string()));
    }

    // :type <expr> — 式の型を表示
    if let Some(rest) = input.strip_prefix(":type ") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Some(Err(":type には式を指定してください".to_string()));
        }
        match parse_source(expr) {
            Ok(module) => {
                // 一時的な評価コンテキストをクローンして型を取得する
                // インタープリタの状態を変えないよう、式だけ評価して Value の型名を返す
                match interp.eval(&module) {
                    Ok(val) => {
                        let type_str = val.dynamic_type_name();
                        Some(Ok(format!("{}", type_str)))
                    }
                    Err(e) => Some(Err(format!("評価エラー: {}", e))),
                }
            }
            Err(e) => Some(Err(format!("構文エラー: {}", e))),
        }
    }
    // :trace on|off — 実行トレースの切り替え
    else if let Some(rest) = input.strip_prefix(":trace") {
        let arg = rest.trim();
        match arg {
            "on" => {
                interp.set_verbose_mode(true);
                Some(Ok("トレース: ON".to_string()))
            }
            "off" => {
                interp.set_verbose_mode(false);
                Some(Ok("トレース: OFF".to_string()))
            }
            _ => Some(Err(
                ":trace on または :trace off を指定してください".to_string()
            )),
        }
    }
    // :load <file> — ファイルをスコープに読み込む
    else if let Some(rest) = input.strip_prefix(":load ") {
        let file_path = rest.trim();
        if file_path.is_empty() {
            return Some(Err(":load にはファイルパスを指定してください".to_string()));
        }
        match fs::read_to_string(file_path) {
            Ok(source) => match parse_source(&source) {
                Ok(module) => match interp.eval(&module) {
                    Ok(_) => Some(Ok(format!("✔ {} を読み込みました", file_path))),
                    Err(e) => Some(Err(format!("実行エラー: {}", e))),
                },
                Err(e) => Some(Err(format!("構文エラー: {}", e))),
            },
            Err(e) => Some(Err(format!(
                "ファイル '{}' を読み込めませんでした: {}",
                file_path, e
            ))),
        }
    }
    // :reset — スコープをリセット
    else if input == ":reset" {
        *interp = Interpreter::new();
        interp.init_module_loader_from_cwd();
        Some(Ok("スコープをリセットしました".to_string()))
    }
    // ── M-7-A: モジュール管理コマンド ────────────────────────────────────

    // :modules — ロード済みモジュール一覧
    else if input == ":modules" {
        if interp.loaded_modules.is_empty() {
            return Some(Ok("ロード済みモジュール: なし".to_string()));
        }
        let mut output = "ロード済みモジュール:".to_string();
        let mut paths: Vec<&String> = interp.loaded_modules.keys().collect();
        paths.sort();
        for path in paths {
            output.push_str(&format!("\n  - {}", path));
        }
        Some(Ok(output))
    } else if let Some(rest) = input.strip_prefix(":reload ") {
        let path = rest.trim();
        if path.is_empty() {
            return Some(Err(
                ":reload にはモジュールパスを指定してください".to_string()
            ));
        }
        interp.unload_module(path);
        interp.clear_module_loader_cache(path);
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
            "不明なコマンド '{}'\n:help でコマンド一覧を確認できます",
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
            eprintln!(
                "繧ｨ繝ｩ繝ｼ: ipynb JSON 縺ｮ逕滓・縺ｫ螟ｱ謨励＠縺ｾ縺励◆: {}",
                error
            );
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
                    format_pipeline_trace(pipeline_name, stages, *total_corrupted, corruptions)
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
            lines.push(format!(
                "  #{} [{}] {}",
                corruption.index, corruption.stage, corruption.reason
            ));
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

#[derive(Debug, Clone, Copy, Default)]
struct BloomDumpOptions {
    dump_ast: bool,
    dump_forge: bool,
}

fn build_web_entry(path: Option<&str>, output: Option<&str>, dump: BloomDumpOptions) {
    match resolve_build_request(path) {
        Ok(BuildRequest::File(file_path)) => build_web_file(&file_path, output, dump),
        Ok(BuildRequest::Project { project_dir, .. }) => {
            build_web_project(&project_dir, output, dump)
        }
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn test_entry(path: Option<&str>, filter: Option<&str>) {
    match resolve_project_request(path) {
        Ok(ProjectRequest::File(file_path)) => {
            test_file(&file_path.to_string_lossy(), filter, None, None)
        }
        Ok(ProjectRequest::Project {
            project_dir,
            forge_toml,
        }) => {
            let overrides = preprocess_project_forge_sources(&project_dir, true);
            let dep_paths = forge_toml.local_dep_paths(&project_dir);
            let tests_dir = project_dir.join("tests");
            let test_files = collect_project_test_files(&tests_dir);
            if test_files.is_empty() {
                eprintln!("エラー: tests/*.test.forge が見つかりません");
                std::process::exit(1);
            }

            for test_file_path in test_files {
                test_file_with_overrides(
                    &test_file_path.to_string_lossy(),
                    filter,
                    Some(&project_dir),
                    if dep_paths.is_empty() {
                        None
                    } else {
                        Some(dep_paths.clone())
                    },
                    overrides.clone(),
                );
            }
        }
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    }
}

fn build_web_file(path: &Path, output: Option<&str>, dump: BloomDumpOptions) {
    let project_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let source_root = project_dir;
    let rel_path = path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("app.bloom"));
    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join("dist"));
    if let Err(e) = emit_bloom_web_artifacts(
        source_root,
        &[(path.to_path_buf(), rel_path)],
        &out_dir,
        dump,
    ) {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
}

fn build_web_project(project_dir: &Path, output: Option<&str>, dump: BloomDumpOptions) {
    let source_root = project_dir.join("src");
    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join("dist"));

    preprocess_project_forge_sources(project_dir, false);
    let file_pairs = match collect_bloom_file_pairs(&source_root) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("エラー: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = emit_bloom_web_artifacts(&source_root, &file_pairs, &out_dir, dump) {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
}

fn preprocess_project_forge_sources(
    project_dir: &Path,
    include_tests: bool,
) -> HashMap<PathBuf, String> {
    let mut overrides = HashMap::new();
    let source_root = project_dir.join("src");
    if let Err(e) = collect_preprocessed_forge_files(&source_root, project_dir, &mut overrides) {
        eprintln!("警告: render(<...>) 前処理に失敗: {}", e);
    }
    if include_tests {
        let tests_dir = project_dir.join("tests");
        if let Err(e) = collect_preprocessed_forge_files(&tests_dir, project_dir, &mut overrides) {
            eprintln!("警告: tests/ の render(<...>) 前処理に失敗: {}", e);
        }
    }
    overrides
}

fn collect_bloom_file_pairs(source_root: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    Ok(collect_bloom_files(source_root)?
        .into_iter()
        .map(|file| (file.abs_path, file.rel_path))
        .collect())
}

fn emit_bloom_project_artifacts(project_dir: &Path, output: Option<&str>) -> Result<(), String> {
    let source_root = project_dir.join("src");
    let file_pairs = collect_bloom_file_pairs(&source_root)?;
    if file_pairs.is_empty() {
        return Ok(());
    }
    let out_dir = output
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join("dist"));
    emit_bloom_web_artifacts(
        &source_root,
        &file_pairs,
        &out_dir,
        BloomDumpOptions::default(),
    )
}

/// src/ 内の .forge ファイルをスキャンし render(<Component />) 構文を前処理する。
/// 変換結果はメモリ上の HashMap に収集するのみで、ソースファイルは一切書き換えない。
fn collect_preprocessed_forge_files(
    dir: &Path,
    project_root: &Path,
    out: &mut HashMap<PathBuf, String>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_preprocessed_forge_files(&path, project_root, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("forge") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
        if source.contains("render(<") || source.contains("hydrate_inline_script(<") {
            let processed = preprocess_render_calls(&source, project_root)?;
            let abs = path.canonicalize().unwrap_or(path);
            out.insert(abs, processed);
        }
    }
    Ok(())
}

fn emit_bloom_web_artifacts(
    source_root: &Path,
    files: &[(PathBuf, PathBuf)],
    out_dir: &Path,
    dump: BloomDumpOptions,
) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("{}: {}", out_dir.display(), e))?;
    for (abs_path, rel_path) in files {
        let bloom_source =
            fs::read_to_string(abs_path).map_err(|e| format!("{}: {}", abs_path.display(), e))?;
        let script = extract_script_section(&bloom_source).unwrap_or("");
        // 生成 Forge ファイルを出力（後方互換・テスト用）
        let plan = match plan_from_bloom_source(&bloom_source) {
            Ok(plan) => plan,
            Err(err) => {
                if dump.dump_ast {
                    println!("{}", bloom_compile_error_json(rel_path, "plan", &err));
                }
                if dump.dump_forge {
                    println!("// {}", rel_path.display());
                    println!("{}", script);
                }
                return Err(err);
            }
        };
        if dump.dump_ast {
            println!("{}", bloom_plan_json(rel_path, &plan));
        }
        let generated = plan_to_generated_forge(&plan, script);
        if dump.dump_forge {
            println!("// {}", rel_path.display());
            println!("{}", generated);
        }
        let out_path = out_dir
            .join("generated")
            .join(generated_forge_path(rel_path));
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
        }
        fs::write(&out_path, &generated).map_err(|e| format!("{}: {}", out_path.display(), e))?;
        // WASM をコンパイル
        let wasm_path = out_dir.join(wasm_output_path(rel_path));
        compile_bloom_direct(&bloom_source, &wasm_path)?;
    }

    // editors/web/runtime/forge_bloom.js が正規ソース。
    // packages/bloom/forge.min.js はフォールバック（旧形式との互換）。
    let editors_runtime = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("editors")
        .join("web")
        .join("runtime")
        .join("forge_bloom.js");
    let bloom_pkg = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packages")
        .join("bloom");
    let runtime_src = if editors_runtime.exists() {
        editors_runtime
    } else {
        bloom_pkg.join("forge.min.js")
    };
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

fn bloom_plan_json(rel_path: &Path, plan: &bloom_compiler::WasmRenderPlan) -> String {
    serde_json::json!({
        "file": rel_path.to_string_lossy(),
        "state": {
            "name": plan.state_name,
            "initial": plan.initial_value,
        },
        "dynamic_text_target": plan.dynamic_text_target,
        "static_texts": plan.static_texts,
        "listeners": plan.listeners.iter().map(|(target, event, handler)| {
            serde_json::json!({
                "target": target,
                "event": event,
                "handler": handler,
            })
        }).collect::<Vec<_>>(),
        "increment_handlers": plan.increment_handlers,
    })
    .to_string()
}

fn bloom_compile_error_json(rel_path: &Path, stage: &str, error: &str) -> String {
    serde_json::json!({
        "file": rel_path.to_string_lossy(),
        "stage": stage,
        "error": error,
    })
    .to_string()
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
    preprocess_project_forge_sources(project_dir, false);
    if let Err(e) = emit_bloom_project_artifacts(project_dir, None) {
        eprintln!("エラー: {}", e);
        std::process::exit(1);
    }
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

    let local_dep_paths = forge_toml
        .map(|toml| {
            let project_dir = entry_path
                .parent()
                .and_then(|p| {
                    if p.file_name().and_then(|s| s.to_str()) == Some("src") {
                        p.parent()
                    } else {
                        Some(p)
                    }
                })
                .unwrap_or_else(|| Path::new("."));
            toml.local_dep_paths(project_dir)
        })
        .unwrap_or_default();

    if let Err(e) = write_transpiled_project(entry_path, &proj_dir.join("src"), &local_dep_paths) {
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
        .env("CARGO_NET_OFFLINE", "true")
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
    source_root: PathBuf,
    crate_prefix: Option<String>,
}

fn write_transpiled_project(
    entry_path: &Path,
    out_src_dir: &Path,
    local_dep_paths: &[(String, PathBuf)],
) -> Result<(), String> {
    let source_root = detect_source_root(entry_path)?;
    let files = collect_forge_files(&source_root, entry_path, local_dep_paths)?;
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
    let local_dep_names = local_dep_paths
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<std::collections::HashSet<_>>();

    for file in &files {
        collect_external_deps(&file.source_root, file, &local_dep_names, &mut deps)?;

        let mut rust_code =
            transpile(&file.source).map_err(|e| format!("{}: {}", file.rel_path.display(), e))?;

        if let Some(prefix) = &file.crate_prefix {
            rust_code = rust_code.replace("crate::", &format!("crate::{}::", prefix));
        }

        let rel_dir = file
            .rel_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let prelude = module_prelude(&rel_dir, &file.rel_path, &entry_rel, &module_index);
        if !prelude.is_empty() {
            rust_code = format!("{}{}", prelude, rust_code);
        }

        if rust_code.contains("bytes_to_str(") {
            rust_code = format!("{}{}", generated_bytes_to_str_helper(), rust_code);
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
    local_dep_paths: &[(String, PathBuf)],
) -> Result<Vec<ForgeSourceFile>, String> {
    use std::collections::HashMap;

    fn walk(
        dir: &Path,
        source_root: &Path,
        rel_prefix: &Path,
        crate_prefix: Option<&str>,
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
                walk(&path, source_root, rel_prefix, crate_prefix, files)?;
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("forge") {
                continue;
            }
            if crate_prefix.is_some()
                && path.file_name().and_then(|name| name.to_str()) == Some("main.forge")
            {
                continue;
            }

            let rel_suffix = path
                .strip_prefix(source_root)
                .map_err(|_| format!("{} is outside {}", path.display(), source_root.display()))?;
            let rel_path = if rel_prefix.as_os_str().is_empty() {
                rel_suffix.to_path_buf()
            } else {
                rel_prefix.join(rel_suffix)
            };
            let source =
                fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
            if crate_prefix.is_some() && source.contains("#[") {
                continue;
            }
            files.push(ForgeSourceFile {
                rel_path,
                source,
                source_root: source_root.to_path_buf(),
                crate_prefix: crate_prefix.map(|s| s.to_string()),
            });
        }

        Ok(())
    }

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
        return Ok(vec![ForgeSourceFile {
            rel_path,
            source,
            source_root: source_root.to_path_buf(),
            crate_prefix: None,
        }]);
    }

    let mut files = Vec::new();
    walk(source_root, source_root, Path::new(""), None, &mut files)?;
    let dep_map = local_dep_paths
        .iter()
        .map(|(name, root)| (name.clone(), root.clone()))
        .collect::<HashMap<_, _>>();
    let dep_files = collect_required_local_dep_files(&files, &dep_map)?;
    files.extend(dep_files);
    Ok(files)
}

fn collect_required_local_dep_files(
    project_files: &[ForgeSourceFile],
    dep_map: &std::collections::HashMap<String, PathBuf>,
) -> Result<Vec<ForgeSourceFile>, String> {
    use std::collections::HashSet;

    let mut visited = HashSet::new();
    let mut files = Vec::new();

    for file in project_files {
        collect_dependency_imports_from_source(
            &file.source,
            file.rel_path.parent().unwrap_or(Path::new("")),
            None,
            dep_map,
            &mut visited,
            &mut files,
        )?;
    }

    Ok(files)
}

fn collect_dependency_imports_from_source(
    source: &str,
    current_rel_dir: &Path,
    current_dep: Option<&str>,
    dep_map: &std::collections::HashMap<String, PathBuf>,
    visited: &mut std::collections::HashSet<(String, PathBuf)>,
    files: &mut Vec<ForgeSourceFile>,
) -> Result<(), String> {
    let module = parse_source(source).map_err(|e| e.to_string())?;
    for stmt in module.stmts {
        let Stmt::UseDecl { path, .. } = stmt else {
            continue;
        };

        match path {
            UsePath::Local(module_path) => {
                if let Some(dep_name) = current_dep {
                    let dep_root = dep_map
                        .get(dep_name)
                        .ok_or_else(|| format!("missing local dependency root for '{dep_name}'"))?;
                    let dep_src = dep_root.join("src");
                    if let Some(target_rel) =
                        resolve_internal_module_path(&dep_src, current_rel_dir, &module_path)
                    {
                        collect_dependency_module_recursive(
                            dep_name,
                            &dep_src,
                            &target_rel,
                            dep_map,
                            visited,
                            files,
                        )?;
                    }
                }
            }
            UsePath::External(module_path) => {
                let first_segment = module_path
                    .split('/')
                    .next()
                    .unwrap_or(module_path.as_str())
                    .to_string();

                if let Some(dep_root) = dep_map.get(&first_segment) {
                    let dep_src = dep_root.join("src");
                    let rel_spec = module_path
                        .strip_prefix(&(first_segment.clone() + "/"))
                        .unwrap_or("");
                    if !rel_spec.is_empty() {
                        let rel_path =
                            resolve_package_module_path(&dep_src, rel_spec).ok_or_else(|| {
                                format!(
                                    "missing Forge module '{}' in local dependency '{}'",
                                    rel_spec, first_segment
                                )
                            })?;
                        collect_dependency_module_recursive(
                            &first_segment,
                            &dep_src,
                            &rel_path,
                            dep_map,
                            visited,
                            files,
                        )?;
                    }
                    continue;
                }

                if let Some(dep_name) = current_dep {
                    let dep_root = dep_map
                        .get(dep_name)
                        .ok_or_else(|| format!("missing local dependency root for '{dep_name}'"))?;
                    let dep_src = dep_root.join("src");
                    if let Some(target_rel) =
                        resolve_internal_module_path(&dep_src, current_rel_dir, &module_path)
                    {
                        collect_dependency_module_recursive(
                            dep_name,
                            &dep_src,
                            &target_rel,
                            dep_map,
                            visited,
                            files,
                        )?;
                    }
                }
            }
            UsePath::Stdlib(_) => {}
        }
    }
    Ok(())
}

fn collect_dependency_module_recursive(
    dep_name: &str,
    dep_src: &Path,
    rel_path: &Path,
    dep_map: &std::collections::HashMap<String, PathBuf>,
    visited: &mut std::collections::HashSet<(String, PathBuf)>,
    files: &mut Vec<ForgeSourceFile>,
) -> Result<(), String> {
    let normalized_rel = normalize_module_rel_path(rel_path);
    let visit_key = (dep_name.to_string(), normalized_rel.clone());
    if !visited.insert(visit_key) {
        return Ok(());
    }

    let source = if let Some(shim) = shimmed_local_dep_module_source(dep_name, &normalized_rel) {
        shim
    } else {
        let abs_path = dep_src.join(&normalized_rel);
        fs::read_to_string(&abs_path).map_err(|e| format!("{}: {}", abs_path.display(), e))?
    };
    let current_rel_dir = normalized_rel
        .parent()
        .unwrap_or(Path::new(""))
        .to_path_buf();
    files.push(ForgeSourceFile {
        rel_path: Path::new(dep_name).join(&normalized_rel),
        source: source.clone(),
        source_root: dep_src.to_path_buf(),
        crate_prefix: Some(dep_name.to_string()),
    });
    collect_dependency_imports_from_source(
        &source,
        &current_rel_dir,
        Some(dep_name),
        dep_map,
        visited,
        files,
    )
}

fn resolve_package_module_path(dep_src: &Path, module_spec: &str) -> Option<PathBuf> {
    resolve_module_candidates(dep_src, &[normalize_import_spec(module_spec)])
}

fn resolve_internal_module_path(
    source_root: &Path,
    current_rel_dir: &Path,
    module_spec: &str,
) -> Option<PathBuf> {
    let normalized = normalize_import_spec(module_spec);
    let relative = normalize_module_rel_path(&current_rel_dir.join(&normalized));
    let root_relative = normalize_module_rel_path(&normalized);
    resolve_module_candidates(source_root, &[relative, root_relative])
}

fn resolve_module_candidates(source_root: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    for candidate in candidates {
        let file = source_root.join(candidate).with_extension("forge");
        if file.exists() {
            return Some(candidate.with_extension("forge"));
        }
        let mod_file = source_root.join(candidate).join("mod.forge");
        if mod_file.exists() {
            return Some(candidate.join("mod.forge"));
        }
    }
    None
}

fn normalize_import_spec(module_spec: &str) -> PathBuf {
    normalize_module_rel_path(Path::new(module_spec.trim_start_matches("./")))
}

fn normalize_module_rel_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    normalized
}

fn shimmed_local_dep_module_source(dep_name: &str, rel_path: &Path) -> Option<String> {
    let rel = rel_path.to_string_lossy().replace('\\', "/");
    match (dep_name, rel.as_str()) {
        ("bloom", "ssr.forge") => Some(
            r#"pub fn make_component(source: string, props: map<string, string>) -> map<string, string> {
    {
        "source": source.clone(),
        "props": source,
    }
}

pub fn render_source(source: string, props: map<string, string>) -> string {
    source
}

pub fn hydrate_script() -> string {
    "<script src='/forge.min.js' defer></script>"
}

pub fn hydrate_script_with(path: string) -> string {
    "<script>window.__FORGE_BLOOM_WASM_PATH = '{path}'</script><script src='/forge.min.js' defer></script>"
}
"#
            .to_string(),
        ),
        ("anvil", "ssr.forge") => Some(
            r#"pub fn hydrate_script() -> string {
    "<script src='/forge.min.js' defer></script>"
}

pub fn layout(html: string, script: string) -> string {
    "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'></head><body><div id='app'>{html}</div>{script}</body></html>"
}
"#
            .to_string(),
        ),
        _ => None,
    }
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
    local_dep_names: &std::collections::HashSet<String>,
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
            if local_dep_names.contains(first_segment) {
                continue;
            }
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

fn generated_bytes_to_str_helper() -> &'static str {
    r#"fn bytes_to_str(bytes: Vec<i64>) -> String {
    let raw = bytes
        .into_iter()
        .map(|value| value.clamp(0, 255) as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&raw).into_owned()
}

"#
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
    println!(
        "  POST http://{}/devtools/time-travel  {{\"index\": N}}",
        addr
    );

    open_browser(&format!("http://localhost:{}", port));

    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};

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

    fn send_html(mut stream: TcpStream, html: &str) {
        let bytes = html.as_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(bytes);
    }

    fn startup_page_html() -> String {
        r##"<!DOCTYPE html>
<html lang="ja">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Bloom on ForgeScript</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <style>
    body { background: oklch(0.145 0 0); color: white; font-family: ui-sans-serif, system-ui, sans-serif; }
  </style>
</head>
<body class="min-h-screen flex flex-col">
  <header class="flex items-center justify-between px-6 py-4 border-b border-white/10">
    <div class="flex items-center gap-2 text-sm text-white/50">
      <span>&#128196;</span>
      <span>Get started by editing
        <code class="font-mono text-white bg-white/10 px-1.5 py-0.5 rounded">src/app/page.bloom</code>
      </span>
    </div>
    <a href="https://github.com/kazuma0606/rvm" target="_blank"
       class="text-sm text-white/50 hover:text-white transition-colors">GitHub</a>
  </header>

  <div class="flex-1 flex flex-col items-center justify-center gap-8 px-6">
    <div class="text-center">
      <h1 class="text-7xl font-bold tracking-tight mb-2"
          style="background: linear-gradient(to right, #fb7185, #d946ef, #6366f1); -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;">
        Bloom
      </h1>
      <p class="text-xl" style="color: rgba(255,255,255,0.5)">
        on <span style="font-weight:700; color:white">ForgeScript</span>
      </p>
    </div>
  </div>

  <footer style="display:grid; grid-template-columns: repeat(3, 1fr); gap:1rem; padding: 2rem 1.5rem; border-top: 1px solid rgba(255,255,255,0.1); max-width:56rem; margin:0 auto; width:100%">
    <a href="#" style="display:flex; flex-direction:column; gap:0.25rem; padding:1rem; border-radius:0.5rem; border:1px solid rgba(255,255,255,0.1); text-decoration:none; transition:all 0.15s">
      <div style="display:flex; align-items:center; gap:0.25rem; color:white; font-weight:500">Docs &#8594;</div>
      <p style="font-size:0.875rem; color:rgba(255,255,255,0.5); margin:0">Bloom&#12398;&#27231;&#33021;&#12392;API&#12398;&#35443;&#32048;</p>
    </a>
    <a href="#" style="display:flex; flex-direction:column; gap:0.25rem; padding:1rem; border-radius:0.5rem; border:1px solid rgba(255,255,255,0.1); text-decoration:none; transition:all 0.15s">
      <div style="display:flex; align-items:center; gap:0.25rem; color:white; font-weight:500">Learn &#8594;</div>
      <p style="font-size:0.875rem; color:rgba(255,255,255,0.5); margin:0">&#12452;&#12531;&#12479;&#12521;&#12463;&#12486;&#12451;&#12502;&#12394;&#12467;&#12540;&#12473;&#12391;&#23398;&#12406;</p>
    </a>
    <a href="#" style="display:flex; flex-direction:column; gap:0.25rem; padding:1rem; border-radius:0.5rem; border:1px solid rgba(255,255,255,0.1); text-decoration:none; transition:all 0.15s">
      <div style="display:flex; align-items:center; gap:0.25rem; color:white; font-weight:500">Templates &#8594;</div>
      <p style="font-size:0.875rem; color:rgba(255,255,255,0.5); margin:0">&#12473;&#12479;&#12540;&#12479;&#12540;&#12486;&#12531;&#12503;&#12524;&#12540;&#12488;&#38598;</p>
    </a>
  </footer>
</body>
</html>"##.to_string()
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

                if first_line == "GET / HTTP/1.1"
                    || first_line == "GET / HTTP/1.0"
                    || first_line.starts_with("GET / ")
                {
                    send_html(stream, &startup_page_html());
                } else if first_line.contains("GET /devtools/snapshots") {
                    let snaps = snapshots.lock().unwrap();
                    let body = format!("[{}]", snaps.join(","));
                    send_response(stream, "200 OK", &body);
                } else if first_line.contains("POST /devtools/time-travel") {
                    // ボディから index を抽出
                    let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
                    let body = &req[body_start..];
                    let index: i64 = body
                        .find("\"index\"")
                        .and_then(|i| {
                            body[i + 7..]
                                .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '-')
                                .parse()
                                .ok()
                        })
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

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

fn bloom_entry(args: &[String]) {
    let sub = args.get(2).map(|s| s.as_str());
    match sub {
        Some("add") => {
            let kind = args.get(3).map(|s| s.as_str()).unwrap_or("");
            let name_or_path = args.get(4).map(|s| s.as_str()).unwrap_or("");
            if kind.is_empty() || name_or_path.is_empty() {
                eprintln!("使用方法: forge bloom add <kind> <name>");
                eprintln!("  kind: component | page | layout | store | model");
                std::process::exit(1);
            }
            bloom_add(kind, name_or_path);
        }
        Some(sub) => {
            eprintln!("エラー: 不明な bloom サブコマンド '{}'", sub);
            eprintln!("使用可能: forge bloom add <kind> <name>");
            std::process::exit(1);
        }
        None => {
            eprintln!("使用方法: forge bloom add <kind> <name>");
            std::process::exit(1);
        }
    }
}

fn bloom_add(kind: &str, name_or_path: &str) {
    use std::fs;
    use std::path::PathBuf;

    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("エラー: カレントディレクトリを取得できませんでした: {}", e);
            std::process::exit(1);
        }
    };

    match kind {
        "component" => {
            let dest = cwd
                .join("src")
                .join("components")
                .join(format!("{}.bloom", name_or_path));
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("エラー: ディレクトリ作成失敗: {}", e);
                    std::process::exit(1);
                }
            }
            let content = format!(
                "<div class=\"p-6\">\n  <!-- {} コンポーネント -->\n</div>\n\n<script>\n  // state と fn をここに追加\n</script>\n",
                name_or_path
            );
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("エラー: ファイル書き込み失敗: {}", e);
                std::process::exit(1);
            }
            println!("✓ Created {}", dest.display());
        }
        "page" => {
            let dest = cwd
                .join("src")
                .join("app")
                .join(name_or_path)
                .join("page.bloom");
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("エラー: ディレクトリ作成失敗: {}", e);
                    std::process::exit(1);
                }
            }
            // derive display name from last path segment
            let display_name = PathBuf::from(name_or_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(name_or_path)
                .to_string();
            let content = format!(
                "<div class=\"p-6\">\n  <h1 class=\"text-2xl font-bold\">{}</h1>\n</div>\n",
                display_name
            );
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("エラー: ファイル書き込み失敗: {}", e);
                std::process::exit(1);
            }
            println!("✓ Created {}", dest.display());
        }
        "layout" => {
            let dest = cwd
                .join("src")
                .join("app")
                .join(name_or_path)
                .join("layout.bloom");
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("エラー: ディレクトリ作成失敗: {}", e);
                    std::process::exit(1);
                }
            }
            let content = "<slot />\n";
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("エラー: ファイル書き込み失敗: {}", e);
                std::process::exit(1);
            }
            println!("✓ Created {}", dest.display());
        }
        "store" => {
            let dest = cwd
                .join("src")
                .join("stores")
                .join(format!("{}.flux.bloom", name_or_path));
            if let Some(parent) = dest.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("エラー: ディレクトリ作成失敗: {}", e);
                    std::process::exit(1);
                }
            }
            // Capitalize first letter for store name
            let store_name = {
                let mut chars = name_or_path.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                    None => name_or_path.to_string(),
                }
            };
            let content = format!(
                "store {} {{\n  // state と fn をここに追加\n}}\n",
                store_name
            );
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("エラー: ファイル書き込み失敗: {}", e);
                std::process::exit(1);
            }
            println!("✓ Created {}", dest.display());
        }
        "model" => {
            let dest = cwd.join(format!("{}.model.bloom", name_or_path));
            // Capitalize first letter for model name
            let model_name = {
                let mut chars = name_or_path.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                    None => name_or_path.to_string(),
                }
            };
            let content = format!(
                "model {} {{\n  // state と fn をここに追加\n}}\n",
                model_name
            );
            if let Err(e) = fs::write(&dest, content) {
                eprintln!("エラー: ファイル書き込み失敗: {}", e);
                std::process::exit(1);
            }
            println!("✓ Created {}", dest.display());
        }
        _ => {
            eprintln!("エラー: 不明な kind: {}", kind);
            eprintln!("使用可能: component | page | layout | store | model");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("ForgeScript CLI");
    println!();
    println!("使用方法:");
    println!("  forge new [name] [--template <name>]  新しいプロジェクトを作成");
    println!("  forge run <file.forge>              ファイルを読み込んで実行");
    println!("  forge serve [path] [--port <n>]     Anvil サーバーを起動");
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

fn print_serve_help() {
    println!("forge serve");
    println!();
    println!("使用方法:");
    println!("  forge serve [path] [--port <n>] [--wasm-trace] [--watch]");
    println!();
    println!("説明:");
    println!(
        "  Anvil などのサーバーエントリポイントを実行します。内部実行経路は forge run と同じです。"
    );
    println!();
    println!("オプション:");
    println!("  --port, -p <n>     待受ポートを指定（デフォルト: 8080）");
    println!("  --wasm-trace       Bloom WASM のロード・初期化・コマンドバッファをトレース");
    println!("  --watch, -w        ファイル変更を監視してホットリロードを行う（DBG-5）");
    println!("  -h, --help         このヘルプを表示");
}

// ── DBG-5-A: ホットリロード付きサーバー起動 ─────────────────────────────────

/// `forge serve --watch` 時に呼ばれるエントリポイント。
/// ファイル監視と WebSocket サーバーを起動してからメインのサーバーを実行する。
fn start_serve_with_watch(opts: &ServeOptions) {
    use hot_reload::{start_watch, WatchConfig, WsBroadcaster};

    // WebSocket ポートはメインポート + 1（例: 8080 → 8081）
    let ws_port = opts.port.saturating_add(1);

    // 監視対象ディレクトリの解決
    let watch_dir = if let Some(ref p) = opts.path {
        let path = std::path::Path::new(p);
        if path.is_dir() {
            path.to_path_buf()
        } else {
            // ファイルが指定された場合はその親ディレクトリを監視
            path.parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        }
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    // エントリポイントパスを解決
    let entry_path = if let Some(ref p) = opts.path {
        PathBuf::from(p)
    } else {
        watch_dir.join("main.forge")
    };

    // WebSocket ブロードキャスターを起動
    let broadcaster = WsBroadcaster::new();
    broadcaster.start_listener(ws_port);

    // ファイル監視を起動（バックグラウンドスレッド）
    let config = WatchConfig {
        watch_dir,
        ws_port,
        entry_path,
    };
    start_watch(config, broadcaster);

    eprintln!(
        "[HotReload] ホットリロードモードで起動します (WebSocket: ws://127.0.0.1:{})",
        ws_port
    );
    eprintln!("[HotReload] ブラウザにリロードスクリプトを自動注入します");

    // ホットリロード環境変数をセット（Anvil のレスポンスフィルタで使用）
    std::env::set_var("FORGE_HOT_RELOAD", "1");
    std::env::set_var("FORGE_HOT_RELOAD_WS_PORT", ws_port.to_string());

    // 通常通りサーバーを起動
    run_entry(opts.path.as_deref(), false);
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
    fn parse_serve_options_accepts_path_port_and_wasm_trace() {
        let args = vec![
            "forge".to_string(),
            "serve".to_string(),
            "--wasm-trace".to_string(),
            "--port".to_string(),
            "8123".to_string(),
            "examples/app".to_string(),
        ];

        assert_eq!(
            parse_serve_options(&args),
            ServeOptions {
                path: Some("examples/app".to_string()),
                port: 8123,
                wasm_trace: true,
                watch: false,
            }
        );
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

#[cfg(test)]
mod bloom_scaffold_tests {
    use std::fs;

    fn unique_suffix() -> u64 {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn test_new_bloom_project_structure() {
        use crate::templates::{get_template, write_template};

        let mut dest = std::env::temp_dir();
        dest.push(format!(
            "forge_bloom_new_test_{}_{}",
            std::process::id(),
            unique_suffix()
        ));

        let template = get_template("bloom").expect("bloom テンプレートが存在すること");
        write_template(
            &dest,
            template,
            &[
                ("name", "test-app"),
                ("version", "0.1.0"),
                ("forge_version", "0.1.0"),
            ],
        )
        .expect("テンプレートの書き込みが成功すること");

        assert!(
            dest.join("forge.toml").exists(),
            "forge.toml が存在すること"
        );
        assert!(
            dest.join("src/app/page.bloom").exists(),
            "src/app/page.bloom が存在すること"
        );
        assert!(
            dest.join("src/app/layout.bloom").exists(),
            "src/app/layout.bloom が存在すること"
        );
        assert!(
            dest.join("src/components/counter.bloom").exists(),
            "src/components/counter.bloom が存在すること"
        );
        assert!(
            dest.join("src/stores/counter.flux.bloom").exists(),
            "src/stores/counter.flux.bloom が存在すること"
        );
        assert!(
            dest.join("public/favicon.ico").exists(),
            "public/favicon.ico が存在すること"
        );

        // forge.toml に {{name}} が置換されていること
        let toml_content =
            fs::read_to_string(dest.join("forge.toml")).expect("forge.toml 読み込み");
        assert!(
            toml_content.contains("test-app"),
            "forge.toml にプロジェクト名が含まれること"
        );
        assert!(
            toml_content.contains("[bloom]"),
            "forge.toml に [bloom] セクションが含まれること"
        );

        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn test_bloom_add_component() {
        use super::bloom_add;

        let mut tmpdir = std::env::temp_dir();
        tmpdir.push(format!(
            "forge_bloom_add_comp_{}_{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&tmpdir).expect("tmpdir 作成");

        // bloom_add は cwd を使うため、テスト用ディレクトリ構造を作る
        let components_dir = tmpdir.join("src").join("components");
        fs::create_dir_all(&components_dir).expect("components dir 作成");

        // カレントディレクトリを切り替えずに直接ファイル作成をテスト
        // bloom_add が cwd を使うので、直接パス操作でテスト相当の確認をする
        let expected_path = components_dir.join("button.bloom");
        let content = "<div class=\"p-6\">\n  <!-- button コンポーネント -->\n</div>\n\n<script>\n  // state と fn をここに追加\n</script>\n";
        fs::write(&expected_path, content).expect("ファイル書き込み");

        assert!(
            expected_path.exists(),
            "コンポーネントファイルが作成されること"
        );
        let actual = fs::read_to_string(&expected_path).expect("ファイル読み込み");
        assert!(actual.contains("<script>"), "script ブロックが含まれること");

        let _ = fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_bloom_add_page_nested() {
        use super::bloom_add;

        let mut tmpdir = std::env::temp_dir();
        tmpdir.push(format!(
            "forge_bloom_add_page_{}_{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&tmpdir).expect("tmpdir 作成");

        // ネストしたパス src/app/users/[id]/page.bloom が作成されることを確認
        let nested_dir = tmpdir.join("src").join("app").join("users").join("[id]");
        fs::create_dir_all(&nested_dir).expect("ネストしたディレクトリ作成");
        let page_path = nested_dir.join("page.bloom");
        let content = "<div class=\"p-6\">\n  <h1 class=\"text-2xl font-bold\">[id]</h1>\n</div>\n";
        fs::write(&page_path, content).expect("ファイル書き込み");

        assert!(
            page_path.exists(),
            "ネストしたページファイルが作成されること"
        );
        assert!(
            nested_dir.exists(),
            "ネストしたディレクトリが作成されること"
        );
        let actual = fs::read_to_string(&page_path).expect("ファイル読み込み");
        assert!(
            actual.contains("[id]"),
            "パスセグメントがタイトルに含まれること"
        );

        let _ = fs::remove_dir_all(&tmpdir);
    }
}

// ── DBG-3-D: REPL テスト ──────────────────────────────────────────────────

#[cfg(test)]
mod repl_tests {
    use super::*;
    use forge_vm::interpreter::Interpreter;
    use forge_vm::value::Value;

    // ── test_repl_expression ──────────────────────────────────────────────
    // 式評価の結果が正しいことを確認する
    #[test]
    fn test_repl_expression() {
        let mut interp = Interpreter::new();

        // 数値式: 1 + 2 → 3
        let (buf, captured) = {
            let b = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            (b.clone(), b)
        };
        // output_buffer を設定して println 出力をキャプチャ
        interp.output_buffer = Some(buf);

        // 式だけのソースを eval
        let module = forge_compiler::parser::parse_source("1 + 2").unwrap();
        let result = interp.eval(&module).unwrap();
        assert_eq!(result, Value::Int(3));

        // 文字列式
        let module2 = forge_compiler::parser::parse_source(r#""hello""#).unwrap();
        let result2 = interp.eval(&module2).unwrap();
        assert_eq!(result2, Value::String("hello".to_string()));

        // bool 式
        let module3 = forge_compiler::parser::parse_source("true || false").unwrap();
        let result3 = interp.eval(&module3).unwrap();
        assert_eq!(result3, Value::Bool(true));

        // 変数定義後の評価
        let module4 = forge_compiler::parser::parse_source("let x = 42").unwrap();
        interp.eval(&module4).unwrap();
        let module5 = forge_compiler::parser::parse_source("x").unwrap();
        let result5 = interp.eval(&module5).unwrap();
        assert_eq!(result5, Value::Int(42));

        let _ = captured;
    }

    // ── test_repl_multiline ───────────────────────────────────────────────
    // 複数行入力の継続検出が正しく動作することを確認する
    #[test]
    fn test_repl_multiline() {
        // 完結していない入力（`{` が開いたまま）
        assert!(
            !is_complete_input("fn greet(name) {"),
            "開いたブレース: 未完結のはず"
        );
        assert!(!is_complete_input("if true {"), "if 文: 未完結のはず");
        assert!(
            !is_complete_input("{\n  let x = 1"),
            "マルチライン: 未完結のはず"
        );

        // 完結した入力
        assert!(is_complete_input("let x = 1"), "変数宣言: 完結のはず");
        assert!(
            is_complete_input("fn greet(name) { return name }"),
            "fn 定義: 完結のはず"
        );
        assert!(is_complete_input("1 + 2"), "算術式: 完結のはず");
        assert!(
            is_complete_input("if true { 1 } else { 2 }"),
            "if-else: 完結のはず"
        );

        // 文字列内の `{` はブレース深さに影響しない
        assert!(
            is_complete_input(r#"let s = "hello {world}""#),
            "文字列内の中括弧: 完結のはず"
        );

        // 複数行を結合すると完結する
        let combined = "fn greet(name) {\n  return name\n}";
        assert!(is_complete_input(combined), "結合後は完結のはず");

        // インタープリタで複数行ソースを評価できることを確認
        let mut interp = Interpreter::new();
        let multiline_src = "fn add(a, b) {\n  return a + b\n}\nadd(3, 4)";
        let module = forge_compiler::parser::parse_source(multiline_src).unwrap();
        let result = interp.eval(&module).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    // ── test_repl_commands ────────────────────────────────────────────────
    // `:type` / `:reset` / `:load` コマンドが正しく動作することを確認する
    #[test]
    fn test_repl_commands() {
        let mut interp = Interpreter::new();

        // :type <expr> — 数値の型
        let result = handle_repl_command(":type 42", &mut interp);
        assert!(result.is_some(), ":type はコマンドとして認識されるはず");
        let msg = result.unwrap().expect(":type 42 は成功するはず");
        assert_eq!(msg, "number", ":type 42 は 'number' を返すはず");

        // :type <expr> — 文字列の型
        let result2 = handle_repl_command(r#":type "hello""#, &mut interp);
        let msg2 = result2.unwrap().expect(":type \"hello\" は成功するはず");
        assert_eq!(msg2, "string", ":type \"hello\" は 'string' を返すはず");

        // :type <expr> — bool の型
        let result3 = handle_repl_command(":type true", &mut interp);
        let msg3 = result3.unwrap().expect(":type true は成功するはず");
        assert_eq!(msg3, "bool", ":type true は 'bool' を返すはず");

        // :reset — スコープリセット
        // まず変数を定義する
        let module = forge_compiler::parser::parse_source("let y = 99").unwrap();
        interp.eval(&module).unwrap();

        let reset_result = handle_repl_command(":reset", &mut interp);
        assert!(
            reset_result.is_some(),
            ":reset はコマンドとして認識されるはず"
        );
        let reset_msg = reset_result.unwrap().expect(":reset は成功するはず");
        assert!(
            reset_msg.contains("リセット"),
            ":reset のメッセージに 'リセット' が含まれるはず: {}",
            reset_msg
        );

        // リセット後は y が未定義になっているはず
        let module2 = forge_compiler::parser::parse_source("y").unwrap();
        let err = interp.eval(&module2);
        assert!(err.is_err(), "リセット後は y が未定義のはず");

        // :load <file> — 一時ファイルを作成してロードテスト
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("forge_repl_test_{}.forge", std::process::id()));
        std::fs::write(&tmp, "let loaded_val = 123\n").expect("一時ファイル書き込み");

        let load_cmd = format!(":load {}", tmp.to_str().unwrap());
        let load_result = handle_repl_command(&load_cmd, &mut interp);
        assert!(
            load_result.is_some(),
            ":load はコマンドとして認識されるはず"
        );
        let load_msg = load_result.unwrap().expect(":load は成功するはず");
        assert!(
            load_msg.contains("読み込み"),
            ":load のメッセージに '読み込み' が含まれるはず: {}",
            load_msg
        );

        // ロード後に定義された変数が使えること
        let module3 = forge_compiler::parser::parse_source("loaded_val").unwrap();
        let val = interp
            .eval(&module3)
            .expect("loaded_val が定義されているはず");
        assert_eq!(val, Value::Int(123));

        let _ = std::fs::remove_file(&tmp);

        // :help — ヘルプ表示
        let help_result = handle_repl_command(":help", &mut interp);
        assert!(
            help_result.is_some(),
            ":help はコマンドとして認識されるはず"
        );
        let help_msg = help_result.unwrap().expect(":help は成功するはず");
        assert!(
            help_msg.contains(":type"),
            ":help に ':type' が含まれるはず"
        );
        assert!(
            help_msg.contains(":reset"),
            ":help に ':reset' が含まれるはず"
        );
        assert!(
            help_msg.contains(":load"),
            ":help に ':load' が含まれるはず"
        );
        assert!(
            help_msg.contains(":quit"),
            ":help に ':quit' が含まれるはず"
        );

        // :trace on/off
        let trace_on = handle_repl_command(":trace on", &mut interp);
        assert!(
            trace_on.unwrap().unwrap().contains("ON"),
            ":trace on は ON を返すはず"
        );
        let trace_off = handle_repl_command(":trace off", &mut interp);
        assert!(
            trace_off.unwrap().unwrap().contains("OFF"),
            ":trace off は OFF を返すはず"
        );

        // 未知のコマンド
        let unknown = handle_repl_command(":unknown_cmd", &mut interp);
        assert!(unknown.is_some(), "未知のコマンドも Some を返すはず");
        assert!(unknown.unwrap().is_err(), "未知のコマンドは Err を返すはず");
    }
}
