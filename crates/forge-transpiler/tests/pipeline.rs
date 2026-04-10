use forge_transpiler::transpile;

#[test]
fn test_transpile_pipeline_iterator_chain() {
    let src = r#"
        use forge/std/pipeline.{ pipeline, ListSource, CollectSink }

        fn main() -> unit {
            pipeline {
                source ListSource::new([1, 2, 3, 4])
                filter value => value % 2 == 0
                map value => value * 10
                sink CollectSink::new()
            }
        }
    "#;

    let output = transpile(src).expect("transpile pipeline iterator chain");
    assert!(
        output.contains(".filter("),
        "filter step should appear in transpiled output"
    );
    assert!(
        output.contains(".map("),
        "map step should appear in transpiled output"
    );
    assert!(
        output.contains(".run("),
        "sink run invocation should be emitted"
    );
}

#[test]
fn test_transpile_pipeline_parallel_rayon() {
    let src = r#"
        use forge/std/pipeline.{ pipeline, ListSource, CollectSink }

        fn main() -> unit {
            pipeline {
                source ListSource::new([1, 2, 3])
                parallel 4
                sink CollectSink::new()
            }
        }
    "#;

    let output = transpile(src).expect("transpile pipeline parallel");
    assert!(
        output.contains("_parallel_degree"),
        "parallel step should capture degree in generated code"
    );
}
