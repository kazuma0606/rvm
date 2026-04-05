// forge-cli: Phase 2-D / 3 E2E テスト

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// ForgeScript ファイルパスを `forge-new run` で実行し、stdout を返す
fn run_forge_file(path: &str) -> Result<String, String> {
    let result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", path])
        .output()
        .map_err(|e| e.to_string())?;

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).to_string())
    }
}

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

/// ForgeScript ソースを `forge-new run` で実行し、stdout を返す
fn run_forge(source: &str) -> Result<String, String> {
    let mut path = std::env::temp_dir();
    path.push(format!("forge_e2e_{}.forge", unique_suffix()));

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

// ── Phase 3 E2E テスト ─────────────────────────────────────────────────────

#[test]
fn e2e_collection_pipeline() {
    let src = r#"
let nums = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
let result = nums.filter(x => x % 2 == 0).map(x => x * x).sum()
print(result)
"#;
    // 偶数: 2,4,6,8,10 → 二乗: 4,16,36,64,100 → 合計: 220
    let out = run_forge(src).unwrap();
    assert_eq!(out, "220\n");
}

#[test]
fn e2e_generics_basic() {
    let src = r#"
struct Response<T> {
    body: T
}

impl<T> Response<T> {
    fn is_ok() -> bool {
        true
    }
}

enum Either<L, R> {
    Left(L),
    Right(R),
}

fn wrap<T>(v: T) -> Response<T> {
    Response { body: v }
}

let r = wrap(42)
print(r.body)
print(r.is_ok())
print(match Either::Left(7) {
    Either::Left(x) => x,
    Either::Right(_) => 0,
})
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "42\ntrue\n7\n");
}

#[test]
fn e2e_for_plus_collection() {
    let src = r#"
let squares = for i in [1..=5] { i * i }
let big = squares.filter(x => x > 5)
print(big)
"#;
    // 二乗: [1,4,9,16,25] → 5より大きい: [9,16,25]
    let out = run_forge(src).unwrap();
    assert_eq!(out, "[9, 16, 25]\n");
}

#[test]
fn e2e_nested_closures() {
    let src = r#"
let multiplier = factor => (x => x * factor)
let double = multiplier(2)
let triple = multiplier(3)
let nums = [1, 2, 3]
print(nums.map(double))
print(nums.map(triple))
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "[2, 4, 6]\n[3, 6, 9]\n");
}

#[test]
fn e2e_range_methods() {
    let src = r#"
let r = [1..=10]
print(r.filter(x => x % 3 == 0))
print(r.take(4).sum())
"#;
    // 3の倍数: [3,6,9]
    // 最初の4要素: [1,2,3,4] → sum: 10
    let out = run_forge(src).unwrap();
    assert_eq!(out, "[3, 6, 9]\n10\n");
}

// ── Phase 4-B E2E テスト（forge check）────────────────────────────────────

/// forge check を実行し (stdout, stderr, exit_code) を返す
fn run_check(source: &str) -> (String, String, bool) {
    let tid = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();
    let mut path = std::env::temp_dir();
    path.push(format!("forge_check_{}.forge", tid));

    std::fs::write(&path, source).expect("write temp file");

    let result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["check", path.to_str().unwrap_or("")])
        .output()
        .expect("run forge-new");

    let _ = std::fs::remove_file(&path);

    (
        String::from_utf8_lossy(&result.stdout).to_string(),
        String::from_utf8_lossy(&result.stderr).to_string(),
        result.status.success(),
    )
}

#[test]
fn e2e_check_no_error() {
    let src = r#"
fn add(a: number, b: number) -> number {
    a + b
}
let x = add(1, 2)
"#;
    let (out, _err, ok) = run_check(src);
    assert!(ok, "exit code 0 のはず");
    assert!(out.contains("エラーなし"), "stdout: {}", out);
}

#[test]
fn e2e_check_type_error() {
    // 数値 + 文字列 → 型エラー
    let src = r#"let x = 1 + "hello""#;
    let (_out, err, ok) = run_check(src);
    assert!(!ok, "exit code 1 のはず");
    assert!(!err.is_empty(), "stderr にエラーメッセージが出るはず");
    assert!(
        err.contains("型エラー") || err.contains("不一致"),
        "stderr: {}",
        err
    );
}

#[test]
fn e2e_check_match_exhaustion() {
    // none アームなし → 網羅性エラー
    let src = r#"
let v: number? = some(42)
match v {
    some(x) => x
}
"#;
    let (_out, err, ok) = run_check(src);
    assert!(!ok, "exit code 1 のはず");
    assert!(
        err.contains("none") || err.contains("網羅"),
        "stderr: {}",
        err
    );
}

// ── ラウンドトリップテスト（forge run == forge build + 実行）────────────────

