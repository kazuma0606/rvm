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

#[test]
fn e2e_event_on_decorator_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_event_on_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("event_on_decorator.forge");
    std::fs::write(
        &file,
        r#"
struct UserCreated {
    email: string
}

@on(UserCreated)
fn handle_user_created(event: UserCreated) -> unit {
    println(event.email)
}

fn main() {
    handle_user_created(UserCreated { email: "user@example.com" })
}

main()
"#,
    )
    .expect("write event_on_decorator.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run event_on_decorator.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "user@example.com\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn e2e_metrics_timed_decorator_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_metrics_timed_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("metrics_timed_decorator.forge");
    std::fs::write(
        &file,
        r#"
@timed(metric: "response_time")
fn handle() -> string {
    "ok"
}

fn main() {
    println(handle())
}

main()
"#,
    )
    .expect("write metrics_timed_decorator.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run metrics_timed_decorator.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");

    let _ = std::fs::remove_dir_all(dir);
}
