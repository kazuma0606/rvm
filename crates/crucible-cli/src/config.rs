use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub migrations: MigrationsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub driver: Option<String>,
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct MigrationsConfig {
    pub directory: Option<String>,
    pub table: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let content = std::fs::read_to_string("crucible.toml")
            .map_err(|e| format!("crucible.toml が見つかりません: {}", e))?;
        let mut config: Config =
            toml::from_str(&content).map_err(|e| format!("crucible.toml のパースエラー: {}", e))?;
        // 環境変数オーバーライド
        if let Ok(v) = env::var("CRUCIBLE_HOST") {
            config.database.host = v;
        }
        if let Ok(v) = env::var("CRUCIBLE_PORT") {
            config.database.port = v.parse().unwrap_or(config.database.port);
        }
        if let Ok(v) = env::var("CRUCIBLE_NAME") {
            config.database.name = v;
        }
        if let Ok(v) = env::var("CRUCIBLE_USER") {
            config.database.user = v;
        }
        if let Ok(v) = env::var("CRUCIBLE_PASSWORD") {
            config.database.password = v;
        }
        Ok(config)
    }

    pub fn migrations_dir(&self) -> String {
        self.migrations
            .directory
            .clone()
            .unwrap_or_else(|| "migrations".to_string())
    }

    pub fn migrations_table(&self) -> String {
        self.migrations
            .table
            .clone()
            .unwrap_or_else(|| "_crucible_migrations".to_string())
    }

    pub fn connection_string(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={}",
            self.database.host,
            self.database.port,
            self.database.name,
            self.database.user,
            self.database.password
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_dir_default() {
        let config = Config {
            database: DatabaseConfig {
                driver: None,
                host: "localhost".to_string(),
                port: 5432,
                name: "myapp".to_string(),
                user: "postgres".to_string(),
                password: "".to_string(),
            },
            migrations: MigrationsConfig {
                directory: None,
                table: None,
            },
        };
        assert_eq!(config.migrations_dir(), "migrations");
        assert_eq!(config.migrations_table(), "_crucible_migrations");
    }

    #[test]
    fn test_connection_string() {
        let config = Config {
            database: DatabaseConfig {
                driver: Some("postgres".to_string()),
                host: "localhost".to_string(),
                port: 5432,
                name: "myapp_dev".to_string(),
                user: "postgres".to_string(),
                password: "secret".to_string(),
            },
            migrations: MigrationsConfig::default(),
        };
        let cs = config.connection_string();
        assert!(cs.contains("host=localhost"));
        assert!(cs.contains("port=5432"));
        assert!(cs.contains("dbname=myapp_dev"));
        assert!(cs.contains("user=postgres"));
        assert!(cs.contains("password=secret"));
    }
}
