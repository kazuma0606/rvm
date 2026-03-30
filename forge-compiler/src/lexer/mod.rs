// forge-compiler: Lexer
// Phase 1-A で実装する
// 仕様: forge/spec_v0.0.1.md §1

pub mod tokens;

pub use tokens::{Token, TokenKind, Span};

/// Lexer エラー
#[derive(Debug, Clone, PartialEq)]
pub enum LexError {
    UnexpectedChar { ch: char, line: usize, col: usize },
    UnterminatedString { line: usize, col: usize },
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexError::UnexpectedChar { ch, line, col } => {
                write!(f, "予期しない文字 '{}' ({}:{})", ch, line, col)
            }
            LexError::UnterminatedString { line, col } => {
                write!(f, "文字列が閉じられていません ({}:{})", line, col)
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Lexer 本体（Phase 1-A で実装）
pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        // Phase 1-A で実装
        Ok(vec![Token {
            kind: TokenKind::Eof,
            span: Span { start: 0, end: 0, line: 1, col: 1 },
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_stub_compiles() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }
}
