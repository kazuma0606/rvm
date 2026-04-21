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
fn e2e_new_generates_clean_arch_template() {
    let temp_dir = make_temp_dir("clean_arch");
    let output = run_forge_new(&["sample-clean", "--template", "clean-arch"], &temp_dir);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project_dir = temp_dir.join("sample-clean");
    let expected = [
        "forge.toml",
        "src/main.forge",
        "src/domain/mod.forge",
        "src/domain/user.forge",
        "src/usecase/mod.forge",
        "src/usecase/register_user_usecase.forge",
        "src/interface/mod.forge",
        "src/interface/user_handler.forge",
        "src/infrastructure/mod.forge",
        "src/infrastructure/postgres_user_repository.forge",
        "src/infrastructure/smtp_email_service.forge",
        "tests/register_user_test.forge",
    ];
    for file in expected {
        assert!(project_dir.join(file).exists(), "{} が存在すること", file);
    }

    let forge_toml = std::fs::read_to_string(project_dir.join("forge.toml")).expect("forge.toml");
    assert!(forge_toml.contains("name = \"sample-clean\""));
    assert!(forge_toml.contains("[architecture]"));

    let check = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["check", project_dir.to_str().expect("dir")])
        .output()
        .expect("forge check clean-arch");
    assert!(
        check.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let run = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", "src/main.forge"])
        .current_dir(&project_dir)
        .output()
        .expect("forge run clean-arch");
    assert!(
        run.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("registered Ada"));

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn e2e_new_generates_anvil_clean_template() {
    let temp_dir = make_temp_dir("anvil_clean");
    let output = run_forge_new(&["sample-anvil", "--template", "anvil-clean"], &temp_dir);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project_dir = temp_dir.join("sample-anvil");
    assert!(project_dir
        .join("src/interface/user_handler.forge")
        .exists());
    let handler = std::fs::read_to_string(project_dir.join("src/interface/user_handler.forge"))
        .expect("user_handler");
    assert!(handler.contains("AnvilRouter"));

    let check = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["check", "--arch-only", project_dir.to_str().expect("dir")])
        .output()
        .expect("forge check anvil-clean");
    assert!(
        check.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

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
