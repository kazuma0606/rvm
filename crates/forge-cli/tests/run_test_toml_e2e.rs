use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", std::process::id(), ts, seq)
}

fn make_project_dir(label: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "forge_run_test_toml_{}_{}_{}",
        label,
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(path.join("src")).expect("create src dir");
    std::fs::create_dir_all(path.join("tests")).expect("create tests dir");
    path
}

#[test]
fn e2e_run_directory_uses_forge_toml_entry() {
    let project_dir = make_project_dir("run");
    std::fs::write(
        project_dir.join("forge.toml"),
        "[package]\nname = \"demo-run\"\nversion = \"0.1.0\"\nentry = \"src/main.forge\"\n",
    )
    .expect("write forge.toml");
    std::fs::write(
        project_dir.join("src/main.forge"),
        "fn main() {\n    println(\"run via forge toml\")\n}\n\nmain()\n",
    )
    .expect("write entry");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", project_dir.to_str().expect("dir")])
        .output()
        .expect("run forge");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "run via forge toml\n"
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_test_directory_uses_forge_toml_tests_dir() {
    let project_dir = make_project_dir("test");
    std::fs::write(
        project_dir.join("forge.toml"),
        "[package]\nname = \"demo-test\"\nversion = \"0.1.0\"\nentry = \"src/main.forge\"\n",
    )
    .expect("write forge.toml");
    std::fs::write(
        project_dir.join("src/main.forge"),
        "fn main() {\n    println(\"unused\")\n}\n\nmain()\n",
    )
    .expect("write main");
    std::fs::write(
        project_dir.join("tests/sample.test.forge"),
        "test \"add\" {\n    assert_eq(1 + 1, 2)\n}\n",
    )
    .expect("write test");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["test", project_dir.to_str().expect("dir")])
        .output()
        .expect("run forge test");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("running 1 tests"), "{}", stdout);
    assert!(stdout.contains("ok. 1 passed; 0 failed"), "{}", stdout);

    let _ = std::fs::remove_dir_all(project_dir);
}
