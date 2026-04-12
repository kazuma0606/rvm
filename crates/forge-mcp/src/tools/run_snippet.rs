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

    // 出力をキャプチャするインタープリタを生成（MCP の stdout を汚染しない）
    let (mut interp, output_buf) = forge_vm::interpreter::Interpreter::with_output_capture();

    match interp.eval(&module) {
        Ok(_) => {
            let output = output_buf.lock().map(|b| b.clone()).unwrap_or_default();
            let result = serde_json::json!({
                "ok": true,
                "output": output
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
        Err(e) => {
            let output = output_buf.lock().map(|b| b.clone()).unwrap_or_default();
            let result = serde_json::json!({
                "ok": false,
                "error": format!("{}", e),
                "output": output
            });
            serde_json::json!({
                "content": [{"type": "text", "text": result.to_string()}]
            })
        }
    }
}
