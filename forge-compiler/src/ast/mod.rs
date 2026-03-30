// forge-compiler: AST ノード定義
// Phase 1-B で全ノードを実装する
// 仕様: forge/spec_v0.0.1.md §3〜§9

use crate::lexer::Span;

/// モジュール（ファイル全体）
#[derive(Debug, Clone)]
pub struct Module {
    pub stmts: Vec<Stmt>,
}

/// 文
#[derive(Debug, Clone)]
pub enum Stmt {
    /// let x: T = expr
    Let {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Expr,
        span: Span,
    },
    /// state x: T = expr
    State {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Expr,
        span: Span,
    },
    /// const NAME: T = expr
    Const {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Expr,
        span: Span,
    },
    /// fn name(params) -> T { body }
    Fn {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Box<Expr>,
        span: Span,
    },
    /// return expr
    Return(Option<Expr>, Span),
    /// 式文
    Expr(Expr),
}

/// 式
#[derive(Debug, Clone)]
pub enum Expr {
    /// リテラル値
    Literal(Literal, Span),
    /// 識別子
    Ident(String, Span),
    /// 二項演算
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    /// 単項演算
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// if 式
    If {
        cond: Box<Expr>,
        then_block: Box<Expr>,
        else_block: Option<Box<Expr>>,
        span: Span,
    },
    /// while 文
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },
    /// for 式
    For {
        var: String,
        iter: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },
    /// match 式
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// ブロック式 { stmts... expr? }
    Block {
        stmts: Vec<Stmt>,
        tail: Option<Box<Expr>>,
        span: Span,
    },
    /// 関数呼び出し f(args)
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// メソッド呼び出し obj.method(args)
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// フィールドアクセス obj.field
    Field {
        object: Box<Expr>,
        field: String,
        span: Span,
    },
    /// インデックスアクセス list[n]
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// クロージャ x => expr  /  (x, y) => expr  /  () => expr
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
        span: Span,
    },
    /// 文字列補間 "Hello, {name}!"
    Interpolation {
        parts: Vec<InterpPart>,
        span: Span,
    },
    /// 範囲 1..=10 / 0..10
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        span: Span,
    },
    /// リストリテラル [1, 2, 3]
    List(Vec<Expr>, Span),
    /// ? 演算子
    Question(Box<Expr>, Span),
    /// 再代入（state のみ） x = expr
    Assign {
        name: String,
        value: Box<Expr>,
        span: Span,
    },
}

/// 文字列補間パーツ
#[derive(Debug, Clone)]
pub enum InterpPart {
    Literal(String),
    Expr(Box<Expr>),
}

/// match アーム
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

/// パターン
#[derive(Debug, Clone)]
pub enum Pattern {
    /// リテラル: 42 / "hello" / true
    Literal(Literal),
    /// ワイルドカード: _
    Wildcard,
    /// 識別子バインディング: x
    Ident(String),
    /// some(x)
    Some(Box<Pattern>),
    /// none
    None,
    /// ok(x)
    Ok(Box<Pattern>),
    /// err(x)
    Err(Box<Pattern>),
    /// 範囲パターン: 1..=10
    Range { start: Literal, end: Literal, inclusive: bool },
}

/// リテラル値
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

/// 型注釈
#[derive(Debug, Clone)]
pub enum TypeAnn {
    Number,
    Float,
    String,
    Bool,
    Option(Box<TypeAnn>),         // T?
    Result(Box<TypeAnn>),         // T!
    ResultWith(Box<TypeAnn>, Box<TypeAnn>), // T![E]
    List(Box<TypeAnn>),           // list<T>
    Named(String),                // ユーザー定義型（Phase 5 以降）
}

/// 関数パラメータ
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub span: Span,
}

/// 二項演算子
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
}

/// 単項演算子
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,  // -x
    Not,  // !x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_stub_compiles() {
        let _module = Module { stmts: vec![] };
    }
}
