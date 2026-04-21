use forge_transpiler::transpile;

#[test]
fn test_container_basic() {
    let src = r#"
        trait UserRepository {}

        @service
        struct RegisterUserUseCase {
            repo: UserRepository
        }

        struct PostgresUserRepository {}

        container {
            bind UserRepository to PostgresUserRepository
        }
    "#;

    let output = transpile(src).expect("transpile container");
    assert!(output.contains("pub struct Container"));
    assert!(output.contains("pub user_repository: std::sync::Arc<dyn UserRepository>"));
    assert!(output.contains("repo: std::sync::Arc<dyn UserRepository>"));
    assert!(output.contains("PostgresUserRepository::default()"));
    assert!(output.contains("pub fn register_register_user_use_case(&self) -> RegisterUserUseCase"));
    assert!(output.contains("repo: std::sync::Arc::clone(&self.user_repository)"));
}

#[test]
fn test_container_match() {
    let src = r#"
        trait Logger {}
        struct JsonLogger {}
        struct SilentLogger {}

        container {
            bind Logger to match env {
                "prod" => JsonLogger
                _ => SilentLogger
            }
        }
    "#;

    let output = transpile(src).expect("transpile container match");
    assert!(output.contains("pub logger: std::sync::Arc<dyn Logger>"));
    assert!(output.contains("match env"));
    assert!(output.contains("JsonLogger"));
    assert!(output.contains("SilentLogger"));
}

#[test]
fn test_container_multi() {
    let src = r#"
        trait UserRepository {}
        trait EmailService {}

        container {
            bind UserRepository to PostgresUserRepository
            bind EmailService to SmtpEmailService
        }
    "#;

    let output = transpile(src).expect("transpile multi container");
    assert!(output.contains("pub user_repository: std::sync::Arc<dyn UserRepository>"));
    assert!(output.contains("pub email_service: std::sync::Arc<dyn EmailService>"));
    assert!(output.contains("PostgresUserRepository::default()"));
    assert!(output.contains("SmtpEmailService::default()"));
}

#[test]
fn test_typestate_container_integration() {
    let src = r#"
        trait UserRepository {}
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
        }
    "#;

    let output = transpile(src).expect("transpile typestate container");
    assert!(output.contains("pub struct Container"));
    assert!(output.contains("pub fn configure(self, c: Container) -> App<Configured>"));
    assert!(output.contains("Container::new()"));
    assert!(output.contains("PostgresUserRepository::default()"));
}
