// forge-dap: DAP アダプタ実装（DBG-4-B/C/D/E）
//
// アーキテクチャ:
// - メインスレッド: DAP メッセージを stdin/stdout でやり取りする
// - インタープリタスレッド: ForgeScript を実行する
// - 通信: std::sync::mpsc チャンネル経由（Value は Rc を含むため Send でない点を回避）
//
// インタープリタスレッドでフックが呼ばれた時、JSON シリアライズした変数情報を
// メインスレッドに送り、停止指示を受けるまで待機する。

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::{Arc, Condvar, Mutex};

use serde_json::{json, Value as JsonValue};

use forge_compiler::lexer::Span;
use forge_vm::interpreter::{DebugHook, Interpreter};
use forge_vm::value::Value;

use crate::protocol::{
    read_message, write_message, DapEvent, DapRequest, DapResponse, EvaluateArgs, LaunchArgs,
    SetBreakpointsArgs,
};
use crate::source_map::BloomSourceMap;

// ── 共有状態（スレッド間通信は JSON 文字列のみ使用）────────────────────────

/// ステップ実行モード
#[derive(Debug, Clone, PartialEq)]
pub enum StepMode {
    /// 自由実行中（ブレークポイントのみで停止）
    Continue,
    /// Step Over: 現在の呼び出し深度と同じか浅い次の文で停止
    Next { depth: usize },
    /// Step Into: 次の文（深度は問わず）で停止
    StepIn,
    /// Step Out: 現在の深度より浅くなったら停止
    StepOut { depth: usize },
    /// 停止要求（terminate）
    Terminate,
}

/// 登録済みブレークポイント
#[derive(Debug, Clone)]
pub struct RegisteredBreakpoint {
    pub id: i64,
    pub file: String,
    /// 1-indexed 行番号
    pub line: i64,
    pub condition: Option<String>,
}

/// フックからメインスレッドへの通知イベント
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// 停止イベント（理由, ファイル, 行, 変数JSON, スコープの変数名リスト）
    Stopped {
        reason: String,
        file: String,
        line: usize,
        col: usize,
        /// scope 0 (locals) の変数: name -> JsonValue
        locals: Vec<(String, JsonValue)>,
        /// scope 1 (globals) の変数: name -> JsonValue
        globals: Vec<(String, JsonValue)>,
    },
    /// プログラム終了
    Exited { exit_code: i32 },
    /// エラー出力
    Output { category: String, text: String },
}

/// インタープリタスレッドが停止中に待機するための同期オブジェクト
pub struct DebugSync {
    /// 停止中かどうか + 再開コマンド
    pub paused: Mutex<Option<StepMode>>,
    pub resume: Condvar,
}

impl DebugSync {
    pub fn new() -> Self {
        DebugSync {
            paused: Mutex::new(None),
            resume: Condvar::new(),
        }
    }
}

/// メインスレッドとフックスレッド間で共有する制御状態
pub struct ControlState {
    /// 登録済みブレークポイント
    pub breakpoints: Mutex<Vec<RegisteredBreakpoint>>,
    /// ステップ実行モード
    pub step_mode: Mutex<StepMode>,
    /// 現在の呼び出し深度
    pub call_depth: Mutex<usize>,
    /// 停止時同期
    pub sync: DebugSync,
    /// Bloom ソースマップ
    pub bloom_map: Mutex<Option<BloomSourceMap>>,
}

impl ControlState {
    pub fn new() -> Arc<Self> {
        Arc::new(ControlState {
            breakpoints: Mutex::new(Vec::new()),
            step_mode: Mutex::new(StepMode::Continue),
            call_depth: Mutex::new(0),
            sync: DebugSync::new(),
            bloom_map: Mutex::new(None),
        })
    }
}

// ── Value を JSON に変換 ─────────────────────────────────────────────────────

