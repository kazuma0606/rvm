//! Integration tests for forge CLI

use std::process::Command;

fn get_forge_path() -> String {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test executable name
    path.pop(); // Remove deps directory
    path.push("forge.exe");
    path.to_string_lossy().to_string()
}

// Note: test_cli_no_args is removed because forge without args now starts REPL,
// which is interactive and would hang in tests.

#[test]
fn test_cli_help() {
    let forge = get_forge_path();
    let output = Command::new(&forge).arg("help").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ForgeScript"));
    assert!(stdout.contains("Usage"));
}

#[test]
fn test_cli_unknown_command() {
    let forge = get_forge_path();
    let output = Command::new(&forge).arg("unknown").output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown command"));
}

#[test]
fn test_run_missing_file() {
    let forge = get_forge_path();
    let output = Command::new(&forge)
        .arg("run")
        .arg("nonexistent.fs")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error reading file") || stderr.contains("nonexistent.fs"));
}

#[test]
fn test_run_syntax_error() {
    use std::io::Write;
    
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("syntax_error.fs");
    let mut file = std::fs::File::create(&test_file).unwrap();
    writeln!(file, "let = 1").unwrap();
    drop(file);

    let forge = get_forge_path();
    let output = Command::new(&forge)
        .arg("run")
        .arg(&test_file)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error") || stderr.contains("Error"));

    std::fs::remove_file(&test_file).ok();
}

#[test]
fn test_run_success() {
    use std::io::Write;
    
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("success.fs");
    let mut file = std::fs::File::create(&test_file).unwrap();
    writeln!(file, "let x = 1").unwrap();
    writeln!(file, "let y = 2").unwrap();
    drop(file);

    let forge = get_forge_path();
    let output = Command::new(&forge)
        .arg("run")
        .arg(&test_file)
        .output()
        .unwrap();

    assert!(output.status.success());

    std::fs::remove_file(&test_file).ok();
}

#[test]
fn test_run_arithmetic() {
    use std::io::Write;
    
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("arithmetic.fs");
    let mut file = std::fs::File::create(&test_file).unwrap();
    writeln!(file, "let result = 2 * 3 + 4").unwrap();
    drop(file);

    let forge = get_forge_path();
    let output = Command::new(&forge)
        .arg("run")
        .arg(&test_file)
        .output()
        .unwrap();

    assert!(output.status.success());

    std::fs::remove_file(&test_file).ok();
}
