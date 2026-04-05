use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn e2e_run_anvil_request_lifecycle_demo() {
    let file = repo_root().join("packages/anvil/src/lifecycle_demo.forge");
    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("fixture path")])
        .output()
        .expect("run forge");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "/lifecycle demo 1\n"
    );
}