fn value_to_json_display(value: &Value) -> (String, String, bool) {
    // (type_str, display_str, has_children)
    match value {
        Value::Int(n) => ("number".to_string(), n.to_string(), false),
        Value::Float(f) => ("number".to_string(), f.to_string(), false),
        Value::Bool(b) => ("bool".to_string(), b.to_string(), false),
        Value::String(s) => ("string".to_string(), format!("\"{}\"", s), false),
        Value::Unit => ("unit".to_string(), "()".to_string(), false),
        Value::Option(None) => ("option".to_string(), "none".to_string(), false),
        Value::Option(Some(inner)) => {
            let (_, disp, _) = value_to_json_display(inner);
            ("option".to_string(), format!("some({})", disp), true)
        }
        Value::Result(Ok(v)) => {
            let (_, disp, _) = value_to_json_display(v);
            ("result".to_string(), format!("ok({})", disp), true)
        }
        Value::Result(Err(e)) => ("result".to_string(), format!("err(\"{}\")", e), false),
        Value::List(items) => {
            let len = items.borrow().len();
            ("list".to_string(), format!("[{} items]", len), len > 0)
        }
        Value::Map(map) => {
            let len = map.len();
            ("map".to_string(), format!("{{{} entries}}", len), len > 0)
        }
        Value::Struct { type_name, fields } => {
            let len = fields.borrow().len();
            (type_name.clone(), format!("{} {{...}}", type_name), len > 0)
        }
        Value::Closure { .. } => ("function".to_string(), "<closure>".to_string(), false),
        Value::NativeFunction(_) => ("function".to_string(), "<native fn>".to_string(), false),
        Value::Enum {
            type_name,
            variant,
            data,
        } => {
            use forge_vm::value::EnumData;
            let s = match data {
                EnumData::Unit => format!("{}::{}", type_name, variant),
                EnumData::Tuple(vs) => {
                    let inner = vs
                        .iter()
                        .map(|v| format!("{}", v))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}::{}({})", type_name, variant, inner)
                }
                EnumData::Struct(fields) => {
                    let inner = fields
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{}::{}{{ {} }}", type_name, variant, inner)
                }
            };
            (type_name.clone(), s, false)
        }
        _ => ("unknown".to_string(), format!("{}", value), false),
    }
}

/// スコープの変数を (name, JsonValue) リストに変換する（has_children 付き）
fn scope_to_json_vars(scope: &HashMap<String, (Value, bool)>) -> Vec<(String, JsonValue)> {
    let mut vars: Vec<(String, JsonValue)> = scope
        .iter()
        .map(|(name, (value, _))| {
            let (type_str, display, has_children) = value_to_json_display(value);
            let jv = json!({
                "name": name,
                "value": display,
                "type": type_str,
                "hasChildren": has_children,
            });
            (name.clone(), jv)
        })
        .collect();
    vars.sort_by(|(a, _), (b, _)| a.cmp(b));
    vars
}

// ── DAP デバッグフック実装 ────────────────────────────────────────────────────

/// インタープリタに差し込むデバッグフック実装
pub struct DapDebugHook {
    /// 共有制御状態
    pub ctrl: Arc<ControlState>,
    /// イベント通知チャンネル（インタープリタスレッド → メインスレッド）
    pub sender: std::sync::mpsc::SyncSender<HookEvent>,
    /// 停止時のスコープキャッシュ（evaluate 用）
    pub stopped_scopes: Vec<HashMap<String, (Value, bool)>>,
}

impl DebugHook for DapDebugHook {
    fn on_statement(&mut self, span: &Span, scopes: &[HashMap<String, (Value, bool)>]) {
        let should_stop = {
            let step_mode = self.ctrl.step_mode.lock().unwrap().clone();
            let call_depth = *self.ctrl.call_depth.lock().unwrap();
            match step_mode {
                StepMode::Terminate => {
                    // 終了要求があれば panic で脱出（簡易実装）
                    std::process::exit(0);
                }
                StepMode::Continue => {
                    let bps = self.ctrl.breakpoints.lock().unwrap();
                    bps.iter().any(|bp| {
                        bp.line == span.line as i64
                            && (span.file.ends_with(&bp.file) || bp.file.ends_with(&span.file))
                    })
                }
                StepMode::Next { depth } => call_depth <= depth,
                StepMode::StepIn => true,
                StepMode::StepOut { depth } => call_depth < depth,
            }
        };

        if should_stop {
            self.stopped_scopes = scopes.to_vec();

            // Bloom ソースマップで行番号変換
            let (display_file, display_line) = {
                let bloom_lock = self.ctrl.bloom_map.lock().unwrap();
                if let Some(bmap) = bloom_lock.as_ref() {
                    bmap.forge_to_bloom(&span.file, span.line)
                        .unwrap_or_else(|| (span.file.clone(), span.line))
                } else {
                    (span.file.clone(), span.line)
                }
            };

            // ステップモードリセット
            {
                let mut step_mode = self.ctrl.step_mode.lock().unwrap();
                let reason = match &*step_mode {
                    StepMode::Continue => "breakpoint",
                    _ => "step",
                };
                *step_mode = StepMode::Continue;

                // スコープを JSON に変換してイベント送信
                let locals = scopes.last().map(scope_to_json_vars).unwrap_or_default();
                let globals = scopes.first().map(scope_to_json_vars).unwrap_or_default();

                let _ = self.sender.send(HookEvent::Stopped {
                    reason: reason.to_string(),
                    file: display_file,
                    line: display_line,
                    col: span.col,
                    locals,
                    globals,
                });
            }

            // 再開指示が来るまで待機
            let mut paused = self.ctrl.sync.paused.lock().unwrap();
            loop {
                if paused.is_some() {
                    break;
                }
                paused = self.ctrl.sync.resume.wait(paused).unwrap();
            }
            // ステップモードを更新して継続
            if let Some(next_mode) = paused.take() {
                *self.ctrl.step_mode.lock().unwrap() = next_mode;
            }
        }
    }

