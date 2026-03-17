//! End-to-end tests for ForgeScript

use std::process::Command;
use std::time::Duration;

pub struct TestResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn run_forge(fixture_path: &str) -> TestResult {
    run_forge_with_timeout(fixture_path, Duration::from_secs(5))
}

pub fn run_forge_with_timeout(fixture_path: &str, _timeout: Duration) -> TestResult {
    let forge_path = get_forge_path();
    
    let output = Command::new(&forge_path)
        .arg("run")
        .arg(fixture_path)
        .output()
        .expect("Failed to execute forge");

    TestResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

fn get_forge_path() -> String {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("forge.exe");
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> String {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        
        let mut path = workspace_root.to_path_buf();
        path.push("fixtures");
        path.push("e2e");
        path.push(name);
        path.to_string_lossy().to_string()
    }

    #[test]
    fn e2e_arithmetic() {
        let result = run_forge(&fixture_path("arithmetic.fs"));
        assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    }

    #[test]
    fn e2e_variables() {
        let result = run_forge(&fixture_path("variables.fs"));
        assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    }

    #[test]
    fn e2e_string_concat() {
        let result = run_forge(&fixture_path("string_concat.fs"));
        assert_eq!(result.exit_code, 0, "stderr: {}", result.stderr);
    }

    #[test]
    fn e2e_error_undefined_variable() {
        let result = run_forge(&fixture_path("error_undefined.fs"));
        assert_ne!(result.exit_code, 0);
        assert!(
            result.stderr.contains("undefined") 
            || result.stderr.contains("Undefined")
            || result.stderr.contains("error")
        );
    }

    #[test]
    fn e2e_error_syntax() {
        let result = run_forge(&fixture_path("error_syntax.fs"));
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("error") || result.stderr.contains("Error"));
    }

    #[test]
    fn e2e_error_division_by_zero() {
        let result = run_forge(&fixture_path("error_division_by_zero.fs"));
        assert_ne!(result.exit_code, 0);
        assert!(
            result.stderr.contains("division") 
            || result.stderr.contains("Division")
            || result.stderr.contains("zero")
        );
    }

    #[test]
    fn e2e_nonexistent_file() {
        let result = run_forge("nonexistent_file.fs");
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("Error") || result.stderr.contains("error"));
    }
}
