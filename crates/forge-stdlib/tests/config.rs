use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Serializes tests that mutate FORGE_ENV to avoid race conditions.
static ENV_LOCK: Mutex<()> = Mutex::new(());

use forge_stdlib::config::Config;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct AppConfig {
    port: u16,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default)]
    debug: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 0,
            log_level: default_log_level(),
            debug: false,
        }
    }
}

fn temp_config(contents: &str, name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time flows forward")
        .as_nanos();
    path.push(format!("forge-config-{}-{}.toml", name, stamp));
    fs::write(&path, contents).expect("write config");
    path
}

#[test]
fn test_config_load_uses_field_defaults() {
    let contents = r#"
[default]
port = 8080
"#;
    let path = temp_config(contents, "defaults");
    let config: AppConfig = Config::load_from(&path).expect("should load config");
    assert_eq!(config.port, 8080);
    assert_eq!(config.log_level, "info");
    assert!(!config.debug);
}

#[test]
fn test_config_load_toml_overrides_defaults() {
    let contents = r#"
[default]
port = 8080
log_level = "warn"
debug = true
"#;
    let path = temp_config(contents, "toml");
    let config: AppConfig = Config::load_from(&path).expect("should load config");
    assert_eq!(config.port, 8080);
    assert_eq!(config.log_level, "warn");
    assert!(config.debug);
}

#[test]
fn test_config_load_env_overrides_toml() {
    let contents = r#"
[default]
port = 8080
log_level = "info"
debug = false

[production]
port = 80
log_level = "warn"
debug = true
"#;
    let path = temp_config(contents, "prod");
    let _lock = ENV_LOCK.lock().unwrap();
    std::env::set_var("FORGE_ENV", "production");
    let config: AppConfig = Config::load_from(&path).expect("should load config");
    std::env::remove_var("FORGE_ENV");
    drop(_lock);
    assert_eq!(config.port, 80);
    assert_eq!(config.log_level, "warn");
    assert!(config.debug);
}

#[test]
fn test_config_load_forge_env_section() {
    let contents = r#"
[default]
port = 8080
log_level = "info"

[staging]
port = 3000
"#;
    let path = temp_config(contents, "staging");
    let _lock = ENV_LOCK.lock().unwrap();
    std::env::set_var("FORGE_ENV", "staging");
    let config: AppConfig = Config::load_from(&path).expect("should load config");
    std::env::remove_var("FORGE_ENV");
    drop(_lock);
    assert_eq!(config.port, 3000);
    assert_eq!(config.log_level, "info");
    assert!(!config.debug);
}

#[test]
fn config_reload_reads_current_file() {
    let contents = r#"
[default]
port = 8080
"#;
    let path = temp_config(contents, "reload");
    let first: AppConfig = Config::load_from(&path).expect("first load");
    assert_eq!(first.port, 8080);
    let updated = r#"
[default]
port = 9090
"#;
    fs::write(&path, updated).expect("rewrite");
    let reloaded: AppConfig = Config::reload(&path).expect("reload from config");
    assert_eq!(reloaded.port, 9090);
}
