//! Integration tests for RVM runtime (compiler + VM)

use fs_compiler::Compiler;
use fs_parser::Parser;
use rvm_core::Value;
use rvm_runtime::Vm;

#[test]
fn test_execute_simple_arithmetic() {
    let source = "let x = 1 + 2";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    vm.execute(chunk).unwrap();
}

#[test]
fn test_execute_variable_reference() {
    let source = "let x = 10\nlet y = x";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    vm.execute(chunk).unwrap();
}

#[test]
fn test_execute_complex_expression() {
    let source = "let result = 2 * 3 + 4";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    vm.execute(chunk).unwrap();
}

#[test]
fn test_execute_with_native_function() {
    fn test_print(args: &[Value]) -> Result<Value, rvm_core::VmError> {
        for arg in args {
            match arg {
                Value::Int(n) => println!("{}", n),
                Value::String(s) => println!("{}", s),
                _ => {}
            }
        }
        Ok(Value::Nil)
    }

    let source = "let x = 42";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    vm.register_native("print", test_print);
    vm.execute(chunk).unwrap();
}

#[test]
fn test_runtime_error_undefined_variable() {
    let source = "let x = undefined_var";
    let module = Parser::parse(source).unwrap();
    let result = Compiler::compile(&module);

    assert!(result.is_err());
}

#[test]
fn test_runtime_error_division_by_zero() {
    let source = "let x = 10 / 0";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    let result = vm.execute(chunk);

    assert!(result.is_err());
}

#[test]
fn test_execute_string_concatenation() {
    let source = r#"let greeting = "Hello" + " " + "World""#;
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();

    let mut vm = Vm::new();
    vm.execute(chunk).unwrap();
}
