//! RVM core types and abstractions

/// Native function type
pub type NativeFn = fn(&[Value]) -> Result<Value, VmError>;

/// Value type for RVM
#[derive(Clone)]
pub enum Value {
    Int(i64),
    String(String),
    Nil,
    NativeFunction(NativeFn),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "Int({})", n),
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Nil => write!(f, "Nil"),
            Value::NativeFunction(_) => write!(f, "NativeFunction(<fn>)"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::NativeFunction(a), Value::NativeFunction(b)) => {
                std::ptr::eq(a as *const NativeFn, b as *const NativeFn)
            }
            _ => false,
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::String(_) => "string",
            Value::Nil => "nil",
            Value::NativeFunction(_) => "function",
        }
    }
}

/// VM error type
#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    StackUnderflow,
    StackOverflow,
    TypeError { expected: String, found: String },
    DivisionByZero,
    UndefinedGlobal { name: String },
    InvalidInstructionPointer { ip: usize, code_len: usize },
    InvalidConstantIndex { index: usize, pool_size: usize },
    InvalidStringIndex { index: usize, pool_size: usize },
    InvalidArgCount { expected: usize, found: usize },
    NotCallable { value_type: String },
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::StackUnderflow => write!(f, "Stack underflow"),
            VmError::StackOverflow => write!(f, "Stack overflow"),
            VmError::TypeError { expected, found } => {
                write!(f, "Type error: expected {}, found {}", expected, found)
            }
            VmError::DivisionByZero => write!(f, "Division by zero"),
            VmError::UndefinedGlobal { name } => write!(f, "Undefined global: {}", name),
            VmError::InvalidInstructionPointer { ip, code_len } => {
                write!(
                    f,
                    "Invalid instruction pointer {} (code length: {})",
                    ip, code_len
                )
            }
            VmError::InvalidConstantIndex { index, pool_size } => {
                write!(
                    f,
                    "Invalid constant index {} (pool size: {})",
                    index, pool_size
                )
            }
            VmError::InvalidStringIndex { index, pool_size } => {
                write!(
                    f,
                    "Invalid string index {} (pool size: {})",
                    index, pool_size
                )
            }
            VmError::InvalidArgCount { expected, found } => {
                write!(
                    f,
                    "Invalid argument count: expected {}, found {}",
                    expected, found
                )
            }
            VmError::NotCallable { value_type } => {
                write!(f, "Value of type {} is not callable", value_type)
            }
        }
    }
}

impl std::error::Error for VmError {}

/// Function identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(pub usize);

/// Module identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(pub usize);

/// Call frame for function execution
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub function_id: FunctionId,
    pub ip: usize,
    pub stack_base: usize,
}

impl CallFrame {
    pub fn new(function_id: FunctionId, stack_base: usize) -> Self {
        debug_assert!(stack_base < 1024 * 1024, "Stack base too large");
        Self {
            function_id,
            ip: 0,
            stack_base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        assert!(true);
    }

    #[test]
    fn test_value_creation() {
        let int_val = Value::Int(42);
        let str_val = Value::String("hello".to_string());
        let nil_val = Value::Nil;

        assert_eq!(int_val.type_name(), "int");
        assert_eq!(str_val.type_name(), "string");
        assert_eq!(nil_val.type_name(), "nil");
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::Int(42), Value::Int(42));
        assert_ne!(Value::Int(42), Value::Int(43));
        assert_eq!(
            Value::String("test".to_string()),
            Value::String("test".to_string())
        );
    }

    #[test]
    fn test_vm_error_display() {
        let err = VmError::StackUnderflow;
        assert_eq!(format!("{}", err), "Stack underflow");

        let err = VmError::TypeError {
            expected: "int".to_string(),
            found: "string".to_string(),
        };
        assert!(format!("{}", err).contains("int"));
        assert!(format!("{}", err).contains("string"));
    }

    #[test]
    fn test_function_id() {
        let id1 = FunctionId(0);
        let id2 = FunctionId(1);
        let id3 = FunctionId(0);

        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_module_id() {
        let id1 = ModuleId(0);
        let id2 = ModuleId(1);

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_call_frame_creation() {
        let frame = CallFrame::new(FunctionId(0), 10);
        
        assert_eq!(frame.function_id, FunctionId(0));
        assert_eq!(frame.ip, 0);
        assert_eq!(frame.stack_base, 10);
    }

    #[test]
    #[should_panic(expected = "Stack base too large")]
    fn test_call_frame_invalid_stack_base() {
        CallFrame::new(FunctionId(0), 2_000_000);
    }
}
