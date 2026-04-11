// forge-mcp: JSON-RPC 2.0 型定義

use serde::Deserialize;
use serde_json::Value;

/// JSON-RPC 2.0 リクエスト（通知も同じ型で扱う）
#[derive(Debug, Deserialize)]
pub struct Request {
    /// id フィールドがなければ通知（None）
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// 成功レスポンスを生成する
pub fn make_result(id: Value, result: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

/// エラーレスポンスを生成する
pub fn make_error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
