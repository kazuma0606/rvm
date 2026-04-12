use std::fs;
use std::path::Path;
use tokio_postgres::NoTls;

use crate::config::Config;

pub struct Migration {
    pub version: String,
    pub filename: String,
    pub up_sql: String,
    pub down_sql: String,
}

/// ファイル名の先頭にある連続する数字を取得する
fn extract_version(filename: &str) -> String {
    filename
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect()
}

/// `-- +migrate Up` と `-- +migrate Down` で SQL を分割する
pub fn parse_migration_file(path: &Path) -> Result<Migration, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("ファイルの読み込みに失敗しました {}: {}", path.display(), e))?;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let version = extract_version(&filename);
    if version.is_empty() {
        return Err(format!(
            "マイグレーションファイル名はバージョン番号で始まる必要があります: {}",
            filename
        ));
    }

    let mut up_lines: Vec<&str> = Vec::new();
    let mut down_lines: Vec<&str> = Vec::new();
    let mut section = "none";

    for line in content.lines() {
        if line == "-- +migrate Up" {
            section = "up";
        } else if line == "-- +migrate Down" {
            section = "down";
        } else if section == "up" {
            up_lines.push(line);
        } else if section == "down" {
            down_lines.push(line);
        }
    }

    let up_sql = up_lines.join("\n").trim().to_string();
    let down_sql = down_lines.join("\n").trim().to_string();

    if up_sql.is_empty() && down_sql.is_empty() {
        return Err(format!(
            "マイグレーションファイルに -- +migrate Up / -- +migrate Down セクションがありません: {}",
            filename
        ));
    }

    Ok(Migration {
        version,
        filename,
        up_sql,
        down_sql,
    })
}

/// ディレクトリからマイグレーションファイルを読み込む（バージョン順ソート）
pub fn read_migrations(dir: &str) -> Result<Vec<Migration>, String> {
    let dir_path = Path::new(dir);
    if !dir_path.exists() {
        return Err(format!("migrations ディレクトリが見つかりません: {}", dir));
    }

    let mut entries: Vec<_> = fs::read_dir(dir_path)
        .map_err(|e| format!("ディレクトリの読み込みに失敗しました: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "sql")
                .unwrap_or(false)
        })
        .collect();

    // ファイル名でソート
    entries.sort_by_key(|e| e.file_name());

    let mut migrations = Vec::new();
    for entry in entries {
        let migration = parse_migration_file(&entry.path())?;
        migrations.push(migration);
    }

    Ok(migrations)
}

/// マイグレーションテーブルを作成する
async fn ensure_migration_table(
    client: &tokio_postgres::Client,
    table: &str,
) -> Result<(), String> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    version     VARCHAR(255) PRIMARY KEY,
    filename    VARCHAR(255) NOT NULL,
    applied_at  TIMESTAMP    NOT NULL DEFAULT now()
)",
        table
    );
    client
        .batch_execute(&sql)
        .await
        .map_err(|e| format!("マイグレーションテーブルの作成に失敗しました: {}", e))?;
    Ok(())
}

/// 適用済みバージョン一覧を取得する
async fn get_applied_versions(
    client: &tokio_postgres::Client,
    table: &str,
) -> Result<Vec<String>, String> {
    let sql = format!("SELECT version FROM {} ORDER BY version", table);
    let rows = client
        .query(&sql, &[])
        .await
        .map_err(|e| format!("適用済みバージョンの取得に失敗しました: {}", e))?;
    let versions = rows.iter().map(|row| row.get::<_, String>(0)).collect();
    Ok(versions)
}

/// 未適用マイグレーションを順番に適用する
pub async fn run_migrate(config: &Config) -> Result<(), String> {
    let (client, connection) = tokio_postgres::connect(&config.connection_string(), NoTls)
        .await
        .map_err(|e| format!("PostgreSQL への接続に失敗しました: {}", e))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("接続エラー: {}", e);
        }
    });

    let table = config.migrations_table();
    ensure_migration_table(&client, &table).await?;

    let applied = get_applied_versions(&client, &table).await?;
    let migrations = read_migrations(&config.migrations_dir())?;

    let mut applied_count = 0;
    for migration in &migrations {
        if applied.contains(&migration.version) {
            continue;
        }

        println!("適用中: {} ...", migration.filename);
        client.batch_execute(&migration.up_sql).await.map_err(|e| {
            format!(
                "マイグレーション {} の適用に失敗しました: {}",
                migration.filename, e
            )
        })?;

        let insert_sql = format!("INSERT INTO {} (version, filename) VALUES ($1, $2)", table);
        client
            .execute(&insert_sql, &[&migration.version, &migration.filename])
            .await
            .map_err(|e| format!("マイグレーション記録の挿入に失敗しました: {}", e))?;

        println!("完了: {}", migration.filename);
        applied_count += 1;
    }

    if applied_count == 0 {
        println!("適用するマイグレーションはありません。");
    } else {
        println!("{} 件のマイグレーションを適用しました。", applied_count);
    }

    Ok(())
}

