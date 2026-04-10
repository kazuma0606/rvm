use dotenvy;
use std::env;

pub fn env(key: impl AsRef<str>) -> Result<Option<String>, String> {
    let key_ref = key.as_ref();
    match env::var(key_ref) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(format!(
            "environment variable '{}' is not valid Unicode",
            key_ref
        )),
    }
}

pub fn env_or(key: impl AsRef<str>, default: impl AsRef<str>) -> Result<String, String> {
    let default_value = default.as_ref().to_string();
    match env(key)? {
        Some(value) => Ok(value),
        None => Ok(default_value),
    }
}

pub fn env_number(key: impl AsRef<str>) -> Result<Option<f64>, String> {
    let key_ref = key.as_ref();
    match env(key_ref)? {
        Some(value) => value
            .parse::<f64>()
            .map(Some)
            .map_err(|_| format!("environment variable '{}' is not a number", key_ref)),
        None => Ok(None),
    }
}

pub fn env_bool(key: impl AsRef<str>) -> Result<Option<bool>, String> {
    let key_ref = key.as_ref();
    match env(key_ref)? {
        Some(value) => parse_bool(&value)
            .map(Some)
            .ok_or_else(|| format!("environment variable '{}' is not a boolean", key_ref)),
        None => Ok(None),
    }
}

pub fn env_require(key: impl AsRef<str>) -> Result<String, String> {
    let key_ref = key.as_ref();
    env(key_ref)?.ok_or_else(|| format!("environment variable '{}' is not set", key_ref))
}

pub fn load_env(path: impl AsRef<str>) -> Result<(), String> {
    dotenvy::from_path(path.as_ref())
        .map(|_| ())
        .map_err(|err| format!("failed to load '{}': {}", path.as_ref(), err))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}
