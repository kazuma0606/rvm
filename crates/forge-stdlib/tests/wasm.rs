use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_stdlib::wasm::{Wasm, WasmOptions};

fn unique_path(name: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("forge-stdlib-wasm-{}-{}.wasm", name, stamp))
}

fn write_wasm_file(name: &str, wat_src: &str) -> std::path::PathBuf {
    let path = unique_path(name);
    let bytes = wat::parse_str(wat_src).expect("wat should compile");
    fs::write(&path, bytes).expect("wasm should write");
    path
}

fn compile_wat(wat_src: &str) -> Vec<u8> {
    wat::parse_str(wat_src).expect("wat should compile")
}

fn fixture_wasm_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("wasm")
        .join("bin")
        .join(name)
}

#[test]
fn test_wasm_load_valid_binary() {
    let path = write_wasm_file(
        "valid",
        r#"(module
            (func (export "noop"))
        )"#,
    );
    let module = Wasm::load(&path);
    assert!(module.is_ok(), "got: {:?}", module.err());
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_load_invalid_path_errors() {
    let path = unique_path("missing");
    let err = match Wasm::load(&path) {
        Ok(_) => panic!("missing path should fail"),
        Err(err) => err,
    };
    assert!(err.contains("WasmLoadError"), "got: {}", err);
}

