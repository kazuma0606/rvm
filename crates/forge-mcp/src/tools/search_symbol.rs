// forge-mcp: search_symbol ツール

use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn call(args: &Value) -> Value {
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            let result = serde_json::json!({"symbols": []});
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    let kind_filter = args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let current_dir = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => {
            let result = serde_json::json!({"symbols": []});
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    let mut forge_files: Vec<PathBuf> = Vec::new();
    collect_forge_files(&current_dir, 0, 3, &mut forge_files);

    let mut symbols: Vec<Value> = Vec::new();

    for file_path in &forge_files {
        let src = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let module = match forge_compiler::parser::parse_source(&src) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_str = file_path.to_string_lossy().to_string();

        for stmt in &module.stmts {
            use forge_compiler::ast::Stmt;
            match stmt {
                Stmt::Fn {
                    name: fn_name,
                    span,
                    ..
                } => {
                    if fn_name.contains(&name) || name.is_empty() {
                        if kind_filter.as_deref().map(|k| k == "fn").unwrap_or(true) {
                            symbols.push(serde_json::json!({
                                "name": fn_name,
                                "kind": "fn",
                                "file": file_str,
                                "line": span.line
                            }));
                        }
                    }
                }
                Stmt::StructDef {
                    name: struct_name,
                    span,
                    ..
                } => {
                    if struct_name.contains(&name) || name.is_empty() {
                        if kind_filter
                            .as_deref()
                            .map(|k| k == "struct")
                            .unwrap_or(true)
                        {
                            symbols.push(serde_json::json!({
                                "name": struct_name,
                                "kind": "struct",
                                "file": file_str,
                                "line": span.line
                            }));
                        }
                    }
                }
                Stmt::EnumDef {
                    name: enum_name,
                    span,
                    ..
                } => {
                    if enum_name.contains(&name) || name.is_empty() {
                        if kind_filter.as_deref().map(|k| k == "enum").unwrap_or(true) {
                            symbols.push(serde_json::json!({
                                "name": enum_name,
                                "kind": "enum",
                                "file": file_str,
                                "line": span.line
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let result = serde_json::json!({"symbols": symbols});
    serde_json::json!({
        "content": [{"type": "text", "text": result.to_string()}]
    })
}

fn collect_forge_files(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_forge_files(&path, depth + 1, max_depth, out);
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "fg" || ext == "forge" {
                out.push(path);
            }
        }
    }
}
