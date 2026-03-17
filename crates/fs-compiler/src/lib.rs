//! ForgeScript compiler (AST to bytecode)

use fs_ast::*;
use fs_bytecode::{Chunk, Instruction};
use rvm_core::Value;
use std::collections::HashMap;

/// Compiler error
#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    UndefinedVariable { name: String, span: Span },
    UnsupportedNode { node_type: String, span: Span },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UndefinedVariable { name, span } => {
                write!(
                    f,
                    "Undefined variable '{}' at {}:{}",
                    name, span.start, span.end
                )
            }
            CompileError::UnsupportedNode { node_type, span } => {
                write!(
                    f,
                    "Unsupported node type '{}' at {}:{}",
                    node_type, span.start, span.end
                )
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// Compiler for ForgeScript
pub struct Compiler {
    chunk: Chunk,
    globals: HashMap<String, usize>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            chunk: Chunk::new(),
            globals: HashMap::new(),
        }
    }

    pub fn with_globals(existing_globals: Vec<String>) -> Self {
        let mut compiler = Self::new();
        for name in existing_globals {
            let idx = compiler.chunk.add_string(name.clone());
            compiler.globals.insert(name, idx);
        }
        compiler
    }

    pub fn compile(module: &Module) -> Result<Chunk, CompileError> {
        let mut compiler = Compiler::new();
        compiler.compile_module(module)?;
        Ok(compiler.chunk)
    }

    pub fn compile_with_context(module: &Module, existing_globals: Vec<String>) -> Result<Chunk, CompileError> {
        let mut compiler = Compiler::with_globals(existing_globals);
        compiler.compile_module(module)?;
        Ok(compiler.chunk)
    }

    fn compile_module(&mut self, module: &Module) -> Result<(), CompileError> {
        for stmt in &module.statements {
            self.compile_statement(stmt)?;
        }
        self.chunk.add_instruction(Instruction::Return);
        Ok(())
    }

    fn compile_statement(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match stmt {
            Stmt::Let { ident, value, .. } => {
                self.compile_expr(value)?;
                
                let name_idx = self.chunk.add_string(ident.name.clone());
                if !self.globals.contains_key(&ident.name) {
                    self.globals.insert(ident.name.clone(), name_idx);
                }
                
                self.chunk.add_instruction(Instruction::StoreGlobal(name_idx));
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                self.compile_expr(expr)?;
                self.chunk.add_instruction(Instruction::Pop);
                Ok(())
            }
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Literal { value, .. } => {
                let val = match value {
                    Literal::Int(n) => Value::Int(*n),
                    Literal::String(s) => Value::String(s.clone()),
                };
                let idx = self.chunk.add_constant(val);
                self.chunk.add_instruction(Instruction::LoadConst(idx));
                Ok(())
            }
            Expr::Ident { ident, span } => {
                if let Some(&name_idx) = self.globals.get(&ident.name) {
                    self.chunk.add_instruction(Instruction::LoadGlobal(name_idx));
                    Ok(())
                } else {
                    Err(CompileError::UndefinedVariable {
                        name: ident.name.clone(),
                        span: *span,
                    })
                }
            }
            Expr::Binary { op, left, right, .. } => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                
                let instruction = match op {
                    BinaryOp::Add => Instruction::Add,
                    BinaryOp::Sub => Instruction::Sub,
                    BinaryOp::Mul => Instruction::Mul,
                    BinaryOp::Div => Instruction::Div,
                };
                
                self.chunk.add_instruction(instruction);
                Ok(())
            }
            Expr::Call { callee, args, .. } => {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                
                self.compile_expr(callee)?;
                
                let arg_count = args.len() as u8;
                self.chunk.add_instruction(Instruction::Call(arg_count));
                Ok(())
            }
        }
    }
}

impl Default for Compiler {
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
    fn test_compile_let_statement() {
        let module = Module::new(vec![Stmt::Let {
            ident: Ident::new("x".to_string(), Span::new(4, 5)),
            value: Expr::Literal {
                value: Literal::Int(1),
                span: Span::new(8, 9),
            },
            span: Span::new(0, 9),
        }]);

        let chunk = Compiler::compile(&module).unwrap();
        
        assert_eq!(chunk.instructions.len(), 3);
        assert_eq!(chunk.instructions[0], Instruction::LoadConst(0));
        assert_eq!(chunk.instructions[1], Instruction::StoreGlobal(0));
        assert_eq!(chunk.instructions[2], Instruction::Return);
        
        assert_eq!(chunk.constants.get(0), Some(&Value::Int(1)));
        assert_eq!(chunk.strings.get(0), Some("x"));
    }

    #[test]
    fn test_compile_binary_expr() {
        let module = Module::new(vec![Stmt::Expr {
            expr: Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Literal {
                    value: Literal::Int(1),
                    span: Span::dummy(),
                }),
                right: Box::new(Expr::Literal {
                    value: Literal::Int(2),
                    span: Span::dummy(),
                }),
                span: Span::dummy(),
            },
            span: Span::dummy(),
        }]);

        let chunk = Compiler::compile(&module).unwrap();
        
        assert!(chunk.instructions.contains(&Instruction::LoadConst(0)));
        assert!(chunk.instructions.contains(&Instruction::LoadConst(1)));
        assert!(chunk.instructions.contains(&Instruction::Add));
        assert!(chunk.instructions.contains(&Instruction::Pop));
    }

    #[test]
    fn test_compile_call() {
        let module = Module::new(vec![Stmt::Expr {
            expr: Expr::Call {
                callee: Box::new(Expr::Ident {
                    ident: Ident::new("print".to_string(), Span::dummy()),
                    span: Span::dummy(),
                }),
                args: vec![Expr::Literal {
                    value: Literal::Int(42),
                    span: Span::dummy(),
                }],
                span: Span::dummy(),
            },
            span: Span::dummy(),
        }]);

        let result = Compiler::compile(&module);
        assert!(result.is_err());
    }

    #[test]
    fn test_undefined_variable() {
        let module = Module::new(vec![Stmt::Expr {
            expr: Expr::Ident {
                ident: Ident::new("undefined".to_string(), Span::new(0, 9)),
                span: Span::new(0, 9),
            },
            span: Span::new(0, 9),
        }]);

        let result = Compiler::compile(&module);
        assert!(result.is_err());
        
        match result {
            Err(CompileError::UndefinedVariable { name, .. }) => {
                assert_eq!(name, "undefined");
            }
            _ => panic!("Expected UndefinedVariable error"),
        }
    }

    #[test]
    fn test_multiple_statements() {
        let module = Module::new(vec![
            Stmt::Let {
                ident: Ident::new("x".to_string(), Span::dummy()),
                value: Expr::Literal {
                    value: Literal::Int(1),
                    span: Span::dummy(),
                },
                span: Span::dummy(),
            },
            Stmt::Let {
                ident: Ident::new("y".to_string(), Span::dummy()),
                value: Expr::Literal {
                    value: Literal::Int(2),
                    span: Span::dummy(),
                },
                span: Span::dummy(),
            },
        ]);

        let chunk = Compiler::compile(&module).unwrap();
        
        assert_eq!(chunk.constants.len(), 2);
        assert_eq!(chunk.strings.len(), 2);
        assert_eq!(chunk.strings.get(0), Some("x"));
        assert_eq!(chunk.strings.get(1), Some("y"));
    }
}