#[test]
fn test_wasm_call_returns_string() {
    let path = write_wasm_file(
        "call",
        r#"(module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 32))
            (data (i32.const 0) "hello")
            (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.tee $ptr
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
            (func (export "render") (param i32 i32) (result i64)
                i32.const 0
                i64.extend_i32_u
                i64.const 32
                i64.shl
                i32.const 5
                i64.extend_i32_u
                i64.or)
        )"#,
    );
    let module = Wasm::load(&path).expect("wasm should load");
    let result = module
        .call("render", r#"{"name":"forge"}"#)
        .expect("call should work");
    assert_eq!(result, "hello");
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_call_unknown_function_errors() {
    let path = write_wasm_file(
        "unknown-fn",
        r#"(module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 32))
            (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.tee $ptr
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
        )"#,
    );
    let module = Wasm::load(&path).expect("wasm should load");
    let err = module
        .call("render", "{}")
        .expect_err("unknown function should fail");
    assert!(err.contains("WasmCallError"), "got: {}", err);
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_options_trusted_has_no_limits() {
    let opts = WasmOptions::trusted();
    assert_eq!(opts.max_instructions, None);
    assert_eq!(opts.max_memory_mb, None);
    assert_eq!(opts.timeout_ms, None);
    assert!(opts.allow_fs);
    assert!(opts.allow_net);
    assert!(opts.allow_env);
}

#[test]
fn test_wasm_options_sandboxed_has_limits() {
    let opts = WasmOptions::sandboxed();
    assert_eq!(opts.max_instructions, Some(1_000_000));
    assert_eq!(opts.max_memory_mb, Some(16));
    assert_eq!(opts.timeout_ms, Some(500));
    assert!(!opts.allow_fs);
    assert!(!opts.allow_net);
    assert!(!opts.allow_env);
}

#[test]
fn test_wasm_options_strict_has_strict_limits() {
    let opts = WasmOptions::strict();
    assert_eq!(opts.max_instructions, Some(100_000));
    assert_eq!(opts.max_memory_mb, Some(4));
    assert_eq!(opts.timeout_ms, Some(100));
    assert!(!opts.allow_fs);
    assert!(!opts.allow_net);
    assert!(!opts.allow_env);
}

#[test]
fn test_wasm_load_with_trusted() {
    let path = write_wasm_file(
        "trusted",
        r#"(module
            (func (export "noop"))
        )"#,
    );
    let module = Wasm::load_with(&path, WasmOptions::trusted()).expect("wasm should load");
    assert_eq!(module.options, WasmOptions::trusted());
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_from_bytes_loads_module() {
    let bytes = compile_wat(
        r#"(module
            (func (export "noop"))
        )"#,
    );
    let module = Wasm::from_bytes(&bytes);
    assert!(module.is_ok(), "got: {:?}", module.err());
}

#[test]
fn test_wasm_from_bytes_invalid_errors() {
    let err = match Wasm::from_bytes(b"not wasm") {
        Ok(_) => panic!("invalid bytes should fail"),
        Err(err) => err,
    };
    assert!(err.contains("WasmLoadError"), "got: {}", err);
}

#[test]
fn test_wasm_from_bytes_sandboxed() {
    let bytes = compile_wat(
        r#"(module
            (func (export "noop"))
        )"#,
    );
    let module = Wasm::from_bytes_with(&bytes, WasmOptions::sandboxed()).expect("load should work");
    assert_eq!(module.options, WasmOptions::sandboxed());
}

#[test]
fn test_wasm_load_c_binary() {
    let module = Wasm::load(fixture_wasm_path("add.wasm"));
    assert!(module.is_ok(), "got: {:?}", module.err());
}

#[test]
fn test_wasm_c_add_returns_correct_sum() {
    let module = Wasm::load(fixture_wasm_path("add.wasm")).expect("C wasm should load");
    let result = module
        .call_i32_i32("add", 2, 3)
        .expect("C add should return a result");
    assert_eq!(result, 5);
}

#[test]
fn test_wasm_load_cpp_binary() {
    let module = Wasm::load(fixture_wasm_path("add_cpp.wasm"));
    assert!(module.is_ok(), "got: {:?}", module.err());
}

#[test]
fn test_wasm_cpp_add_matches_c_output() {
    let c_module = Wasm::load(fixture_wasm_path("add.wasm")).expect("C wasm should load");
    let cpp_module = Wasm::load(fixture_wasm_path("add_cpp.wasm")).expect("C++ wasm should load");
    let c_result = c_module
        .call_i32_i32("add", 9, 4)
        .expect("C add should return a result");
    let cpp_result = cpp_module
        .call_i32_i32("add", 9, 4)
        .expect("C++ add should return a result");
    assert_eq!(c_result, cpp_result);
}

#[test]
fn test_wasm_echo_string_roundtrip() {
    let module = Wasm::load(fixture_wasm_path("echo.wasm")).expect("echo wasm should load");
    let input = r#"{"message":"forge","ok":true}"#;
    let result = module.call("echo", input).expect("echo should round-trip");
    assert_eq!(result, input);
}

#[test]
fn test_wasm_output_encoding_consistent() {
    let fixture = Wasm::load(fixture_wasm_path("echo.wasm")).expect("fixture wasm should load");
    let inline = Wasm::from_bytes(&compile_wat(
        r#"(module
                (memory (export "memory") 1)
                (global $heap (mut i32) (i32.const 64))
                (func (export "alloc") (param $len i32) (result i32)
                    (local $ptr i32)
                    global.get $heap
                    local.tee $ptr
                    local.get $len
                    i32.add
                    global.set $heap
                    local.get $ptr)
                (func (export "echo") (param $ptr i32) (param $len i32) (result i64)
                    (local $out i32)
                    local.get $len
                    call 0
                    local.set $out
                    local.get $out
                    local.get $ptr
                    local.get $len
                    memory.copy
                    local.get $out
                    i64.extend_i32_u
                    i64.const 32
                    i64.shl
                    local.get $len
                    i64.extend_i32_u
                    i64.or)
            )"#,
    ))
    .expect("inline wasm should load");
    let input = r#"{"items":[1,2,3],"label":"same-bytes"}"#;
    let fixture_result = fixture
        .call("echo", input)
        .expect("fixture echo should work");
    let inline_result = inline.call("echo", input).expect("inline echo should work");
    assert_eq!(fixture_result.as_bytes(), inline_result.as_bytes());
}

