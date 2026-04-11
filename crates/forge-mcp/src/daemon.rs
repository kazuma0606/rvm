// forge-mcp: デーモンプロセス管理

use std::fs;
use std::path::PathBuf;

fn forge_mcp_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".forge").join("mcp")
}

fn pid_file() -> PathBuf {
    forge_mcp_dir().join("forge-mcp.pid")
}

fn log_file_path() -> PathBuf {
    forge_mcp_dir().join("forge-mcp.log")
}

/// デーモンを起動する
pub fn start() -> Result<(), String> {
    // 既に起動中か確認
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            return Err(format!("forge-mcp はすでに起動しています (pid {})", pid));
        }
    }

    let dir = forge_mcp_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("ディレクトリの作成に失敗しました: {}", e))?;

    let pid = spawn_detached(&["mcp", "--daemon-inner"])
        .map_err(|e| format!("デーモンの起動に失敗しました: {}", e))?;

    fs::write(pid_file(), pid.to_string())
        .map_err(|e| format!("PID ファイルの書き込みに失敗しました: {}", e))?;

    println!("forge-mcp を起動しました (pid {})", pid);
    Ok(())
}

/// デーモンを停止する
pub fn stop() -> Result<(), String> {
    let pid = read_pid().ok_or_else(|| "forge-mcp は起動していません".to_string())?;

    if !is_running(pid) {
        // PID ファイルが残っているがプロセスは既に終了済み — クリーンアップして正常終了
        let _ = fs::remove_file(pid_file());
        println!("forge-mcp を停止しました (pid {} — 既に終了済み)", pid);
        return Ok(());
    }

    kill_process(pid)?;

    let _ = fs::remove_file(pid_file());
    println!("forge-mcp を停止しました (pid {})", pid);
    Ok(())
}

/// デーモンを再起動する
pub fn restart() -> Result<(), String> {
    // 停止エラーは無視（起動していなくても再起動できる）
    let _ = stop();
    start()
}

/// デーモンの状態を表示する
pub fn status() -> Result<(), String> {
    match read_pid() {
        Some(pid) if is_running(pid) => {
            println!("forge-mcp: running (pid {})", pid);
        }
        _ => {
            println!("forge-mcp: not running");
        }
    }

    // ログファイルの末尾3行を表示
    let log_path = log_file_path();
    if log_path.exists() {
        if let Ok(content) = fs::read_to_string(&log_path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = if lines.len() > 3 { lines.len() - 3 } else { 0 };
            if start < lines.len() {
                println!("\n最新ログ:");
                for line in &lines[start..] {
                    println!("  {}", line);
                }
            }
        }
    }

    Ok(())
}

/// PID ファイルを読み込む
fn read_pid() -> Option<u32> {
    let content = fs::read_to_string(pid_file()).ok()?;
    content.trim().parse::<u32>().ok()
}

/// プロセスが実行中か確認する
#[cfg(unix)]
fn is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn is_running(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn is_running(_pid: u32) -> bool {
    false
}

/// プロセスを終了する
#[cfg(unix)]
fn kill_process(pid: u32) -> Result<(), String> {
    let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!("プロセス {} の終了に失敗しました", pid))
    }
}

#[cfg(windows)]
fn kill_process(pid: u32) -> Result<(), String> {
    use std::process::Command;
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .map_err(|e| format!("taskkill の実行に失敗しました: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("プロセス {} の終了に失敗しました", pid))
    }
}

#[cfg(not(any(unix, windows)))]
fn kill_process(pid: u32) -> Result<(), String> {
    Err(format!(
        "このプラットフォームではプロセス終了をサポートしていません (pid {})",
        pid
    ))
}

/// デタッチしたプロセスとして起動する
#[cfg(unix)]
fn spawn_detached(args: &[&str]) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe()
        .map_err(|e| format!("実行ファイルのパスを取得できません: {}", e))?;

    let child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("プロセスの起動に失敗しました: {}", e))?;

    Ok(child.id())
}

#[cfg(windows)]
fn spawn_detached(args: &[&str]) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe = std::env::current_exe()
        .map_err(|e| format!("実行ファイルのパスを取得できません: {}", e))?;

    let child = Command::new(exe)
        .args(args)
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("プロセスの起動に失敗しました: {}", e))?;

    Ok(child.id())
}

#[cfg(not(any(unix, windows)))]
fn spawn_detached(args: &[&str]) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe()
        .map_err(|e| format!("実行ファイルのパスを取得できません: {}", e))?;

    let child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("プロセスの起動に失敗しました: {}", e))?;

    Ok(child.id())
}
