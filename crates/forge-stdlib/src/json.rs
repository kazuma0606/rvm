use std::cell::RefCell;
use std::rc::Rc;

use forge_vm::value::Value;

pub fn parse(src: impl AsRef<str>) -> Result<Value, String> {
    let src = src.as_ref();
    let parsed: serde_json::Value =
        serde_json::from_str(src).map_err(|err| format!("invalid json: {}", err))?;
    json_value_to_forge_value(parsed)
}

fn json_value_to_forge_value(value: serde_json::Value) -> Result<Value, String> {
    match value {
        serde_json::Value::Null => Ok(Value::Unit),
        serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
        serde_json::Value::String(value) => Ok(Value::String(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Int(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Float(value))
            } else {
                Err("unsupported json number".to_string())
            }
        }
        serde_json::Value::Array(values) => {
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(json_value_to_forge_value(value)?);
            }
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }
        serde_json::Value::Object(entries) => {
            let mut items = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                items.push((Value::String(key), json_value_to_forge_value(value)?));
            }
            Ok(Value::Map(items))
        }
    }
}
