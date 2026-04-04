// forge-compiler: Lexer
// Phase 1-A 実装
// 仕様: forge/spec_v0.0.1.md §1

pub mod tokens;

pub use tokens::{Span, StrPart, Token, TokenKind};

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

/// Lexer 本体
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

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn make_span(&self, start: usize, start_line: usize, start_col: usize) -> Span {
        Span {
            start,
            end: self.pos,
            line: start_line,
            col: start_col,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn read_string(
        &mut self,
        start: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Token, LexError> {
        // opening `"` already consumed
        let mut parts: Vec<StrPart> = Vec::new();
        let mut current_literal = String::new();
        let mut has_interp = false;

        loop {
            match self.peek() {
                None | Some('\n') => {
                    return Err(LexError::UnterminatedString {
                        line: start_line,
                        col: start_col,
                    });
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.advance() {
                        Some('n') => current_literal.push('\n'),
                        Some('t') => current_literal.push('\t'),
                        Some('r') => current_literal.push('\r'),
                        Some('"') => current_literal.push('"'),
                        Some('\\') => current_literal.push('\\'),
                        Some('{') => current_literal.push('{'),
                        Some(c) => {
                            current_literal.push('\\');
                            current_literal.push(c);
                        }
                        None => {
                            return Err(LexError::UnterminatedString {
                                line: start_line,
                                col: start_col,
                            });
                        }
                    }
                }
                Some('{') => {
                    has_interp = true;
                    if !current_literal.is_empty() {
                        parts.push(StrPart::Literal(std::mem::take(&mut current_literal)));
                    }
                    self.advance(); // consume '{'
                    let mut expr_src = String::new();
                    let mut depth = 1usize;
                    loop {
                        match self.peek() {
                            None => {
                                return Err(LexError::UnterminatedString {
                                    line: start_line,
                                    col: start_col,
                                });
                            }
                            Some('{') => {
                                depth += 1;
                                expr_src.push('{');
                                self.advance();
                            }
                            Some('}') => {
                                depth -= 1;
                                if depth == 0 {
                                    self.advance();
                                    break;
                                } else {
                                    expr_src.push('}');
                                    self.advance();
                                }
                            }
                            Some(c) => {
                                expr_src.push(c);
                                self.advance();
                            }
                        }
                    }
                    parts.push(StrPart::Expr(expr_src));
                }
                Some(c) => {
                    current_literal.push(c);
                    self.advance();
                }
            }
        }

        let span = self.make_span(start, start_line, start_col);
        if has_interp {
            if !current_literal.is_empty() {
                parts.push(StrPart::Literal(current_literal));
            }
            Ok(Token {
                kind: TokenKind::StrInterp(parts),
                span,
            })
        } else {
            Ok(Token {
                kind: TokenKind::Str(current_literal),
                span,
            })
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();

            let start = self.pos;
            let start_line = self.line;
            let start_col = self.col;

            match self.peek() {
                None => {
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        span: self.make_span(start, start_line, start_col),
                    });
                    break;
                }
                Some('/') if self.peek_next() == Some('/') => {
                    self.advance();
                    self.advance();
                    self.skip_line_comment();
                }
                Some('"') => {
                    self.advance();
                    tokens.push(self.read_string(start, start_line, start_col)?);
                }
                Some(ch) if ch.is_ascii_digit() => {
                    let mut raw = String::new();
                    raw.push(ch);
                    self.advance();
                    // peek for more digits / float
                    let mut is_float = false;
                    loop {
                        match self.peek() {
                            Some(c) if c.is_ascii_digit() => {
                                raw.push(c);
                                self.advance();
                            }
                            Some('_') => {
                                self.advance();
                            }
                            Some('.')
                                if !is_float
                                    && self
                                        .peek_next()
                                        .map(|c| c.is_ascii_digit())
                                        .unwrap_or(false) =>
                            {
                                is_float = true;
                                self.advance();
                                raw.push('.');
                            }
                            Some('e') | Some('E') if is_float => {
                                let e = self.peek().unwrap();
                                self.advance();
                                raw.push(e);
                                if let Some(sign) = self.peek() {
                                    if sign == '+' || sign == '-' {
                                        raw.push(sign);
                                        self.advance();
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    let span = self.make_span(start, start_line, start_col);
                    if is_float {
                        let v = raw.parse::<f64>().map_err(|_| LexError::UnexpectedChar {
                            ch: '.',
                            line: start_line,
                            col: start_col,
                        })?;
                        tokens.push(Token {
                            kind: TokenKind::Float(v),
                            span,
                        });
                    } else {
                        let v = raw.parse::<i64>().map_err(|_| LexError::UnexpectedChar {
                            ch: '0',
                            line: start_line,
                            col: start_col,
                        })?;
                        tokens.push(Token {
                            kind: TokenKind::Int(v),
                            span,
                        });
                    }
                }
                Some(ch) if ch.is_alphabetic() || ch == '_' => {
                    self.advance();
                    let mut ident = String::new();
                    ident.push(ch);
                    while let Some(c) = self.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            ident.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let span = self.make_span(start, start_line, start_col);
                    let kind = match ident.as_str() {
                        "let" => TokenKind::Let,
                        "state" => TokenKind::State,
                        "const" => TokenKind::Const,
                        "fn" => TokenKind::Fn,
                        "return" => TokenKind::Return,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "for" => TokenKind::For,
                        "in" => TokenKind::In,
                        "while" => TokenKind::While,
                        "match" => TokenKind::Match,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "none" => TokenKind::None,
                        "some" => TokenKind::Some,
                        "ok" => TokenKind::Ok,
                        "err" => TokenKind::Err,
                        "struct" => TokenKind::Struct,
                        "impl" => TokenKind::Impl,
                        "self" => TokenKind::SelfVal,
                        "Self" => TokenKind::SelfType,
                        "trait" => TokenKind::Trait,
                        "mixin" => TokenKind::Mixin,
                        "data" => TokenKind::Data,
                        "typestate" => TokenKind::Typestate,
                        "use" => TokenKind::Use,
                        "pub" => TokenKind::Pub,
                        "as" => TokenKind::As,
                        "when" => TokenKind::When,
                        "test" => TokenKind::Test,
                        _ => TokenKind::Ident(ident),
                    };
                    tokens.push(Token { kind, span });
                }
                Some('+') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Plus,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('-') => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::ThinArrow,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Minus,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('*') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('/') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Slash,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('%') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Percent,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('=') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::EqEq,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::Arrow,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Eq,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('!') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::BangEq,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Bang,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('<') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::LtEq,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Lt,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('>') => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::GtEq,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Gt,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('&') => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::And,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        return Err(LexError::UnexpectedChar {
                            ch: '&',
                            line: start_line,
                            col: start_col,
                        });
                    }
                }
                Some('|') => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::Or,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        return Err(LexError::UnexpectedChar {
                            ch: '|',
                            line: start_line,
                            col: start_col,
                        });
                    }
                }
                Some(':') => {
                    self.advance();
                    if self.peek() == Some(':') {
                        self.advance();
                        tokens.push(Token {
                            kind: TokenKind::ColonColon,
                            span: self.make_span(start, start_line, start_col),
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Colon,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('?') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Question,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('.') => {
                    self.advance();
                    if self.peek() == Some('.') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            tokens.push(Token {
                                kind: TokenKind::DotDotEq,
                                span: self.make_span(start, start_line, start_col),
                            });
                        } else {
                            tokens.push(Token {
                                kind: TokenKind::DotDot,
                                span: self.make_span(start, start_line, start_col),
                            });
                        }
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Dot,
                            span: self.make_span(start, start_line, start_col),
                        });
                    }
                }
                Some('(') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LParen,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some(')') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RParen,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('{') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LBrace,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('}') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RBrace,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('[') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LBracket,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some(']') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RBracket,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some(',') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some(';') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some('@') => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::At,
                        span: self.make_span(start, start_line, start_col),
                    });
                }
                Some(ch) => {
                    let c = ch;
                    self.advance();
                    return Err(LexError::UnexpectedChar {
                        ch: c,
                        line: start_line,
                        col: start_col,
                    });
                }
            }
        }

        Ok(tokens)
    }
}

