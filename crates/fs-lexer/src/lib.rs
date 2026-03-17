//! ForgeScript lexical analyzer

use fs_ast::Span;

/// Token type
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    String(String),
    
    // Identifiers and keywords
    Ident(String),
    Let,
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Eq,
    
    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    
    // Special
    Eof,
}

/// Token with span information
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Lexer error
#[derive(Debug, Clone, PartialEq)]
pub enum LexError {
    UnexpectedChar { ch: char, position: usize },
    UnterminatedString { start: usize },
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexError::UnexpectedChar { ch, position } => {
                write!(f, "Unexpected character '{}' at position {}", ch, position)
            }
            LexError::UnterminatedString { start } => {
                write!(f, "Unterminated string starting at position {}", start)
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Lexer for ForgeScript
pub struct Lexer {
    input: Vec<char>,
    position: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();
            
            if self.is_at_end() {
                break;
            }

            let start = self.position;
            let ch = self.current_char();

            let token = match ch {
                '(' => {
                    self.advance();
                    Token::new(TokenKind::LParen, Span::new(start, self.position))
                }
                ')' => {
                    self.advance();
                    Token::new(TokenKind::RParen, Span::new(start, self.position))
                }
                '{' => {
                    self.advance();
                    Token::new(TokenKind::LBrace, Span::new(start, self.position))
                }
                '}' => {
                    self.advance();
                    Token::new(TokenKind::RBrace, Span::new(start, self.position))
                }
                ',' => {
                    self.advance();
                    Token::new(TokenKind::Comma, Span::new(start, self.position))
                }
                ';' => {
                    self.advance();
                    Token::new(TokenKind::Semicolon, Span::new(start, self.position))
                }
                '=' => {
                    self.advance();
                    Token::new(TokenKind::Eq, Span::new(start, self.position))
                }
                '+' => {
                    self.advance();
                    Token::new(TokenKind::Plus, Span::new(start, self.position))
                }
                '-' => {
                    self.advance();
                    Token::new(TokenKind::Minus, Span::new(start, self.position))
                }
                '*' => {
                    self.advance();
                    Token::new(TokenKind::Star, Span::new(start, self.position))
                }
                '/' => {
                    self.advance();
                    Token::new(TokenKind::Slash, Span::new(start, self.position))
                }
                '"' => self.read_string(start)?,
                _ if ch.is_ascii_digit() => self.read_number(start)?,
                _ if ch.is_alphabetic() || ch == '_' => self.read_ident_or_keyword(start)?,
                _ => {
                    return Err(LexError::UnexpectedChar {
                        ch,
                        position: start,
                    });
                }
            };

            tokens.push(token);
        }

        tokens.push(Token::new(TokenKind::Eof, Span::new(self.position, self.position)));
        Ok(tokens)
    }

    fn current_char(&self) -> char {
        self.input[self.position]
    }

    fn advance(&mut self) -> char {
        let ch = self.current_char();
        self.position += 1;
        ch
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() && self.current_char().is_whitespace() {
            self.advance();
        }
    }

    fn read_number(&mut self, start: usize) -> Result<Token, LexError> {
        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            self.advance();
        }

        let text: String = self.input[start..self.position].iter().collect();
        let value = text.parse::<i64>().unwrap();

        Ok(Token::new(TokenKind::Int(value), Span::new(start, self.position)))
    }

    fn read_string(&mut self, start: usize) -> Result<Token, LexError> {
        self.advance(); // Skip opening quote

        let string_start = self.position;

        while !self.is_at_end() && self.current_char() != '"' {
            self.advance();
        }

        if self.is_at_end() {
            return Err(LexError::UnterminatedString { start });
        }

        let text: String = self.input[string_start..self.position].iter().collect();
        self.advance(); // Skip closing quote

        Ok(Token::new(TokenKind::String(text), Span::new(start, self.position)))
    }

    fn read_ident_or_keyword(&mut self, start: usize) -> Result<Token, LexError> {
        while !self.is_at_end() {
            let ch = self.current_char();
            if ch.is_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let text: String = self.input[start..self.position].iter().collect();
        
        let kind = match text.as_str() {
            "let" => TokenKind::Let,
            _ => TokenKind::Ident(text),
        };

        Ok(Token::new(kind, Span::new(start, self.position)))
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
    fn test_empty_input() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_single_tokens() {
        let mut lexer = Lexer::new("(){}+");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::LParen);
        assert_eq!(tokens[1].kind, TokenKind::RParen);
        assert_eq!(tokens[2].kind, TokenKind::LBrace);
        assert_eq!(tokens[3].kind, TokenKind::RBrace);
        assert_eq!(tokens[4].kind, TokenKind::Plus);
        assert_eq!(tokens[5].kind, TokenKind::Eof);
    }

    #[test]
    fn test_integer() {
        let mut lexer = Lexer::new("123");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Int(123));
        assert_eq!(tokens[0].span, Span::new(0, 3));
    }

    #[test]
    fn test_string() {
        let mut lexer = Lexer::new(r#""hello""#);
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::String("hello".to_string()));
    }

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new(r#""hello"#);
        let result = lexer.tokenize();
        
        assert!(result.is_err());
        match result {
            Err(LexError::UnterminatedString { start }) => assert_eq!(start, 0),
            _ => panic!("Expected UnterminatedString error"),
        }
    }

    #[test]
    fn test_identifier() {
        let mut lexer = Lexer::new("foo bar_baz");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Ident("foo".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Ident("bar_baz".to_string()));
    }

    #[test]
    fn test_let_keyword() {
        let mut lexer = Lexer::new("let");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Let);
    }

    #[test]
    fn test_unexpected_char() {
        let mut lexer = Lexer::new("@");
        let result = lexer.tokenize();
        
        assert!(result.is_err());
        match result {
            Err(LexError::UnexpectedChar { ch, position }) => {
                assert_eq!(ch, '@');
                assert_eq!(position, 0);
            }
            _ => panic!("Expected UnexpectedChar error"),
        }
    }

    #[test]
    fn test_complete_statement() {
        let mut lexer = Lexer::new("let x = 1");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].kind, TokenKind::Let);
        assert_eq!(tokens[1].kind, TokenKind::Ident("x".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::Eq);
        assert_eq!(tokens[3].kind, TokenKind::Int(1));
        assert_eq!(tokens[4].kind, TokenKind::Eof);
    }

    #[test]
    fn test_token_spans() {
        let mut lexer = Lexer::new("let x = 123");
        let tokens = lexer.tokenize().unwrap();
        
        assert_eq!(tokens[0].span, Span::new(0, 3));   // "let"
        assert_eq!(tokens[1].span, Span::new(4, 5));   // "x"
        assert_eq!(tokens[2].span, Span::new(6, 7));   // "="
        assert_eq!(tokens[3].span, Span::new(8, 11));  // "123"
    }
}