    fn on_enter_fn(&mut self, _name: &str, _args: &[(String, Value)]) {
        let mut depth = self.ctrl.call_depth.lock().unwrap();
        *depth += 1;
    }

    fn on_exit_fn(&mut self, _name: &str, _ret: &Value) {
        let mut depth = self.ctrl.call_depth.lock().unwrap();
        if *depth > 0 {
            *depth -= 1;
        }
    }

    fn on_assign(&mut self, _name: &str, _value: &Value, _span: &Span) {
        // ウォッチポイント用（将来実装）
    }
}

// ── DAP サーバー ──────────────────────────────────────────────────────────────

pub struct DapServer {
    seq: i64,
    ctrl: Arc<ControlState>,
    /// フックからのイベント受信チャンネル
    event_receiver: std::sync::mpsc::Receiver<HookEvent>,
    /// フックへ渡す送信端
    event_sender: std::sync::mpsc::SyncSender<HookEvent>,
    /// 停止時のスコープ（JSON キャッシュ）
    stopped_locals: Vec<(String, JsonValue)>,
    stopped_globals: Vec<(String, JsonValue)>,
    stopped_file: String,
    stopped_line: usize,
    /// 変数参照ストア（variablesReference ID → (name, JSON children)）
    var_store: HashMap<i64, Vec<(String, JsonValue)>>,
    var_store_next: i64,
    /// インタープリタスレッドのハンドル
    interp_thread: Option<std::thread::JoinHandle<()>>,
    /// 停止時のスコープをインタープリタスレッドから転送するチャンネル
    /// （evaluate 用: stopped_scopes の Value を評価するためにインタープリタスレッドに式を送る）
    eval_request_tx: Option<
        std::sync::mpsc::SyncSender<(String, std::sync::mpsc::SyncSender<Result<String, String>>)>,
    >,
    // ── DBG-5-B: ホットリロード統合 ──────────────────────────────
    /// リロード後にブレークポイントを再登録するための設定を保持する。
    /// `forge serve --watch` が起動している間、フラグファイルを定期的にポーリングして
    /// リロードイベントを検知する。
    hot_reload_flag_path: Option<std::path::PathBuf>,
    /// 最後に読んだフラグファイルのタイムスタンプ（秒）
    hot_reload_last_ts: u64,
    /// ブレークポイント再登録が必要なことを示すフラグ
    needs_bp_reregister: bool,
}

