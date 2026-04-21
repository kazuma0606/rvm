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
    assert!(output.contains("std::time::Instant::now()"));
    assert!(output.contains("__forge_timed_start.elapsed().as_secs_f64()"));
    assert!(output.contains("metrics.histogram(\"response_time\""));
}
