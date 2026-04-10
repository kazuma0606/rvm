use forge_transpiler::transpile;

#[test]
fn test_transpile_memoize_decorator() {
    let src = r#"
        use forge/std/cache.{ memoize }

        @memoize
        fn compute(input: number) -> number {
            input * 2
        }
    "#;

    let output = transpile(src).expect("transpile memoize decorator");
    assert!(output.contains("memoize"));
}

#[test]
fn test_transpile_cache_ttl_decorator() {
    let src = r#"
        use forge/std/cache.{ cache }

        @cache(ttl: 10)
        fn fetch() -> string {
            "value"
        }
    "#;

    let output = transpile(src).expect("transpile cache decorator");
    assert!(output.contains("cache"));
}
