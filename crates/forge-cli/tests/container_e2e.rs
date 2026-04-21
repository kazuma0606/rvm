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
fn e2e_container_basic_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_container_basic_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("container_basic.forge");
    std::fs::write(
        &file,
        r#"
struct UserRepository {}

@repository
struct PostgresUserRepository {}

@service
struct RegisterUserUseCase {
    repo: UserRepository
}

container {
    bind UserRepository to PostgresUserRepository
}

fn main() {
    let usecase = RegisterUserUseCase {}
    println(usecase)
}

main()
"#,
    )
    .expect("write container_basic.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run container_basic.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "RegisterUserUseCase { repo: PostgresUserRepository { } }\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn e2e_container_env_switch_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_container_env_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("container_env_switch.forge");
    std::fs::write(
        &file,
        r#"
struct Logger {}
struct JsonLogger {}
struct SilentLogger {}

@service
struct AppService {
    logger: Logger
}

let env = "prod"

container {
    bind Logger to match env {
        "prod" => JsonLogger
        _ => SilentLogger
    }
}

fn main() {
    let service = AppService {}
    println(service)
}

main()
"#,
    )
    .expect("write container_env_switch.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run container_env_switch.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "AppService { logger: JsonLogger { } }\n"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn e2e_app_typestate_di_forge() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("forge_typestate_di_e2e_{}", unique_suffix()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let file = dir.join("app_typestate_di.forge");
    std::fs::write(
        &file,
        r#"
struct UserRepository {}
struct PostgresUserRepository {}

typestate App {
    states: [Unconfigured, Configured, Running]

    Unconfigured {
        fn configure(c: container) -> Configured
    }

    Configured {
        fn start() -> Running!
    }
}

fn main() {
    let app = App::new<Unconfigured>()
    let configured = app.configure(container {
        bind UserRepository to PostgresUserRepository
    })
    let running = configured.start()?
    println(running)
}

main()
"#,
    )
    .expect("write app_typestate_di.forge");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["run", file.to_str().expect("file")])
        .output()
        .expect("forge run app_typestate_di.forge");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "App<Running>\n");

    let _ = std::fs::remove_dir_all(dir);
}
