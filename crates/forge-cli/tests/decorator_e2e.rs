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
fn e2e_decorator_service_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_decorator_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("decorator_service.forge");
    std::fs::write(
        &file,
        r#"
@service @derive(Debug)
struct RegisterUserUseCase {
    name: string
}

@repository
struct PostgresUserRepository {
    url: string
}

fn main() {
    let usecase = RegisterUserUseCase { name: "register-user" }
    let repo = PostgresUserRepository { url: "postgres://localhost/app" }
    println(usecase.name)
    println(repo.url)
}

main()
"#,
    )
    .expect("write decorator_service.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run decorator_service.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "register-user\npostgres://localhost/app\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}
