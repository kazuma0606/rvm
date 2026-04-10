use forge_transpiler::transpile;

#[test]
fn test_transpile_defer_decorator() {
    let src = r#"
        struct Resource {}
        impl Resource {
            fn cleanup(self) -> unit {
            }
        }

        @defer(cleanup: "cleanup")
        fn open() -> Resource {
            Resource {}
        }

        fn main() -> unit {
            let r = open()
        }
    "#;

    let output = transpile(src).expect("transpile");
    assert!(output.contains("scopeguard::defer"));
    assert!(output.contains("__forge_defer_guard"));
    assert!(output.contains("r.cleanup();"));
}
