//! ForgeScript REPL (Read-Eval-Print Loop)

use fs_compiler::Compiler;
use fs_parser::Parser;
use rvm_core::Value;
use rvm_runtime::Vm;
use std::io::{self, Write};

/// REPL session
pub struct Repl {
    vm: Vm,
}

impl Repl {
    pub fn new() -> Self {
        let mut vm = Vm::new();
        vm.register_std_natives();
        
        Self { vm }
    }

    /// Start the REPL session
    pub fn run(&mut self) -> io::Result<()> {
        println!("ForgeScript REPL v0.1.0");
        println!("Type 'exit' or 'quit' to exit, 'help' for help");
        println!();

        loop {
            print!(">>> ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            match input {
                "exit" | "quit" => {
                    println!("Goodbye!");
                    break;
                }
                "help" => {
                    self.print_help();
                    continue;
                }
                "clear" => {
                    self.vm = Vm::new();
                    self.vm.register_std_natives();
                    println!("Environment cleared");
                    continue;
                }
                _ => {}
            }

            match self.eval(input) {
                Ok(value) => {
                    if !matches!(value, Value::Nil) {
                        println!("{}", self.format_value(&value));
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Evaluate a single line of input
    pub fn eval(&mut self, input: &str) -> Result<Value, String> {
        let module = Parser::parse(input).map_err(|e| format!("{}", e))?;

        let existing_globals = self.vm.get_global_names();
        let chunk = Compiler::compile_with_context(&module, existing_globals)
            .map_err(|e| format!("{}", e))?;

        self.vm.execute(chunk).map_err(|e| format!("{}", e))?;

        Ok(Value::Nil)
    }

    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::Int(n) => format!("{}", n),
            Value::String(s) => format!("\"{}\"", s),
            Value::Nil => "nil".to_string(),
            Value::NativeFunction(_) => "<function>".to_string(),
        }
    }

    fn print_help(&self) {
        println!("ForgeScript REPL Commands:");
        println!("  exit, quit    Exit the REPL");
        println!("  help          Show this help message");
        println!("  clear         Clear the environment");
        println!();
        println!("Supported ForgeScript syntax:");
        println!("  let x = 1              Variable declaration");
        println!("  let y = x + 2          Arithmetic operations");
        println!("  let s = \"Hello\"        String literals");
        println!("  let t = s + \" World\"   String concatenation");
        println!();
    }
}

impl Default for Repl {
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
    fn test_repl_creation() {
        let _repl = Repl::new();
    }

    #[test]
    fn test_eval_let_statement() {
        let mut repl = Repl::new();
        let result = repl.eval("let x = 42");
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_arithmetic() {
        let mut repl = Repl::new();
        let r1 = repl.eval("let x = 1");
        assert!(r1.is_ok(), "Failed to eval 'let x = 1': {:?}", r1);
        
        let r2 = repl.eval("let y = 2");
        assert!(r2.is_ok(), "Failed to eval 'let y = 2': {:?}", r2);
        
        let result = repl.eval("let sum = x + y");
        assert!(result.is_ok(), "Failed to eval 'let sum = x + y': {:?}", result);
    }

    #[test]
    fn test_eval_error() {
        let mut repl = Repl::new();
        let result = repl.eval("let x = undefined");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_value() {
        let repl = Repl::new();
        assert_eq!(repl.format_value(&Value::Int(42)), "42");
        assert_eq!(repl.format_value(&Value::String("hello".to_string())), "\"hello\"");
        assert_eq!(repl.format_value(&Value::Nil), "nil");
    }
}
