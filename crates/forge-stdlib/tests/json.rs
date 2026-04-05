use std::cell::RefCell;
use std::rc::Rc;

use forge_stdlib::json::parse;
use forge_vm::value::Value;

fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

#[test]
fn parse_valid_json_to_forge_value() {
    let value =
        parse(r#"{"name":"forge","count":2,"items":[true,null]}"#).expect("json should parse");
    let Value::Map(entries) = value else {
        panic!("expected map");
    };

    assert!(entries.contains(&(
        Value::String("name".to_string()),
        Value::String("forge".to_string())
    )));
    assert!(entries.contains(&(Value::String("count".to_string()), Value::Int(2))));
    assert!(entries.contains(&(
        Value::String("items".to_string()),
        list(vec![Value::Bool(true), Value::Unit])
    )));
}

#[test]
fn parse_invalid_json_returns_error() {
    let err = parse("{").expect_err("invalid json should fail");
    assert!(err.contains("invalid json"));
}