/// ForgeScript ソースを `forge build` でバイナリ化して実行し、stdout を返す
fn run_built(source: &str) -> Result<String, String> {
    use std::fs;
    let mut src_path = std::env::temp_dir();
    src_path.push(format!("forge_rt_{}.forge", unique_suffix()));
    let mut bin_path = std::env::temp_dir();
    bin_path.push(format!("forge_rt_{}_bin", unique_suffix()));

    fs::write(&src_path, source).map_err(|e| e.to_string())?;

    let build_result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args([
            "build",
            src_path.to_str().unwrap_or(""),
            "-o",
            bin_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let _ = fs::remove_file(&src_path);

    if !build_result.status.success() {
        return Err(String::from_utf8_lossy(&build_result.stderr).to_string());
    }

    let run_result = Command::new(&bin_path)
        .output()
        .map_err(|e| e.to_string())?;

    let _ = fs::remove_file(&bin_path);

    if run_result.status.success() {
        Ok(String::from_utf8_lossy(&run_result.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&run_result.stderr).to_string())
    }
}

fn run_built_file(path: &str) -> Result<String, String> {
    use std::fs;

    let mut bin_path = std::env::temp_dir();
    bin_path.push(format!("forge_file_rt_{}_bin", unique_suffix()));

    let build_result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", path, "-o", bin_path.to_str().unwrap_or("")])
        .output()
        .map_err(|e| e.to_string())?;

    if !build_result.status.success() {
        return Err(String::from_utf8_lossy(&build_result.stderr).to_string());
    }

    let run_result = Command::new(&bin_path)
        .output()
        .map_err(|e| e.to_string())?;

    let _ = fs::remove_file(&bin_path);

    if run_result.status.success() {
        Ok(String::from_utf8_lossy(&run_result.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&run_result.stderr).to_string())
    }
}

fn run_transpile_error(source: &str) -> Result<String, String> {
    use std::fs;

    let mut src_path = std::env::temp_dir();
    src_path.push(format!("forge_tp_{}.forge", unique_suffix()));
    let mut out_path = std::env::temp_dir();
    out_path.push(format!("forge_tp_{}.rs", unique_suffix()));

    fs::write(&src_path, source).map_err(|e| e.to_string())?;

    let result = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args([
            "transpile",
            src_path.to_str().unwrap_or(""),
            "-o",
            out_path.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let _ = fs::remove_file(&src_path);
    let _ = fs::remove_file(&out_path);

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).to_string())
    }
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
fn roundtrip_hello_world() {
    let src = r#"print("Hello, World!")"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_arithmetic() {
    let src = r#"
print(2 + 3)
print(10 - 4)
print(3 * 4)
print(10 / 4)
print(10 % 3)
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_let_state() {
    let src = r#"
let x = 10
state y = 0
y = y + 5
print(x)
print(y)
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_if_expression() {
    let src = r#"
let x = 10
let label = if x > 5 { "big" } else { "small" }
print(label)
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_function_def() {
    let src = r#"
fn add(a: number, b: number) -> number {
    a + b
}
print(add(3, 4))
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_for_loop() {
    let src = r#"
for i in [1, 2, 3] {
    print(i)
}
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_string_interpolation() {
    let src = r#"
let name = "Forge"
print("Hello, {name}!")
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_option() {
    let src = r#"
let x: number? = some(42)
match x {
    some(n) => print(n),
    none    => print("none"),
}
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_collection_map() {
    let src = r#"
let nums = [1, 2, 3]
let doubled = nums.map(x => x * 2)
for n in doubled {
    print(n)
}
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_closure_fnmut() {
    let src = r#"
state total = 0
let add = x => {
    total = total + x
    total
}
print(add(2))
print(add(3))
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_struct_basic() {
    let src = r#"
struct Point {
    x: number
    y: number
}
let p = Point { x: 10, y: 20 }
println(p.x)
println(p.y)
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_enum_basic() {
    let src = r#"
enum Direction {
    North
    South
}

let d = Direction::North
match d {
    Direction::North => println("up")
    _ => println("other")
}
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_data_validate() {
    let src = r#"
data UserRegistration {
    username: string
} validate {
    username: length(3..20), alphanumeric
}

let reg = UserRegistration { username: "alice" }
match reg.validate() {
    ok(_) => println("valid"),
    err(msg) => println(msg),
}
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn roundtrip_when_platform() {
    let src = r#"
when platform.windows {
    fn platform_name() -> string { "windows" }
}
when platform.linux {
    fn platform_name() -> string { "linux" }
}
when platform.macos {
    fn platform_name() -> string { "macos" }
}
println(platform_name())
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

#[test]
fn build_async_basic() {
    let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(41) }
}

fn load() -> number! {
    fetch_num().await
}

println(load().await?)
"#;
    assert_eq!(run_built(src).unwrap(), "41\n");
}

#[test]
fn build_async_propagation() {
    let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(41) }
}

fn load() -> number! {
    fetch_num().await
}

fn render() -> number! {
    load()
}

println(render().await?)
"#;
    assert_eq!(run_built(src).unwrap(), "41\n");
}

#[test]
fn closure_with_await_compile_error() {
    let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(1) }
}

let f = () => fetch_num().await
"#;
    let err = run_transpile_error(src).unwrap_err();
    assert!(
        err.contains("クロージャ内での .await はサポートされていません"),
        "stderr: {}",
        err
    );
}

// ── Phase T-1 E2E テスト ─────────────────────────────────────────────────

#[test]
fn struct_basic() {
    let src = r#"
struct Point {
    x: number
    y: number
}
let p = Point { x: 10, y: 20 }
println(p.x)
println(p.y)
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "10\n20\n");
}

#[test]
fn struct_methods() {
    let src = r#"
struct Rectangle {
    width: number
    height: number
}

impl Rectangle {
    fn area() -> number {
        self.width * self.height
    }

    fn perimeter() -> number {
        (self.width + self.height) * 2
    }
}

let r = Rectangle { width: 3, height: 4 }
println(r.area())
println(r.perimeter())
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "12\n14\n");
}

#[test]
fn struct_derive() {
    let src = r#"
@derive(Debug, Clone, Accessor)
struct User {
    name: string
    age: number
}
let u = User { name: "Alice", age: 30 }
println(u.get_name())
println(u.get_age())
u.set_name("Bob")
println(u.get_name())
"#;
    let out = run_forge(src).unwrap();
    assert_eq!(out, "Alice\n30\nBob\n");
}

// ── Phase T-2: enum E2E テスト ──────────────────────────────────────────

#[test]
fn e2e_enum_basic() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/enum_basic.forge"
    ))
    .expect("enum_basic.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "up\nother\n");
}

#[test]
fn e2e_enum_match() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/enum_match.forge"
    ))
    .expect("enum_match.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "radius=5\n3x4\nmove 10,20\nhello\n");
}

// ── Phase T-3: trait / mixin E2E テスト ─────────────────────────────────

#[test]
fn e2e_trait_basic() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/trait_basic.forge"
    ))
    .expect("trait_basic.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "Hello\nHello\nHello\n");
}

#[test]
fn e2e_mixin_basic() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/mixin_basic.forge"
    ))
    .expect("mixin_basic.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "2026-01-01\npost-1\n");
}

