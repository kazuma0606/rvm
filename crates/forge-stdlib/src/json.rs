use std::cell::RefCell;
use std::rc::Rc;

use forge_vm::value::{EnumData, Value};

pub fn stringify(value: &Value) -> Result<String, String> {
    serde_json::to_string(&forge_value_to_json_value(value)?)
        .map_err(|err| format!("failed to stringify: {}", err))
}

pub fn stringify_pretty(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(&forge_value_to_json_value(value)?)
        .map_err(|err| format!("failed to stringify pretty: {}", err))
}

pub fn parse(src: impl AsRef<str>) -> Result<Value, String> {
    let src = src.as_ref();
    let parsed: serde_json::Value =
        serde_json::from_str(src).map_err(|err| format!("invalid json: {}", err))?;
    json_value_to_forge_value(parsed)
}

fn forge_value_to_json_value(value: &Value) -> Result<serde_json::Value, String> {
    match value {
        Value::Int(value) => Ok(serde_json::Value::Number((*value).into())),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("float {} is not json representable", value)),
        Value::Bool(value) => Ok(serde_json::Value::Bool(*value)),
        Value::String(value) => Ok(serde_json::Value::String(value.clone())),
        Value::Unit => Ok(serde_json::Value::Null),
        Value::Option(Some(inner)) => forge_value_to_json_value(inner),
        Value::Option(None) => Ok(serde_json::Value::Null),
        Value::Result(Ok(inner)) => forge_value_to_json_value(inner),
        Value::Result(Err(err)) => Ok(serde_json::Value::String(format!("err({})", err))),
        Value::List(items) => {
            let converted = items
                .borrow()
                .iter()
                .map(forge_value_to_json_value)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(serde_json::Value::Array(converted))
        }
        Value::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (key, value) in entries {
                if let Value::String(key_str) = key {
                    let converted = forge_value_to_json_value(value)?;
                    map.insert(key_str.clone(), converted);
                } else {
                    return Err("map keys must be strings".to_string());
                }
            }
            Ok(serde_json::Value::Object(map))
        }
        Value::Struct { type_name, fields } => {
            let mut map = serde_json::Map::new();
            for (key, value) in fields.borrow().iter() {
                map.insert(key.clone(), forge_value_to_json_value(value)?);
            }
            map.insert(
                "__type".to_string(),
                serde_json::Value::String(type_name.clone()),
            );
            Ok(serde_json::Value::Object(map))
        }
        Value::Enum {
            type_name,
            variant,
            data,
        } => {
            let mut map = serde_json::Map::new();
            map.insert(
                "__enum".to_string(),
                serde_json::Value::String(format!("{}::{}", type_name, variant)),
            );
            match data {
                EnumData::Unit => {}
                EnumData::Tuple(items) => {
                    let array = items
                        .iter()
                        .map(forge_value_to_json_value)
                        .collect::<Result<Vec<_>, _>>()?;
                    map.insert("__data".to_string(), serde_json::Value::Array(array));
                }
                EnumData::Struct(fields) => {
                    for (key, value) in fields.iter() {
                        map.insert(key.clone(), forge_value_to_json_value(value)?);
                    }
                }
            }
            Ok(serde_json::Value::Object(map))
        }
        Value::Typestate {
            type_name,
            current_state,
            fields,
        } => {
            let mut map = serde_json::Map::new();
            map.insert(
                "__typestate".to_string(),
                serde_json::Value::String(type_name.clone()),
            );
            map.insert(
                "__state".to_string(),
                serde_json::Value::String(current_state.clone()),
            );
            for (key, value) in fields.borrow().iter() {
                map.insert(key.clone(), forge_value_to_json_value(value)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Err(format!("unsupported Forge value for JSON: {}", value)),
    }
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
