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
        "forge_build_toml_{}_{}_{}",
        label,
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(path.join("src")).expect("create src dir");
    path
}

fn write_project(path: &std::path::Path, output: Option<&str>) {
    let build_section = output
        .map(|output| format!("\n[build]\noutput = \"{}\"\nedition = \"2021\"\n", output))
        .unwrap_or_default();

    let forge_toml = format!(
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n{}\n[dependencies]\nanyhow = \"1\"\n",
        build_section
    );
    std::fs::write(path.join("forge.toml"), forge_toml).expect("write forge.toml");
    std::fs::write(
        path.join("src/main.forge"),
        "fn main() {\n    println(\"hello from toml\")\n}\n\nmain()\n",
    )
    .expect("write main.forge");
}

#[test]
fn e2e_build_directory_uses_forge_toml() {
    let project_dir = make_project_dir("dir");
    write_project(&project_dir, Some("dist/demo-app"));

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project_dir.join("dist/demo-app.exe").exists() || project_dir.join("dist/demo-app").exists());

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_uses_current_directory_forge_toml() {
    let project_dir = make_project_dir("cwd");
    write_project(&project_dir, None);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .current_dir(&project_dir)
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project_dir.join("target/demo-app.exe").exists() || project_dir.join("target/demo-app").exists());

    let _ = std::fs::remove_dir_all(project_dir);
}
