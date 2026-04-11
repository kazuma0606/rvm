// forge-mcp: parse_file ツール

use serde_json::Value;

pub fn call(args: &Value) -> Value {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => {
            return serde_json::json!({
                "content": [{"type": "text", "text": "{\"ok\":false,\"errors\":[{\"message\":\"path パラメータが必要です\"}]}"}]
            });
        }
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            let result = serde_json::json!({
                "ok": false,
                "errors": [{"message": format!("ファイルを読み込めません: {}", e)}]
            });
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    match forge_compiler::parser::parse_source(&src) {
        Ok(_) => {
            let result = serde_json::json!({
                "ok": true,
                "message": "パース成功"
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
        Err(e) => {
            // ParseError から行・カラムを抽出する
            let err_str = e.to_string();
            let (line, col) = extract_line_col(&err_str);
            let result = serde_json::json!({
                "ok": false,
                "errors": [{
                    "message": err_str,
                    "line": line,
                    "col": col
                }]
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
    }
}

/// エラーメッセージから行番号・カラム番号を抽出する
fn extract_line_col(msg: &str) -> (usize, usize) {
    // "構文エラー: ... (N:M)" 形式
    if let Some(paren_start) = msg.rfind('(') {
        if let Some(paren_end) = msg.rfind(')') {
            if paren_start < paren_end {
                let inner = &msg[paren_start + 1..paren_end];
                let parts: Vec<&str> = inner.split(':').collect();
                if parts.len() == 2 {
                    let line = parts[0].trim().parse::<usize>().unwrap_or(0);
                    let col = parts[1].trim().parse::<usize>().unwrap_or(0);
                    return (line, col);
                }
            }
        }
    }
    (0, 0)
}
