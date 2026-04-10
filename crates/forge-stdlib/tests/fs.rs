use std::fs as std_fs;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_stdlib::fs::{
    delete_file, file_exists, list_dir, make_dir, path_absolute, path_exists, path_ext,
    path_is_dir, path_join, path_stem, read_file, write_file,
};

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

#[test]
fn list_dir_returns_entries() {
    let dir = unique_path("listdir");
    make_dir(dir.to_str().unwrap()).expect("should make dir");
    let file = dir.join("item.txt");
    write_file(file.to_str().unwrap(), "data").expect("write succeed");
    let items = list_dir(dir.to_str().unwrap()).expect("should list");
    assert!(items.contains(&"item.txt".to_string()));
    delete_file(file.to_str().unwrap()).expect("delete ok");
    let _ = std_fs::remove_dir_all(dir);
}

#[test]
fn path_helpers_work() {
    let dir = unique_path("path_helpers");
    make_dir(dir.to_str().unwrap()).expect("mkdir ok");
    let abs = path_absolute(dir.to_str().unwrap()).expect("abs path");
    assert!(path_exists(&abs));
    assert!(path_is_dir(&abs));
    assert!(path_ext("file.txt").as_deref() == Some("txt"));
    assert!(path_stem("file.txt").as_deref() == Some("file"));
    let joined = path_join(
        dir.to_str().unwrap(),
        vec!["sub".to_string(), "file.txt".to_string()],
    );
    assert!(joined.contains("sub"));
    let _ = std_fs::remove_dir_all(dir);
}
