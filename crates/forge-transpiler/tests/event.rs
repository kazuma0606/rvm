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

        struct UserCreated {
            email: string
        }

        @on(UserCreated)
        fn handle(event: UserCreated) -> unit {
        }

        container {
            bind EventBus to EventBus::new()
        }
    "#;

    let output = transpile(src).expect("transpile on decorator");
    assert!(output.contains("pub fn register_event_handlers(&self)"));
    assert!(output.contains("self.event_bus.on::<UserCreated, _>(handle);"));
}