/// 便利関数: ソース文字列からトークン列（Eof 含む）を返す
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}

/// テスト用ヘルパー: トークン種別のみ抽出
pub fn lex_kinds(source: &str) -> Result<Vec<TokenKind>, LexError> {
    lex(source).map(|tokens| tokens.into_iter().map(|t| t.kind).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokens::StrPart;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex_kinds(src).expect("lex failed")
    }

    #[test]
    fn test_lexer_stub_compiles() {
        let mut lexer = Lexer::new("");
        let tokens = lexer.tokenize().expect("tokenize failed");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    // ── Phase 1-A tests ───────────────────────────────────────────────────

    #[test]
    fn test_lex_integer() {
        assert_eq!(kinds("42"), vec![TokenKind::Int(42), TokenKind::Eof]);
    }

    #[test]
    fn test_lex_float() {
        assert_eq!(kinds("3.14"), vec![TokenKind::Float(3.14), TokenKind::Eof]);
    }

    #[test]
    fn test_lex_bool() {
        assert_eq!(kinds("true"), vec![TokenKind::True, TokenKind::Eof]);
        assert_eq!(kinds("false"), vec![TokenKind::False, TokenKind::Eof]);
    }

    #[test]
    fn test_lex_string() {
        assert_eq!(
            kinds(r#""hello""#),
            vec![TokenKind::Str("hello".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn test_lex_keywords() {
        let src = "let state const fn return if else for in while match";
        let got = kinds(src);
        let expected = vec![
            TokenKind::Let,
            TokenKind::State,
            TokenKind::Const,
            TokenKind::Fn,
            TokenKind::Return,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::For,
            TokenKind::In,
            TokenKind::While,
            TokenKind::Match,
            TokenKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_lex_operators() {
        let src = "+ - * / % == != < > <= >= && || !";
        let got = kinds(src);
        let expected = vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Bang,
            TokenKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_lex_symbols() {
        let src = "=> -> ? : . .. ..= [ ]";
        let got = kinds(src);
        let expected = vec![
            TokenKind::Arrow,
            TokenKind::ThinArrow,
            TokenKind::Question,
            TokenKind::Colon,
            TokenKind::Dot,
            TokenKind::DotDot,
            TokenKind::DotDotEq,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_lex_comment() {
        let src = "// comment\nlet x = 1";
        let got = kinds(src);
        let expected = vec![
            TokenKind::Let,
            TokenKind::Ident("x".to_string()),
            TokenKind::Eq,
            TokenKind::Int(1),
            TokenKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_lex_string_interpolation() {
        let src = r#""Hello, {name}!""#;
        let tokens = lex(src).expect("lex failed");
        assert_eq!(tokens.len(), 2);
        match &tokens[0].kind {
            TokenKind::StrInterp(parts) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(parts[0], StrPart::Literal("Hello, ".to_string()));
                assert_eq!(parts[1], StrPart::Expr("name".to_string()));
                assert_eq!(parts[2], StrPart::Literal("!".to_string()));
            }
            other => panic!("expected StrInterp, got {:?}", other),
        }
        assert_eq!(tokens[1].kind, TokenKind::Eof);
    }

    #[test]
    fn test_lex_unknown_char() {
        // @ は現在 At トークンとして認識されるので、別の未知文字でテスト
        let result = lex("#");
        assert!(matches!(
            result,
            Err(LexError::UnexpectedChar { ch: '#', .. })
        ));
    }

    #[test]
    fn test_lex_at_token() {
        let kinds = kinds("@");
        assert_eq!(kinds, vec![TokenKind::At, TokenKind::Eof]);
    }

    #[test]
    fn test_lex_struct_keywords() {
        let src = "struct impl self Self trait mixin data typestate";
        let got = kinds(src);
        let expected = vec![
            TokenKind::Struct,
            TokenKind::Impl,
            TokenKind::SelfVal,
            TokenKind::SelfType,
            TokenKind::Trait,
            TokenKind::Mixin,
            TokenKind::Data,
            TokenKind::Typestate,
            TokenKind::Eof,
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn test_lex_colon_colon() {
        let got = kinds("Foo::bar");
        assert_eq!(
            got,
            vec![
                TokenKind::Ident("Foo".to_string()),
                TokenKind::ColonColon,
                TokenKind::Ident("bar".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_unterminated_string() {
        let result = lex(r#""hello"#);
        assert!(matches!(result, Err(LexError::UnterminatedString { .. })));
    }

    #[test]
    fn test_lex_span() {
        let tokens = lex("42").expect("lex failed");
        let tok = &tokens[0];
        assert_eq!(tok.span.start, 0);
        assert_eq!(tok.span.end, 2);
        assert_eq!(tok.span.line, 1);
        assert_eq!(tok.span.col, 1);
    }
}
