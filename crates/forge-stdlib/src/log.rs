use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub const LOG_LEVEL: &str = "LOG_LEVEL";

/// Log context is optional metadata attached to log entries.
pub type LogContext = HashMap<String, String>;

/// Log level used for filtering and formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn from_str(level: &str) -> Option<Self> {
        match level.to_ascii_uppercase().as_str() {
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn from_env(default: LogLevel) -> LogLevel {
        std::env::var(LOG_LEVEL)
            .ok()
            .and_then(|value| LogLevel::from_str(&value))
            .unwrap_or(default)
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Logger trait that backends must implement.
pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>);

    fn debug(&self, msg: &str, ctx: Option<&LogContext>) {
        self.log(LogLevel::Debug, msg, ctx)
    }

    fn info(&self, msg: &str, ctx: Option<&LogContext>) {
        self.log(LogLevel::Info, msg, ctx)
    }

    fn warn(&self, msg: &str, ctx: Option<&LogContext>) {
        self.log(LogLevel::Warn, msg, ctx)
    }

    fn error(&self, msg: &str, ctx: Option<&LogContext>) {
        self.log(LogLevel::Error, msg, ctx)
    }
}

impl Logger for Arc<dyn Logger> {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>) {
        (**self).log(level, msg, ctx)
    }
}

fn format_context(ctx: Option<&LogContext>) -> String {
    if let Some(ctx) = ctx {
        let mut parts = Vec::with_capacity(ctx.len());
        for (k, v) in ctx {
            parts.push(format!("{}={}", k, v));
        }
        parts.join(" ")
    } else {
        String::new()
    }
}

fn should_log(required: LogLevel, minimum: LogLevel) -> bool {
    required >= minimum
}

struct StreamWriter {
    inner: Arc<Mutex<dyn Write + Send>>,
}

impl StreamWriter {
    fn new(inner: impl Write + Send + 'static) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    fn from_mutex<W: Write + Send + 'static>(inner: Arc<Mutex<W>>) -> Self {
        Self { inner }
    }

    fn write(&self, text: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            let _ = guard.write_all(text.as_bytes());
        }
    }
}

/// Simple logger that writes formatted messages to a stream.
pub struct ConsoleLogger {
    minimum_level: LogLevel,
    writer: StreamWriter,
}

impl ConsoleLogger {
    pub fn new(minimum_level: LogLevel) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::new(io::stdout()),
        }
    }

    pub fn with_writer(minimum_level: LogLevel, writer: impl Write + Send + 'static) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::new(writer),
        }
    }

    pub fn with_stream<W: Write + Send + 'static>(
        minimum_level: LogLevel,
        writer: Arc<Mutex<W>>,
    ) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::from_mutex(writer),
        }
    }

    pub fn from_env() -> Self {
        Self::new(LogLevel::from_env(LogLevel::Info))
    }
}

impl Logger for ConsoleLogger {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>) {
        if should_log(level, self.minimum_level) {
            let ctx = format_context(ctx);
            let text = if ctx.is_empty() {
                format!("[{}] {}\n", level.as_str(), msg)
            } else {
                format!("[{}] {} | {}\n", level.as_str(), msg, ctx)
            };
            self.writer.write(&text);
        }
    }
}

/// Logger that emits JSON objects per message.
pub struct JsonLogger {
    minimum_level: LogLevel,
    writer: StreamWriter,
}

impl JsonLogger {
    pub fn new(minimum_level: LogLevel) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::new(io::stdout()),
        }
    }

    pub fn with_writer(minimum_level: LogLevel, writer: impl Write + Send + 'static) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::new(writer),
        }
    }

    pub fn with_stream<W: Write + Send + 'static>(
        minimum_level: LogLevel,
        writer: Arc<Mutex<W>>,
    ) -> Self {
        Self {
            minimum_level,
            writer: StreamWriter::from_mutex(writer),
        }
    }
}

impl Logger for JsonLogger {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>) {
        if should_log(level, self.minimum_level) {
            let mut object = serde_json::Map::new();
            object.insert(
                "level".to_string(),
                serde_json::Value::String(level.as_str().to_string()),
            );
            object.insert(
                "msg".to_string(),
                serde_json::Value::String(msg.to_string()),
            );
            if let Some(ctx) = ctx {
                let mut map = serde_json::Map::new();
                for (k, v) in ctx {
                    map.insert(k.clone(), serde_json::Value::String(v.clone()));
                }
                object.insert("ctx".to_string(), serde_json::Value::Object(map));
            }
            let encoded = serde_json::Value::Object(object).to_string() + "\n";
            self.writer.write(&encoded);
        }
    }
}

/// Logger that discards everything.
pub struct SilentLogger;

impl SilentLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Logger for SilentLogger {
    fn log(&self, _: LogLevel, _: &str, _: Option<&LogContext>) {}
}

/// Logger that writes to a file.
pub struct FileLogger {
    minimum_level: LogLevel,
    file: Arc<Mutex<File>>,
}

impl FileLogger {
    pub fn new(path: impl AsRef<std::path::Path>, minimum_level: LogLevel) -> Result<Self, String> {
        let file = File::options()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .map_err(|err| format!("failed to open log file: {}", err))?;
        Ok(Self {
            minimum_level,
            file: Arc::new(Mutex::new(file)),
        })
    }
}

impl Logger for FileLogger {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>) {
        if should_log(level, self.minimum_level) {
            if let Ok(mut guard) = self.file.lock() {
                let ctx = format_context(ctx);
                let text = if ctx.is_empty() {
                    format!("[{}] {}\n", level.as_str(), msg)
                } else {
                    format!("[{}] {} | {}\n", level.as_str(), msg, ctx)
                };
                let _ = guard.write_all(text.as_bytes());
            }
        }
    }
}

/// Logger that broadcasts to multiple loggers.
pub struct MultiLogger {
    loggers: Vec<Arc<dyn Logger>>,
}

impl MultiLogger {
    pub fn new(loggers: Vec<Arc<dyn Logger>>) -> Self {
        Self { loggers }
    }
}

impl Logger for MultiLogger {
    fn log(&self, level: LogLevel, msg: &str, ctx: Option<&LogContext>) {
        for logger in &self.loggers {
            logger.log(level, msg, ctx);
        }
    }
}
