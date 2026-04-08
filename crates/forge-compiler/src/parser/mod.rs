// forge-compiler: Parser
// Phase 1-C 実装
// 仕様: forge/spec_v0.0.1.md §3〜§9

use crate::ast::*;
use crate::lexer::{lex, Span, StrPart, Token, TokenKind};

// ── エラー ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedToken {
        expected: String,
        found: TokenKind,
        span: Span,
    },
    UnexpectedEof {
        expected: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken {
                expected,
                found,
                span,
            } => write!(
                f,
                "構文エラー: {} を期待しましたが {:?} が見つかりました ({}:{})",
                expected, found, span.line, span.col
            ),
            ParseError::UnexpectedEof { expected } => write!(
                f,
                "構文エラー: {} を期待しましたがファイルが終了しました",
                expected
            ),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BraceExprKind {
    Block,
    EmptyMap,
    Map,
    Set,
}

fn is_block_start_token(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Let
            | TokenKind::State
            | TokenKind::Const
            | TokenKind::Fn
            | TokenKind::Return
            | TokenKind::Struct
            | TokenKind::Trait
            | TokenKind::Impl
            | TokenKind::Typestate
            | TokenKind::Pub
            | TokenKind::Use
            | TokenKind::When
            | TokenKind::Test
            | TokenKind::At
    ) || matches!(kind, TokenKind::Ident(name) if name == "enum")
}

