// forge-mcp: run_snippet ツール

use serde_json::Value;

pub fn call(args: &Value) -> Value {
    let code = match args.get("code").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => {
            let result = serde_json::json!({
                "ok": false,
                "error": "code パラメータが必要です"
            });
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    let module = match forge_compiler::parser::parse_source(&code) {
        Ok(m) => m,
        Err(e) => {
            let result = serde_json::json!({
                "ok": false,
                "error": format!("パースエラー: {}", e)
            });
            return serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            });
        }
    };

    let mut interp = forge_vm::interpreter::Interpreter::new();
    match interp.eval(&module) {
        Ok(value) => {
            let result = serde_json::json!({
                "ok": true,
                "output": format!("{:?}", value)
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
        Err(e) => {
            let result = serde_json::json!({
                "ok": false,
                "error": format!("{}", e)
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
    }
}
