// forge-mcp: セッション状態

use std::collections::HashMap;
use std::time::Instant;

pub struct McpSessionState {
    pub started_at: Instant,
    pub request_count: u64,
    pub error_count: u64,
    pub last_error: Option<String>,
    pub tool_counts: HashMap<String, u64>,
}

impl McpSessionState {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            request_count: 0,
            error_count: 0,
            last_error: None,
            tool_counts: HashMap::new(),
        }
    }

    pub fn record_request(&mut self, tool: &str) {
        self.request_count += 1;
        *self.tool_counts.entry(tool.to_string()).or_insert(0) += 1;
    }

    pub fn record_error(&mut self, _tool: &str, msg: &str) {
        self.error_count += 1;
        self.last_error = Some(msg.to_string());
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
