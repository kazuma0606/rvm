// forge-mcp: JSON Lines ロギング

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct McpLogger {
    log_path: PathBuf,
    max_bytes: u64,
    max_generations: u32,
}

fn log_file() -> PathBuf {
    forge_mcp_log_path()
}

fn forge_mcp_log_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".forge")
        .join("mcp")
        .join("forge-mcp.log")
}

impl McpLogger {
    pub fn new() -> Self {
        Self {
            log_path: log_file(),
            max_bytes: 10 * 1024 * 1024, // 10MB
            max_generations: 3,
        }
    }

    pub fn log(
        &self,
        level: &str,
        tool: &str,
        req_id: &str,
        elapsed_ms: u64,
        msg: &str,
        detail: &str,
    ) {
        if let Err(_) = self.try_log(level, tool, req_id, elapsed_ms, msg, detail) {
            // ログ書き込み失敗は無視
        }
    }

    fn try_log(
        &self,
        level: &str,
        tool: &str,
        req_id: &str,
        elapsed_ms: u64,
        msg: &str,
        detail: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // ローテートが必要か確認
        if let Ok(meta) = fs::metadata(&self.log_path) {
            if meta.len() > self.max_bytes {
                self.rotate()?;
            }
        }

        // ログディレクトリを作成
        if let Some(parent) = self.log_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let ts = {
            use std::time::SystemTime;
            let secs = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{}", secs)
        };

        let entry = serde_json::json!({
            "ts": ts,
            "level": level,
            "tool": tool,
            "req_id": req_id,
            "elapsed_ms": elapsed_ms,
            "msg": msg,
            "detail": detail
        });

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        writeln!(file, "{}", entry)?;
        Ok(())
    }

    fn rotate(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 最古の世代を削除
        let oldest = self
            .log_path
            .with_extension(format!("log.{}", self.max_generations));
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }

        // 世代を繰り上げ
        for i in (1..self.max_generations).rev() {
            let from = self.log_path.with_extension(format!("log.{}", i));
            let to = self.log_path.with_extension(format!("log.{}", i + 1));
            if from.exists() {
                fs::rename(&from, &to)?;
            }
        }

        // 現行ログを .1 に移動
        let rotated = self.log_path.with_extension("log.1");
        if self.log_path.exists() {
            fs::rename(&self.log_path, &rotated)?;
        }

        Ok(())
    }
}

/// ログを表示する
pub fn show_logs(follow: bool, errors_only: bool) -> Result<(), String> {
    let log_path = forge_mcp_log_path();
    if !log_path.exists() {
        println!("ログファイルが存在しません: {}", log_path.display());
        return Ok(());
    }

    let content = fs::read_to_string(&log_path)
        .map_err(|e| format!("ログファイルの読み込みに失敗しました: {}", e))?;

    for line in content.lines() {
        if errors_only {
            if line.contains("\"level\":\"ERROR\"") || line.contains("\"level\": \"ERROR\"") {
                println!("{}", line);
            }
        } else {
            println!("{}", line);
        }
    }

    if follow {
        // --follow モードは簡易実装（ポーリング）
        use std::io::{BufRead, BufReader, Seek, SeekFrom};
        let mut file = std::fs::File::open(&log_path)
            .map_err(|e| format!("ログファイルを開けません: {}", e))?;
        file.seek(SeekFrom::End(0))
            .map_err(|e| format!("シーク失敗: {}", e))?;
        let mut reader = BufReader::new(file);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if errors_only {
                        if trimmed.contains("\"level\":\"ERROR\"")
                            || trimmed.contains("\"level\": \"ERROR\"")
                        {
                            println!("{}", trimmed);
                        }
                    } else {
                        println!("{}", trimmed);
                    }
                }
                Err(e) => return Err(format!("ログ読み込みエラー: {}", e)),
            }
        }
    }

    Ok(())
}

/// ログをクリアする
pub fn clear_logs() -> Result<(), String> {
    let log_path = forge_mcp_log_path();
    if log_path.exists() {
        fs::remove_file(&log_path)
            .map_err(|e| format!("ログファイルの削除に失敗しました: {}", e))?;
    }
    // ローテートされたログも削除
    for i in 1..=3 {
        let rotated = log_path.with_extension(format!("log.{}", i));
        if rotated.exists() {
            let _ = fs::remove_file(&rotated);
        }
    }
    println!("ログをクリアしました");
    Ok(())
}
