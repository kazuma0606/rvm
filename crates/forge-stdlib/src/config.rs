use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

use toml::value::{Table, Value};

/// Configuration loader that understands `[default]`/`[env]` sections and `FORGE_ENV`.
pub struct Config;

impl Config {
    pub fn load<T>() -> Result<T, String>
    where
        T: DeserializeOwned + Default,
    {
        Self::load_from("config.toml")
    }

    pub fn load_from<T>(path: impl AsRef<Path>) -> Result<T, String>
    where
        T: DeserializeOwned + Default,
    {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .map_err(|err| format!("failed to read '{}': {}", path.display(), err))?;
        let document: Value =
            toml::from_str(&text).map_err(|err| format!("invalid toml: {}", err))?;
        let env = std::env::var("FORGE_ENV").unwrap_or_else(|_| "default".to_string());
        let mut default_table = document
            .get("default")
            .and_then(Value::as_table)
            .cloned()
            .unwrap_or_default();
        if let Some(env_table) = document.get(&env).and_then(Value::as_table) {
            merge_tables(&mut default_table, env_table);
        }
        let merged = Value::Table(default_table);
        let serialized = toml::to_string(&merged)
            .map_err(|err| format!("failed to serialize merged config: {}", err))?;
        toml::from_str(&serialized).map_err(|err| format!("failed to deserialize config: {}", err))
    }

    pub fn reload<T>(path: impl AsRef<Path>) -> Result<T, String>
    where
        T: DeserializeOwned + Default,
    {
        Self::load_from(path)
    }
}

fn merge_tables(base: &mut Table, overlay: &Table) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(Value::Table(base_table)), Value::Table(overlay_table)) => {
                merge_tables(base_table, overlay_table);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}