// ── パーサー本体 ────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ── ヘルパー ──────────────────────────────────────────────────────────

    fn peek(&self) -> &Token {
        // EOF sentinel が必ず末尾にある前提
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("empty token stream"))
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn kind_at(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn current_span(&self) -> Span {
        self.peek().span.clone()
    }

    fn skip_sep(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Semicolon) {
            self.advance();
        }
    }

    /// 指定トークンと完全一致を期待（ペイロードなしトークン用）
    fn expect_token(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        let tok = self.peek().clone();
        if &tok.kind == kind {
            Ok(self.advance())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: format!("{:?}", kind),
                found: tok.kind,
                span: tok.span,
            })
        }
    }

    /// 識別子トークンを期待
    fn expect_ident(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Ident(name) => {
                self.advance();
                Ok((name, tok.span))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    /// ドット後のフィールド/メソッド名（ident または一部キーワードも許容）
    fn expect_name(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.peek().clone();
        let name = match &tok.kind {
            TokenKind::Ident(n) => n.clone(),
            TokenKind::Some => "some".to_string(),
            TokenKind::None => "none".to_string(),
            TokenKind::Ok => "ok".to_string(),
            TokenKind::Err => "err".to_string(),
            TokenKind::SelfVal => "self".to_string(),
            TokenKind::Use => "use".to_string(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "identifier".to_string(),
                    found: tok.kind.clone(),
                    span: tok.span.clone(),
                });
            }
        };
        let span = tok.span.clone();
        self.advance();
        Ok((name, span))
    }

    /// `(idents*) =>` または `() =>` パターンの先読み判定
    fn is_closure_parens_start(&self) -> bool {
        // pos にいるのは `(`
        let mut i = 1usize; // pos からのオフセット
                            // () => の場合
        if matches!(self.kind_at(i), Some(TokenKind::RParen)) {
            return matches!(self.kind_at(i + 1), Some(TokenKind::Arrow));
        }
        // (ident, ...) => の場合
        loop {
            match self.kind_at(i) {
                Some(TokenKind::Ident(_)) => i += 1,
                _ => return false,
            }
            match self.kind_at(i) {
                Some(TokenKind::Comma) => i += 1,
                Some(TokenKind::RParen) => {
                    return matches!(self.kind_at(i + 1), Some(TokenKind::Arrow));
                }
                _ => return false,
            }
        }
    }

    // ── パース: トップレベル ──────────────────────────────────────────────

    pub fn parse(&mut self) -> Result<Module, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::Eof) {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(Module { stmts })
    }

    // ── パース: 文 ────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        // `enum` は Ident("enum") として届く
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            if name == "enum" {
                self.advance(); // consume 'enum'
                return self.parse_enum_def_body(vec![]);
            }
        }
        match self.peek_kind().clone() {
            TokenKind::Let => self.parse_let(),
            TokenKind::State => self.parse_state(),
            TokenKind::Const => {
                if matches!(self.kind_at(1), Some(TokenKind::Fn)) {
                    self.parse_const_fn()
                } else {
                    self.parse_const()
                }
            }
            TokenKind::Fn => self.parse_fn(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Yield => self.parse_yield(),
            TokenKind::Struct => self.parse_struct_def(vec![]),
            TokenKind::Impl => self.parse_impl_or_impl_trait(),
            TokenKind::Trait => self.parse_trait_def(),
            TokenKind::Mixin => self.parse_mixin_def(),
            TokenKind::At => self.parse_annotated_stmt(),
            TokenKind::Data => self.parse_data_def(),
            TokenKind::Typestate => self.parse_typestate_def(),
            TokenKind::Use => self.parse_use_decl(false),
            TokenKind::Pub => self.parse_pub_stmt(),
            TokenKind::When => self.parse_when_stmt(),
            TokenKind::Test => self.parse_test_block(),
            _ => {
                let expr = self.parse_expr()?;
                self.skip_sep();
                Ok(Stmt::Expr(expr))
            }
        }
    }

    // ── パース: pub 文 ──────────────────────────────────────────────────

    fn parse_pub_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.expect_token(&TokenKind::Pub)?;
        match self.peek_kind().clone() {
            TokenKind::At => self.parse_annotated_stmt_with_pub(true),
            TokenKind::Use => self.parse_use_decl(true),
            TokenKind::Fn => self.parse_fn_with_pub(true),
            TokenKind::Let => self.parse_let_with_pub(true),
            TokenKind::Const => {
                if matches!(self.kind_at(1), Some(TokenKind::Fn)) {
                    self.parse_const_fn_with_pub(true)
                } else {
                    self.parse_const_with_pub(true)
                }
            }
            TokenKind::Struct => self.parse_struct_def_with_pub(vec![], true),
            TokenKind::Trait => self.parse_trait_def_with_pub(true),
            TokenKind::Mixin => self.parse_mixin_def_with_pub(true),
            TokenKind::Data => self.parse_data_def_with_pub(true),
            TokenKind::Ident(ref name) if name == "enum" => {
                self.advance(); // consume 'enum'
                self.parse_enum_def_body_with_pub(vec![], true)
            }
            _ => {
                let tok = self.peek().clone();
                Err(ParseError::UnexpectedToken {
                    expected: "use, fn, let, const, struct, enum, data, trait, または mixin"
                        .to_string(),
                    found: tok.kind,
                    span: tok.span,
                })
            }
        }
    }

    fn parse_annotated_stmt_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let mut derives = Vec::new();
        while matches!(self.peek_kind(), TokenKind::At) {
            self.advance();
            let (ann_name, ann_span) = self.expect_ident()?;
            if ann_name != "derive" {
                return Err(ParseError::UnexpectedToken {
                    expected: "@derive".to_string(),
                    found: TokenKind::Ident(ann_name),
                    span: ann_span,
                });
            }
            self.expect_token(&TokenKind::LParen)?;
            loop {
                let (name, _) = self.expect_ident()?;
                derives.push(name);
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
            self.expect_token(&TokenKind::RParen)?;
            self.skip_sep();
        }

        match self.peek_kind().clone() {
            TokenKind::Struct => self.parse_struct_def_with_pub(derives, is_pub),
            TokenKind::Ident(ref name) if name == "enum" => {
                self.advance();
                self.parse_enum_def_body_with_pub(derives, is_pub)
            }
            TokenKind::Typestate => self.parse_typestate_def_with_meta(derives),
            _ => {
                let tok = self.peek().clone();
                Err(ParseError::UnexpectedToken {
                    expected: "struct or enum or typestate after annotation".to_string(),
                    found: tok.kind,
                    span: tok.span,
                })
            }
        }
    }

    // ── パース: when 文 ─────────────────────────────────────────────────

    /// `when platform.linux { ... }` / `when feature.debug { ... }` / `when test { ... }`
    fn parse_when_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::When)?;

        let condition = self.parse_when_condition()?;

        // ボディブロック { stmt... } をパース
        self.expect_token(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            body.push(self.parse_stmt()?);
            self.skip_sep();
        }
        self.expect_token(&TokenKind::RBrace)?;
        self.skip_sep();

        Ok(Stmt::When {
            condition,
            body,
            span,
        })
    }

    /// `test "テスト名" { ... }` をパース（FT-1-C）
    fn parse_test_block(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Test)?;

        // テスト名は文字列リテラルまたは補間なし文字列
        let name = match self.peek_kind().clone() {
            TokenKind::Str(s) => {
                self.advance();
                s
            }
            _ => {
                let tok = self.peek().clone();
                return Err(ParseError::UnexpectedToken {
                    expected: "テスト名（文字列リテラル）".to_string(),
                    found: tok.kind,
                    span: tok.span,
                });
            }
        };

        // ボディブロック { stmt... } をパース
        self.expect_token(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            body.push(self.parse_stmt()?);
            self.skip_sep();
        }
        self.expect_token(&TokenKind::RBrace)?;
        self.skip_sep();

        Ok(Stmt::TestBlock { name, body, span })
    }

    /// `when` の後に続く条件をパース
    fn parse_when_condition(&mut self) -> Result<WhenCondition, ParseError> {
        // `not` キーワードのチェック（Ident("not") として届く）
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            if name == "not" {
                self.advance(); // consume 'not'
                let inner = self.parse_when_condition()?;
                return Ok(WhenCondition::Not(Box::new(inner)));
            }
        }

        // `test` キーワード（TokenKind::Test として届く）
        if matches!(self.peek_kind(), TokenKind::Test) {
            self.advance(); // consume 'test'
            return Ok(WhenCondition::Test);
        }

        // `platform` / `feature` / `env` + `.` + name
        let (prefix, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::Dot)?;
        let (name, _) = self.expect_ident()?;

        match prefix.as_str() {
            "platform" => Ok(WhenCondition::Platform(name)),
            "feature" => Ok(WhenCondition::Feature(name)),
            "env" => Ok(WhenCondition::Env(name)),
            _ => {
                let tok = self.peek().clone();
                Err(ParseError::UnexpectedToken {
                    expected: "platform, feature, または env".to_string(),
                    found: TokenKind::Ident(prefix),
                    span: tok.span,
                })
            }
        }
    }

    // ── パース: use 宣言 ────────────────────────────────────────────────

    fn parse_use_decl(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Use)?;

        // `use raw { ... }` のパース（M-6-B）
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            if name == "raw" {
                self.advance(); // consume 'raw'
                return self.parse_use_raw(span);
            }
        }

        // パスを読み取る
        // ローカル: ./utils/helper
        // 外部: serde
        // 標準ライブラリ: forge/std/io
        let path = self.parse_use_path()?;

        // . の後にシンボルを読み取る（オプション: パスのみの場合もある）
        let symbols = if matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance(); // consume '.'
            self.parse_use_symbols()?
        } else {
            // シンボルなし → 外部クレート全体のインポート（符号なし）
            UseSymbols::All
        };

        self.skip_sep();
        Ok(Stmt::UseDecl {
            path,
            symbols,
            is_pub,
            span,
        })
    }

    /// `use raw { ... }` をパース（M-6-B）
    /// `use` と `raw` は消費済みで呼び出されること
    /// `{` から対応する `}` までの内容を生文字列として保持する
    fn parse_use_raw(&mut self, span: Span) -> Result<Stmt, ParseError> {
        self.expect_token(&TokenKind::LBrace)?;

        // ブレースのネストを考慮して { ... } の内容を生文字列として収集する
        // トークン列からソース文字列を再構成するのではなく、
        // トークン位置をそのまま利用してソーステキストの区間を取得する
        let mut rust_code = String::new();
        let mut depth: usize = 1;

        // 行番号とカラムを利用して元のソースを再現するのは難しいため、
        // トークンを stringify して文字列を構築する
        loop {
            let tok = self.peek().clone();
            match &tok.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                    rust_code.push('{');
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        self.advance(); // consume 終了の '}'
                        break;
                    }
                    self.advance();
                    rust_code.push('}');
                }
                TokenKind::Eof => {
                    return Err(ParseError::UnexpectedEof {
                        expected: "use raw ブロックの閉じ括弧 '}'".to_string(),
                    });
                }
                _ => {
                    // トークンを文字列化して追加
                    let tok_str = token_kind_to_raw_str(&tok.kind);
                    rust_code.push_str(&tok_str);
                    self.advance();
                }
            }
        }

        self.skip_sep();
        Ok(Stmt::UseRaw {
            rust_code: rust_code.trim().to_string(),
            span,
        })
    }

    /// `./utils/helper` / `serde` / `forge/std/io` をパース
    /// 返す文字列はプレフィックス除いたパス
    fn parse_use_path(&mut self) -> Result<UsePath, ParseError> {
        let tok = self.peek().clone();

        // `./` で始まるローカルパス
        if matches!(tok.kind, TokenKind::Dot) {
            // ./ かどうか確認
            if matches!(self.kind_at(1), Some(TokenKind::Slash)) {
                self.advance(); // consume '.'
                self.advance(); // consume '/'
                                // パスセグメントを読み取る: utils/helper
                let path = self.read_path_segments()?;
                return Ok(UsePath::Local(path));
            }
        }

        // 識別子（外部クレートまたは forge/std/...）
        let (first_ident, _) = self.expect_ident()?;

        // `forge` で始まる場合は `forge/std/...` の可能性
        if first_ident == "forge" && matches!(self.peek_kind(), TokenKind::Slash) {
            self.advance(); // consume '/'
            let (second, _) = self.expect_ident()?;
            if second == "std" && matches!(self.peek_kind(), TokenKind::Slash) {
                self.advance(); // consume '/'
                let rest = self.read_path_segments()?;
                return Ok(UsePath::Stdlib(format!("forge/std/{}", rest)));
            }
            // forge/something/... → 外部扱い
            if matches!(self.peek_kind(), TokenKind::Slash) {
                self.advance();
                let rest = self.read_path_segments()?;
                return Ok(UsePath::External(format!(
                    "{}/{}/{}",
                    first_ident, second, rest
                )));
            }
            return Ok(UsePath::External(format!("{}/{}", first_ident, second)));
        }

        // それ以外はすべて外部クレート
        // 追加のパスセグメント（reqwest/client など）があれば読む
        let mut path = first_ident;
        while matches!(self.peek_kind(), TokenKind::Slash) {
            self.advance(); // consume '/'
            let (seg, _) = self.expect_ident()?;
            path.push('/');
            path.push_str(&seg);
        }
        Ok(UsePath::External(path))
    }

    /// `/` 区切りのパスセグメントを読み取る（`utils/helper` など）
    fn read_path_segments(&mut self) -> Result<String, ParseError> {
        let (first, _) = self.expect_ident()?;
        let mut path = first;
        while matches!(self.peek_kind(), TokenKind::Slash) {
            self.advance(); // consume '/'
            let (seg, _) = self.expect_ident()?;
            path.push('/');
            path.push_str(&seg);
        }
        Ok(path)
    }

    /// `.` の後のシンボル指定をパース
    /// `add` / `{add, subtract}` / `*`
    fn parse_use_symbols(&mut self) -> Result<UseSymbols, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Star => {
                self.advance(); // consume '*'
                Ok(UseSymbols::All)
            }
            TokenKind::LBrace => {
                self.advance(); // consume '{'
                let mut symbols = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                    let (name, _) = self.expect_ident()?;
                    let alias = if matches!(self.peek_kind(), TokenKind::As) {
                        self.advance(); // consume 'as'
                        let (alias_name, _) = self.expect_ident()?;
                        Some(alias_name)
                    } else {
                        None
                    };
                    symbols.push((name, alias));
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RBrace)?;
                Ok(UseSymbols::Multiple(symbols))
            }
            _ => {
                // 単一シンボル: add [as alias]
                let (name, _) = self.expect_ident()?;
                let alias = if matches!(self.peek_kind(), TokenKind::As) {
                    self.advance(); // consume 'as'
                    let (alias_name, _) = self.expect_ident()?;
                    Some(alias_name)
                } else {
                    None
                };
                Ok(UseSymbols::Single(name, alias))
            }
        }
    }

    fn parse_binding_rhs(&mut self) -> Result<(Option<TypeAnn>, Expr, Span), ParseError> {
        let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };
        self.expect_token(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let span = self.current_span();
        Ok((type_ann, value, span))
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.parse_let_with_pub(false)
    }

    fn parse_let_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Let)?;
        let (name, _) = self.expect_ident()?;
        let (type_ann, value, _) = self.parse_binding_rhs()?;
        Ok(Stmt::Let {
            name,
            type_ann,
            value,
            is_pub,
            span,
        })
    }

    fn parse_state(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::State)?;
        let (name, _) = self.expect_ident()?;
        let (type_ann, value, _) = self.parse_binding_rhs()?;
        Ok(Stmt::State {
            name,
            type_ann,
            value,
            span,
        })
    }

    fn parse_const(&mut self) -> Result<Stmt, ParseError> {
        self.parse_const_with_pub(false)
    }

    fn parse_const_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Const)?;
        let (name, _) = self.expect_ident()?;
        let (type_ann, value, _) = self.parse_binding_rhs()?;
        Ok(Stmt::Const {
            name,
            type_ann,
            value,
            is_pub,
            span,
        })
    }

    fn parse_fn(&mut self) -> Result<Stmt, ParseError> {
        self.parse_fn_with_const(false, false)
    }

    fn parse_const_fn(&mut self) -> Result<Stmt, ParseError> {
        self.parse_fn_with_const(false, true)
    }

    fn parse_fn_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        self.parse_fn_with_const(is_pub, false)
    }

    fn parse_const_fn_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        self.parse_fn_with_const(is_pub, true)
    }

    fn parse_fn_with_const(&mut self, is_pub: bool, is_const: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        if is_const {
            self.expect_token(&TokenKind::Const)?;
        }
        self.expect_token(&TokenKind::Fn)?;
        let (name, _) = self.expect_name()?;
        let type_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LParen)?;

        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let (param_name, param_span) = self.expect_ident()?;
            let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };
            params.push(Param {
                name: param_name,
                type_ann,
                span: param_span,
            });
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RParen)?;

        let return_type = if matches!(self.peek_kind(), TokenKind::ThinArrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(Stmt::Fn {
            name,
            type_params,
            params,
            return_type,
            body: Box::new(body),
            is_pub,
            is_const,
            span,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Return)?;
        let value = if matches!(
            self.peek_kind(),
            TokenKind::RBrace | TokenKind::Semicolon | TokenKind::Eof
        ) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return(value, span))
    }

    fn parse_yield(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Yield)?;
        let value = self.parse_expr()?;
        self.skip_sep();
        Ok(Stmt::Yield {
            value: Box::new(value),
            span,
        })
    }

    fn parse_type_ann(&mut self) -> Result<TypeAnn, ParseError> {
        let tok = self.peek().clone();
        let base = match tok.kind {
            // () → TypeAnn::Unit
            TokenKind::LParen => {
                self.advance();
                self.expect_token(&TokenKind::RParen)?;
                TypeAnn::Unit
            }
            TokenKind::Fn => {
                self.advance();
                self.expect_token(&TokenKind::LParen)?;
                let mut params = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    params.push(self.parse_type_ann()?);
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect_token(&TokenKind::RParen)?;
                self.expect_token(&TokenKind::ThinArrow)?;
                let return_type = self.parse_type_ann()?;
                TypeAnn::Fn {
                    params,
                    return_type: Box::new(return_type),
                }
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                self.advance();
                match name.as_str() {
                    "number" => TypeAnn::Number,
                    "float" => TypeAnn::Float,
                    "string" => TypeAnn::String,
                    "bool" => TypeAnn::Bool,
                    "list" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let inner = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::List(Box::new(inner))
                    }
                    "generate" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let inner = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::Generate(Box::new(inner))
                    }
                    "map" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let key = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Comma)?;
                        let val = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::Map(Box::new(key), Box::new(val))
                    }
                    "set" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let inner = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::Set(Box::new(inner))
                    }
                    "ordered_map" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let key = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Comma)?;
                        let val = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::OrderedMap(Box::new(key), Box::new(val))
                    }
                    "ordered_set" => {
                        self.expect_token(&TokenKind::Lt)?;
                        let inner = self.parse_type_ann()?;
                        self.expect_token(&TokenKind::Gt)?;
                        TypeAnn::OrderedSet(Box::new(inner))
                    }
                    other => {
                        // Named の後に `<` が続く場合は Generic
                        if matches!(self.peek_kind(), TokenKind::Lt) {
                            self.advance(); // consume '<'
                            let mut args = Vec::new();
                            loop {
                                // Pick/Omit の Keys 引数: "id" | "name" 形式
                                if let TokenKind::Str(s) = self.peek_kind().clone() {
                                    let mut keys = vec![s.clone()];
                                    self.advance();
                                    while matches!(self.peek_kind(), TokenKind::Pipe) {
                                        self.advance(); // '|' を消費
                                        if let TokenKind::Str(next) = self.peek_kind().clone() {
                                            keys.push(next.clone());
                                            self.advance();
                                        } else {
                                            break;
                                        }
                                    }
                                    args.push(TypeAnn::StringLiteralUnion(keys));
                                } else {
                                    args.push(self.parse_type_ann()?);
                                }
                                match self.peek_kind() {
                                    TokenKind::Comma => {
                                        self.advance();
                                    }
                                    _ => break,
                                }
                            }
                            self.expect_token(&TokenKind::Gt)?;
                            TypeAnn::Generic {
                                name: other.to_string(),
                                args,
                            }
                        } else {
                            TypeAnn::Named(other.to_string())
                        }
                    }
                }
            }
            _ => {
                return Err(ParseError::UnexpectedToken {
                    expected: "type annotation".to_string(),
                    found: tok.kind,
                    span: tok.span,
                });
            }
        };

        // postfix: `?`, `!`, `![E]`、および `=> ReturnType`（関数型注釈）
        match self.peek_kind() {
            TokenKind::Question => {
                self.advance();
                Ok(TypeAnn::Option(Box::new(base)))
            }
            TokenKind::Bang => {
                self.advance();
                // T![E] チェック
                if matches!(self.peek_kind(), TokenKind::LBracket) {
                    self.advance(); // consume '['
                    let err_type = self.parse_type_ann()?;
                    self.expect_token(&TokenKind::RBracket)?;
                    Ok(TypeAnn::ResultWith(Box::new(base), Box::new(err_type)))
                } else {
                    Ok(TypeAnn::Result(Box::new(base)))
                }
            }
            TokenKind::Arrow => {
                // T => U 関数型注釈
                self.advance(); // consume '=>'
                let return_type = self.parse_type_ann()?;
                Ok(TypeAnn::Fn {
                    params: vec![base],
                    return_type: Box::new(return_type),
                })
            }
            _ => Ok(base),
        }
    }

    // ── パース: 式 ────────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        // 代入: base を or レベルで解析し、直後に `=` があれば Assign / FieldAssign / IndexAssign
        let mut expr = self.parse_or()?;
        while matches!(self.peek_kind(), TokenKind::QuestionQuestion) {
            let span = self.current_span();
            self.advance();
            let right = self.parse_or()?;
            expr = Expr::NullCoalesce {
                value: Box::new(expr),
                default: Box::new(right),
                span,
            };
        }
        if matches!(self.peek_kind(), TokenKind::Eq) {
            match expr {
                Expr::Ident(name, span) => {
                    self.advance(); // consume '='
                    let value = self.parse_expr()?;
                    return Ok(Expr::Assign {
                        name,
                        value: Box::new(value),
                        span,
                    });
                }
                Expr::Field {
                    object,
                    field,
                    span,
                } => {
                    self.advance(); // consume '='
                    let value = self.parse_expr()?;
                    return Ok(Expr::FieldAssign {
                        object,
                        field,
                        value: Box::new(value),
                        span,
                    });
                }
                Expr::Index {
                    object,
                    index,
                    span,
                } => {
                    self.advance(); // consume '='
                    let value = self.parse_expr()?;
                    return Ok(Expr::IndexAssign {
                        object,
                        index,
                        value: Box::new(value),
                        span,
                    });
                }
                other => return Ok(other),
            }
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek_kind(), TokenKind::Or) {
            let span = self.current_span();
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while matches!(self.peek_kind(), TokenKind::And) {
            let span = self.current_span();
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::Le,
                TokenKind::GtEq => BinOp::Ge,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            let span = self.current_span();
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        match self.peek_kind() {
            TokenKind::Bang => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                TokenKind::Question => {
                    let span = self.current_span();
                    self.advance();
                    expr = Expr::Question(Box::new(expr), span);
                }
                TokenKind::ColonColon => {
                    // TypeName::method() / TypeName::Variant / TypeName::Variant(expr) / TypeName::Variant { field: expr }
                    // TypeName::new<StateName>() — typestate インスタンス生成
                    let span = self.current_span();
                    self.advance(); // consume '::'
                    let (name, _) = self.expect_name()?;
                    // TypeName::new<StateName>() — typestate 専用構文
                    if matches!(self.peek_kind(), TokenKind::Lt) {
                        self.advance(); // consume '<'
                        let (state_name, state_span) = self.expect_ident()?;
                        self.expect_token(&TokenKind::Gt)?;
                        self.expect_token(&TokenKind::LParen)?;
                        let extra_args = self.parse_call_args()?;
                        self.expect_token(&TokenKind::RParen)?;
                        // state_name を文字列リテラル引数として MethodCall に変換
                        let mut all_args =
                            vec![Expr::Literal(Literal::String(state_name), state_span)];
                        all_args.extend(extra_args);
                        expr = Expr::MethodCall {
                            object: Box::new(expr),
                            method: name,
                            args: all_args,
                            span,
                        };
                    } else if matches!(self.peek_kind(), TokenKind::LParen) {
                        // TypeName::Variant(expr, ...) または TypeName::method(args)
                        // enum_name が大文字かどうか、Variant が大文字かどうかで判定
                        let is_enum_variant = if let Expr::Ident(ref type_name, _) = expr {
                            is_type_name(type_name) && is_type_name(&name)
                        } else {
                            false
                        };
                        self.advance(); // consume '('
                        if is_enum_variant {
                            let args = self.parse_call_args()?;
                            self.expect_token(&TokenKind::RParen)?;
                            let enum_name = if let Expr::Ident(ref n, _) = expr {
                                n.clone()
                            } else {
                                unreachable!()
                            };
                            expr = Expr::EnumInit {
                                enum_name,
                                variant: name,
                                data: EnumInitData::Tuple(args),
                                span,
                            };
                        } else {
                            let args = self.parse_call_args()?;
                            self.expect_token(&TokenKind::RParen)?;
                            expr = Expr::MethodCall {
                                object: Box::new(expr),
                                method: name,
                                args,
                                span,
                            };
                        }
                    } else if matches!(self.peek_kind(), TokenKind::LBrace) {
                        // TypeName::Variant { field: expr, ... } — enum struct variant init
                        let is_enum_variant = if let Expr::Ident(ref type_name, _) = expr {
                            is_type_name(type_name) && is_type_name(&name)
                        } else {
                            false
                        };
                        if is_enum_variant {
                            self.advance(); // consume '{'
                            let mut fields: Vec<(String, Expr)> = Vec::new();
                            loop {
                                self.skip_sep();
                                if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                                    break;
                                }
                                let (field_name, _) = self.expect_ident()?;
                                self.expect_token(&TokenKind::Colon)?;
                                let value = self.parse_expr()?;
                                fields.push((field_name, value));
                                if matches!(
                                    self.peek_kind(),
                                    TokenKind::Comma | TokenKind::Semicolon
                                ) {
                                    self.advance();
                                }
                            }
                            self.expect_token(&TokenKind::RBrace)?;
                            let enum_name = if let Expr::Ident(ref n, _) = expr {
                                n.clone()
                            } else {
                                unreachable!()
                            };
                            expr = Expr::EnumInit {
                                enum_name,
                                variant: name,
                                data: EnumInitData::Struct(fields),
                                span,
                            };
                        } else {
                            // 通常のフィールドアクセスとして処理
                            expr = Expr::Field {
                                object: Box::new(expr),
                                field: name,
                                span,
                            };
                        }
                    } else {
                        // TypeName::Variant（Unit）または TypeName::method
                        let is_enum_variant = if let Expr::Ident(ref type_name, _) = expr {
                            is_type_name(type_name) && is_type_name(&name)
                        } else {
                            false
                        };
                        if is_enum_variant {
                            let enum_name = if let Expr::Ident(ref n, _) = expr {
                                n.clone()
                            } else {
                                unreachable!()
                            };
                            expr = Expr::EnumInit {
                                enum_name,
                                variant: name,
                                data: EnumInitData::None,
                                span,
                            };
                        } else {
                            expr = Expr::Field {
                                object: Box::new(expr),
                                field: name,
                                span,
                            };
                        }
                    }
                }
                TokenKind::Dot => {
                    self.advance();
                    let (name, span) = self.expect_name()?;
                    if name == "await" && !matches!(self.peek_kind(), TokenKind::LParen) {
                        expr = Expr::Await {
                            expr: Box::new(expr),
                            span,
                        };
                    } else if matches!(self.peek_kind(), TokenKind::LParen) {
                        self.advance();
                        let args = self.parse_call_args()?;
                        self.expect_token(&TokenKind::RParen)?;
                        expr = Expr::MethodCall {
                            object: Box::new(expr),
                            method: name,
                            args,
                            span,
                        };
                    } else {
                        expr = Expr::Field {
                            object: Box::new(expr),
                            field: name,
                            span,
                        };
                    }
                }
                TokenKind::QuestionDot => {
                    let span = self.current_span();
                    self.advance();
                    let (name, _) = self.expect_name()?;
                    let chain = if matches!(self.peek_kind(), TokenKind::LParen) {
                        self.advance();
                        let args = self.parse_call_args()?;
                        self.expect_token(&TokenKind::RParen)?;
                        ChainKind::Method { name, args }
                    } else {
                        ChainKind::Field(name)
                    };
                    expr = Expr::OptionalChain {
                        object: Box::new(expr),
                        chain,
                        span,
                    };
                }
                TokenKind::LBracket => {
                    let span = self.current_span();
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect_token(&TokenKind::RBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::LParen => {
                    let span = self.current_span();
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect_token(&TokenKind::RParen)?;
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        span,
                    };
                }
                _ => break,
            }
        }
        while matches!(self.peek_kind(), TokenKind::PipeArrow) {
            let span = self.current_span();
            self.advance();
            let (method, args) = self.parse_pipe_call_suffix()?;
            expr = Expr::MethodCall {
                object: Box::new(expr),
                method,
                args,
                span,
            };
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            args.push(self.parse_expr()?);
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(args)
    }

    fn parse_pipe_call_suffix(&mut self) -> Result<(String, Vec<Expr>), ParseError> {
        let (name, _) = self.expect_ident()?;
        let args = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.advance();
            let args = self.parse_call_args()?;
            self.expect_token(&TokenKind::RParen)?;
            args
        } else {
            Vec::new()
        };
        Ok((name, args))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek().clone();
        match tok.kind.clone() {
            // ── リテラル ──
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Int(n), tok.span))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(f), tok.span))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(s), tok.span))
            }
            TokenKind::StrInterp(parts) => {
                self.advance();
                self.parse_interp(parts, tok.span)
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true), tok.span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false), tok.span))
            }

            // ── 識別子 / 単引数クロージャ / StructInit ──
            TokenKind::Ident(_) => {
                if matches!(self.kind_at(1), Some(TokenKind::Arrow)) {
                    self.parse_closure()
                } else {
                    let (name, span) = self.expect_ident()?;
                    // StructInit: 大文字から始まる識別子の後に `{` が来る場合
                    // ただし `{` がブロックとして解釈される文脈とは区別できないため
                    // 大文字から始まる名前限定で試みる
                    if matches!(self.peek_kind(), TokenKind::LBrace) && is_type_name(&name) {
                        self.parse_struct_init(name, span)
                    } else {
                        Ok(Expr::Ident(name, span))
                    }
                }
            }

            // ── キーワードを識別子として扱う（some/none/ok/err/self） ──
            TokenKind::Some
            | TokenKind::None
            | TokenKind::Ok
            | TokenKind::Err
            | TokenKind::SelfVal => {
                let tok = self.advance();
                let name = match &tok.kind {
                    TokenKind::Some => "some",
                    TokenKind::None => "none",
                    TokenKind::Ok => "ok",
                    TokenKind::Err => "err",
                    TokenKind::SelfVal => "self",
                    _ => unreachable!(),
                };
                Ok(Expr::Ident(name.to_string(), tok.span))
            }

            // ── 括弧式 / 多引数クロージャ ──
            TokenKind::LParen => {
                if self.is_closure_parens_start() {
                    self.parse_closure()
                } else {
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect_token(&TokenKind::RParen)?;
                    Ok(expr)
                }
            }

            // ── ブロック / Map / Set ──
            TokenKind::LBrace => self.parse_brace_expr(),

            TokenKind::Spawn => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Expr::Spawn {
                    body: Box::new(body),
                    span: tok.span,
                })
            }

            // ── if / while / for / match ──
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Match => self.parse_match(),

            // ── [ ] リスト / 範囲 ──
            TokenKind::LBracket => self.parse_bracket_expr(),

            _ => Err(ParseError::UnexpectedToken {
                expected: "expression".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    // ── パース: 複合式 ────────────────────────────────────────────────────

    fn parse_block(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::LBrace)?;

        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr>> = None;

        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }

            // `enum` は Ident("enum") として届く（ブロック内でも対応）
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                if name == "enum" {
                    self.advance(); // consume 'enum'
                    stmts.push(self.parse_enum_def_body(vec![])?);
                    continue;
                }
            }
            match self.peek_kind().clone() {
                TokenKind::Let => stmts.push(self.parse_let()?),
                TokenKind::State => stmts.push(self.parse_state()?),
                TokenKind::Const => stmts.push(self.parse_const()?),
                TokenKind::Fn => stmts.push(self.parse_fn()?),
                TokenKind::Return => stmts.push(self.parse_return()?),
                TokenKind::Struct => stmts.push(self.parse_struct_def(vec![])?),
                TokenKind::Impl => stmts.push(self.parse_impl_or_impl_trait()?),
                TokenKind::Trait => stmts.push(self.parse_trait_def()?),
                TokenKind::Mixin => stmts.push(self.parse_mixin_def()?),
                TokenKind::At => stmts.push(self.parse_annotated_stmt()?),
                TokenKind::Data => stmts.push(self.parse_data_def()?),
                TokenKind::Typestate => stmts.push(self.parse_typestate_def()?),
                TokenKind::Yield => stmts.push(self.parse_yield()?),
                _ => {
                    let expr = self.parse_expr()?;
                    if matches!(self.peek_kind(), TokenKind::Semicolon) {
                        self.advance();
                        stmts.push(Stmt::Expr(expr));
                    } else if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        tail = Some(Box::new(expr));
                        break;
                    } else {
                        stmts.push(Stmt::Expr(expr));
                    }
                }
            }
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Expr::Block { stmts, tail, span })
    }

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::If)?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;

        let else_block = if matches!(self.peek_kind(), TokenKind::Else) {
            self.advance();
            if matches!(self.peek_kind(), TokenKind::If) {
                Some(Box::new(self.parse_if()?))
            } else {
                Some(Box::new(self.parse_block()?))
            }
        } else {
            None
        };

        Ok(Expr::If {
            cond: Box::new(cond),
            then_block: Box::new(then_block),
            else_block,
            span,
        })
    }

    fn parse_while(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::While)?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::While {
            cond: Box::new(cond),
            body: Box::new(body),
            span,
        })
    }

    fn parse_for(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::For)?;
        let (var, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::In)?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Expr::For {
            var,
            iter: Box::new(iter),
            body: Box::new(body),
            span,
        })
    }

    fn parse_match(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Match)?;
        let scrutinee = self.parse_expr()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut arms = Vec::new();
        loop {
            while matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let arm_span = self.current_span();
            let pattern = self.parse_pattern()?;
            self.expect_token(&TokenKind::Arrow)?;
            let body = self.parse_expr()?;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_span,
            });
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span,
        })
    }

    fn parse_closure(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();

        let params = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.advance(); // consume '('
            let mut ps = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let (name, _) = self.expect_ident()?;
                ps.push(name);
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect_token(&TokenKind::RParen)?;
            ps
        } else {
            let (name, _) = self.expect_ident()?;
            vec![name]
        };

        self.expect_token(&TokenKind::Arrow)?;

        let body = if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.parse_block()?
        } else {
            self.parse_expr()?
        };

        Ok(Expr::Closure {
            params,
            body: Box::new(body),
            span,
        })
    }

    // ── T-3-B: trait / mixin / impl for パース ────────────────────────────

    fn parse_trait_def(&mut self) -> Result<Stmt, ParseError> {
        self.parse_trait_def_with_pub(false)
    }

    fn parse_trait_def_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Trait)?;
        let (name, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut methods: Vec<TraitMethod> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            methods.push(self.parse_trait_method()?);
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::TraitDef {
            name,
            methods,
            is_pub,
            span,
        })
    }

    /// trait 内のメソッドをパース（抽象 or デフォルト実装）
    fn parse_trait_method(&mut self) -> Result<TraitMethod, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Fn)?;
        let (name, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::LParen)?;

        let mut params = Vec::new();
        let mut has_self = false;
        let mut has_state_self = false;

        // 最初のパラメータが `state self` または `self` の場合を処理
        if matches!(self.peek_kind(), TokenKind::State) {
            self.advance(); // consume 'state'
            if matches!(self.peek_kind(), TokenKind::SelfVal) {
                self.advance(); // consume 'self'
                has_self = true;
                has_state_self = true;
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
        } else if matches!(self.peek_kind(), TokenKind::SelfVal) {
            self.advance(); // consume 'self'
            has_self = true;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }

        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let (param_name, param_span) = self.expect_ident()?;
            let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };
            params.push(Param {
                name: param_name,
                type_ann,
                span: param_span,
            });
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RParen)?;

        let return_type = if matches!(self.peek_kind(), TokenKind::ThinArrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        // body が `{` で始まるならデフォルト実装、そうでなければ抽象
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            let body = self.parse_block()?;
            Ok(TraitMethod::Default {
                name,
                params,
                return_type,
                body: Box::new(body),
                has_self,
                has_state_self,
                span,
            })
        } else {
            // 抽象メソッド: セミコロンがあれば消費する
            if matches!(self.peek_kind(), TokenKind::Semicolon) {
                self.advance();
            }
            Ok(TraitMethod::Abstract {
                name,
                params,
                return_type,
                has_self,
                span,
            })
        }
    }

    fn parse_mixin_def(&mut self) -> Result<Stmt, ParseError> {
        self.parse_mixin_def_with_pub(false)
    }

    fn parse_mixin_def_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Mixin)?;
        let (name, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut methods: Vec<FnDef> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            methods.push(self.parse_fn_def()?);
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::MixinDef {
            name,
            methods,
            is_pub,
            span,
        })
    }

    /// `impl ...` を見て、`impl Trait for Type` か `impl Name { }` かを判断する
    fn parse_impl_or_impl_trait(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Impl)?;
        let type_params = self.parse_type_params()?;
        let first_name = self.expect_ident()?.0;
        let first_type_args = self.parse_type_args()?;

        // `impl Name for Type ...` 形式か判断（for は TokenKind::For として届く）
        let is_for = matches!(self.peek_kind(), TokenKind::For);
        if is_for {
            self.advance(); // consume 'for'
            let (target, _) = self.expect_ident()?;

            // body があるか（`{` が来れば本体あり）
            if matches!(self.peek_kind(), TokenKind::LBrace) {
                self.expect_token(&TokenKind::LBrace)?;
                let mut methods: Vec<FnDef> = Vec::new();
                loop {
                    self.skip_sep();
                    if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        break;
                    }
                    methods.push(self.parse_fn_def()?);
                }
                self.expect_token(&TokenKind::RBrace)?;
                Ok(Stmt::ImplTrait {
                    trait_name: first_name,
                    target,
                    methods,
                    span,
                })
            } else {
                // 本体なし: `impl MixinName for TypeName` — セミコロンは省略可
                if matches!(self.peek_kind(), TokenKind::Semicolon) {
                    self.advance();
                }
                Ok(Stmt::ImplTrait {
                    trait_name: first_name,
                    target,
                    methods: vec![],
                    span,
                })
            }
        } else {
            // 旧来の `impl Name { ... }` 形式（trait_name: None の ImplBlock）
            self.expect_token(&TokenKind::LBrace)?;
            let mut methods: Vec<FnDef> = Vec::new();
            let mut operators: Vec<OperatorDef> = Vec::new();
            loop {
                self.skip_sep();
                if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                    break;
                }
                if matches!(self.peek_kind(), TokenKind::Const)
                    || matches!(self.peek_kind(), TokenKind::Fn)
                {
                    methods.push(self.parse_fn_def()?);
                } else if matches!(self.peek_kind(), TokenKind::Operator) {
                    operators.push(self.parse_operator_def()?);
                } else {
                    let tok = self.peek().clone();
                    return Err(ParseError::UnexpectedToken {
                        expected: "fn, const fn, or operator".to_string(),
                        found: tok.kind,
                        span: tok.span,
                    });
                }
            }
            self.expect_token(&TokenKind::RBrace)?;
            Ok(Stmt::ImplBlock {
                target: first_name,
                type_params,
                target_type_args: first_type_args,
                trait_name: None,
                methods,
                operators,
                span,
            })
        }
    }

    // ── T-1-C: struct / impl / @derive パース ──────────────────────────────

    fn parse_annotated_stmt(&mut self) -> Result<Stmt, ParseError> {
        // @derive(...) などのアノテーションをパースし、直後の struct 定義に付与
        let mut derives: Vec<String> = Vec::new();
        while matches!(self.peek_kind(), TokenKind::At) {
            self.advance(); // consume '@'
            let (ann_name, _) = self.expect_ident()?;
            if ann_name == "derive" {
                self.expect_token(&TokenKind::LParen)?;
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    let (d, _) = self.expect_ident()?;
                    derives.push(d);
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RParen)?;
            }
            self.skip_sep();
        }
        // アノテーションの直後に struct または enum が来ることを期待
        match self.peek_kind().clone() {
            TokenKind::Struct => self.parse_struct_def(derives),
            TokenKind::Ident(ref name) if name == "enum" => {
                self.advance(); // consume 'enum'
                self.parse_enum_def_body(derives)
            }
            TokenKind::Typestate => self.parse_typestate_def_with_meta(derives),
            _ => {
                let tok = self.peek().clone();
                Err(ParseError::UnexpectedToken {
                    expected: "struct or enum or typestate after annotation".to_string(),
                    found: tok.kind,
                    span: tok.span,
                })
            }
        }
    }

    fn parse_struct_def(&mut self, derives: Vec<String>) -> Result<Stmt, ParseError> {
        self.parse_struct_def_with_pub(derives, false)
    }

    fn parse_struct_def_with_pub(
        &mut self,
        derives: Vec<String>,
        is_pub: bool,
    ) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Struct)?;
        let (name, _) = self.expect_ident()?;
        let generic_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut fields: Vec<(String, TypeAnn)> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let (field_name, _) = self.expect_ident()?;
            self.expect_token(&TokenKind::Colon)?;
            let type_ann = self.parse_type_ann()?;
            fields.push((field_name, type_ann));
            // フィールド区切りはカンマまたは改行（セミコロン）
            if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::StructDef {
            name,
            generic_params,
            fields,
            derives,
            is_pub,
            span,
        })
    }

    #[allow(dead_code)]
    fn parse_impl_block(&mut self) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Impl)?;
        let type_params = self.parse_type_params()?;

        let first_name = self.expect_ident()?.0;
        let first_type_args = self.parse_type_args()?;

        let (target, target_type_args, trait_name) =
            if matches!(self.peek_kind(), TokenKind::Ident(_)) {
                let tok = self.peek().clone();
                if let TokenKind::Ident(ref s) = tok.kind {
                    if s == "for" {
                        self.advance();
                        let (target, _) = self.expect_ident()?;
                        let target_type_args = self.parse_type_args()?;
                        (target, target_type_args, Some(first_name))
                    } else {
                        (first_name, first_type_args, None)
                    }
                } else {
                    (first_name, first_type_args, None)
                }
            } else {
                (first_name, first_type_args, None)
            };

        self.expect_token(&TokenKind::LBrace)?;

        let mut methods: Vec<FnDef> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            methods.push(self.parse_fn_def()?);
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::ImplBlock {
            target,
            type_params,
            target_type_args,
            trait_name,
            methods,
            operators: Vec::new(),
            span,
        })
    }
    /// impl ブロック内の fn 定義をパース
    fn parse_fn_def(&mut self) -> Result<FnDef, ParseError> {
        let span = self.current_span();
        let mut is_const = false;
        if matches!(self.peek_kind(), TokenKind::Const) {
            is_const = true;
            self.advance();
        }
        self.expect_token(&TokenKind::Fn)?;
        let (name, _) = self.expect_name()?;
        let type_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LParen)?;

        let mut params = Vec::new();
        let mut has_self = false;
        let mut has_state_self = false;

        // 最初のパラメータが `state self` または `self` の場合を処理
        if matches!(self.peek_kind(), TokenKind::State) {
            self.advance(); // consume 'state'
            if matches!(self.peek_kind(), TokenKind::SelfVal) {
                self.advance(); // consume 'self'
                has_self = true;
                has_state_self = true;
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
        } else if matches!(self.peek_kind(), TokenKind::SelfVal) {
            self.advance(); // consume 'self'
            has_self = true;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }

        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let (param_name, param_span) = self.expect_ident()?;
            let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };
            params.push(Param {
                name: param_name,
                type_ann,
                span: param_span,
            });
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RParen)?;

        let return_type = if matches!(self.peek_kind(), TokenKind::ThinArrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(FnDef {
            name,
            type_params,
            params,
            return_type,
            body: Box::new(body),
            has_self,
            has_state_self,
            is_const,
            span,
        })
    }

    fn parse_struct_init(&mut self, name: String, span: Span) -> Result<Expr, ParseError> {
        self.expect_token(&TokenKind::LBrace)?;
        let mut fields: Vec<(String, Expr)> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let (field_name, _) = self.expect_ident()?;
            self.expect_token(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));
            if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RBrace)?;
        Ok(Expr::StructInit { name, fields, span })
    }

    fn parse_brace_expr(&mut self) -> Result<Expr, ParseError> {
        match self.classify_brace_expr() {
            BraceExprKind::Block => self.parse_block(),
            BraceExprKind::EmptyMap => {
                let span = self.current_span();
                self.expect_token(&TokenKind::LBrace)?;
                self.expect_token(&TokenKind::RBrace)?;
                Ok(Expr::MapLiteral {
                    pairs: vec![],
                    span,
                })
            }
            BraceExprKind::Map => {
                let span = self.current_span();
                self.expect_token(&TokenKind::LBrace)?;
                let mut pairs = Vec::new();
                loop {
                    self.skip_sep();
                    if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        break;
                    }
                    let key = self.parse_expr()?;
                    self.expect_token(&TokenKind::Colon)?;
                    let value = self.parse_expr()?;
                    pairs.push((key, value));
                    if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RBrace)?;
                Ok(Expr::MapLiteral { pairs, span })
            }
            BraceExprKind::Set => {
                let span = self.current_span();
                self.expect_token(&TokenKind::LBrace)?;
                let mut items = Vec::new();
                loop {
                    self.skip_sep();
                    if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        break;
                    }
                    items.push(self.parse_expr()?);
                    if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RBrace)?;
                Ok(Expr::SetLiteral { items, span })
            }
        }
    }

    /// `enum` キーワードを消費した後の本体をパース
    fn parse_enum_def_body(&mut self, derives: Vec<String>) -> Result<Stmt, ParseError> {
        self.parse_enum_def_body_with_pub(derives, false)
    }

    fn parse_enum_def_body_with_pub(
        &mut self,
        derives: Vec<String>,
        is_pub: bool,
    ) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        let (name, _) = self.expect_ident()?;
        let generic_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut variants: Vec<EnumVariant> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            // バリアント名（大文字から始まる識別子を期待）
            let (variant_name, _) = self.expect_ident()?;

            if matches!(self.peek_kind(), TokenKind::LParen) {
                // Tuple バリアント: Variant(Type, Type, ...)
                self.advance(); // consume '('
                let mut types: Vec<TypeAnn> = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    types.push(self.parse_type_ann()?);
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RParen)?;
                variants.push(EnumVariant::Tuple(variant_name, types));
            } else if matches!(self.peek_kind(), TokenKind::LBrace) {
                // Struct バリアント: Variant { field: Type, ... }
                self.advance(); // consume '{'
                let mut fields: Vec<(String, TypeAnn)> = Vec::new();
                loop {
                    self.skip_sep();
                    if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        break;
                    }
                    let (field_name, _) = self.expect_ident()?;
                    self.expect_token(&TokenKind::Colon)?;
                    let type_ann = self.parse_type_ann()?;
                    fields.push((field_name, type_ann));
                    if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                        self.advance();
                    }
                }
                self.expect_token(&TokenKind::RBrace)?;
                variants.push(EnumVariant::Struct(variant_name, fields));
            } else {
                // Unit バリアント
                variants.push(EnumVariant::Unit(variant_name));
            }

            // バリアント間のカンマ・セミコロンは省略可能
            if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::EnumDef {
            name,
            generic_params,
            variants,
            derives,
            is_pub,
            span,
        })
    }

    fn parse_bracket_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::LBracket)?;

        // 空リスト
        if matches!(self.peek_kind(), TokenKind::RBracket) {
            self.advance();
            return Ok(Expr::List(vec![], span));
        }

        let first = self.parse_expr()?;

        // 範囲: first .. end  または  first ..= end
        if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
            let inclusive = matches!(self.peek_kind(), TokenKind::DotDotEq);
            self.advance();
            let end = self.parse_expr()?;
            self.expect_token(&TokenKind::RBracket)?;
            return Ok(Expr::Range {
                start: Box::new(first),
                end: Box::new(end),
                inclusive,
                span,
            });
        }

        // リスト
        let mut items = vec![first];
        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            if matches!(self.peek_kind(), TokenKind::RBracket) {
                break; // trailing comma
            }
            items.push(self.parse_expr()?);
        }
        self.expect_token(&TokenKind::RBracket)?;
        Ok(Expr::List(items, span))
    }

    fn parse_interp(&mut self, parts: Vec<StrPart>, span: Span) -> Result<Expr, ParseError> {
        let mut interp_parts = Vec::new();
        for part in parts {
            match part {
                StrPart::Literal(s) => interp_parts.push(InterpPart::Literal(s)),
                StrPart::Expr(src) => {
                    let tokens = lex(&src).map_err(|e| ParseError::UnexpectedEof {
                        expected: format!("valid interpolation expression: {}", e),
                    })?;
                    let mut sub = Parser::new(tokens);
                    let expr = sub.parse_expr()?;
                    interp_parts.push(InterpPart::Expr(Box::new(expr)));
                }
            }
        }
        Ok(Expr::Interpolation {
            parts: interp_parts,
            span,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.peek().clone();
        match tok.kind.clone() {
            TokenKind::Int(n) => {
                self.advance();
                let lit = Literal::Int(n);
                if matches!(self.peek_kind(), TokenKind::DotDot | TokenKind::DotDotEq) {
                    let inclusive = matches!(self.peek_kind(), TokenKind::DotDotEq);
                    self.advance();
                    let end_tok = self.peek().clone();
                    let end_lit = match end_tok.kind {
                        TokenKind::Int(m) => {
                            self.advance();
                            Literal::Int(m)
                        }
                        TokenKind::Float(f) => {
                            self.advance();
                            Literal::Float(f)
                        }
                        _ => {
                            return Err(ParseError::UnexpectedToken {
                                expected: "range end literal".to_string(),
                                found: end_tok.kind,
                                span: end_tok.span,
                            })
                        }
                    };
                    Ok(Pattern::Range {
                        start: lit,
                        end: end_lit,
                        inclusive,
                    })
                } else {
                    Ok(Pattern::Literal(lit))
                }
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Pattern::Literal(Literal::Float(f)))
            }
            TokenKind::Str(s) => {
                self.advance();
                Ok(Pattern::Literal(Literal::String(s)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Pattern::Literal(Literal::Bool(true)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Pattern::Literal(Literal::Bool(false)))
            }
            TokenKind::Ident(name) => {
                self.advance();
                if name == "_" {
                    return Ok(Pattern::Wildcard);
                }
                // TypeName::Variant のパターン（大文字で始まる識別子 + ::）
                if is_type_name(&name) && matches!(self.peek_kind(), TokenKind::ColonColon) {
                    self.advance(); // consume '::'
                    let (variant, _) = self.expect_ident()?;
                    return self.parse_enum_pattern_tail(Some(name), variant);
                }
                // 大文字で始まる識別子 + ( または { はバリアントパターンの可能性
                if is_type_name(&name) {
                    if matches!(self.peek_kind(), TokenKind::LParen) {
                        return self.parse_enum_pattern_tail(None, name);
                    }
                    if matches!(self.peek_kind(), TokenKind::LBrace) {
                        return self.parse_enum_pattern_tail(None, name);
                    }
                    // Unit バリアントとして扱う
                    return Ok(Pattern::EnumUnit {
                        enum_name: None,
                        variant: name,
                    });
                }
                Ok(Pattern::Ident(name))
            }
            TokenKind::Some => {
                self.advance();
                self.expect_token(&TokenKind::LParen)?;
                let inner = self.parse_pattern()?;
                self.expect_token(&TokenKind::RParen)?;
                Ok(Pattern::Some(Box::new(inner)))
            }
            TokenKind::None => {
                self.advance();
                Ok(Pattern::None)
            }
            TokenKind::Ok => {
                self.advance();
                self.expect_token(&TokenKind::LParen)?;
                let inner = self.parse_pattern()?;
                self.expect_token(&TokenKind::RParen)?;
                Ok(Pattern::Ok(Box::new(inner)))
            }
            TokenKind::Err => {
                self.advance();
                self.expect_token(&TokenKind::LParen)?;
                let inner = self.parse_pattern()?;
                self.expect_token(&TokenKind::RParen)?;
                Ok(Pattern::Err(Box::new(inner)))
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "pattern".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    // ── T-4-B: data キーワードのパース ───────────────────────────────────

    fn parse_data_def(&mut self) -> Result<Stmt, ParseError> {
        self.parse_data_def_with_pub(false)
    }

    fn parse_data_def_with_pub(&mut self, is_pub: bool) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Data)?;
        let (name, _) = self.expect_ident()?;
        let generic_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut fields: Vec<(String, TypeAnn)> = Vec::new();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let (field_name, _) = self.expect_ident()?;
            self.expect_token(&TokenKind::Colon)?;
            let type_ann = self.parse_type_ann()?;
            fields.push((field_name, type_ann));
            if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RBrace)?;

        // validate ブロック（オプション）
        let validate_rules = if matches!(self.peek_kind(), TokenKind::Ident(ref s) if s == "validate")
        {
            self.advance(); // consume 'validate'
            self.parse_validate_block()?
        } else {
            vec![]
        };

        Ok(Stmt::DataDef {
            name,
            generic_params,
            fields,
            validate_rules,
            is_pub,
            span,
        })
    }

    // ── T-5-B: typestate キーワードのパース ──────────────────────────────

    fn parse_typestate_def(&mut self) -> Result<Stmt, ParseError> {
        self.parse_typestate_def_with_meta(vec![])
    }

    fn parse_typestate_def_with_meta(&mut self, derives: Vec<String>) -> Result<Stmt, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Typestate)?;
        let (name, _) = self.expect_ident()?;
        let generic_params = self.parse_type_params()?;
        self.expect_token(&TokenKind::LBrace)?;

        let mut fields: Vec<(String, TypeAnn)> = Vec::new();
        let mut states: Vec<TypestateMarker> = Vec::new();
        let mut state_methods: Vec<TypestateState> = Vec::new();
        let mut any_methods: Vec<FnDef> = Vec::new();
        let mut any_block_count = 0usize;

        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }

            // `states: [State1, State2, ...]` の宣言
            if let TokenKind::Ident(ref kw) = self.peek_kind().clone() {
                if kw == "states" {
                    self.advance(); // consume 'states'
                    self.expect_token(&TokenKind::Colon)?;
                    self.expect_token(&TokenKind::LBracket)?;
                    loop {
                        self.skip_sep();
                        if matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                            break;
                        }
                        states.push(self.parse_typestate_marker_decl()?);
                        if matches!(self.peek_kind(), TokenKind::Comma) {
                            self.advance();
                        }
                    }
                    self.expect_token(&TokenKind::RBracket)?;
                    continue;
                }

                if kw == "any" {
                    self.advance();
                    any_block_count += 1;
                    self.expect_token(&TokenKind::LBrace)?;
                    loop {
                        self.skip_sep();
                        if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                            break;
                        }
                        any_methods.push(self.parse_typestate_method()?);
                    }
                    self.expect_token(&TokenKind::RBrace)?;
                    continue;
                }
            }

            let (entry_name, _) = self.expect_ident()?;
            match self.peek_kind() {
                TokenKind::Colon => {
                    self.advance();
                    let type_ann = self.parse_type_ann()?;
                    fields.push((entry_name, type_ann));
                    if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                        self.advance();
                    }
                }
                TokenKind::LBrace => {
                    self.advance();

                    let mut methods: Vec<FnDef> = Vec::new();
                    loop {
                        self.skip_sep();
                        if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                            break;
                        }
                        methods.push(self.parse_typestate_method()?);
                    }
                    self.expect_token(&TokenKind::RBrace)?;
                    state_methods.push(TypestateState {
                        name: entry_name,
                        methods,
                    });
                }
                _ => {
                    let tok = self.peek().clone();
                    return Err(ParseError::UnexpectedToken {
                        expected: "':' or '{' in typestate entry".to_string(),
                        found: tok.kind,
                        span: tok.span,
                    });
                }
            }
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(Stmt::TypestateDef {
            name,
            fields,
            states,
            state_methods,
            any_methods,
            any_block_count,
            derives,
            generic_params,
            span,
        })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        let mut params = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::Lt) {
            return Ok(params);
        }

        self.advance();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::Gt | TokenKind::Eof) {
                break;
            }
            let (name, _) = self.expect_ident()?;
            params.push(name);
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::Gt)?;
        Ok(params)
    }

    fn parse_type_args(&mut self) -> Result<Vec<TypeAnn>, ParseError> {
        let mut args = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::Lt) {
            return Ok(args);
        }

        self.advance();
        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::Gt | TokenKind::Eof) {
                break;
            }
            // Pick/Omit の Keys 引数: "id" | "name" | "email" 形式
            if let TokenKind::Str(s) = self.peek_kind().clone() {
                let mut keys = vec![s.clone()];
                self.advance();
                while matches!(self.peek_kind(), TokenKind::Pipe) {
                    self.advance(); // '|' を消費
                    if let TokenKind::Str(next) = self.peek_kind().clone() {
                        keys.push(next.clone());
                        self.advance();
                    } else {
                        break;
                    }
                }
                args.push(TypeAnn::StringLiteralUnion(keys));
            } else {
                args.push(self.parse_type_ann()?);
            }
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::Gt)?;
        Ok(args)
    }

    fn parse_typestate_marker_decl(&mut self) -> Result<TypestateMarker, ParseError> {
        let (name, _) = self.expect_ident()?;
        if matches!(self.peek_kind(), TokenKind::LParen) {
            self.advance();
            let mut fields = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                fields.push(self.parse_type_ann()?);
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect_token(&TokenKind::RParen)?;
            Ok(TypestateMarker::Tuple(name, fields))
        } else if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.advance();
            let mut fields = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                let (field_name, _) = self.expect_ident()?;
                self.expect_token(&TokenKind::Colon)?;
                let type_ann = self.parse_type_ann()?;
                fields.push((field_name, type_ann));
                if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                    self.advance();
                }
            }
            self.expect_token(&TokenKind::RBrace)?;
            Ok(TypestateMarker::Struct(name, fields))
        } else {
            Ok(TypestateMarker::Unit(name))
        }
    }

    fn parse_typestate_method(&mut self) -> Result<FnDef, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Fn)?;
        let (name, _) = self.expect_ident()?;
        self.expect_token(&TokenKind::LParen)?;

        let mut params = Vec::new();
        let mut has_self = false;
        let mut has_state_self = false;

        if matches!(self.peek_kind(), TokenKind::State) {
            self.advance();
            if matches!(self.peek_kind(), TokenKind::SelfVal) {
                self.advance();
                has_self = true;
                has_state_self = true;
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
        } else if matches!(self.peek_kind(), TokenKind::SelfVal) {
            self.advance();
            has_self = true;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }

        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let (param_name, param_span) = self.expect_ident()?;
            let type_ann = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };
            params.push(Param {
                name: param_name,
                type_ann,
                span: param_span,
            });
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.expect_token(&TokenKind::RParen)?;

        let return_type = if matches!(self.peek_kind(), TokenKind::ThinArrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.parse_block()?
        } else {
            if matches!(self.peek_kind(), TokenKind::Semicolon) {
                self.advance();
            }
            Expr::Block {
                stmts: vec![],
                tail: None,
                span: span.clone(),
            }
        };
        Ok(FnDef {
            name,
            type_params: vec![],
            params,
            return_type,
            body: Box::new(body),
            has_self,
            has_state_self,
            is_const: false,
            span,
        })
    }

    fn parse_operator_kind(&mut self) -> Result<OperatorKind, ParseError> {
        let tok = self.peek().clone();
        let span = tok.span;
        match tok.kind {
            TokenKind::Ident(ref name) if name == "unary" => {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::Minus) {
                    self.advance();
                    Ok(OperatorKind::Neg)
                } else {
                    let found = self.peek_kind().clone();
                    Err(ParseError::UnexpectedToken {
                        expected: " '-' after unary".to_string(),
                        found,
                        span,
                    })
                }
            }
            TokenKind::Plus => {
                self.advance();
                Ok(OperatorKind::Add)
            }
            TokenKind::Minus => {
                self.advance();
                Ok(OperatorKind::Sub)
            }
            TokenKind::Star => {
                self.advance();
                Ok(OperatorKind::Mul)
            }
            TokenKind::Slash => {
                self.advance();
                Ok(OperatorKind::Div)
            }
            TokenKind::Percent => {
                self.advance();
                Ok(OperatorKind::Rem)
            }
            TokenKind::EqEq => {
                self.advance();
                Ok(OperatorKind::Eq)
            }
            TokenKind::Lt => {
                self.advance();
                Ok(OperatorKind::Lt)
            }
            TokenKind::LBracket => {
                if matches!(self.kind_at(1), Some(TokenKind::RBracket)) {
                    self.advance();
                    self.advance();
                    Ok(OperatorKind::Index)
                } else {
                    let found = self.peek_kind().clone();
                    Err(ParseError::UnexpectedToken {
                        expected: "']' after '[' for operator []".to_string(),
                        found,
                        span,
                    })
                }
            }
            other => Err(ParseError::UnexpectedToken {
                expected: "operator symbol".to_string(),
                found: other.clone(),
                span,
            }),
        }
    }

    fn parse_operator_def(&mut self) -> Result<OperatorDef, ParseError> {
        let span = self.current_span();
        self.expect_token(&TokenKind::Operator)?;
        let op = self.parse_operator_kind()?;
        self.expect_token(&TokenKind::LParen)?;

        let mut params = Vec::new();
        let mut has_self = false;
        let mut has_state_self = false;

        if matches!(self.peek_kind(), TokenKind::State) {
            self.advance();
            if matches!(self.peek_kind(), TokenKind::SelfVal) {
                self.advance();
                has_self = true;
                has_state_self = true;
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
        } else if matches!(self.peek_kind(), TokenKind::SelfVal) {
            self.advance();
            has_self = true;
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }

        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let (param_name, param_span) = self.expect_ident()?;
            self.expect_token(&TokenKind::Colon)?;
            let type_ann = self.parse_type_ann()?;
            params.push(Param {
                name: param_name,
                type_ann: Some(type_ann),
                span: param_span,
            });
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect_token(&TokenKind::RParen)?;

        let return_type = if matches!(self.peek_kind(), TokenKind::ThinArrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = if matches!(self.peek_kind(), TokenKind::LBrace) {
            self.parse_block()?
        } else {
            if matches!(self.peek_kind(), TokenKind::Semicolon) {
                self.advance();
            }
            Expr::Block {
                stmts: vec![],
                tail: None,
                span: span.clone(),
            }
        };

        Ok(OperatorDef {
            op,
            params,
            return_type,
            body: Box::new(body),
            has_self,
            has_state_self,
            span,
        })
    }

    fn classify_brace_expr(&self) -> BraceExprKind {
        let mut i = self.pos + 1;
        let Some(first) = self.tokens.get(i) else {
            return BraceExprKind::Block;
        };
        if matches!(first.kind, TokenKind::RBrace) {
            return BraceExprKind::EmptyMap;
        }
        if is_block_start_token(&first.kind) {
            return BraceExprKind::Block;
        }

        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut saw_comma = false;

        while let Some(tok) = self.tokens.get(i) {
            match tok.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace => {
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                        return if saw_comma {
                            BraceExprKind::Set
                        } else {
                            BraceExprKind::Block
                        };
                    }
                    brace_depth = brace_depth.saturating_sub(1);
                }
                TokenKind::Colon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    return BraceExprKind::Map;
                }
                TokenKind::Semicolon
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
                {
                    return BraceExprKind::Block;
                }
                TokenKind::Comma if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    saw_comma = true;
                }
                _ => {}
            }
            i += 1;
        }

        BraceExprKind::Block
    }

    /// validate { field: constraint, constraint, ... } をパース
    fn parse_validate_block(&mut self) -> Result<Vec<ValidateRule>, ParseError> {
        self.expect_token(&TokenKind::LBrace)?;
        let mut rules: Vec<ValidateRule> = Vec::new();

        loop {
            self.skip_sep();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let (field_name, _) = self.expect_ident()?;
            self.expect_token(&TokenKind::Colon)?;

            let mut constraints: Vec<Constraint> = Vec::new();
            // 最初の制約を必ずパース
            constraints.push(self.parse_constraint()?);
            // カンマで区切られた追加制約
            while matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance(); // consume ','
                                // 次が識別子なら制約、そうでなければ次のフィールドの開始（改行後）
                                // セミコロンや改行で区切られたフィールド境界の判定:
                                // 次の識別子の後に `:` が来るなら新しいフィールドの開始
                if !self.is_next_constraint() {
                    break;
                }
                constraints.push(self.parse_constraint()?);
            }
            // セミコロンをスキップ
            if matches!(self.peek_kind(), TokenKind::Semicolon) {
                self.advance();
            }

            rules.push(ValidateRule {
                field: field_name,
                constraints,
            });
        }

        self.expect_token(&TokenKind::RBrace)?;
        Ok(rules)
    }

    /// 次のトークンが制約かどうかを判定（先読み）
    /// `ident ':'` のパターンなら新しいフィールド定義なので false
    fn is_next_constraint(&self) -> bool {
        match self.peek_kind() {
            TokenKind::Ident(_) => {
                // ident の後に ':' が来れば新しいフィールド
                match self.kind_at(1) {
                    Some(TokenKind::Colon) => false,
                    _ => true,
                }
            }
            _ => false,
        }
    }

    /// 単一の制約をパース
    fn parse_constraint(&mut self) -> Result<Constraint, ParseError> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                match name.as_str() {
                    "length" => {
                        self.expect_token(&TokenKind::LParen)?;
                        let (min, max) = self.parse_length_args()?;
                        self.expect_token(&TokenKind::RParen)?;
                        Ok(Constraint::Length { min, max })
                    }
                    "alphanumeric" => Ok(Constraint::Alphanumeric),
                    "email_format" => Ok(Constraint::EmailFormat),
                    "url_format" => Ok(Constraint::UrlFormat),
                    "range" => {
                        self.expect_token(&TokenKind::LParen)?;
                        let (min, max) = self.parse_range_args()?;
                        self.expect_token(&TokenKind::RParen)?;
                        Ok(Constraint::Range { min, max })
                    }
                    "contains_digit" => Ok(Constraint::ContainsDigit),
                    "contains_uppercase" => Ok(Constraint::ContainsUppercase),
                    "contains_lowercase" => Ok(Constraint::ContainsLowercase),
                    "not_empty" => Ok(Constraint::NotEmpty),
                    "matches" => {
                        self.expect_token(&TokenKind::LParen)?;
                        let tok = self.peek().clone();
                        // 文字列リテラルを期待
                        let regex_str = match tok.kind {
                            TokenKind::Str(ref s) => s.clone(),
                            TokenKind::StrInterp(ref parts) => {
                                // 補間なし文字列として処理（Literal パーツのみ結合）
                                parts
                                    .iter()
                                    .map(|p| match p {
                                        StrPart::Literal(s) => s.clone(),
                                        StrPart::Expr(_) => String::new(),
                                    })
                                    .collect::<String>()
                            }
                            _ => {
                                return Err(ParseError::UnexpectedToken {
                                    expected: "string literal for matches()".to_string(),
                                    found: tok.kind,
                                    span: tok.span,
                                })
                            }
                        };
                        self.advance();
                        self.expect_token(&TokenKind::RParen)?;
                        Ok(Constraint::Matches(regex_str))
                    }
                    other => Err(ParseError::UnexpectedToken {
                        expected: "constraint name (length, alphanumeric, email_format, ...)"
                            .to_string(),
                        found: TokenKind::Ident(other.to_string()),
                        span: tok.span,
                    }),
                }
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "constraint name".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    /// length() の引数をパース: `min..max` / `min: n` / `max: n` / `n..m`
    fn parse_length_args(&mut self) -> Result<(Option<usize>, Option<usize>), ParseError> {
        // `min:` か `max:` か数値リテラルかを判定
        if let TokenKind::Ident(ref kw) = self.peek_kind().clone() {
            if kw == "min" {
                self.advance(); // consume 'min'
                self.expect_token(&TokenKind::Colon)?;
                let n = self.parse_usize_literal()?;
                return Ok((Some(n), None));
            } else if kw == "max" {
                self.advance(); // consume 'max'
                self.expect_token(&TokenKind::Colon)?;
                let n = self.parse_usize_literal()?;
                return Ok((None, Some(n)));
            }
        }
        // 数値リテラル: `n..m` / `n`
        let min = self.parse_usize_literal()?;
        if matches!(self.peek_kind(), TokenKind::DotDot) {
            self.advance(); // consume '..'
            let max = self.parse_usize_literal()?;
            Ok((Some(min), Some(max)))
        } else {
            Ok((Some(min), None))
        }
    }

    /// range() の引数をパース: `min..max` / `min: n` / `max: n`
    fn parse_range_args(&mut self) -> Result<(Option<f64>, Option<f64>), ParseError> {
        if let TokenKind::Ident(ref kw) = self.peek_kind().clone() {
            if kw == "min" {
                self.advance();
                self.expect_token(&TokenKind::Colon)?;
                let n = self.parse_f64_literal()?;
                return Ok((Some(n), None));
            } else if kw == "max" {
                self.advance();
                self.expect_token(&TokenKind::Colon)?;
                let n = self.parse_f64_literal()?;
                return Ok((None, Some(n)));
            }
        }
        let min = self.parse_f64_literal()?;
        if matches!(self.peek_kind(), TokenKind::DotDot) {
            self.advance();
            let max = self.parse_f64_literal()?;
            Ok((Some(min), Some(max)))
        } else {
            Ok((Some(min), None))
        }
    }

    /// 非負整数リテラルをパース
    fn parse_usize_literal(&mut self) -> Result<usize, ParseError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(n) => {
                self.advance();
                if n < 0 {
                    Err(ParseError::UnexpectedToken {
                        expected: "non-negative integer".to_string(),
                        found: TokenKind::Int(n),
                        span: tok.span,
                    })
                } else {
                    Ok(n as usize)
                }
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "integer literal".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    /// f64 リテラルをパース（整数も許容）
    fn parse_f64_literal(&mut self) -> Result<f64, ParseError> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(n) => {
                self.advance();
                Ok(n as f64)
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(f)
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "number literal".to_string(),
                found: tok.kind,
                span: tok.span,
            }),
        }
    }

    /// バリアント名の後のパターン本体をパース
    fn parse_enum_pattern_tail(
        &mut self,
        enum_name: Option<String>,
        variant: String,
    ) -> Result<Pattern, ParseError> {
        if matches!(self.peek_kind(), TokenKind::LParen) {
            // Tuple パターン: Variant(x, y, ...)
            self.advance(); // consume '('
            let mut bindings: Vec<String> = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                let (binding, _) = self.expect_ident()?;
                bindings.push(binding);
                if matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect_token(&TokenKind::RParen)?;
            Ok(Pattern::EnumTuple {
                enum_name,
                variant,
                bindings,
            })
        } else if matches!(self.peek_kind(), TokenKind::LBrace) {
            // Struct パターン: Variant { x, y }
            self.advance(); // consume '{'
            let mut fields: Vec<String> = Vec::new();
            loop {
                self.skip_sep();
                if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                    break;
                }
                let (field, _) = self.expect_ident()?;
                fields.push(field);
                if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::Semicolon) {
                    self.advance();
                }
            }
            self.expect_token(&TokenKind::RBrace)?;
            Ok(Pattern::EnumStruct {
                enum_name,
                variant,
                fields,
            })
        } else {
            // Unit パターン
            Ok(Pattern::EnumUnit { enum_name, variant })
        }
    }
}