// ── Phase T-4: data E2E テスト ───────────────────────────────────────────

#[test]
fn e2e_data_basic() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/data_basic.forge"
    ))
    .expect("data_basic.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "Alice\n1\nBob\n");
}

#[test]
fn e2e_data_validate() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/data_validate.forge"
    ))
    .expect("data_validate.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "valid\ninvalid: username: length\n");
}

// ── Phase T-5: typestate E2E テスト ─────────────────────────────────────

#[test]
fn e2e_typestate_connection() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/typestate_connection.forge"
    ))
    .expect("typestate_connection.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "SELECT 1\n");
}

#[test]
fn e2e_typestate_door() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/typestate_door.forge"
    ))
    .expect("typestate_door.forge が見つかりません");
    let out = run_forge(&src).unwrap();
    assert_eq!(out, "done\n");
}

#[test]
fn e2e_modules_basic() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/basic/main.forge"
    );
    let out = run_forge_file(main_path).unwrap();
    assert_eq!(out, "7\n7\nHello, World!\n");
}

#[test]
fn e2e_modules_pub_visibility() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/pub_visibility/main.forge"
    );
    let out = run_forge_file(main_path).unwrap();
    assert_eq!(out, "I am public\n42\n");
}

#[test]
fn e2e_modules_mod_forge() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/mod_forge/main.forge"
    );
    let out = run_forge_file(main_path).unwrap();
    assert_eq!(out, "7\n12\n16\n");
}

#[test]
fn roundtrip_modules_basic() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/basic/main.forge"
    );
    assert_eq!(
        run_forge_file(main_path).unwrap(),
        run_built_file(main_path).unwrap()
    );
}

#[test]
fn roundtrip_modules_pub_visibility() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/pub_visibility/main.forge"
    );
    assert_eq!(
        run_forge_file(main_path).unwrap(),
        run_built_file(main_path).unwrap()
    );
}

#[test]
fn roundtrip_modules_mod_forge() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/mod_forge/main.forge"
    );
    assert_eq!(
        run_forge_file(main_path).unwrap(),
        run_built_file(main_path).unwrap()
    );
}

