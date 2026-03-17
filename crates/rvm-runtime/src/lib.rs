//! RVM bytecode interpreter

use fs_bytecode::{Chunk, Instruction};
use rvm_core::{Value, VmError};
use rvm_host::Output;
use std::collections::HashMap;

const STACK_MAX: usize = 1024;

/// Virtual machine for executing bytecode
pub struct Vm {
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
    chunk: Option<Chunk>,
    ip: usize,
    output: Box<dyn Output>,
}

impl Vm {
    /// Get list of defined global variable names (including native functions)
    pub fn get_global_names(&self) -> Vec<String> {
        self.globals.keys().cloned().collect()
    }

    /// Register a native function
    pub fn register_native(&mut self, name: &str, func: rvm_core::NativeFn) {
        self.globals
            .insert(name.to_string(), Value::NativeFunction(func));
    }

    /// Register standard native functions (like print)
    pub fn register_std_natives(&mut self) {
        self.register_native("print", |args| {
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    print!(" ");
                }
                match arg {
                    Value::Int(n) => print!("{}", n),
                    Value::String(s) => print!("{}", s),
                    Value::Nil => print!("nil"),
                    Value::NativeFunction(_) => print!("<function>"),
                }
            }
            println!();
            Ok(Value::Nil)
        });
    }

    /// Print a value using the configured output
    pub fn print_value(&mut self, value: &Value) -> Result<(), VmError> {
        let text = match value {
            Value::Int(n) => format!("{}", n),
            Value::String(s) => s.clone(),
            Value::Nil => "nil".to_string(),
            Value::NativeFunction(_) => "<function>".to_string(),
        };
        
        self.output
            .write(&text)
            .map_err(|_| VmError::UndefinedGlobal {
                name: "output error".to_string(),
            })?;
        
        self.output
            .write("\n")
            .map_err(|_| VmError::UndefinedGlobal {
                name: "output error".to_string(),
            })?;
        
        Ok(())
    }
}