impl DapServer {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel(64);
        DapServer {
            seq: 1,
            ctrl: ControlState::new(),
            event_receiver: rx,
            event_sender: tx,
            stopped_locals: Vec::new(),
            stopped_globals: Vec::new(),
            stopped_file: String::new(),
            stopped_line: 0,
            var_store: HashMap::new(),
            var_store_next: 1000,
            interp_thread: None,
            eval_request_tx: None,
            hot_reload_flag_path: None,
            hot_reload_last_ts: 0,
            needs_bp_reregister: false,
        }
    }

    // ── DBG-5-B: ホットリロード統合 ──────────────────────────────

    /// フラグファイルのパスを設定する（`forge serve --watch` と連携する）。
    /// `watch_dir/.forge-dap-reload` に書き込まれたタイムスタンプを定期的にポーリングして
    /// リロードイベントを検知し、ブレークポイントを再登録する。
    pub fn set_hot_reload_flag(&mut self, flag_path: std::path::PathBuf) {
        self.hot_reload_flag_path = Some(flag_path);
    }

    /// リロードフラグファイルを確認し、新しいリロードが発生していれば `true` を返す。
    fn check_hot_reload_flag(&mut self) -> bool {
        let Some(ref path) = self.hot_reload_flag_path else {
            return false;
        };
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return false,
        };
        let ts: u64 = content.trim().parse().unwrap_or(0);
        if ts > self.hot_reload_last_ts {
            self.hot_reload_last_ts = ts;
            true
        } else {
            false
        }
    }

    /// リロード後にブレークポイントを再登録する。
    /// 登録済みのブレークポイント一覧はそのまま保持し、インタープリタ側に通知するだけ。
    /// 実際の setBreakpoints は VS Code が再送するのを待つが、
    /// このメソッドで `needs_bp_reregister` フラグを立てて
    /// `drain_hook_events` 内でクライアントに `refreshed` イベントを送る。
    pub fn on_hot_reload<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        eprintln!("[forge-dap] ホットリロードを検知: ブレークポイントを再登録します");
        // セッションを維持したまま、まず continued イベントを送信して停止状態を解除
        self.send_event(
            writer,
            "continued",
            Some(serde_json::json!({ "threadId": 1, "allThreadsContinued": true })),
        )?;
        // ブレークポイント再登録フラグを立てる
        self.needs_bp_reregister = true;
        // 登録済みのブレークポイントを再検証済みとして VS Code に通知
        let bps_snapshot = {
            let guard = self
                .ctrl
                .breakpoints
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        for bp in &bps_snapshot {
            self.send_event(
                writer,
                "breakpoint",
                Some(serde_json::json!({
                    "reason": "changed",
                    "breakpoint": {
                        "id": bp.id,
                        "verified": true,
                        "line": bp.line,
                        "source": { "path": bp.file },
                    }
                })),
            )?;
        }
        eprintln!(
            "[forge-dap] {} 件のブレークポイントを再登録しました",
            bps_snapshot.len()
        );
        Ok(())
    }

    fn next_seq(&mut self) -> i64 {
        let s = self.seq;
        self.seq += 1;
        s
    }

    fn send_response<W: Write>(
        &mut self,
        writer: &mut W,
        request_seq: i64,
        command: &str,
        body: Option<JsonValue>,
    ) -> std::io::Result<()> {
        let seq = self.next_seq();
        let resp = DapResponse::success(seq, request_seq, command, body);
        let json = serde_json::to_value(&resp).unwrap_or(JsonValue::Null);
        write_message(writer, &json)
    }

    fn send_error_response<W: Write>(
        &mut self,
        writer: &mut W,
        request_seq: i64,
        command: &str,
        message: &str,
    ) -> std::io::Result<()> {
        let seq = self.next_seq();
        let resp = DapResponse::error(seq, request_seq, command, message);
        let json = serde_json::to_value(&resp).unwrap_or(JsonValue::Null);
        write_message(writer, &json)
    }

    fn send_event<W: Write>(
        &mut self,
        writer: &mut W,
        event: &str,
        body: Option<JsonValue>,
    ) -> std::io::Result<()> {
        let seq = self.next_seq();
        let ev = DapEvent::new(seq, event, body);
        let json = serde_json::to_value(&ev).unwrap_or(JsonValue::Null);
        write_message(writer, &json)
    }

    /// フックからのイベントを処理してクライアントに送信する
    fn drain_hook_events<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        // DBG-5-B: ホットリロードフラグを確認してブレークポイントを再登録する
        if self.check_hot_reload_flag() {
            self.on_hot_reload(writer)?;
        }

        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                HookEvent::Stopped {
                    reason,
                    file,
                    line,
                    col,
                    locals,
                    globals,
                } => {
                    self.stopped_locals = locals;
                    self.stopped_globals = globals;
                    self.stopped_file = file.clone();
                    self.stopped_line = line;
                    self.var_store.clear();
                    self.var_store_next = 1000;
                    self.send_event(
                        writer,
                        "stopped",
                        Some(json!({
                            "reason": reason,
                            "threadId": 1,
                            "allThreadsStopped": true,
                            "source": { "path": file },
                            "line": line,
                            "column": col,
                        })),
                    )?;
                }
                HookEvent::Exited { exit_code } => {
                    self.send_event(writer, "exited", Some(json!({ "exitCode": exit_code })))?;
                    self.send_event(writer, "terminated", Some(json!({})))?;
                }
                HookEvent::Output { category, text } => {
                    self.send_event(
                        writer,
                        "output",
                        Some(json!({
                            "category": category,
                            "output": text,
                        })),
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_request<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> std::io::Result<bool> {
        // フックイベントを先に処理
        self.drain_hook_events(writer)?;

        let msg = match read_message(reader)? {
            Some(m) => m,
            None => return Ok(false),
        };

        let request: DapRequest = match serde_json::from_value(msg) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[forge-dap] failed to parse request: {}", e);
                return Ok(true);
            }
        };

        let req_seq = request.seq;
        let command = request.command.clone();

        match command.as_str() {
            "initialize" => {
                let body = json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsSetBreakpointsRequest": true,
                    "supportsContinueRequest": true,
                    "supportsNextRequest": true,
                    "supportsStepInRequest": true,
                    "supportsStepOutRequest": true,
                    "supportsVariablesRequest": true,
                    "supportsScopesRequest": true,
                    "supportsEvaluateForHovers": true,
                    "supportsConditionalBreakpoints": true,
                    "supportsStackTraceRequest": true,
                });
                self.send_response(writer, req_seq, "initialize", Some(body))?;
                self.send_event(writer, "initialized", None)?;
            }
            "configurationDone" => {
                self.send_response(writer, req_seq, "configurationDone", None)?;
            }
            "launch" => {
                if let Some(args_val) = request.arguments {
                    match serde_json::from_value::<LaunchArgs>(args_val) {
                        Ok(args) => {
                            self.send_response(writer, req_seq, "launch", None)?;
                            self.launch_program(&args);
                        }
                        Err(e) => {
                            self.send_error_response(
                                writer,
                                req_seq,
                                "launch",
                                &format!("invalid launch args: {}", e),
                            )?;
                        }
                    }
                } else {
                    self.send_error_response(writer, req_seq, "launch", "missing arguments")?;
                }
            }
            "disconnect" | "terminate" => {
                // インタープリタスレッドに終了を通知
                *self.ctrl.step_mode.lock().unwrap() = StepMode::Terminate;
                // 停止中なら再開させてから終了
                *self.ctrl.sync.paused.lock().unwrap() = Some(StepMode::Terminate);
                self.ctrl.sync.resume.notify_all();
                self.send_response(writer, req_seq, &command, None)?;
                return Ok(false);
            }
            "setBreakpoints" => {
                if let Some(args_val) = request.arguments {
                    match serde_json::from_value::<SetBreakpointsArgs>(args_val) {
                        Ok(args) => {
                            let result_bps = self.handle_set_breakpoints(args);
                            self.send_response(
                                writer,
                                req_seq,
                                "setBreakpoints",
                                Some(json!({ "breakpoints": result_bps })),
                            )?;
                        }
                        Err(e) => {
                            self.send_error_response(
                                writer,
                                req_seq,
                                "setBreakpoints",
                                &format!("invalid args: {}", e),
                            )?;
                        }
                    }
                }
            }
            "threads" => {
                self.send_response(
                    writer,
                    req_seq,
                    "threads",
                    Some(json!({ "threads": [{ "id": 1, "name": "main" }] })),
                )?;
            }
            "stackTrace" => {
                let frame = if !self.stopped_file.is_empty() {
                    vec![json!({
                        "id": 0,
                        "name": "current",
                        "source": { "path": self.stopped_file },
                        "line": self.stopped_line,
                        "column": 0,
                    })]
                } else {
                    vec![]
                };
                let len = frame.len();
                self.send_response(
                    writer,
                    req_seq,
                    "stackTrace",
                    Some(json!({ "stackFrames": frame, "totalFrames": len })),
                )?;
            }
            "scopes" => {
                let local_count = self.stopped_locals.len();
                let global_count = self.stopped_globals.len();
                let scopes = vec![
                    json!({
                        "name": "Locals",
                        "variablesReference": 1,
                        "namedVariables": local_count,
                        "expensive": false,
                    }),
                    json!({
                        "name": "Globals",
                        "variablesReference": 2,
                        "namedVariables": global_count,
                        "expensive": false,
                    }),
                ];
                self.send_response(writer, req_seq, "scopes", Some(json!({ "scopes": scopes })))?;
            }
            "variables" => {
                if let Some(args_val) = request.arguments {
                    let var_ref = args_val
                        .get("variablesReference")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let vars = self.handle_variables(var_ref);
                    self.send_response(
                        writer,
                        req_seq,
                        "variables",
                        Some(json!({ "variables": vars })),
                    )?;
                }
            }
            "evaluate" => {
                if let Some(args_val) = request.arguments {
                    match serde_json::from_value::<EvaluateArgs>(args_val) {
                        Ok(args) => match self.handle_evaluate(&args.expression) {
                            Ok(val) => {
                                self.send_response(
                                    writer,
                                    req_seq,
                                    "evaluate",
                                    Some(json!({
                                        "result": val,
                                        "variablesReference": 0,
                                    })),
                                )?;
                            }
                            Err(e) => {
                                self.send_error_response(writer, req_seq, "evaluate", &e)?;
                            }
                        },
                        Err(e) => {
                            self.send_error_response(
                                writer,
                                req_seq,
                                "evaluate",
                                &format!("invalid args: {}", e),
                            )?;
                        }
                    }
                }
            }
            "continue" => {
                self.resume_interpreter(StepMode::Continue);
                self.send_response(
                    writer,
                    req_seq,
                    "continue",
                    Some(json!({ "allThreadsContinued": true })),
                )?;
            }
            "next" => {
                let depth = *self.ctrl.call_depth.lock().unwrap();
                self.resume_interpreter(StepMode::Next { depth });
                self.send_response(writer, req_seq, "next", None)?;
            }
            "stepIn" => {
                self.resume_interpreter(StepMode::StepIn);
                self.send_response(writer, req_seq, "stepIn", None)?;
            }
            "stepOut" => {
                let depth = *self.ctrl.call_depth.lock().unwrap();
                self.resume_interpreter(StepMode::StepOut { depth });
                self.send_response(writer, req_seq, "stepOut", None)?;
            }
            other => {
                eprintln!("[forge-dap] unknown command: {}", other);
                self.send_error_response(
                    writer,
                    req_seq,
                    other,
                    &format!("unknown command: {}", other),
                )?;
            }
        }

        // コマンド処理後にもフックイベントをドレイン
        self.drain_hook_events(writer)?;
        Ok(true)
    }

    fn resume_interpreter(&mut self, mode: StepMode) {
        *self.ctrl.sync.paused.lock().unwrap() = Some(mode);
        self.ctrl.sync.resume.notify_all();
    }

    fn handle_set_breakpoints(&mut self, args: SetBreakpointsArgs) -> Vec<JsonValue> {
        let source_path = args.source.path.unwrap_or_default();
        let mut bps = self.ctrl.breakpoints.lock().unwrap();
        bps.retain(|bp| !(bp.file.ends_with(&source_path) || source_path.ends_with(&bp.file)));

        let mut result_bps = Vec::new();
        let new_bps = args.breakpoints.unwrap_or_default();
        for (i, bp) in new_bps.into_iter().enumerate() {
            let id = (bps.len() + i + 1) as i64;
            bps.push(RegisteredBreakpoint {
                id,
                file: source_path.clone(),
                line: bp.line,
                condition: bp.condition.clone(),
            });
            result_bps.push(json!({
                "id": id,
                "verified": true,
                "line": bp.line,
                "source": { "path": source_path },
            }));
        }
        result_bps
    }

    fn handle_variables(&mut self, var_ref: i64) -> Vec<JsonValue> {
        if var_ref >= 1000 {
            return self
                .var_store
                .get(&var_ref)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(_, jv)| jv)
                .collect();
        }

        let vars = if var_ref == 1 {
            &self.stopped_locals
        } else {
            &self.stopped_globals
        };

        vars.iter()
            .map(|(name, jv)| {
                let mut v = jv.clone();
                // has_children があれば variablesReference を割り当て
                if jv
                    .get("hasChildren")
                    .and_then(|h| h.as_bool())
                    .unwrap_or(false)
                {
                    let ref_id = self.var_store_next;
                    self.var_store_next += 1;
                    // 子要素（簡易: 型情報のみ）
                    self.var_store
                        .insert(ref_id, vec![(name.clone(), jv.clone())]);
                    v["variablesReference"] = json!(ref_id);
                } else {
                    v["variablesReference"] = json!(0);
                }
                v
            })
            .collect()
    }

    fn handle_evaluate(&mut self, expression: &str) -> Result<String, String> {
        if self.stopped_locals.is_empty() && self.stopped_globals.is_empty() {
            return Err("not stopped".to_string());
        }

        // 停止中スコープからインタープリタを構築して式を評価
        let mut interp = Interpreter::new();
        for (name, jv) in &self.stopped_globals {
            if let Some(val) = json_to_value(jv) {
                interp.define_const(name, val);
            }
        }
        for (name, jv) in &self.stopped_locals {
            if let Some(val) = json_to_value(jv) {
                interp.define_const(name, val);
            }
        }

        match interp.eval_expr_str(expression) {
            Ok(val) => Ok(format!("{}", val)),
            Err(e) => Err(format!("{}", e)),
        }
    }

    fn launch_program(&mut self, args: &LaunchArgs) {
        let program_path = args.program.clone();
        let ctrl = Arc::clone(&self.ctrl);
        let sender = self.event_sender.clone();

        // Bloom ソースマップを読み込む（DBG-4-F）
        {
            let p = std::path::Path::new(&program_path);
            let bloom_map_path = p.with_extension("bloom.map");
            if bloom_map_path.exists() {
                if let Ok(map) = BloomSourceMap::load(&bloom_map_path) {
                    *ctrl.bloom_map.lock().unwrap() = Some(map);
                }
            }
        }

        let eval_tx = self.event_sender.clone();

        std::thread::spawn(move || {
            let path = std::path::Path::new(&program_path);
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    let _ = sender.send(HookEvent::Output {
                        category: "stderr".to_string(),
                        text: format!("failed to read file: {}\n", e),
                    });
                    let _ = sender.send(HookEvent::Exited { exit_code: 1 });
                    return;
                }
            };

            let mut interp = Interpreter::with_file_path(path);
            let hook = Box::new(DapDebugHook {
                ctrl: Arc::clone(&ctrl),
                sender: eval_tx,
                stopped_scopes: Vec::new(),
            });
            interp.set_debug_hook(hook);

            if let Err(e) = interp.run(&source) {
                let _ = sender.send(HookEvent::Output {
                    category: "stderr".to_string(),
                    text: format!("runtime error: {}\n", e),
                });
            }
            let _ = sender.send(HookEvent::Exited { exit_code: 0 });
        });
    }

    pub fn run<R: BufRead, W: Write>(
        &mut self,
        reader: &mut R,
        writer: &mut W,
    ) -> std::io::Result<()> {
        loop {
            match self.handle_request(reader, writer) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    eprintln!("[forge-dap] IO error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    // ── テスト用アクセサ ──────────────────────────────────────────────────

    #[cfg(test)]
    pub fn inject_stopped_locals(&mut self, locals: Vec<(String, JsonValue)>) {
        self.stopped_locals = locals;
    }

    #[cfg(test)]
    pub fn inject_stopped_globals(&mut self, globals: Vec<(String, JsonValue)>) {
        self.stopped_globals = globals;
    }

    #[cfg(test)]
    pub fn variables_for_test(&mut self, var_ref: i64) -> Vec<JsonValue> {
        self.handle_variables(var_ref)
    }

    #[cfg(test)]
    pub fn evaluate_for_test(&mut self, expression: &str) -> Result<String, String> {
        self.handle_evaluate(expression)
    }

    #[cfg(test)]
    pub fn ctrl_for_test(&self) -> Arc<ControlState> {
        Arc::clone(&self.ctrl)
    }

    #[cfg(test)]
    pub fn event_sender_for_test(&self) -> std::sync::mpsc::SyncSender<HookEvent> {
        self.event_sender.clone()
    }
}

/// JSON (表示値) から Value を復元する（簡易実装: 数値・文字列・bool のみ）
fn json_to_value(jv: &JsonValue) -> Option<Value> {
    let type_str = jv.get("type").and_then(|v| v.as_str())?;
    let value_str = jv.get("value").and_then(|v| v.as_str())?;
    match type_str {
        "number" => {
            if let Ok(n) = value_str.parse::<i64>() {
                return Some(Value::Int(n));
            }
            if let Ok(f) = value_str.parse::<f64>() {
                return Some(Value::Float(f));
            }
            None
        }
        "string" => {
            // "..." → ...
            let s = value_str.trim_matches('"').to_string();
            Some(Value::String(s))
        }
        "bool" => Some(Value::Bool(value_str == "true")),
        "unit" => Some(Value::Unit),
        _ => None,
    }
}
