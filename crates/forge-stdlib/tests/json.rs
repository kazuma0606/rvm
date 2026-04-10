use std::cell::RefCell;
use std::rc::Rc;

use forge_stdlib::json::{parse, stringify, stringify_pretty};
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

#[test]
fn stringify_and_parse_roundtrip() {
    let source = Value::Map(vec![
        (
            Value::String("name".to_string()),
            Value::String("forge".to_string()),
        ),
        (Value::String("count".to_string()), Value::Int(2)),
    ]);

    let text = stringify(&source).expect("should stringify to json");
    assert!(text.contains("\"name\""));

    let restored = parse(text).expect("should parse back");
    assert_eq!(restored, source);
}

#[test]
fn stringify_pretty_includes_newlines() {
    let value = Value::List(Rc::new(RefCell::new(vec![
        Value::Bool(true),
        Value::Bool(false),
    ])));
    let pretty = stringify_pretty(&value).expect("pretty should succeed");
    assert!(pretty.contains('\n'));
}
