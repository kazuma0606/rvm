use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_stdlib::env;

fn make_unique_name(base: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should advance")
        .as_nanos();
    format!("forge_std_env_{}_{}", base, stamp)
}

fn temp_env_file(name: &str, contents: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("forge_std_env_file_{}.env", name));
    fs::write(&path, contents).expect("should write env file");
    path
}

#[test]
fn test_env_or_returns_default() {
    let key = make_unique_name("or");
    std::env::remove_var(&key);
    assert_eq!(
        env::env_or(&key, "default").expect("should provide default"),
        "default"
    );

    std::env::set_var(&key, "present");
    assert_eq!(
        env::env_or(&key, "default").expect("should read set value"),
        "present"
    );
    std::env::remove_var(&key);
}

#[test]
fn test_env_require_missing_errors() {
    let key = make_unique_name("require");
    std::env::remove_var(&key);
    let err = env::env_require(&key).expect_err("missing key should error");
    assert!(err.contains("not set"));
}

#[test]
fn test_env_bool_recognizes_variants() {
    let key = make_unique_name("bool");
    std::env::set_var(&key, "yes");
    assert_eq!(env::env_bool(&key).unwrap(), Some(true));

    std::env::set_var(&key, "0");
    assert_eq!(env::env_bool(&key).unwrap(), Some(false));

    std::env::remove_var(&key);
    assert_eq!(env::env_bool(&key).unwrap(), None);
}

#[test]
fn test_load_env_custom_path() {
    let path = temp_env_file("load", "FORGE_STD_LOAD=loaded\n");
    std::env::remove_var("FORGE_STD_LOAD");
    env::load_env(path.to_str().unwrap()).expect("should load file");
    assert_eq!(
        std::env::var("FORGE_STD_LOAD").unwrap(),
        "loaded".to_string()
    );
    std::env::remove_var("FORGE_STD_LOAD");
    fs::remove_file(path).expect("cleanup");
}
