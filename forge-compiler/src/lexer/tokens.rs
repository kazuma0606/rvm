// forge-compiler: トークン定義
// Phase 1-A で全トークンを実装する
// 仕様: forge/spec_v0.0.1.md §1

/// ソース上の位置情報
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

/// トークンの種類（Phase 1-A で全種類を追加）
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── リテラル ──────────────────────────────
    Int(i64),
    Float(f64),
    Str(String),
    StrInterp(Vec<StrPart>),  // "Hello, {name}!"
    True,
    False,

    // ── キーワード ────────────────────────────
    Let,
    State,
    Const,
    Fn,
    Return,
    If,
    Else,
    For,
    In,
    While,
    Match,
    None,
    Some,
    Ok,
    Err,
    // ── 型定義キーワード (Phase T-1 以降) ────
    Struct,
    Impl,
    SelfVal,    // self (小文字)
    SelfType,   // Self (大文字)
    Trait,
    Mixin,
    Data,
    Typestate,
    // ── モジュールキーワード (Phase M-0) ─────
    Use,        // use
    Pub,        // pub
    As,         // as
    When,       // when
    // ── テストキーワード (Phase FT-1) ─────
    Test,       // test
    // ── アノテーション ────────────────────────
    At,         // @

    // ── 識別子 ────────────────────────────────
    Ident(String),

    // ── 算術演算子 ────────────────────────────
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // ── 比較演算子 ────────────────────────────
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,

    // ── 論理演算子 ────────────────────────────
    And,   // &&
    Or,    // ||
    Bang,  // !

    // ── 代入・型注釈 ─────────────────────────
    Eq,          // =
    Colon,       // :
    ThinArrow,   // ->
    Arrow,       // =>
    Question,    // ?
    ColonColon,  // ::

    // ── 範囲 ─────────────────────────────────
    DotDot,      // ..
    DotDotEq,    // ..=
    Dot,         // .

    // ── 区切り文字 ────────────────────────────
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,

    // ── 特殊 ─────────────────────────────────
    Eof,
}

/// 文字列補間の各パーツ
#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Literal(String),
    Expr(String),  // 補間式（Phase 1-A ではソース文字列のまま保持）
}

/// トークン（種類 + 位置情報）
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
