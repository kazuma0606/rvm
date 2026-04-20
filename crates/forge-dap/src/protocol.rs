// forge-dap: DAP プロトコル型定義（DBG-4-B）
// JSON over stdio による DAP メッセージの送受信

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── メッセージ基底型 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapMessage {
    pub seq: i64,
    #[serde(rename = "type")]
    pub msg_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapRequest {
    pub seq: i64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub command: String,
    pub arguments: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapResponse {
    pub seq: i64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub request_seq: i64,
    pub success: bool,
    pub command: String,
    pub message: Option<String>,
    pub body: Option<JsonValue>,
}

impl DapResponse {
    pub fn success(seq: i64, request_seq: i64, command: &str, body: Option<JsonValue>) -> Self {
        DapResponse {
            seq,
            msg_type: "response".to_string(),
            request_seq,
            success: true,
            command: command.to_string(),
            message: None,
            body,
        }
    }

    pub fn error(seq: i64, request_seq: i64, command: &str, message: &str) -> Self {
        DapResponse {
            seq,
            msg_type: "response".to_string(),
            request_seq,
            success: false,
            command: command.to_string(),
            message: Some(message.to_string()),
            body: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DapEvent {
    pub seq: i64,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub event: String,
    pub body: Option<JsonValue>,
}

impl DapEvent {
    pub fn new(seq: i64, event: &str, body: Option<JsonValue>) -> Self {
        DapEvent {
            seq,
            msg_type: "event".to_string(),
            event: event.to_string(),
            body,
        }
    }
}

// ── リクエスト引数型 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeArgs {
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    #[serde(rename = "clientID")]
    pub client_id: Option<String>,
    #[serde(rename = "clientName")]
    pub client_name: Option<String>,
    #[serde(rename = "pathFormat")]
    pub path_format: Option<String>,
    #[serde(rename = "linesStartAt1")]
    pub lines_start_at1: Option<bool>,
    #[serde(rename = "columnsStartAt1")]
    pub columns_start_at1: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchArgs {
    pub program: String,
    pub mode: Option<String>,
    pub port: Option<u16>,
    #[serde(rename = "noDebug")]
    pub no_debug: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: Option<String>,
    pub path: Option<String>,
    #[serde(rename = "sourceReference")]
    pub source_reference: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBreakpoint {
    pub line: i64,
    pub column: Option<i64>,
    pub condition: Option<String>,
    #[serde(rename = "hitCondition")]
    pub hit_condition: Option<String>,
    #[serde(rename = "logMessage")]
    pub log_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBreakpointsArgs {
    pub source: Source,
    pub breakpoints: Option<Vec<SourceBreakpoint>>,
    pub lines: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: Option<i64>,
    pub verified: bool,
    pub message: Option<String>,
    pub source: Option<Source>,
    pub line: Option<i64>,
    pub column: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinueArgs {
    #[serde(rename = "threadId")]
    pub thread_id: i64,
    #[serde(rename = "singleThread")]
    pub single_thread: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextArgs {
    #[serde(rename = "threadId")]
    pub thread_id: i64,
    pub granularity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInArgs {
    #[serde(rename = "threadId")]
    pub thread_id: i64,
    #[serde(rename = "targetId")]
    pub target_id: Option<i64>,
    pub granularity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutArgs {
    #[serde(rename = "threadId")]
    pub thread_id: i64,
    pub granularity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopesArgs {
    #[serde(rename = "frameId")]
    pub frame_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariablesArgs {
    #[serde(rename = "variablesReference")]
    pub variables_reference: i64,
    pub filter: Option<String>,
    pub start: Option<i64>,
    pub count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateArgs {
    pub expression: String,
    #[serde(rename = "frameId")]
    pub frame_id: Option<i64>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackTraceArgs {
    #[serde(rename = "threadId")]
    pub thread_id: i64,
    #[serde(rename = "startFrame")]
    pub start_frame: Option<i64>,
    pub levels: Option<i64>,
}

// ── DAP メッセージ送受信 ──────────────────────────────────────────────────────

/// DAP メッセージを stdio から読み込む。
/// 形式: Content-Length: N\r\n\r\n{JSON}
pub fn read_message<R: std::io::BufRead>(reader: &mut R) -> std::io::Result<Option<JsonValue>> {
    let mut header = String::new();
    let mut content_length = 0usize;

    // ヘッダー行を読む
    loop {
        header.clear();
        let bytes_read = reader.read_line(&mut header)?;
        if bytes_read == 0 {
            // EOF
            return Ok(None);
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            // ヘッダー終端の空行
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }

    if content_length == 0 {
        return Ok(None);
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let json: JsonValue = serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(json))
}

/// DAP メッセージを stdio に書き出す。
pub fn write_message<W: std::io::Write>(writer: &mut W, value: &JsonValue) -> std::io::Result<()> {
    let body = serde_json::to_string(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}
