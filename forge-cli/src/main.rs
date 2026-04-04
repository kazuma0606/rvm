// forge-cli: ForgeScript CLI
// Phase 2-D 実装

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use forge_compiler::ast::{Stmt, UsePath};
use forge_compiler::deps::DepsManager;
use forge_compiler::parser::parse_source;
use forge_compiler::typechecker::type_check_source;
use forge_vm::interpreter::Interpreter;
use forge_vm::value::Value;
use forge_transpiler::transpile;

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    use std::process::Command;

    let entry_path = Path::new(path);

    let stem = entry_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("forge_out");

    // 一時 Cargo プロジェクトを作成する
    let mut proj_dir = std::env::temp_dir();
    proj_dir.push(format!("forge_build_{}_{}", stem, unique_suffix()));

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

    if let Err(e) = write_transpiled_project(entry_path, &proj_dir.join("src")) {
        eprintln!("エラー: トランスパイル済みプロジェクトの生成に失敗しました: {}", e);
        std::process::exit(1);
    }

    format_generated_project(&proj_dir);

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
            "--offline",
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
        .map_err(|_| format!("entry path '{}' is outside source root", entry_path.display()))?
        .to_path_buf();

    let mut deps = DepsManager::new();
    let module_index = build_module_index(&files, &entry_rel);

    for file in &files {
        collect_external_deps(&source_root, file, &mut deps)?;

        let mut rust_code = transpile(&file.source)
            .map_err(|e| format!("{}: {}", file.rel_path.display(), e))?;

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

    update_generated_cargo_toml(
        &out_src_dir.parent().unwrap_or(out_src_dir).join("Cargo.toml"),
        &deps,
    )?;

    Ok(())
}

fn detect_source_root(entry_path: &Path) -> Result<PathBuf, String> {
    let parent = entry_path
        .parent()
        .ok_or_else(|| format!("cannot determine parent directory for '{}'", entry_path.display()))?;

    if parent.file_name().and_then(|s| s.to_str()) == Some("src") {
        Ok(parent.to_path_buf())
    } else {
        Ok(parent.to_path_buf())
    }
}

fn collect_forge_files(source_root: &Path, entry_path: &Path) -> Result<Vec<ForgeSourceFile>, String> {
    if entry_path.file_name().and_then(|s| s.to_str()) != Some("main.forge") {
        let rel_path = entry_path
            .strip_prefix(source_root)
            .map_err(|_| format!("{} is outside {}", entry_path.display(), source_root.display()))?
            .to_path_buf();
        let source = fs::read_to_string(entry_path)
            .map_err(|e| format!("{}: {}", entry_path.display(), e))?;
        return Ok(vec![ForgeSourceFile { rel_path, source }]);
    }

    fn walk(dir: &Path, source_root: &Path, files: &mut Vec<ForgeSourceFile>) -> Result<(), String> {
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
            let source = fs::read_to_string(&path)
                .map_err(|e| format!("{}: {}", path.display(), e))?;
            files.push(ForgeSourceFile {
                rel_path,
                source,
            });
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
                    index.entry(rel_dir.clone()).or_default().insert(stem.to_string());
                }
            }
        }

        if !rel_dir.as_os_str().is_empty() {
            let parent = rel_dir.parent().map(Path::to_path_buf).unwrap_or_default();
            if let Some(dir_name) = rel_dir.file_name().and_then(|s| s.to_str()) {
                index.entry(parent).or_default().insert(dir_name.to_string());
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
        if let Stmt::UseDecl { path: UsePath::External(crate_name), .. } = stmt {
            let current_dir = source_root.join(
                file.rel_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
            );
            let first_segment = crate_name.split('/').next().unwrap_or(&crate_name);
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

    let mut others = crates
        .iter()
        .filter(|name| name.as_str() != "once_cell" && name.as_str() != "serde")
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

    let status = Command::new("cargo")
        .args([
            "fmt",
            "--all",
            "--manifest-path",
            proj_dir.join("Cargo.toml").to_str().unwrap_or(""),
        ])
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
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", std::process::id(), ts, seq)
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
