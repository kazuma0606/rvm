// forge-mcp: type_check ツール

use serde_json::Value;

pub fn call(args: &Value) -> Value {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => {
            let result = serde_json::json!({
                "ok": false,
                "errors": [{"message": "path パラメータが必要です"}]
            });
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
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

    let errors = forge_compiler::typechecker::type_check_source(&src);

    if errors.is_empty() {
        let result = serde_json::json!({
            "ok": true,
            "errors": []
        });
        serde_json::json!({
            "content": [{"type": "text", "text": result.to_string()}]
        })
    } else {
        let error_list: Vec<Value> = errors
            .iter()
            .map(|e| {
                let line = e.span.as_ref().map(|s| s.line).unwrap_or(0);
                serde_json::json!({
                    "message": e.message,
                    "line": line
                })
            })
            .collect();
        let result = serde_json::json!({
            "ok": false,
            "errors": error_list
        });
        serde_json::json!({
            "content": [{"type": "text", "text": result.to_string()}]
        })
    }
}