#[test]
fn typestate_constraint_unit_state_error() {
    let src = r#"
typestate Door {
    states: [Closed, Open(number)]
}
"#;
    let err = run_transpile_error(src).unwrap_err();
    assert!(
        err.contains("typestate の状態は Unit 型のみサポートされます"),
        "stderr: {}",
        err
    );
}

#[test]
fn typestate_constraint_generic_error() {
    let src = r#"
typestate Query<T> {
    states: [Init]
}
"#;
    let err = run_transpile_error(src).unwrap_err();
    assert!(
        err.contains("ジェネリクス付き typestate は未サポートです"),
        "stderr: {}",
        err
    );
}

#[test]
fn typestate_constraint_derive_error() {
    let src = r#"
@derive(Debug)
typestate Door {
    states: [Closed, Open]
}
"#;
    let err = run_transpile_error(src).unwrap_err();
    assert!(
        err.contains("typestate への @derive は未サポートです"),
        "stderr: {}",
        err
    );
}

#[test]
fn typestate_constraint_any_block_error() {
    let src = r#"
typestate Door {
    states: [Closed, Open]

    any {
        fn label() -> string { "door" }
    }

    any {
        fn extra() -> string { "extra" }
    }
}
"#;
    let err = run_transpile_error(src).unwrap_err();
    assert!(
        err.contains("any ブロックは1つのみ定義できます"),
        "stderr: {}",
        err
    );
}

#[test]
fn roundtrip_typestate_basic() {
    let src = r#"
typestate Door {
    states: [Closed, Open, Locked]

    Closed {
        fn open() -> Open
        fn lock() -> Locked
    }

    Open {
        fn close() -> Closed
    }

    Locked {
        fn unlock(key: string) -> Closed
    }
}

fn main() {
    let door = Door::new<Closed>()
    let opened = door.open()
    let closed = opened.close()
    let locked = closed.lock()
    println("done")
}

main()
"#;
    assert_eq!(run_forge(src).unwrap(), run_built(src).unwrap());
}

// ── Phase M-4 E2E テスト ────────────────────────────────────────────────────

/// M-4-E: 循環参照ファイルの実行がエラーになることを確認
#[test]
fn e2e_modules_circular_error() {
    let main_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/modules/circular/main.forge"
    );
    let result = run_forge_file(main_path);
    assert!(
        result.is_err(),
        "循環参照ファイルの実行はエラーになるべきですが成功しました"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("循環参照")
            || err_msg.contains("circular")
            || err_msg.contains("Circular"),
        "エラーメッセージに循環参照の説明が含まれるべきです: {}",
        err_msg
    );
}

// ── Phase M-5: when キーワード E2E テスト ──────────────────────────────────

/// M-5-D E2E: platform 条件 — 実行環境に応じた出力を確認
#[test]
fn e2e_when_platform() {
    let fixture_path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/when_basic.forge");
    let result = run_forge_file(fixture_path);
    assert!(
        result.is_ok(),
        "when_basic.forge の実行に失敗しました: {:?}",
        result.err()
    );

    let output = result.unwrap();
    // 実行環境に応じた出力（windows/linux/macos）を動的に決定
    let expected_os = std::env::consts::OS;
    let trimmed = output.trim();
    assert_eq!(
        trimmed, expected_os,
        "when platform.{} ブロックが実行され '{}' が出力されるべきですが '{}' でした",
        expected_os, expected_os, trimmed
    );
}

// ── Phase FT-1 E2E テスト ─────────────────────────────────────────────────

/// `forge test <file>` を実行して stdout を返す
/// 失敗時（exit code != 0）は Err(stdout) を返す（stderr ではなく stdout）
fn run_forge_test(file: &str, filter: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_forge-new"));
    cmd.arg("test").arg(file);
    if let Some(f) = filter {
        cmd.arg("--filter").arg(f);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(stdout)
    }
}

#[test]
fn e2e_forge_test_pass() {
    let fixtures_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/fixtures/test_pass.forge", fixtures_dir);
    let out = run_forge_test(&path, None).expect("forge test should pass");
    assert!(out.contains("ok. 3 passed; 0 failed"), "出力: {}", out);
}

#[test]
fn e2e_forge_test_fail() {
    let fixtures_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/fixtures/test_fail.forge", fixtures_dir);
    let out = run_forge_test(&path, None).expect_err("forge test should fail with exit 1");
    assert!(out.contains("FAILED. 1 passed; 1 failed"), "出力: {}", out);
}

#[test]
fn e2e_forge_test_filter() {
    let fixtures_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/fixtures/test_pass.forge", fixtures_dir);
    let out = run_forge_test(&path, Some("add")).expect("forge test with filter should pass");
    // "add: 基本" と "add: 負の数" の2テストのみ実行
    assert!(out.contains("running 2 tests"), "出力: {}", out);
    assert!(out.contains("ok. 2 passed; 0 failed"), "出力: {}", out);
}