// ── ヘルパー関数 ─────────────────────────────────────────────────────────

/// 型名かどうかを判定（大文字から始まる識別子）
fn is_type_name(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

/// `use raw { ... }` のパース用: TokenKind を生文字列に変換する（M-6-B）
/// ブレースのネストを考慮して内部コードを再構成するために使用する
fn token_kind_to_raw_str(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!(" {}", s),
        TokenKind::Int(n) => format!(" {}", n),
        TokenKind::Float(f) => format!(" {}", f),
        TokenKind::Str(s) => format!(" \"{}\"", s),
        TokenKind::Plus => " +".to_string(),
        TokenKind::Minus => " -".to_string(),
        TokenKind::Star => " *".to_string(),
        TokenKind::Slash => "/".to_string(),
        TokenKind::Percent => " %".to_string(),
        TokenKind::Eq => " =".to_string(),
        TokenKind::EqEq => " ==".to_string(),
        TokenKind::BangEq => " !=".to_string(),
        TokenKind::Lt => " <".to_string(),
        TokenKind::Gt => " >".to_string(),
        TokenKind::LtEq => " <=".to_string(),
        TokenKind::GtEq => " >=".to_string(),
        TokenKind::And => " &&".to_string(),
        TokenKind::Or => " ||".to_string(),
        TokenKind::Bang => " !".to_string(),
        TokenKind::Dot => ".".to_string(),
        TokenKind::Comma => ",".to_string(),
        TokenKind::Colon => ":".to_string(),
        TokenKind::Semicolon => ";".to_string(),
        TokenKind::ThinArrow => " ->".to_string(),
        TokenKind::Arrow => " =>".to_string(),
        TokenKind::LParen => "(".to_string(),
        TokenKind::RParen => ")".to_string(),
        TokenKind::LBracket => "[".to_string(),
        TokenKind::RBracket => "]".to_string(),
        TokenKind::ColonColon => "::".to_string(),
        TokenKind::Question => "?".to_string(),
        TokenKind::DotDot => "..".to_string(),
        TokenKind::DotDotEq => "..=".to_string(),
        TokenKind::Let => " let".to_string(),
        TokenKind::Fn => " fn".to_string(),
        TokenKind::Return => " return".to_string(),
        TokenKind::If => " if".to_string(),
        TokenKind::Else => " else".to_string(),
        TokenKind::True => " true".to_string(),
        TokenKind::False => " false".to_string(),
        TokenKind::Use => " use".to_string(),
        TokenKind::Pub => " pub".to_string(),
        TokenKind::At => "@".to_string(),
        _ => " ".to_string(),
    }
}

