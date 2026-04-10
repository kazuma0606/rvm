use forge_transpiler::transpile;

#[test]
fn test_transpile_timed_decorator() {
    let src = r#"
        use forge/std/metrics.{ MetricsBackend }

        @timed(metric: "response_time")
        fn handle() -> unit {
            process()
        }
    "#;

    let output = transpile(src).expect("transpile timed decorator");
    assert!(output.contains("@timed"));
}
