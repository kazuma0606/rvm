use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run_forge_new(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("new")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run forge-new new")
}

fn make_temp_dir(label: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "forge_new_{}_{}_{}",
        label,
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", std::process::id(), ts, seq)
}

#[test]
fn e2e_new_generates_script_template() {
    let temp_dir = make_temp_dir("script");
    let output = run_forge_new(&["my-test-app"], &temp_dir);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project_dir = temp_dir.join("my-test-app");
    let forge_toml = std::fs::read_to_string(project_dir.join("forge.toml")).expect("forge.toml");
    let main_forge =
        std::fs::read_to_string(project_dir.join("src/main.forge")).expect("src/main.forge");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        forge_toml.contains("name = \"my-test-app\""),
        "{}",
        forge_toml
    );
    assert!(main_forge.contains("Hello, my-test-app!"), "{}", main_forge);
    assert!(stdout.contains("Created my-test-app/"), "{}", stdout);
    assert!(
        stdout.contains("cd my-test-app && forge run src/main.forge"),
        "{}",
        stdout
    );

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn e2e_new_generates_cli_template() {
    let temp_dir = make_temp_dir("cli");
    let output = run_forge_new(&["my-cli-app", "--template", "cli"], &temp_dir);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project_dir = temp_dir.join("my-cli-app");
    let main_forge =
        std::fs::read_to_string(project_dir.join("src/main.forge")).expect("src/main.forge");

    assert!(main_forge.contains("TODO: parse command line arguments here"));
    assert!(main_forge.contains("let command_name = \"my-cli-app\""));

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn e2e_new_fails_for_existing_directory() {
    let temp_dir = make_temp_dir("exists");
    let project_dir = temp_dir.join("existing-app");
    std::fs::create_dir_all(&project_dir).expect("existing dir");

    let output = run_forge_new(&["existing-app"], &temp_dir);
    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("既に存在"), "{}", stderr);

    let _ = std::fs::remove_dir_all(temp_dir);
}
