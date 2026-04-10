use std::sync::{Arc, Mutex};

use forge_stdlib::log::{
    ConsoleLogger, JsonLogger, LogContext, LogLevel, Logger, MultiLogger, SilentLogger, LOG_LEVEL,
};
use serde_json::Value;

fn read_buffer(buffer: &Arc<Mutex<Vec<u8>>>) -> String {
    let guard = buffer.lock().unwrap();
    String::from_utf8(guard.clone()).unwrap()
}

#[test]
fn test_console_logger_outputs_with_level() {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    std::env::set_var(LOG_LEVEL, "DEBUG");
    let logger =
        ConsoleLogger::with_stream(LogLevel::from_env(LogLevel::Info), Arc::clone(&buffer));
    logger.debug("debug", None);
    std::env::remove_var(LOG_LEVEL);
    let output = read_buffer(&buffer);
    assert!(output.contains("[DEBUG]"));
}

#[test]
fn test_log_level_filter_debug() {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let logger = ConsoleLogger::with_stream(LogLevel::Info, Arc::clone(&buffer));
    logger.debug("debug", None);
    logger.error("error", Some(&LogContext::new()));
    let output = read_buffer(&buffer);
    assert!(!output.contains("[DEBUG]"));
    assert!(output.contains("[ERROR]"));
}

#[test]
fn json_logger_outputs_valid_json() {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let logger = JsonLogger::with_stream(LogLevel::Debug, Arc::clone(&buffer));
    let mut ctx = LogContext::new();
    ctx.insert("user".to_string(), "alice".to_string());
    logger.info("hello", Some(&ctx));
    let output = read_buffer(&buffer);
    let value: Value = serde_json::from_str(output.trim()).expect("should parse json");
    assert_eq!(value["level"], "INFO");
    assert_eq!(value["msg"], "hello");
    assert_eq!(value["ctx"]["user"], "alice");
}

#[test]
fn test_silent_logger_outputs_nothing() {
    let logger = SilentLogger::new();
    logger.info("silent", None);
    logger.warn("still silent", None);
}

#[test]
fn test_multi_logger_calls_all_backends() {
    let buffer_a = Arc::new(Mutex::new(Vec::new()));
    let buffer_b = Arc::new(Mutex::new(Vec::new()));
    let logger_a: Arc<dyn forge_stdlib::log::Logger> = Arc::new(ConsoleLogger::with_stream(
        LogLevel::Debug,
        Arc::clone(&buffer_a),
    ));
    let logger_b: Arc<dyn forge_stdlib::log::Logger> = Arc::new(JsonLogger::with_stream(
        LogLevel::Debug,
        Arc::clone(&buffer_b),
    ));
    let multi = MultiLogger::new(vec![logger_a, logger_b]);
    multi.error("multi", None);
    assert!(read_buffer(&buffer_a).contains("[ERROR]"));
    assert!(read_buffer(&buffer_b).contains("\"level\":\"ERROR\""));
}
