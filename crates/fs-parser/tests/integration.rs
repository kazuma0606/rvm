//! Integration tests for ForgeScript frontend (lexer + parser)

use fs_parser::Parser;
use test_utils::load_fixture;

#[test]
fn test_sample_fixture() {
    let source = load_fixture("sample.fs", "parser integration test").unwrap();
    let module = Parser::parse(&source).unwrap();
    
    assert_eq!(module.statements.len(), 3);
}

#[test]
fn test_simple_add_fixture() {
    let source = load_fixture("simple_add.fs", "parser integration test").unwrap();
    let module = Parser::parse(&source).unwrap();
    
    assert_eq!(module.statements.len(), 1);
}

#[test]
fn test_error_unclosed_string() {
    let source = load_fixture("errors/unclosed_string.fs", "error test").unwrap();
    let result = Parser::parse(&source);
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("Unterminated") || err_msg.contains("string"));
}

#[test]
fn test_error_unexpected_token() {
    let source = load_fixture("errors/unexpected_token.fs", "error test").unwrap();
    let result = Parser::parse(&source);
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("identifier") || err_msg.contains("Unexpected"));
}

#[test]
fn test_error_unexpected_char() {
    let source = load_fixture("errors/unexpected_char.fs", "error test").unwrap();
    let result = Parser::parse(&source);
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("Unexpected") || err_msg.contains("@"));
}

#[test]
fn test_error_contains_position_info() {
    let result = Parser::parse("let x = @");
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("position") || err_msg.contains("8"));
}