/// 最後に適用されたマイグレーションをロールバックする
pub async fn run_rollback(config: &Config) -> Result<(), String> {
    let (client, connection) = tokio_postgres::connect(&config.connection_string(), NoTls)
        .await
        .map_err(|e| format!("PostgreSQL への接続に失敗しました: {}", e))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("接続エラー: {}", e);
        }
    });

    let table = config.migrations_table();
    ensure_migration_table(&client, &table).await?;

    let applied = get_applied_versions(&client, &table).await?;
    if applied.is_empty() {
        println!("ロールバックするマイグレーションはありません。");
        return Ok(());
    }

    let last_version = applied.last().unwrap();
    let migrations = read_migrations(&config.migrations_dir())?;

    let migration = migrations
        .iter()
        .find(|m| &m.version == last_version)
        .ok_or_else(|| {
            format!(
                "バージョン {} のマイグレーションファイルが見つかりません",
                last_version
            )
        })?;

    println!("ロールバック中: {} ...", migration.filename);
    client
        .batch_execute(&migration.down_sql)
        .await
        .map_err(|e| {
            format!(
                "マイグレーション {} のロールバックに失敗しました: {}",
                migration.filename, e
            )
        })?;

    let delete_sql = format!("DELETE FROM {} WHERE version = $1", table);
    client
        .execute(&delete_sql, &[last_version])
        .await
        .map_err(|e| format!("マイグレーション記録の削除に失敗しました: {}", e))?;

    println!("ロールバック完了: {}", migration.filename);
    Ok(())
}

/// マイグレーションの状態を表示する
pub async fn run_status(config: &Config) -> Result<(), String> {
    let (client, connection) = tokio_postgres::connect(&config.connection_string(), NoTls)
        .await
        .map_err(|e| format!("PostgreSQL への接続に失敗しました: {}", e))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("接続エラー: {}", e);
        }
    });

    let table = config.migrations_table();
    ensure_migration_table(&client, &table).await?;

    let applied = get_applied_versions(&client, &table).await?;
    let migrations = read_migrations(&config.migrations_dir())?;

    if migrations.is_empty() {
        println!("マイグレーションファイルが見つかりません。");
        return Ok(());
    }

    println!("{:<8} {:<50} {}", "状態", "ファイル名", "バージョン");
    println!("{}", "-".repeat(70));

    for migration in &migrations {
        let status = if applied.contains(&migration.version) {
            "✅ applied"
        } else {
            "⬜ pending"
        };
        println!(
            "{:<12} {:<50} {}",
            status, migration.filename, migration.version
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version("001_create_users.sql"), "001");
        assert_eq!(extract_version("042_add_index.sql"), "042");
        assert_eq!(extract_version("no_version.sql"), "");
    }

    #[test]
    fn test_parse_migration_file_up_down() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        let content = "-- +migrate Up\nCREATE TABLE users (\n    id SERIAL PRIMARY KEY\n);\n\n-- +migrate Down\nDROP TABLE users;\n";
        tmpfile.write_all(content.as_bytes()).unwrap();

        // ファイル名を 001_create_users.sql に変更してパスを作成
        let dir = tmpfile.path().parent().unwrap();
        let new_path = dir.join("001_create_users.sql");
        std::fs::copy(tmpfile.path(), &new_path).unwrap();

        let migration = parse_migration_file(&new_path).unwrap();
        assert_eq!(migration.version, "001");
        assert_eq!(migration.filename, "001_create_users.sql");
        assert!(migration.up_sql.contains("CREATE TABLE users"));
        assert!(migration.down_sql.contains("DROP TABLE users"));

        std::fs::remove_file(&new_path).unwrap();
    }

    #[test]
    fn test_parse_migration_file_only_up() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        let content = "-- +migrate Up\nCREATE TABLE test (id SERIAL);\n";
        tmpfile.write_all(content.as_bytes()).unwrap();

        let dir = tmpfile.path().parent().unwrap();
        let new_path = dir.join("001_only_up.sql");
        std::fs::copy(tmpfile.path(), &new_path).unwrap();

        let migration = parse_migration_file(&new_path).unwrap();
        assert_eq!(migration.version, "001");
        assert!(migration.up_sql.contains("CREATE TABLE test"));
        assert!(migration.down_sql.is_empty());

        std::fs::remove_file(&new_path).unwrap();
    }
}
