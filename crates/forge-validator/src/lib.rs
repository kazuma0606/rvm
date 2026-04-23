use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use forge_vm::interpreter::Interpreter;
use forge_vm::value::{NativeObject, Value};

pub type RuleCheck = Box<dyn Fn(&Value) -> Result<(), String>>;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub fields: Vec<String>,
    pub message: String,
    pub value: Option<Value>,
}

impl ValidationError {
    pub fn new(fields: Vec<String>, message: impl Into<String>, value: Option<Value>) -> Self {
        Self {
            fields,
            message: message.into(),
            value,
        }
    }

    pub fn to_value(&self) -> Value {
        let mut fields = HashMap::new();
        fields.insert(
            "fields".to_string(),
            Value::List(Rc::new(RefCell::new(
                self.fields.iter().cloned().map(Value::String).collect(),
            ))),
        );
        fields.insert("message".to_string(), Value::String(self.message.clone()));
        fields.insert(
            "value".to_string(),
            Value::Option(self.value.clone().map(Box::new)),
        );
        Value::Struct {
            type_name: "ValidationError".to_string(),
            fields: Rc::new(RefCell::new(fields)),
        }
    }
}

pub struct RuleChain {
    pub checks: Vec<RuleCheck>,
    pub default_message: Option<String>,
}

impl std::fmt::Debug for RuleChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleChain")
            .field("checks_len", &self.checks.len())
            .field("default_message", &self.default_message)
            .finish()
    }
}

impl Default for RuleChain {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleChain {
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
            default_message: None,
        }
    }

    pub fn push<F>(&mut self, check: F)
    where
        F: Fn(&Value) -> Result<(), String> + 'static,
    {
        self.checks.push(Box::new(check));
    }

    pub fn run(&self, value: &Value) -> Result<(), String> {
        for check in &self.checks {
            if let Err(message) = check(value) {
                return Err(self.default_message.clone().unwrap_or(message));
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Rule {
    pub chain: RuleChain,
}

impl Default for Rule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule {
    pub fn new() -> Self {
        Self {
            chain: RuleChain::new(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.chain.default_message = Some(message.into());
        self
    }
}

impl NativeObject for Rule {
    fn type_name(&self) -> &'static str {
        "Rule"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct FieldRule {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CrossRule {
    pub fields: Vec<String>,
    pub predicate: Value,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct WhenRule {
    pub condition: Value,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct EachRule {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct NestedRule {
    pub field: String,
    pub validator: Rc<Validator>,
}

#[derive(Debug, Clone)]
pub enum ValidatorRule {
    Field(FieldRule),
    Cross(CrossRule),
    When(WhenRule),
    Each(EachRule),
    Nested(NestedRule),
}

#[derive(Debug, Clone)]
pub struct Validator {
    pub type_name: String,
    pub rules: Vec<ValidatorRule>,
}

impl Validator {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            rules: Vec::new(),
        }
    }
}

impl NativeObject for Validator {
    fn type_name(&self) -> &'static str {
        "Validator"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn register_validator_module(_interp: &mut Interpreter) {
    // forge-vm owns its private type registry. The VM calls its native
    // registration path directly; this function keeps the crate-level entry
    // point available for embedders while avoiding a cyclic dependency.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_error_to_value() {
        let error = ValidationError::new(
            vec!["email".to_string()],
            "invalid email",
            Some(Value::String("x".to_string())),
        );
        match error.to_value() {
            Value::Struct { type_name, fields } => {
                assert_eq!(type_name, "ValidationError");
                let fields = fields.borrow();
                assert_eq!(
                    fields.get("message"),
                    Some(&Value::String("invalid email".to_string()))
                );
                assert!(matches!(fields.get("value"), Some(Value::Option(Some(_)))));
            }
            other => panic!("expected ValidationError struct, got {:?}", other),
        }
    }

    #[test]
    fn rule_chain_returns_first_error() {
        let mut chain = RuleChain::new();
        chain.push(|_| Err("first".to_string()));
        chain.push(|_| Err("second".to_string()));

        assert_eq!(chain.run(&Value::Unit), Err("first".to_string()));
    }

    #[test]
    fn rule_chain_uses_default_message() {
        let mut chain = RuleChain::new();
        chain.default_message = Some("default".to_string());
        chain.push(|_| Err("specific".to_string()));

        assert_eq!(chain.run(&Value::Unit), Err("default".to_string()));
    }
}