#[test]
fn test_wasm_c_binary_respects_fuel_limit() {
    let module = Wasm::load_with(
        fixture_wasm_path("echo.wasm"),
        WasmOptions {
            max_instructions: Some(0),
            ..WasmOptions::trusted()
        },
    )
    .expect("C wasm should load");
    let err = module
        .call("echo", r#"{"message":"fuel-check"}"#)
        .expect_err("fuel limit should interrupt execution");
    assert!(err.contains("WasmFuelExhausted"), "got: {}", err);
}

#[test]
fn test_wasm_c_binary_respects_memory_limit() {
    let module = Wasm::load_with(
        fixture_wasm_path("echo.wasm"),
        WasmOptions {
            max_memory_mb: Some(0),
            ..WasmOptions::trusted()
        },
    )
    .expect("C wasm should load");
    let err = module
        .call("echo", r#"{"message":"memory-check"}"#)
        .expect_err("memory limit should interrupt execution");
    assert!(err.contains("WasmMemoryExceeded"), "got: {}", err);
}

#[test]
fn test_wasm_fuel_exhausted_returns_error() {
    let path = write_wasm_file(
        "fuel",
        r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "ok")
            (func (export "alloc") (param $len i32) (result i32)
                i32.const 0)
            (func (export "run") (param i32 i32) (result i64)
                i32.const 0
                i64.extend_i32_u
                i64.const 32
                i64.shl
                i32.const 2
                i64.extend_i32_u
                i64.or)
        )"#,
    );
    let wasm = Wasm::load_with(
        &path,
        WasmOptions {
            max_instructions: Some(0),
            ..WasmOptions::trusted()
        },
    )
    .expect("wasm should load");
    let err = wasm.call("run", "{}").expect_err("fuel should exhaust");
    assert!(err.contains("WasmFuelExhausted"), "got: {}", err);
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_memory_exceeded_returns_error() {
    let path = write_wasm_file(
        "memory",
        r#"(module
            (memory (export "memory") 32)
            (func (export "alloc") (param $len i32) (result i32)
                i32.const 0)
            (func (export "run") (param i32 i32) (result i64)
                i64.const 0)
        )"#,
    );
    let wasm = Wasm::load_with(
        &path,
        WasmOptions {
            max_memory_mb: Some(1),
            ..WasmOptions::trusted()
        },
    )
    .expect("wasm should load");
    let err = wasm.call("run", "{}").expect_err("memory should exceed");
    assert!(err.contains("WasmMemoryExceeded"), "got: {}", err);
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_timeout_returns_error() {
    let path = write_wasm_file(
        "timeout",
        r#"(module
            (memory (export "memory") 1)
            (func (export "alloc") (param $len i32) (result i32)
                i32.const 0)
            (func (export "run") (param i32 i32) (result i64)
                (local $i i32)
                i32.const 2000000000
                local.set $i
                (loop $l
                    local.get $i
                    i32.const 1
                    i32.sub
                    local.tee $i
                    br_if $l)
                i64.const 0)
        )"#,
    );
    let wasm = Wasm::load_with(
        &path,
        WasmOptions {
            timeout_ms: Some(0),
            ..WasmOptions::trusted()
        },
    )
    .expect("wasm should load");
    let err = wasm.call("run", "{}").expect_err("timeout should trigger");
    assert!(err.contains("WasmTimeout"), "got: {}", err);
    let _ = fs::remove_file(path);
}

#[test]
fn test_wasm_infinite_loop_blocked() {
    let path = write_wasm_file(
        "infinite-loop",
        r#"(module
            (memory (export "memory") 1)
            (data (i32.const 0) "ok")
            (func (export "alloc") (param $len i32) (result i32)
                i32.const 0)
            (func (export "run") (param i32 i32) (result i64)
                i32.const 0
                i64.extend_i32_u
                i64.const 32
                i64.shl
                i32.const 2
                i64.extend_i32_u
                i64.or)
        )"#,
    );
    let wasm = Wasm::load_with(
        &path,
        WasmOptions {
            max_instructions: Some(0),
            max_memory_mb: Some(4),
            timeout_ms: None,
            allow_fs: false,
            allow_net: false,
            allow_env: false,
        },
    )
    .expect("wasm should load");
    let err = wasm.call("run", "{}").expect_err("loop should be blocked");
    assert!(err.contains("WasmFuelExhausted"), "got: {}", err);
    let _ = fs::remove_file(path);
}
