//! ForgeScript AST definitions

/// Source code span information for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "Span start must not exceed end");
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0 }
    }
}

/// A module represents a single ForgeScript source file
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub statements: Vec<Stmt>,
}

impl Module {
    pub fn new(statements: Vec<Stmt>) -> Self {
        Self { statements }
    }
}

/// Statement
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let { ident: Ident, value: Expr, span: Span },
    Expr { expr: Expr, span: Span },
}

/// Expression
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal { value: Literal, span: Span },
    Ident { ident: Ident, span: Span },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr>, span: Span },
    Call { callee: Box<Expr>, args: Vec<Expr>, span: Span },
}

/// Binary operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Literal value
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    String(String),
}

/// Identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: String, span: Span) -> Self {
        debug_assert!(!name.is_empty(), "Identifier name must not be empty");
        Self { name, span }
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
    fn test_span_creation() {
        let span = Span::new(0, 5);
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 5);
    }

    #[test]
    #[should_panic(expected = "Span start must not exceed end")]
    fn test_span_invalid_range() {
        Span::new(10, 5);
    }

    #[test]
    fn test_ident_creation() {
        let ident = Ident::new("x".to_string(), Span::new(0, 1));
        assert_eq!(ident.name, "x");
        assert_eq!(ident.span, Span::new(0, 1));
    }

    #[test]
    #[should_panic(expected = "Identifier name must not be empty")]
    fn test_ident_empty_name() {
        Ident::new("".to_string(), Span::dummy());
    }

    #[test]
    fn test_module_creation() {
        let module = Module::new(vec![]);
        assert_eq!(module.statements.len(), 0);
    }

    #[test]
    fn test_let_statement() {
        let stmt = Stmt::Let {
            ident: Ident::new("x".to_string(), Span::new(4, 5)),
            value: Expr::Literal {
                value: Literal::Int(42),
                span: Span::new(8, 10),
            },
            span: Span::new(0, 10),
        };
        
        match stmt {
            Stmt::Let { ident, .. } => assert_eq!(ident.name, "x"),
            _ => panic!("Expected Let statement"),
        }
    }

    #[test]
    fn test_binary_expr() {
        let left = Box::new(Expr::Literal {
            value: Literal::Int(1),
            span: Span::new(0, 1),
        });
        let right = Box::new(Expr::Literal {
            value: Literal::Int(2),
            span: Span::new(4, 5),
        });
        
        let expr = Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
            span: Span::new(0, 5),
        };
        
        match expr {
            Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Add),
            _ => panic!("Expected Binary expression"),
        }
    }

    #[test]
    fn test_call_expr() {
        let callee = Box::new(Expr::Ident {
            ident: Ident::new("print".to_string(), Span::new(0, 5)),
            span: Span::new(0, 5),
        });
        
        let args = vec![Expr::Literal {
            value: Literal::Int(42),
            span: Span::new(6, 8),
        }];
        
        let expr = Expr::Call {
            callee,
            args,
            span: Span::new(0, 9),
        };
        
        match expr {
            Expr::Call { args, .. } => assert_eq!(args.len(), 1),
            _ => panic!("Expected Call expression"),
        }
    }
}
