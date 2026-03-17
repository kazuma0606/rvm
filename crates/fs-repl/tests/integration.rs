use fs_repl::Repl;

#[test]
fn test_repl_basic_operations() {
    let mut repl = Repl::new();
    
    // Test basic let statements
    assert!(repl.eval("let x = 1").is_ok());
    assert!(repl.eval("let y = 2").is_ok());
    
    // Test arithmetic with previously defined variables
    assert!(repl.eval("let sum = x + y").is_ok());
    assert!(repl.eval("let product = x * y").is_ok());
}

#[test]
fn test_repl_string_operations() {
    let mut repl = Repl::new();
    
    assert!(repl.eval("let s = \"Hello\"").is_ok());
    assert!(repl.eval("let t = \" World\"").is_ok());
    assert!(repl.eval("let greeting = s + t").is_ok());
}

#[test]
fn test_repl_complex_expressions() {
    let mut repl = Repl::new();
    
    assert!(repl.eval("let a = 10").is_ok());
    assert!(repl.eval("let b = 20").is_ok());
    assert!(repl.eval("let c = 30").is_ok());
    assert!(repl.eval("let result = a + b * c").is_ok());
}

#[test]
fn test_repl_undefined_variable_error() {
    let mut repl = Repl::new();
    
    let result = repl.eval("let x = undefined_var");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Undefined variable"));
}

#[test]
fn test_repl_syntax_error() {
    let mut repl = Repl::new();
    
    let result = repl.eval("let x =");
    assert!(result.is_err());
}

#[test]
fn test_repl_native_function() {
    let mut repl = Repl::new();
    
    // print is a native function, should work
    let r1 = repl.eval("let message = \"test\"");
    assert!(r1.is_ok(), "Failed to define message: {:?}", r1);
    
    let r2 = repl.eval("print(message)");
    assert!(r2.is_ok(), "Failed to call print: {:?}", r2);
}
