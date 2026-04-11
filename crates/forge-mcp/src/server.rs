// forge-mcp: MCP サーバーハンドラ

use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::protocol::{make_error, make_result, Request};
use crate::state::McpSessionState;
use crate::tools;

pub struct McpServer {
    state: Arc<Mutex<McpSessionState>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(McpSessionState::new())),
        }
    }

    /// リクエストを処理してレスポンスを返す
    /// 通知（id なし）は None を返す
    pub fn handle(&self, req: &Request) -> Option<Value> {
        let id = req.id.clone().unwrap_or(Value::Null);
        let is_notification = req.id.is_none();

        match req.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "forge-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                Some(make_result(id, result))
            }

            "notifications/initialized" => {
                // 通知なのでレスポンスなし
                None
            }

            "tools/list" => {
                let result = serde_json::json!({
                    "tools": tools::tool_list()
                });
                Some(make_result(id, result))
            }

            "tools/call" => {
                let tool_name = match req.params.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => {
                        return Some(make_error(
                            id,
                            -32602,
                            "tools/call には name パラメータが必要です",
                        ));
                    }
                };

                let args = req.params.get("arguments").unwrap_or(&Value::Null);

                // セッション状態を更新
                if let Ok(mut state) = self.state.lock() {
                    state.record_request(&tool_name);
                }

                let result = tools::dispatch(&tool_name, args);

                if is_notification {
                    None
                } else {
                    Some(make_result(id, result))
                }
            }

            _ => {
                if is_notification {
                    // 未知の通知は無視
                    None
                } else {
                    Some(make_error(
                        id,
                        -32601,
                        &format!("不明なメソッド: {}", req.method),
                    ))
                }
            }
        }
    }
}