impl Vm {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(STACK_MAX),
            globals: HashMap::new(),
            chunk: None,
            ip: 0,
            output: Box::new(rvm_host::StdOutput),
        }
    }

    pub fn with_output(output: Box<dyn Output>) -> Self {
        Self {
            stack: Vec::with_capacity(STACK_MAX),
            globals: HashMap::new(),
            chunk: None,
            ip: 0,
            output,
        }
    }

    pub fn execute(&mut self, chunk: Chunk) -> Result<(), VmError> {
        self.chunk = Some(chunk);
        self.ip = 0;
        self.stack.clear();

        self.run()
    }

    fn run(&mut self) -> Result<(), VmError> {
        loop {
            let chunk = self.chunk.as_ref().unwrap();

            if self.ip >= chunk.instructions.len() {
                return Err(VmError::InvalidInstructionPointer {
                    ip: self.ip,
                    code_len: chunk.instructions.len(),
                });
            }

            let instruction = &chunk.instructions[self.ip];
            self.ip += 1;

            match instruction {
                Instruction::LoadConst(idx) => {
                    let chunk = self.chunk.as_ref().unwrap();
                    let value = chunk
                        .constants
                        .get(*idx)
                        .ok_or(VmError::InvalidConstantIndex {
                            index: *idx,
                            pool_size: chunk.constants.len(),
                        })?;
                    self.push(value.clone())?;
                }
                Instruction::LoadGlobal(idx) => {
                    let name = {
                        let chunk = self.chunk.as_ref().unwrap();
                        chunk.strings.get(*idx).ok_or(VmError::InvalidStringIndex {
                            index: *idx,
                            pool_size: chunk.strings.len(),
                        })?.to_string()
                    };

                    let value = self
                        .globals
                        .get(&name)
                        .ok_or_else(|| VmError::UndefinedGlobal {
                            name: name.clone(),
                        })?;
                    self.push(value.clone())?;
                }
                Instruction::StoreGlobal(idx) => {
                    let name = {
                        let chunk = self.chunk.as_ref().unwrap();
                        chunk.strings.get(*idx).ok_or(VmError::InvalidStringIndex {
                            index: *idx,
                            pool_size: chunk.strings.len(),
                        })?.to_string()
                    };

                    let value = self.pop()?;
                    self.globals.insert(name, value);
                }
                Instruction::Add => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = self.binary_op_add(a, b)?;
                    self.push(result)?;
                }
                Instruction::Sub => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = self.binary_op_sub(a, b)?;
                    self.push(result)?;
                }
                Instruction::Mul => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = self.binary_op_mul(a, b)?;
                    self.push(result)?;
                }
                Instruction::Div => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let result = self.binary_op_div(a, b)?;
                    self.push(result)?;
                }
                Instruction::Call(argc) => {
                    let arg_count = *argc as usize;
                    let callee = self.pop()?;
                    
                    match callee {
                        Value::NativeFunction(func) => {
                            if self.stack.len() < arg_count {
                                return Err(VmError::StackUnderflow);
                            }
                            
                            let args_start = self.stack.len() - arg_count;
                            let args: Vec<Value> = self.stack.drain(args_start..).collect();
                            
                            let result = func(&args)?;
                            self.push(result)?;
                        }
                        _ => {
                            return Err(VmError::NotCallable {
                                value_type: callee.type_name().to_string(),
                            });
                        }
                    }
                }
                Instruction::Pop => {
                    self.pop()?;
                }
                Instruction::Return => {
                    return Ok(());
                }
            }
        }
    }

    fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack.len() >= STACK_MAX {
            return Err(VmError::StackOverflow);
        }
        self.stack.push(value);
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn binary_op_add(&self, a: Value, b: Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            (a, b) => Err(VmError::TypeError {
                expected: "int or string".to_string(),
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }

    fn binary_op_sub(&self, a: Value, b: Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (a, b) => Err(VmError::TypeError {
                expected: "int".to_string(),
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }

    fn binary_op_mul(&self, a: Value, b: Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (a, b) => Err(VmError::TypeError {
                expected: "int".to_string(),
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }

    fn binary_op_div(&self, a: Value, b: Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Int(_), Value::Int(0)) => Err(VmError::DivisionByZero),
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            (a, b) => Err(VmError::TypeError {
                expected: "int".to_string(),
                found: format!("{} and {}", a.type_name(), b.type_name()),
            }),
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
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
    fn test_vm_creation() {
        let vm = Vm::new();
        assert_eq!(vm.stack.len(), 0);
        assert_eq!(vm.globals.len(), 0);
    }

    #[test]
    fn test_load_const() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_constant(Value::Int(42));
        chunk.add_instruction(Instruction::LoadConst(idx));
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        vm.execute(chunk).unwrap();

        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Int(42));
    }

    #[test]
    fn test_store_and_load_global() {
        let mut chunk = Chunk::new();
        let const_idx = chunk.add_constant(Value::Int(100));
        let name_idx = chunk.add_string("x".to_string());

        chunk.add_instruction(Instruction::LoadConst(const_idx));
        chunk.add_instruction(Instruction::StoreGlobal(name_idx));
        chunk.add_instruction(Instruction::LoadGlobal(name_idx));
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        vm.execute(chunk).unwrap();

        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Int(100));
    }

    #[test]
    fn test_add_instruction() {
        let mut chunk = Chunk::new();
        let idx1 = chunk.add_constant(Value::Int(1));
        let idx2 = chunk.add_constant(Value::Int(2));

        chunk.add_instruction(Instruction::LoadConst(idx1));
        chunk.add_instruction(Instruction::LoadConst(idx2));
        chunk.add_instruction(Instruction::Add);
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        vm.execute(chunk).unwrap();

        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Int(3));
    }

    #[test]
    fn test_arithmetic_operations() {
        let mut chunk = Chunk::new();
        let idx10 = chunk.add_constant(Value::Int(10));
        let idx3 = chunk.add_constant(Value::Int(3));

        chunk.add_instruction(Instruction::LoadConst(idx10));
        chunk.add_instruction(Instruction::LoadConst(idx3));
        chunk.add_instruction(Instruction::Mul);
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        vm.execute(chunk).unwrap();

        assert_eq!(vm.stack[0], Value::Int(30));
    }

    #[test]
    fn test_division_by_zero() {
        let mut chunk = Chunk::new();
        let idx1 = chunk.add_constant(Value::Int(10));
        let idx0 = chunk.add_constant(Value::Int(0));

        chunk.add_instruction(Instruction::LoadConst(idx1));
        chunk.add_instruction(Instruction::LoadConst(idx0));
        chunk.add_instruction(Instruction::Div);
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        let result = vm.execute(chunk);

        assert!(result.is_err());
        match result {
            Err(VmError::DivisionByZero) => {}
            _ => panic!("Expected DivisionByZero error"),
        }
    }

    #[test]
    fn test_stack_underflow() {
        let mut chunk = Chunk::new();
        chunk.add_instruction(Instruction::Pop);
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        let result = vm.execute(chunk);

        assert!(result.is_err());
        match result {
            Err(VmError::StackUnderflow) => {}
            _ => panic!("Expected StackUnderflow error"),
        }
    }

    #[test]
    fn test_undefined_global() {
        let mut chunk = Chunk::new();
        let name_idx = chunk.add_string("undefined".to_string());
        chunk.add_instruction(Instruction::LoadGlobal(name_idx));
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        let result = vm.execute(chunk);

        assert!(result.is_err());
        match result {
            Err(VmError::UndefinedGlobal { name }) => {
                assert_eq!(name, "undefined");
            }
            _ => panic!("Expected UndefinedGlobal error"),
        }
    }

    #[test]
    fn test_type_error() {
        let mut chunk = Chunk::new();
        let idx_int = chunk.add_constant(Value::Int(1));
        let idx_str = chunk.add_constant(Value::String("hello".to_string()));

        chunk.add_instruction(Instruction::LoadConst(idx_int));
        chunk.add_instruction(Instruction::LoadConst(idx_str));
        chunk.add_instruction(Instruction::Sub);
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        let result = vm.execute(chunk);

        assert!(result.is_err());
        match result {
            Err(VmError::TypeError { .. }) => {}
            _ => panic!("Expected TypeError"),
        }
    }

    #[test]
    fn test_native_function_call() {
        fn test_add(args: &[Value]) -> Result<Value, VmError> {
            if args.len() != 2 {
                return Err(VmError::InvalidArgCount {
                    expected: 2,
                    found: args.len(),
                });
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                _ => Err(VmError::TypeError {
                    expected: "int".to_string(),
                    found: "other".to_string(),
                }),
            }
        }

        let mut chunk = Chunk::new();
        let idx1 = chunk.add_constant(Value::Int(5));
        let idx2 = chunk.add_constant(Value::Int(3));
        let func_idx = chunk.add_string("test_add".to_string());

        chunk.add_instruction(Instruction::LoadConst(idx1));
        chunk.add_instruction(Instruction::LoadConst(idx2));
        chunk.add_instruction(Instruction::LoadGlobal(func_idx));
        chunk.add_instruction(Instruction::Call(2));
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        vm.register_native("test_add", test_add);
        vm.execute(chunk).unwrap();

        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Int(8));
    }

    #[test]
    fn test_not_callable() {
        let mut chunk = Chunk::new();
        let idx = chunk.add_constant(Value::Int(42));

        chunk.add_instruction(Instruction::LoadConst(idx));
        chunk.add_instruction(Instruction::Call(0));
        chunk.add_instruction(Instruction::Return);

        let mut vm = Vm::new();
        let result = vm.execute(chunk);

        assert!(result.is_err());
        match result {
            Err(VmError::NotCallable { .. }) => {}
            _ => panic!("Expected NotCallable error"),
        }
    }

    #[test]
    fn test_print_value() {
        let mut vm = Vm::new();
        
        let result = vm.print_value(&Value::Int(42));
        assert!(result.is_ok());
        
        let result = vm.print_value(&Value::String("test".to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_std_natives() {
        let mut vm = Vm::new();
        vm.register_std_natives();
        
        assert!(vm.globals.contains_key("print"));
    }
}
