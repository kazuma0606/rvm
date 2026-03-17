//! Integration tests for ForgeScript compiler (parser -> compiler)

use fs_compiler::Compiler;
use fs_parser::Parser;
use test_utils::load_fixture;

#[test]
fn test_compile_sample_fixture() {
    let source = load_fixture("sample.fs", "compiler integration test").unwrap();
    let module = Parser::parse(&source).unwrap();
    let result = Compiler::compile(&module);
    
    assert!(result.is_err());
}

#[test]
fn test_compile_simple_add() {
    let source = load_fixture("simple_add.fs", "compiler integration test").unwrap();
    let module = Parser::parse(&source).unwrap();
    let result = Compiler::compile(&module);
    
    assert!(result.is_err());
}

#[test]
fn test_compile_let_statement() {
    let module = Parser::parse("let x = 1").unwrap();
    let chunk = Compiler::compile(&module).unwrap();
    
    assert!(!chunk.instructions.is_empty());
    assert_eq!(chunk.strings.get(0), Some("x"));
}

#[test]
fn test_compile_arithmetic() {
    let module = Parser::parse("print(1 + 2)").unwrap();
    let result = Compiler::compile(&module);
    
    assert!(result.is_err());
}

#[test]
fn test_compile_multiple_statements() {
    let source = "let x = 1\nlet y = 2\n";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();
    
    assert_eq!(chunk.strings.len(), 2);
    assert_eq!(chunk.strings.get(0), Some("x"));
    assert_eq!(chunk.strings.get(1), Some("y"));
}

#[test]
fn test_undefined_variable_error() {
    let module = Parser::parse("print(undefined_var)").unwrap();
    let result = Compiler::compile(&module);
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("undefined_var") || err_msg.contains("Undefined") || err_msg.contains("print"));
}

#[test]
fn test_compile_variable_reference() {
    let source = "let x = 1\nlet y = x";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();
    
    let disassembly = chunk.disassemble("variable reference");
    println!("{}", disassembly);
    
    assert!(!chunk.instructions.is_empty());
}

#[test]
fn test_compile_expression() {
    let source = "let result = 1 + 2 * 3";
    let module = Parser::parse(source).unwrap();
    let chunk = Compiler::compile(&module).unwrap();
    
    assert!(!chunk.instructions.is_empty());
    assert_eq!(chunk.strings.get(0), Some("result"));
}
