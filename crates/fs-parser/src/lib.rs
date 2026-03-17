//! ForgeScript parser

use fs_ast::*;
use fs_lexer::{Lexer, LexError, Token, TokenKind};

/// Parser error
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    LexError(LexError),
    UnexpectedToken { expected: String, found: TokenKind, span: Span },
    UnexpectedEof { expected: String },
    UnclosedDelimiter { delimiter: char, start: Span },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::LexError(e) => write!(f, "Lexer error: {}", e),
            ParseError::UnexpectedToken { expected, found, span } => {
                write!(f, "Expected {} but found {:?} at {:?}", expected, found, span)
            }
            ParseError::UnexpectedEof { expected } => {
                write!(f, "Unexpected end of file, expected {}", expected)
            }
            ParseError::UnclosedDelimiter { delimiter, start } => {
                write!(f, "Unclosed delimiter '{}' starting at {:?}", delimiter, start)
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError::LexError(e)
    }
}

/// Parser for ForgeScript
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub fn parse(input: &str) -> Result<Module, ParseError> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens);
        parser.parse_module()
    }

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
        }

        Ok(Module::new(statements))
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        if self.check(&TokenKind::Let) {
            self.parse_let_statement()
        } else {
            self.parse_expr_statement()
        }
    }

    fn parse_let_statement(&mut self) -> Result<Stmt, ParseError> {
        let start_span = self.current().span;
        self.expect(&TokenKind::Let, "let keyword")?;

        let ident = match &self.current().kind {
            TokenKind::Ident(name) => {
                let span = self.current().span;
                let ident = Ident::new(name.clone(), span);
                self.advance();
                ident
            }
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "identifier".to_string(),
                    found: self.current().kind.clone(),
                    span: self.current().span,
                });
            }
        };

        self.expect(&TokenKind::Eq, "=")?;

        let value = self.parse_expression()?;
        let end_span = self.previous().span;

        Ok(Stmt::Let {
            ident,
            value,
            span: Span::new(start_span.start, end_span.end),
        })
    }

    fn parse_expr_statement(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expression()?;
        let span = match &expr {
            Expr::Literal { span, .. } => *span,
            Expr::Ident { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
        };

        Ok(Stmt::Expr { expr, span })
    }

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;

        while self.check(&TokenKind::Plus) || self.check(&TokenKind::Minus) {
            let op = if self.check(&TokenKind::Plus) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            self.advance();

            let right = self.parse_multiplicative()?;
            let left_span = Self::expr_span(&left);
            let right_span = Self::expr_span(&right);

            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(left_span.start, right_span.end),
            };
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_call()?;

        while self.check(&TokenKind::Star) || self.check(&TokenKind::Slash) {
            let op = if self.check(&TokenKind::Star) {
                BinaryOp::Mul
            } else {
                BinaryOp::Div
            };
            self.advance();

            let right = self.parse_call()?;
            let left_span = Self::expr_span(&left);
            let right_span = Self::expr_span(&right);

            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span: Span::new(left_span.start, right_span.end),
            };
        }

        Ok(left)
    }

    fn parse_call(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        while self.check(&TokenKind::LParen) {
            let lparen_span = self.current().span;
            self.advance();

            let mut args = Vec::new();

            if !self.check(&TokenKind::RParen) {
                loop {
                    args.push(self.parse_expression()?);

                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance();
                }
            }

            if !self.check(&TokenKind::RParen) {
                return Err(ParseError::UnclosedDelimiter {
                    delimiter: '(',
                    start: lparen_span,
                });
            }

            let rparen_span = self.current().span;
            self.advance();

            let callee_span = Self::expr_span(&expr);
            expr = Expr::Call {
                callee: Box::new(expr),
                args,
                span: Span::new(callee_span.start, rparen_span.end),
            };
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.current();

        match &token.kind {
            TokenKind::Int(n) => {
                let span = token.span;
                let value = *n;
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::Int(value),
                    span,
                })
            }
            TokenKind::String(s) => {
                let span = token.span;
                let value = s.clone();
                self.advance();
                Ok(Expr::Literal {
                    value: Literal::String(value),
                    span,
                })
            }
            TokenKind::Ident(name) => {
                let span = token.span;
                let name = name.clone();
                self.advance();
                Ok(Expr::Ident {
                    ident: Ident::new(name, span),
                    span,
                })
            }
            TokenKind::LParen => {
                let lparen_span = token.span;
                self.advance();
                let expr = self.parse_expression()?;

                if !self.check(&TokenKind::RParen) {
                    return Err(ParseError::UnclosedDelimiter {
                        delimiter: '(',
                        start: lparen_span,
                    });
                }

                self.advance();
                Ok(expr)
            }
            TokenKind::Eof => Err(ParseError::UnexpectedEof {
                expected: "expression".to_string(),
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "expression".to_string(),
                found: token.kind.clone(),
                span: token.span,
            }),
        }
    }

    fn expr_span(expr: &Expr) -> Span {
        match expr {
            Expr::Literal { span, .. } => *span,
            Expr::Ident { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.position - 1]
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.position += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn expect(&mut self, kind: &TokenKind, desc: &str) -> Result<(), ParseError> {
        if !self.check(kind) {
            return Err(ParseError::UnexpectedToken {
                expected: desc.to_string(),
                found: self.current().kind.clone(),
                span: self.current().span,
            });
        }
        self.advance();
        Ok(())
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
    fn test_parse_let_statement() {
        let module = Parser::parse("let x = 1").unwrap();
        assert_eq!(module.statements.len(), 1);

        match &module.statements[0] {
            Stmt::Let { ident, value, .. } => {
                assert_eq!(ident.name, "x");
                match value {
                    Expr::Literal { value: Literal::Int(1), .. } => {}
                    _ => panic!("Expected integer literal"),
                }
            }
            _ => panic!("Expected let statement"),
        }
    }

    #[test]
    fn test_parse_addition() {
        let module = Parser::parse("1 + 2").unwrap();
        assert_eq!(module.statements.len(), 1);

        match &module.statements[0] {
            Stmt::Expr { expr, .. } => match expr {
                Expr::Binary { op, left, right, .. } => {
                    assert_eq!(*op, BinaryOp::Add);
                    match **left {
                        Expr::Literal { value: Literal::Int(1), .. } => {}
                        _ => panic!("Expected 1"),
                    }
                    match **right {
                        Expr::Literal { value: Literal::Int(2), .. } => {}
                        _ => panic!("Expected 2"),
                    }
                }
                _ => panic!("Expected binary expression"),
            },
            _ => panic!("Expected expression statement"),
        }
    }

    #[test]
    fn test_operator_precedence() {
        let module = Parser::parse("1 + 2 * 3").unwrap();

        match &module.statements[0] {
            Stmt::Expr { expr, .. } => match expr {
                Expr::Binary { op: BinaryOp::Add, left, right, .. } => {
                    match **left {
                        Expr::Literal { value: Literal::Int(1), .. } => {}
                        _ => panic!("Expected 1"),
                    }
                    match **right {
                        Expr::Binary { op: BinaryOp::Mul, .. } => {}
                        _ => panic!("Expected multiplication on right side"),
                    }
                }
                _ => panic!("Expected addition at top level"),
            },
            _ => panic!("Expected expression statement"),
        }
    }

    #[test]
    fn test_parse_call() {
        let module = Parser::parse("print(42)").unwrap();

        match &module.statements[0] {
            Stmt::Expr { expr, .. } => match expr {
                Expr::Call { callee, args, .. } => {
                    match &**callee {
                        Expr::Ident { ident, .. } => assert_eq!(ident.name, "print"),
                        _ => panic!("Expected identifier"),
                    }
                    assert_eq!(args.len(), 1);
                }
                _ => panic!("Expected call expression"),
            },
            _ => panic!("Expected expression statement"),
        }
    }

    #[test]
    fn test_parse_call_with_expr() {
        let module = Parser::parse("print(x + 1)").unwrap();

        match &module.statements[0] {
            Stmt::Expr { expr, .. } => match expr {
                Expr::Call { args, .. } => {
                    assert_eq!(args.len(), 1);
                    match &args[0] {
                        Expr::Binary { op: BinaryOp::Add, .. } => {}
                        _ => panic!("Expected addition in argument"),
                    }
                }
                _ => panic!("Expected call expression"),
            },
            _ => panic!("Expected expression statement"),
        }
    }

    #[test]
    fn test_unexpected_token() {
        let result = Parser::parse("let = 1");
        assert!(result.is_err());

        match result {
            Err(ParseError::UnexpectedToken { expected, .. }) => {
                assert_eq!(expected, "identifier");
            }
            _ => panic!("Expected UnexpectedToken error"),
        }
    }

    #[test]
    fn test_unclosed_paren() {
        let result = Parser::parse("print(42");
        assert!(result.is_err());

        match result {
            Err(ParseError::UnclosedDelimiter { delimiter, .. }) => {
                assert_eq!(delimiter, '(');
            }
            _ => panic!("Expected UnclosedDelimiter error"),
        }
    }

    #[test]
    fn test_unexpected_eof() {
        let result = Parser::parse("let x =");
        assert!(result.is_err());

        match result {
            Err(ParseError::UnexpectedEof { expected }) => {
                assert_eq!(expected, "expression");
            }
            _ => panic!("Expected UnexpectedEof error"),
        }
    }

    #[test]
    fn test_lexer_parser_integration() {
        let module = Parser::parse("let x = 1\nlet y = 2\nprint(x + y)").unwrap();
        assert_eq!(module.statements.len(), 3);

        match &module.statements[0] {
            Stmt::Let { ident, .. } => assert_eq!(ident.name, "x"),
            _ => panic!("Expected let statement"),
        }

        match &module.statements[1] {
            Stmt::Let { ident, .. } => assert_eq!(ident.name, "y"),
            _ => panic!("Expected let statement"),
        }

        match &module.statements[2] {
            Stmt::Expr { expr, .. } => match expr {
                Expr::Call { .. } => {}
                _ => panic!("Expected call expression"),
            },
            _ => panic!("Expected expression statement"),
        }
    }
}