// ── 公開ユーティリティ ────────────────────────────────────────────────────

pub fn parse_source(source: &str) -> Result<Module, ParseError> {
    let tokens = lex(source).map_err(|e| ParseError::UnexpectedEof {
        expected: format!("lex error: {}", e),
    })?;
    Parser::new(tokens).parse()
}

// ── テスト ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Module {
        parse_source(src).expect("parse failed")
    }

    fn first_stmt(src: &str) -> Stmt {
        parse(src).stmts.into_iter().next().expect("no stmts")
    }

    #[test]
    fn test_parser_stub_compiles() {
        let tokens = vec![Token {
            kind: TokenKind::Eof,
            span: Span {
                start: 0,
                end: 0,
                line: 1,
                col: 1,
            },
        }];
        let module = Parser::new(tokens).parse().expect("parse failed");
        assert_eq!(module.stmts.len(), 0);
    }

    // ── Phase 1-C tests ───────────────────────────────────────────────────

    #[test]
    fn test_parse_let() {
        match first_stmt("let x = 10") {
            Stmt::Let {
                name,
                value: Expr::Literal(Literal::Int(10), _),
                ..
            } => {
                assert_eq!(name, "x");
            }
            other => panic!("expected Let, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_state() {
        match first_stmt("state count = 0") {
            Stmt::State {
                name,
                value: Expr::Literal(Literal::Int(0), _),
                ..
            } => {
                assert_eq!(name, "count");
            }
            other => panic!("expected State, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_const() {
        match first_stmt("const MAX = 100") {
            Stmt::Const {
                name,
                value: Expr::Literal(Literal::Int(100), _),
                ..
            } => {
                assert_eq!(name, "MAX");
            }
            other => panic!("expected Const, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_fn() {
        match first_stmt("fn add(a: number, b: number) -> number { a + b }") {
            Stmt::Fn {
                name,
                type_params,
                params,
                return_type: Some(TypeAnn::Number),
                ..
            } => {
                assert_eq!(name, "add");
                assert!(type_params.is_empty());
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, "a");
                assert_eq!(params[1].name, "b");
            }
            other => panic!("expected Fn, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_if_expr() {
        let module = parse(r#"let r = if x > 0 { "pos" } else { "neg" }"#);
        match &module.stmts[0] {
            Stmt::Let {
                value: Expr::If { .. },
                ..
            } => {}
            other => panic!("expected If expr, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_while() {
        match first_stmt("while i < 10 { i = i + 1 }") {
            Stmt::Expr(Expr::While { .. }) => {}
            other => panic!("expected While, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_for() {
        match first_stmt("for x in items { print(x) }") {
            Stmt::Expr(Expr::For { var, .. }) => {
                assert_eq!(var, "x");
            }
            other => panic!("expected For, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_match() {
        match first_stmt("match x { some(v) => v, none => 0 }") {
            Stmt::Expr(Expr::Match { arms, .. }) => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[0].pattern, Pattern::Some(_)));
                assert!(matches!(arms[1].pattern, Pattern::None));
            }
            other => panic!("expected Match, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_closure_single() {
        match first_stmt("let f = x => x * 2") {
            Stmt::Let {
                value: Expr::Closure { params, .. },
                ..
            } => {
                assert_eq!(params, vec!["x".to_string()]);
            }
            other => panic!("expected single-param closure, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_closure_multi_arg() {
        match first_stmt("let f = (a, b) => a + b") {
            Stmt::Let {
                value: Expr::Closure { params, .. },
                ..
            } => {
                assert_eq!(params, vec!["a".to_string(), "b".to_string()]);
            }
            other => panic!("expected multi-arg closure, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_closure_no_arg() {
        match first_stmt(r#"let f = () => print("hi")"#) {
            Stmt::Let {
                value: Expr::Closure { params, .. },
                ..
            } => {
                assert!(params.is_empty());
            }
            other => panic!("expected no-arg closure, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_closure_block() {
        match first_stmt("let f = x => { let y = x * 2; y + 1 }") {
            Stmt::Let {
                value: Expr::Closure { params, body, .. },
                ..
            } => {
                assert_eq!(params, vec!["x".to_string()]);
                assert!(matches!(*body, Expr::Block { .. }));
            }
            other => panic!("expected block closure, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_method_call() {
        match first_stmt("items.map(x => x * 2)") {
            Stmt::Expr(Expr::MethodCall { method, args, .. }) => {
                assert_eq!(method, "map");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Closure { .. }));
            }
            other => panic!("expected MethodCall, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_use_method_call() {
        match first_stmt("app.use(logger())") {
            Stmt::Expr(Expr::MethodCall { method, .. }) => {
                assert_eq!(method, "use");
            }
            other => panic!("expected MethodCall, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_question_op() {
        match first_stmt("parse(s)?") {
            Stmt::Expr(Expr::Question(inner, _)) => {
                assert!(matches!(*inner, Expr::Call { .. }));
            }
            other => panic!("expected Question, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_string_interpolation() {
        match first_stmt(r#"let s = "Hello, {name}!""#) {
            Stmt::Let {
                value: Expr::Interpolation { parts, .. },
                ..
            } => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(&parts[0], InterpPart::Literal(s) if s == "Hello, "));
                assert!(matches!(&parts[1], InterpPart::Expr(_)));
                assert!(matches!(&parts[2], InterpPart::Literal(s) if s == "!"));
            }
            other => panic!("expected Interpolation, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_range() {
        match first_stmt("let r = [1..=10]") {
            Stmt::Let {
                value: Expr::Range {
                    inclusive: true, ..
                },
                ..
            } => {}
            other => panic!("expected Range inclusive, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_list_literal() {
        match first_stmt("let xs = [1, 2, 3]") {
            Stmt::Let {
                value: Expr::List(items, _),
                ..
            } => {
                assert_eq!(items.len(), 3);
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_operator_precedence() {
        // 1 + 2 * 3  →  1 + (2 * 3)
        match first_stmt("let r = 1 + 2 * 3") {
            Stmt::Let {
                value:
                    Expr::BinOp {
                        op: BinOp::Add,
                        right,
                        ..
                    },
                ..
            } => {
                assert!(matches!(*right, Expr::BinOp { op: BinOp::Mul, .. }));
            }
            other => panic!("expected Add(_, Mul), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann() {
        match first_stmt("let x: number? = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Option(inner)),
                ..
            } => {
                assert!(matches!(*inner, TypeAnn::Number));
            }
            other => panic!("expected Let with Option(Number) type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann_map() {
        match first_stmt("let x: map<string, number> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Map(key, value)),
                ..
            } => {
                assert_eq!(*key, TypeAnn::String);
                assert_eq!(*value, TypeAnn::Number);
            }
            other => panic!(
                "expected Let with Map(String, Number) type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_type_ann_set() {
        match first_stmt("let x: set<string> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Set(inner)),
                ..
            } => {
                assert_eq!(*inner, TypeAnn::String);
            }
            other => panic!("expected Let with Set(String) type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann_ordered_map() {
        match first_stmt("let x: ordered_map<string, number> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::OrderedMap(key, value)),
                ..
            } => {
                assert_eq!(*key, TypeAnn::String);
                assert_eq!(*value, TypeAnn::Number);
            }
            other => panic!(
                "expected Let with OrderedMap(String, Number) type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_type_ann_ordered_set() {
        match first_stmt("let x: ordered_set<string> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::OrderedSet(inner)),
                ..
            } => {
                assert_eq!(*inner, TypeAnn::String);
            }
            other => panic!("expected Let with OrderedSet(String) type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann_unit() {
        match first_stmt("let x: () = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Unit),
                ..
            } => {}
            other => panic!("expected Let with Unit type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann_generic_single() {
        match first_stmt("let x: Response<string> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Generic { name, args }),
                ..
            } => {
                assert_eq!(name, "Response");
                assert_eq!(args, vec![TypeAnn::String]);
            }
            other => panic!(
                "expected Let with Generic(Response<String>) type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_type_ann_generic_multi() {
        match first_stmt("let x: Pair<string, number> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Generic { name, args }),
                ..
            } => {
                assert_eq!(name, "Pair");
                assert_eq!(args, vec![TypeAnn::String, TypeAnn::Number]);
            }
            other => panic!(
                "expected Let with Generic(Pair<String, Number>) type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_type_ann_generic_nested() {
        match first_stmt("let x: Response<list<string>> = none") {
            Stmt::Let {
                type_ann: Some(TypeAnn::Generic { name, args }),
                ..
            } => {
                assert_eq!(name, "Response");
                assert_eq!(args, vec![TypeAnn::List(Box::new(TypeAnn::String))]);
            }
            other => panic!(
                "expected Let with Generic(Response<List<String>>) type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_type_ann_fn() {
        match first_stmt("let f: string => bool = none") {
            Stmt::Let {
                type_ann:
                    Some(TypeAnn::Fn {
                        params,
                        return_type,
                    }),
                ..
            } => {
                assert_eq!(params, vec![TypeAnn::String]);
                assert_eq!(*return_type, TypeAnn::Bool);
            }
            other => panic!("expected Let with Fn(String => Bool) type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_type_ann_fn_multi_param() {
        match first_stmt("let f: fn(string, number) -> bool = none") {
            Stmt::Let {
                type_ann:
                    Some(TypeAnn::Fn {
                        params,
                        return_type,
                    }),
                ..
            } => {
                assert_eq!(params, vec![TypeAnn::String, TypeAnn::Number]);
                assert_eq!(*return_type, TypeAnn::Bool);
            }
            other => panic!(
                "expected Let with fn(String, Number) -> Bool type, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_parse_map_literal() {
        match first_stmt(r#"{"a": 1, "b": 2}"#) {
            Stmt::Expr(Expr::MapLiteral { pairs, .. }) => {
                assert_eq!(pairs.len(), 2);
            }
            other => panic!("expected MapLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_set_literal() {
        match first_stmt(r#"{"rust", "forge"}"#) {
            Stmt::Expr(Expr::SetLiteral { items, .. }) => {
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected SetLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_index_assign() {
        match first_stmt(r#"m["a"] = 1"#) {
            Stmt::Expr(Expr::IndexAssign { .. }) => {}
            other => panic!("expected IndexAssign, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_error_unexpected_token() {
        // `let = 10` — let の後に識別子が必要
        let tokens = lex("let = 10").expect("lex failed");
        let result = Parser::new(tokens).parse();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::UnexpectedToken { .. }
        ));
    }

    // ── Phase T-1 tests ───────────────────────────────────────────────────

    #[test]
    fn test_parse_struct_def() {
        match first_stmt("struct Point { x: number, y: number }") {
            Stmt::StructDef {
                name,
                generic_params,
                fields,
                derives,
                ..
            } => {
                assert_eq!(name, "Point");
                assert!(generic_params.is_empty());
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
                assert!(matches!(fields[0].1, TypeAnn::Number));
                assert!(derives.is_empty());
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_struct_derive() {
        let src = "@derive(Debug, Clone)\nstruct Point { x: number }";
        match first_stmt(src) {
            Stmt::StructDef { name, derives, .. } => {
                assert_eq!(name, "Point");
                assert_eq!(derives, vec!["Debug".to_string(), "Clone".to_string()]);
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_impl_block() {
        let src = "impl Rectangle { fn area() -> number { self.width * self.height } }";
        match first_stmt(src) {
            Stmt::ImplBlock {
                target,
                type_params,
                target_type_args,
                trait_name,
                methods,
                ..
            } => {
                assert_eq!(target, "Rectangle");
                assert!(type_params.is_empty());
                assert!(target_type_args.is_empty());
                assert!(trait_name.is_none());
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name, "area");
            }
            other => panic!("expected ImplBlock, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_struct_init() {
        match first_stmt("let p = Point { x: 1, y: 2 }") {
            Stmt::Let {
                value: Expr::StructInit { name, fields, .. },
                ..
            } => {
                assert_eq!(name, "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
            }
            other => panic!("expected StructInit, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_field_access() {
        match first_stmt("p.x") {
            Stmt::Expr(Expr::Field { field, .. }) => {
                assert_eq!(field, "x");
            }
            other => panic!("expected Field access, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_impl_state_self() {
        let src = "impl Counter { fn increment(state self) { self.count = self.count + 1 } }";
        match first_stmt(src) {
            Stmt::ImplBlock { methods, .. } => {
                assert_eq!(methods.len(), 1);
                assert!(methods[0].has_state_self);
            }
            other => panic!("expected ImplBlock, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_pub_struct_derive() {
        match first_stmt("pub @derive(Clone) struct Route { method: string }") {
            Stmt::StructDef {
                name,
                derives,
                is_pub,
                ..
            } => {
                assert_eq!(name, "Route");
                assert_eq!(derives, vec!["Clone".to_string()]);
                assert!(is_pub);
            }
            other => panic!("expected StructDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_struct_single() {
        match first_stmt("struct Response<T> { status: number, body: T }") {
            Stmt::StructDef {
                name,
                generic_params,
                fields,
                ..
            } => {
                assert_eq!(name, "Response");
                assert_eq!(generic_params, vec!["T".to_string()]);
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[1].0, "body");
                assert_eq!(fields[1].1, TypeAnn::Named("T".to_string()));
            }
            other => panic!("expected generic StructDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_struct_multi() {
        match first_stmt("struct Pair<A, B> { first: A, second: B }") {
            Stmt::StructDef {
                name,
                generic_params,
                fields,
                ..
            } => {
                assert_eq!(name, "Pair");
                assert_eq!(generic_params, vec!["A".to_string(), "B".to_string()]);
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].1, TypeAnn::Named("A".to_string()));
                assert_eq!(fields[1].1, TypeAnn::Named("B".to_string()));
            }
            other => panic!("expected generic StructDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_enum() {
        match first_stmt("enum Either<L, R> { Left(L), Right(R) }") {
            Stmt::EnumDef {
                name,
                generic_params,
                variants,
                ..
            } => {
                assert_eq!(name, "Either");
                assert_eq!(generic_params, vec!["L".to_string(), "R".to_string()]);
                assert_eq!(variants.len(), 2);
                assert_eq!(
                    variants[0],
                    EnumVariant::Tuple("Left".to_string(), vec![TypeAnn::Named("L".to_string())])
                );
                assert_eq!(
                    variants[1],
                    EnumVariant::Tuple("Right".to_string(), vec![TypeAnn::Named("R".to_string())])
                );
            }
            other => panic!("expected generic EnumDef, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_fn() {
        match first_stmt("fn wrap<T>(value: T) -> Response<T> { value }") {
            Stmt::Fn {
                name,
                type_params,
                params,
                return_type:
                    Some(TypeAnn::Generic {
                        name: ret_name,
                        args,
                    }),
                ..
            } => {
                assert_eq!(name, "wrap");
                assert_eq!(type_params, vec!["T".to_string()]);
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].type_ann, Some(TypeAnn::Named("T".to_string())));
                assert_eq!(ret_name, "Response");
                assert_eq!(args, vec![TypeAnn::Named("T".to_string())]);
            }
            other => panic!("expected generic Fn, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic_impl() {
        let src = "impl<T> Response<T> { fn is_ok(self) -> bool { true } }";
        match first_stmt(src) {
            Stmt::ImplBlock {
                target,
                type_params,
                target_type_args,
                methods,
                ..
            } => {
                assert_eq!(target, "Response");
                assert_eq!(type_params, vec!["T".to_string()]);
                assert_eq!(target_type_args, vec![TypeAnn::Named("T".to_string())]);
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].name, "is_ok");
            }
            other => panic!("expected generic ImplBlock, got {:?}", other),
        }
    }

    // ── Phase FT-1 tests ──────────────────────────────────────────────────

    #[test]
    fn test_parse_test_block() {
        let src = r#"test "add works" { let x = 1 }"#;
        match first_stmt(src) {
            Stmt::TestBlock { name, body, .. } => {
                assert_eq!(name, "add works");
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected TestBlock, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_test_block_empty() {
        let src = r#"test "empty" { }"#;
        match first_stmt(src) {
            Stmt::TestBlock { name, body, .. } => {
                assert_eq!(name, "empty");
                assert_eq!(body.len(), 0);
            }
            other => panic!("expected TestBlock, got {:?}", other),
        }
    }
}
