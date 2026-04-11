// forge-mcp: get_spec_section ツール

use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn call(args: &Value) -> Value {
    let section = match args.get("section").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            let result = serde_json::json!({"found": false, "section": "", "content": ""});
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    let spec_dir = find_spec_dir();

    let md_files = match &spec_dir {
        Some(dir) => collect_md_files(dir),
        None => {
            let result = serde_json::json!({"found": false, "section": "", "content": ""});
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    for file_path in &md_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Some(section_content) = extract_section(&content, &section) {
            let result = serde_json::json!({
                "found": true,
                "section": section,
                "content": section_content
            });
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    }

    let result = serde_json::json!({"found": false, "section": "", "content": ""});
    serde_json::json!({
        "content": [{"type": "text", "text": result.to_string()}]
    })
}

/// FORGE_SPEC_DIR 環境変数またはカレントディレクトリから lang/ を探す
fn find_spec_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FORGE_SPEC_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }

    // current_dir から上に遡って lang/ を探す
    let current = std::env::current_dir().ok()?;
    let mut dir = current.as_path();
    loop {
        let candidate = dir.join("lang");
        if candidate.exists() && candidate.is_dir() {
            return Some(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    None
}

/// ディレクトリ内の .md ファイルを再帰的に収集する
fn collect_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_md_files_inner(dir, &mut files);
    files
}

fn collect_md_files_inner(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md_files_inner(&path, out);
        } else if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                out.push(path);
            }
        }
    }
}

/// markdown コンテンツからセクションを抽出する
/// section キーワードを含む見出し行から次の見出し行（同レベル以上）までを返す
fn extract_section(content: &str, keyword: &str) -> Option<String> {
    let lower_keyword = keyword.to_lowercase();
    let lines: Vec<&str> = content.lines().collect();

    let mut start_idx: Option<usize> = None;
    let mut start_level: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            let heading_text = line.trim_start_matches('#').trim().to_lowercase();

            if heading_text.contains(&lower_keyword) {
                start_idx = Some(i);
                start_level = level;
                break;
            }
        }
    }

    let start = start_idx?;

    // 次の同レベル以上の見出しを探す
    let mut end_idx = lines.len();
    for i in (start + 1)..lines.len() {
        let line = lines[i];
        if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            if level <= start_level {
                end_idx = i;
                break;
            }
        }
    }

    let section_lines = &lines[start..end_idx];
    Some(section_lines.join("\n"))
}
