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
        "forge_arch_e2e_{}_{}_{}",
        label,
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(path.join("src/domain")).expect("create domain dir");
    std::fs::create_dir_all(path.join("src/usecase")).expect("create usecase dir");
    path
}

fn write_manifest(project_dir: &std::path::Path, extra: &str) {
    std::fs::write(
        project_dir.join("forge.toml"),
        format!(
            "[package]\nname = \"arch-demo\"\nversion = \"0.1.0\"\nentry = \"src/main.forge\"\n\n[architecture]\nlayers = [\"src/domain\", \"src/usecase\"]\n{}\n",
            extra
        ),
    )
    .expect("write forge.toml");
}

fn run_check(project_dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["check", "--arch-only", project_dir.to_str().expect("dir")])
        .output()
        .expect("run forge check")
}

fn run_full_check(project_dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["check", project_dir.to_str().expect("dir")])
        .output()
        .expect("run forge check")
}

#[test]
fn e2e_arch_valid_project_passes() {
    let project_dir = make_project_dir("valid");
    write_manifest(&project_dir, "");
    std::fs::write(project_dir.join("src/main.forge"), "fn main() { }\n").expect("main");
    std::fs::write(
        project_dir.join("src/domain/user.forge"),
        "data User { name: string }\n",
    )
    .expect("domain");
    std::fs::write(
        project_dir.join("src/usecase/register.forge"),
        "use ./domain/user.*\nstruct RegisterUseCase { name: string }\n",
    )
    .expect("usecase");

    let output = run_check(&project_dir);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("アーキテクチャチェック"));

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_arch_valid_project_full_check_passes() {
    let project_dir = make_project_dir("full");
    write_manifest(&project_dir, "");
    std::fs::write(project_dir.join("src/main.forge"), "fn main() { }\n").expect("main");
    std::fs::write(
        project_dir.join("src/domain/user.forge"),
        "data User { name: string }\n",
    )
    .expect("domain");
    std::fs::write(
        project_dir.join("src/usecase/register.forge"),
        "use ./domain/user.*\nstruct RegisterUseCase { name: string }\n",
    )
    .expect("usecase");

    let output = run_full_check(&project_dir);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("型チェック: エラーなし"));
    assert!(stdout.contains("アーキテクチャチェック: エラーなし"));

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_arch_violation_fails() {
    let project_dir = make_project_dir("violation");
    write_manifest(&project_dir, "");
    std::fs::write(project_dir.join("src/main.forge"), "fn main() { }\n").expect("main");
    std::fs::write(
        project_dir.join("src/domain/user.forge"),
        "use ./usecase/register.*\ndata User { name: string }\n",
    )
    .expect("domain");
    std::fs::write(
        project_dir.join("src/usecase/register.forge"),
        "struct RegisterUseCase { name: string }\n",
    )
    .expect("usecase");

    let output = run_check(&project_dir);
    assert!(
        !output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("依存方向違反"));

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_arch_naming_warn_passes_with_warning() {
    let project_dir = make_project_dir("naming");
    write_manifest(
        &project_dir,
        "\n[architecture.naming]\n\"src/usecase\" = { suffix = [\"UseCase\"] }\n",
    );
    std::fs::write(project_dir.join("src/main.forge"), "fn main() { }\n").expect("main");
    std::fs::write(
        project_dir.join("src/usecase/register.forge"),
        "struct Register { }\n",
    )
    .expect("usecase");

    let output = run_check(&project_dir);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("命名規則違反"));

    let _ = std::fs::remove_dir_all(project_dir);
}
