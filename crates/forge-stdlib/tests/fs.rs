use std::time::{SystemTime, UNIX_EPOCH};

use forge_stdlib::fs::{file_exists, read_file, write_file};

fn unique_path(name: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("forge-stdlib-{}-{}", name, stamp))
}

#[test]
fn write_and_read_file_roundtrip() {
    let path = unique_path("roundtrip.txt");
    write_file(path.to_str().expect("utf-8 path"), "hello").expect("write should succeed");

    let content = read_file(path.to_str().expect("utf-8 path")).expect("read should succeed");
    assert_eq!(content, "hello");
    assert!(file_exists(path.to_str().expect("utf-8 path")));

    let _ = std::fs::remove_file(path);
}

#[test]
fn read_file_returns_error_for_missing_path() {
    let path = unique_path("missing.txt");
    let err = read_file(path.to_str().expect("utf-8 path")).expect_err("missing file should fail");
    assert!(err.contains("failed to read"));
}
