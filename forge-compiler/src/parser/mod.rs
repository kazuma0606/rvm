// forge-compiler: Parser
// Phase 1-C で実装する
// 仕様: forge/spec_v0.0.1.md §3〜§9

use crate::lexer::{Token, TokenKind, Span};
use crate::ast::*;

/// Parser エラー
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedToken { expected: String, found: TokenKind, span: Span },
    UnexpectedEof { expected: String },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken { expected, found, span } => {
                write!(f, "構文エラー: {} を期待しましたが {:?} が見つかりました ({}:{})",
                    expected, found, span.line, span.col)
            }
            ParseError::UnexpectedEof { expected } => {
                write!(f, "構文エラー: {} を期待しましたがファイルが終了しました", expected)
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parser 本体（Phase 1-C で実装）
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Module, ParseError> {
        // Phase 1-C で実装
        Ok(Module { stmts: vec![] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::TokenKind;

    #[test]
    fn test_parser_stub_compiles() {
        let tokens = vec![Token {
            kind: TokenKind::Eof,
            span: Span { start: 0, end: 0, line: 1, col: 1 },
        }];
        let mut parser = Parser::new(tokens);
        let module = parser.parse().unwrap();
        assert_eq!(module.stmts.len(), 0);
    }
}
