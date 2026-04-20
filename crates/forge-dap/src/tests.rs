// forge-dap: テスト（DBG-4-H）

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufReader, Cursor};
    use std::sync::Arc;

    use serde_json::json;

    use crate::adapter::{ControlState, DapDebugHook, DapServer, HookEvent, StepMode};
    use crate::protocol::{read_message, write_message};
    use forge_compiler::lexer::Span;
    use forge_vm::interpreter::DebugHook;
    use forge_vm::value::Value;

    // ── ヘルパー ──────────────────────────────────────────────────────────

    fn make_span(line: usize) -> Span {
        Span {
            file: "test.forge".to_string(),
            start: 0,
            end: 0,
            line,
            col: 0,
        }
    }

    fn dap_message(seq: i64, command: &str, args: Option<serde_json::Value>) -> Vec<u8> {
        let mut msg = json!({
            "seq": seq,
            "type": "request",
            "command": command,
        });
        if let Some(a) = args {
            msg["arguments"] = a;
        }
        let body = serde_json::to_string(&msg).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    // ── DBG-4-H: test_dap_initialize ─────────────────────────────────────

    /// initialize シーケンスが正しい（DBG-4-H）
    #[test]
    fn test_dap_initialize() {
        let mut server = DapServer::new();
        let input = dap_message(
            1,
            "initialize",
            Some(json!({
                "adapterID": "forge",
                "clientID": "vscode",
                "linesStartAt1": true,
            })),
        );
        let mut reader = BufReader::new(Cursor::new(input));
        let mut output = Vec::<u8>::new();

        let cont = server.handle_request(&mut reader, &mut output).unwrap();
        assert!(cont, "handle_request should return true");

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("\"command\":\"initialize\""),
            "should contain initialize response"
        );
        assert!(
            output_str.contains("\"success\":true"),
            "initialize should succeed"
        );
        assert!(
            output_str.contains("\"event\":\"initialized\""),
            "should send initialized event"
        );
    }

    // ── DBG-4-H: test_dap_breakpoint ─────────────────────────────────────

    /// ブレークポイントで停止する（DBG-4-H）
    #[test]
    fn test_dap_breakpoint() {
        let ctrl = ControlState::new();
        let (tx, rx) = std::sync::mpsc::sync_channel(64);

        // ブレークポイントを登録
        {
            let mut bps = ctrl.breakpoints.lock().unwrap();
            bps.push(crate::adapter::RegisteredBreakpoint {
                id: 1,
                file: "test.forge".to_string(),
                line: 5,
                condition: None,
            });
        }

        let mut hook = DapDebugHook {
            ctrl: Arc::clone(&ctrl),
            sender: tx,
            stopped_scopes: Vec::new(),
        };

        // 行 3 でのフック → 停止しない（チャンネルにイベントが来ない）
        // 別スレッドで hook を実行する（停止するとブロックするため）
        let ctrl2 = Arc::clone(&ctrl);
        let span3 = make_span(3);
        hook.on_statement(&span3, &[HashMap::new()]);
        let event = rx.try_recv();
        assert!(event.is_err(), "should not stop at line 3");

        // 行 5 でのフック → 停止する
        // フックは stopped イベントを送った後、paused ロックで待機する
        // テスト用に別スレッドで実行して、すぐに再開させる
        let (tx2, rx2) = std::sync::mpsc::sync_channel(64);
        let mut hook2 = DapDebugHook {
            ctrl: Arc::clone(&ctrl),
            sender: tx2,
            stopped_scopes: Vec::new(),
        };

        let ctrl3 = Arc::clone(&ctrl);
        let handle = std::thread::spawn(move || {
            // 少し待ってから再開
            std::thread::sleep(std::time::Duration::from_millis(10));
            *ctrl3.sync.paused.lock().unwrap() = Some(StepMode::Continue);
            ctrl3.sync.resume.notify_all();
        });

        let span5 = make_span(5);
        hook2.on_statement(&span5, &[HashMap::new()]);
        handle.join().unwrap();

        let event = rx2.try_recv();
        assert!(event.is_ok(), "should stop at line 5");
        if let Ok(HookEvent::Stopped { reason, line, .. }) = event {
            assert_eq!(reason, "breakpoint");
            assert_eq!(line, 5);
        } else {
            panic!("expected Stopped event");
        }
    }

    // ── DBG-4-H: test_dap_step_over ──────────────────────────────────────

    /// Step Over が正しく動作する（DBG-4-H）
    #[test]
    fn test_dap_step_over() {
        let ctrl = ControlState::new();
        let (tx, _rx) = std::sync::mpsc::sync_channel(64);

        // Step Over モードを設定（depth=0）
        *ctrl.step_mode.lock().unwrap() = StepMode::Next { depth: 0 };
        *ctrl.call_depth.lock().unwrap() = 0;

        let mut hook = DapDebugHook {
            ctrl: Arc::clone(&ctrl),
            sender: tx.clone(),
            stopped_scopes: Vec::new(),
        };

        // depth=0 で文実行 → 停止する
        let ctrl2 = Arc::clone(&ctrl);
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            *ctrl2.sync.paused.lock().unwrap() = Some(StepMode::Continue);
            ctrl2.sync.resume.notify_all();
        });
        hook.on_statement(&make_span(10), &[HashMap::new()]);
        handle.join().unwrap();

        // depth=1（関数内）で Next { depth: 0 } → 停止しない
        let (tx3, rx3) = std::sync::mpsc::sync_channel(64);
        *ctrl.step_mode.lock().unwrap() = StepMode::Next { depth: 0 };
        *ctrl.call_depth.lock().unwrap() = 1;

        let mut hook3 = DapDebugHook {
            ctrl: Arc::clone(&ctrl),
            sender: tx3,
            stopped_scopes: Vec::new(),
        };
        hook3.on_statement(&make_span(11), &[HashMap::new()]);
        let event = rx3.try_recv();
        assert!(
            event.is_err(),
            "should not stop inside deeper call with Step Over"
        );
    }

    // ── DBG-4-H: test_dap_variables ──────────────────────────────────────

    /// 変数一覧が正しく返る（DBG-4-H）
    #[test]
    fn test_dap_variables() {
        let mut server = DapServer::new();

        // 停止状態をシミュレート: locals を注入
        server.inject_stopped_locals(vec![
            (
                "x".to_string(),
                json!({
                    "name": "x",
                    "value": "42",
                    "type": "number",
                    "hasChildren": false,
                }),
            ),
            (
                "name".to_string(),
                json!({
                    "name": "name",
                    "value": "\"alice\"",
                    "type": "string",
                    "hasChildren": false,
                }),
            ),
        ]);

        let vars = server.variables_for_test(1);
        assert!(
            vars.iter()
                .any(|v| v.get("name").and_then(|n| n.as_str()) == Some("x")),
            "should contain variable 'x'"
        );
        assert!(
            vars.iter()
                .any(|v| v.get("value").and_then(|n| n.as_str()) == Some("42")),
            "x should have value 42"
        );
        assert!(
            vars.iter()
                .any(|v| v.get("name").and_then(|n| n.as_str()) == Some("name")),
            "should contain variable 'name'"
        );
    }

    // ── DBG-4-H: test_dap_evaluate ───────────────────────────────────────

    /// 任意の式を評価できる（DBG-4-H）
    #[test]
    fn test_dap_evaluate() {
        let mut server = DapServer::new();

        // locals を注入
        server.inject_stopped_locals(vec![
            (
                "x".to_string(),
                json!({
                    "name": "x",
                    "value": "10",
                    "type": "number",
                    "hasChildren": false,
                }),
            ),
            (
                "y".to_string(),
                json!({
                    "name": "y",
                    "value": "5",
                    "type": "number",
                    "hasChildren": false,
                }),
            ),
        ]);

        // 単純変数評価
        let result = server.evaluate_for_test("x");
        assert!(result.is_ok(), "should evaluate 'x': {:?}", result);
        assert_eq!(result.unwrap(), "10");

        // 加算式評価
        let result2 = server.evaluate_for_test("x + y");
        assert!(result2.is_ok(), "should evaluate 'x + y': {:?}", result2);
        assert_eq!(result2.unwrap(), "15");
    }

    // ── プロトコル送受信テスト ─────────────────────────────────────────────

    #[test]
    fn test_dap_protocol_roundtrip() {
        let original = json!({
            "seq": 1,
            "type": "request",
            "command": "initialize",
            "arguments": { "adapterID": "forge" }
        });

        let mut buf = Vec::<u8>::new();
        write_message(&mut buf, &original).unwrap();

        let mut reader = BufReader::new(Cursor::new(buf));
        let parsed = read_message(&mut reader).unwrap().unwrap();

        assert_eq!(parsed["command"], "initialize");
        assert_eq!(parsed["arguments"]["adapterID"], "forge");
    }
}
