use forge_transpiler::transpile;

#[test]
fn test_transpile_retry_decorator() {
    let src = r#"
        use forge/std/retry.{ retry }

        @retry(max: 2)
        fn call() -> unit {
            let value = fetch()
            value
        }
    "#;

    let output = transpile(src).expect("transpile retry decorator");
    assert!(output.contains("retry("));
}

#[test]
fn test_transpile_circuit_breaker_decorator() {
    let src = r#"
        use forge/std/retry::{ circuit_breaker }

        @circuit_breaker(threshold: 2, timeout_ms: 100)
        fn call() -> unit {
            fetch()
        }
    "#;

    let output = transpile(src).expect("transpile circuit breaker decorator");
    assert!(output.contains("circuit_breaker"));
}
