// forge-mcp: MCP サーバーライブラリ

pub mod daemon;
pub mod log;
pub mod state;

mod protocol;
mod server;
mod tools;

use std::io::{self, BufRead, Write};

use server::McpServer;

/// stdio モードで MCP サーバーを起動する
/// stdin から1行ずつ JSON-RPC メッセージを読み、stdout に応答する
pub fn run_stdio() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let server = McpServer::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: protocol::Request = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                // パースエラーは JSON-RPC エラーレスポンスとして返す
                let resp = protocol::make_error(
                    serde_json::Value::Null,
                    -32700,
                    &format!("JSON パースエラー: {}", e),
                );
                let mut out = stdout.lock();
                let _ = writeln!(out, "{}", resp);
                let _ = out.flush();
                continue;
            }
        };

        if let Some(resp) = server.handle(&req) {
            let mut out = stdout.lock();
            let _ = writeln!(out, "{}", resp);
            let _ = out.flush();
        }
    }
}

/// --daemon-inner フラグで起動した場合のエントリポイント
/// ログを書き込みながら run_stdio() を実行する
pub fn run_daemon_inner() {
    run_stdio()
}

/// ログを表示する
pub fn show_logs(follow: bool, errors_only: bool) -> Result<(), String> {
    log::show_logs(follow, errors_only)
}

/// ログをクリアする
pub fn clear_logs() -> Result<(), String> {
    log::clear_logs()
}
