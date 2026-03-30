// forge-cli: Phase 2-D E2E テスト

use std::process::Command;

/// ForgeScript ソースを `forge-new run` で実行し、stdout を返す
fn run_forge(source: &str) -> Result<String, String> {
    // スレッドIDをファイル名に使って並列テストでも衝突しない
    let tid = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();
    let mut path = std::env::temp_dir();
    path.push(format!("forge_e2e_{}.forge", tid));

    std::fs::write(&path, source).map_err(|e| e.to_string())?;

    let result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", path.to_str().unwrap_or("")])
        .output()
        .map_err(|e| e.to_string())?;

    let _ = std::fs::remove_file(&path);

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).to_string())
    }
}

// ── E2E テスト ────────────────────────────────────────────────────────────

#[test]
fn e2e_hello_world() {
    let out = run_forge(r#"print("Hello, World!")"#).unwrap();
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn e2e_arithmetic() {
    let src = r#"
print(2 + 3)
print(10 - 4)
print(3 * 4)
print(10 / 4)
print(10 % 3)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "5\n6\n12\n2\n1\n");
}

#[test]
fn e2e_string_concat() {
    let src = r#"
let a = "Hello"
let b = ", World"
print(a + b)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "Hello, World\n");
}

#[test]
fn e2e_bool_logic() {
    let src = r#"
print(true && false)
print(true || false)
print(!true)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "false\ntrue\nfalse\n");
}

#[test]
fn e2e_let_state() {
    let src = r#"
let x = 10
state y = 0
y = y + 5
print(x)
print(y)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "10\n5\n");
}

#[test]
fn e2e_const() {
    let src = r#"
const MAX = 100
print(MAX)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "100\n");
}

#[test]
fn e2e_if_else_expr() {
    let src = r#"
let a = if true { 1 } else { 2 }
let b = if false { 3 } else { 4 }
print(a)
print(b)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "1\n4\n");
}

#[test]
fn e2e_while_loop() {
    let src = r#"
state i = 0
while i < 3 {
    print(i)
    i = i + 1
}
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "0\n1\n2\n");
}

#[test]
fn e2e_for_range() {
    let src = r#"
for i in [1..=3] {
    print(i)
}
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn e2e_for_expr() {
    let src = r#"
let doubled = for i in [1..=3] { i * 2 }
print(doubled)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "[2, 4, 6]\n");
}

#[test]
fn e2e_function_def() {
    let src = r#"
fn add(a: number, b: number) -> number {
    a + b
}
print(add(3, 4))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "7\n");
}

#[test]
fn e2e_function_return() {
    let src = r#"
fn abs(n: number) -> number {
    if n < 0 {
        return n * -1
    }
    n
}
print(abs(-5))
print(abs(3))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "5\n3\n");
}

#[test]
fn e2e_closure_basic() {
    let src = r#"
let double = x => x * 2
print(double(5))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "10\n");
}

#[test]
fn e2e_closure_capture() {
    let src = r#"
let factor = 3
let multiply = x => x * factor
print(multiply(4))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "12\n");
}

#[test]
fn e2e_match_literal() {
    let src = r#"
let n = 2
let s = match n {
    1 => "one",
    2 => "two",
    _ => "other"
}
print(s)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "two\n");
}

#[test]
fn e2e_match_option() {
    let src = r#"
let v = some(42)
match v {
    some(x) => print(x),
    none => print("nothing")
}
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "42\n");
}

#[test]
fn e2e_match_result() {
    let src = r#"
let r = ok(10)
match r {
    ok(v) => print(v),
    err(e) => print(e)
}
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "10\n");
}

#[test]
fn e2e_question_op() {
    let src = r#"
fn safe_double(s: string) -> number! {
    let n = number(s)?
    ok(n * 2)
}
let good = safe_double("21")
match good {
    ok(v) => print(v),
    err(e) => print("error")
}
let bad = safe_double("abc")
match bad {
    ok(v) => print(v),
    err(e) => print("error")
}
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "42\nerror\n");
}

#[test]
fn e2e_string_interpolation() {
    let src = r#"
let name = "World"
print("Hello, {name}!")
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn e2e_recursion() {
    let src = r#"
fn fib(n: number) -> number {
    if n <= 1 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}
print(fib(10))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "55\n");
}

#[test]
fn e2e_nested_scope() {
    let src = r#"
let x = 1
{
    let x = 2
    print(x)
}
print(x)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "2\n1\n");
}

#[test]
fn e2e_type_of() {
    let src = r#"
print(type_of(42))
print(type_of("hello"))
print(type_of(true))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "number\nstring\nbool\n");
}
