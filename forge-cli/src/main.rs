// forge-cli: ForgeScript CLI
// Phase 2-D 実装

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};

use forge_compiler::parser::parse_source;
use forge_compiler::typechecker::type_check_source;
use forge_vm::interpreter::Interpreter;
use forge_vm::value::Value;
use forge_transpiler::transpile;

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
                let output = args.iter().position(|s| s == "-o")
                    .and_then(|i| args.get(i + 1));
                transpile_file(path, output.map(|s| s.as_str()));
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge transpile <file.forge> [-o output.rs]");
                std::process::exit(1);
            }
        }
        Some("build") => {
            if let Some(path) = args.get(2) {
                let output = args.iter().position(|s| s == "-o")
                    .and_then(|i| args.get(i + 1));
                build_file(path, output.map(|s| s.as_str()));
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge build <file.forge> [-o binary]");
                std::process::exit(1);
            }
        }
        Some("test") => {
            if let Some(path) = args.get(2) {
                // --filter オプションを手動でパース
                let filter = args.iter().position(|s| s == "--filter")
                    .and_then(|i| args.get(i + 1))
                    .map(|s| s.as_str());
                test_file(path, filter);
            } else {
                eprintln!("エラー: ファイルパスを指定してください");
                eprintln!("使用方法: forge test <file.forge> [--filter <pattern>]");
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

    let file_path = std::path::Path::new(path);
    let mut interp = Interpreter::with_file_path(file_path);
    if let Err(e) = interp.eval(&module) {
        eprintln!("実行エラー: {}", e);
        std::process::exit(1);
    }
}

fn test_file(path: &str, filter: Option<&str>) {
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
                        let has_use_decl = module.stmts.iter().any(|s| {
                            matches!(s, forge_compiler::ast::Stmt::UseDecl { .. })
                        });
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
                                        let entry = interp.loaded_modules
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
            return Some(Err(":reload にはモジュールパスを指定してください".to_string()));
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
                            let entry = interp.loaded_modules
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
                    Err(e) => Some(Err(format!("モジュール '{}' の再ロードに失敗しました: {}", path, e))),
                }
            }
            Err(e) => Some(Err(format!("パースエラー: {}", e))),
        }
    } else if let Some(rest) = input.strip_prefix(":unload ") {
        let path = rest.trim();
        if path.is_empty() {
            return Some(Err(":unload にはモジュールパスを指定してください".to_string()));
        }
        if interp.loaded_modules.contains_key(path) {
            interp.unload_module(path);
            Some(Ok(format!("✔ {} をアンロードしました", path)))
        } else {
            Some(Err(format!("モジュール '{}' はロードされていません", path)))
        }
    } else if input.starts_with(':') {
        // 未知のコマンド
        Some(Err(format!("不明なコマンド '{}'\n利用可能: :modules, :reload <path>, :unload <path>", input)))
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
                eprintln!("エラー: ファイル '{}' への書き込みに失敗しました: {}", out_path, e);
                std::process::exit(1);
            }
            println!("Rust コードを '{}' に書き込みました", out_path);
        }
        None => {
            print!("{}", rust_code);
        }
    }
}

fn build_file(path: &str, output: Option<&str>) {
    use std::path::Path;
    use std::process::Command;

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

    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("forge_out");

    // 一時 Cargo プロジェクトを作成する
    let mut proj_dir = std::env::temp_dir();
    proj_dir.push(format!("forge_build_{}", stem));

    if let Err(e) = fs::create_dir_all(proj_dir.join("src")) {
        eprintln!("エラー: 一時プロジェクトディレクトリを作成できませんでした: {}", e);
        std::process::exit(1);
    }

    // Cargo.toml を生成
    let cargo_toml = format!(
        "[package]\nname = \"{stem}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nanyhow = \"1\"\n"
    );
    if let Err(e) = fs::write(proj_dir.join("Cargo.toml"), cargo_toml) {
        eprintln!("エラー: Cargo.toml の書き込みに失敗しました: {}", e);
        std::process::exit(1);
    }

    // main.rs を生成
    if let Err(e) = fs::write(proj_dir.join("src/main.rs"), &rust_code) {
        eprintln!("エラー: main.rs の書き込みに失敗しました: {}", e);
        std::process::exit(1);
    }

    // 出力先バイナリパスを決定
    let binary_name = output.unwrap_or(stem);
    let out_dir = std::path::PathBuf::from("target/forge");
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("エラー: 出力ディレクトリを作成できませんでした: {}", e);
        std::process::exit(1);
    }
    // 絶対パスにする
    let out_dir_abs = match out_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => out_dir.clone(),
    };
    let binary_path = out_dir_abs.join(binary_name);

    // cargo build --release を呼び出す
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--manifest-path",
            proj_dir.join("Cargo.toml").to_str().unwrap_or(""),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            // バイナリを出力先にコピー
            let built_bin = proj_dir.join(format!("target/release/{}", stem));
            let built_bin_exe = proj_dir.join(format!("target/release/{}.exe", stem));
            let src_bin = if built_bin_exe.exists() { built_bin_exe } else { built_bin };

            if let Err(e) = fs::copy(&src_bin, &binary_path) {
                eprintln!("エラー: バイナリのコピーに失敗しました: {}", e);
                eprintln!("  コピー元: {}", src_bin.display());
                eprintln!("  コピー先: {}", binary_path.display());
                let _ = fs::remove_dir_all(&proj_dir);
                std::process::exit(1);
            }
            let _ = fs::remove_dir_all(&proj_dir);
            println!("ビルド成功: {}", binary_path.display());
        }
        Ok(s) => {
            let _ = fs::remove_dir_all(&proj_dir);
            eprintln!("cargo build がエラーを返しました (exit code: {:?})", s.code());
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

fn print_help() {
    println!("ForgeScript CLI");
    println!();
    println!("使用方法:");
    println!("  forge run <file.forge>              ファイルを読み込んで実行");
    println!("  forge test <file.forge>             インラインテストを実行");
    println!("  forge test <file.forge> --filter <pattern>  テスト名で絞り込み");
    println!("  forge check <file.forge>            型チェックのみ（実行しない）");
    println!("  forge transpile <file.forge>        Rust コードを stdout に出力");
    println!("  forge transpile <file.forge> -o out.rs  Rust コードをファイルに出力");
    println!("  forge build <file.forge>            ネイティブバイナリを生成");
    println!("  forge build <file.forge> -o myapp   出力バイナリ名を指定");
    println!("  forge repl                          対話型 REPL を起動");
    println!("  forge help                          このヘルプを表示");
}
