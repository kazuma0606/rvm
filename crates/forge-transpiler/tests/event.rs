use forge_transpiler::transpile;

#[test]
fn test_transpile_emit_async() {
    let src = r#"
        use forge/std/event.{ EventBus, EventMode }

        fn main() -> unit {
            let bus = EventBus::new(EventMode::Async)
            bus.emit(UserCreated { user_id: "u1", email: "e" })
        }
    "#;

    let output = transpile(src).expect("transpile emit async");
    assert!(output.contains("EventBus::new"));
    assert!(output.contains("bus.emit"));
}

#[test]
fn test_transpile_on_decorator() {
    let src = r#"
        use forge/std/event.{ EventBus }

        @on(UserCreated)
        fn handle(self, event: UserCreated) -> unit {
            log(event.email)
        }
    "#;

    let output = transpile(src).expect("transpile on decorator");
    assert!(output.contains("@on"));
}
