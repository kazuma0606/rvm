use forge_transpiler::transpile;

#[test]
fn test_transpile_logger_json() {
    let src = r#"
        use forge/std/log.{ JsonLogger }

        fn main() -> unit {
            let logger = JsonLogger::new()
            logger.info("starting", { module: "log" })
        }
    "#;

    let output = transpile(src).expect("transpile logger script");
    assert!(output.contains("JsonLogger::new"));
    assert!(output.contains("logger.info"));
}

#[test]
fn test_transpile_config_load() {
    let src = r#"
        use forge/std/config.{ Config }

        data AppConfig {
            port: number = 8080
            debug: bool = false
        }

        fn main() -> unit {
            let config = Config::load(AppConfig { })
            config.port
        }
    "#;

    let output = transpile(src).expect("transpile config script");
    assert!(output.contains("Config::load"));
    assert!(output.contains("AppConfig"));
}
