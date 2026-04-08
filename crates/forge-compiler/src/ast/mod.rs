// forge-compiler: AST ノード定義
// Phase 1-B で全ノードを実装する
// 仕様: forge/spec_v0.0.1.md §3〜§9

use crate::lexer::Span;

/// モジュール（ファイル全体）
#[derive(Debug, Clone)]
pub struct Module {
    pub stmts: Vec<Stmt>,
}

/// when 条件の種類（M-5-A）
#[derive(Debug, Clone, PartialEq)]
pub enum WhenCondition {
    /// when platform.linux / platform.windows / platform.macos
    Platform(String),
    /// when feature.debug / feature.release
    Feature(String),
    /// when env.dev / env.prod / env.test
    Env(String),
    /// when test
    Test,
    /// when not <condition>
    Not(Box<WhenCondition>),
}

/// use パスの種類
#[derive(Debug, Clone, PartialEq)]
pub enum UsePath {
    /// `./utils/helper` — ./ で始まるローカルファイル
    Local(String),
    /// `serde` — 裸の識別子（外部クレート）
    External(String),
    /// `forge/std/io` — forge/std/ で始まる標準ライブラリ
    Stdlib(String),
}

/// use でインポートするシンボルの指定方法
#[derive(Debug, Clone, PartialEq)]
pub enum UseSymbols {
    /// `.add` — 単一シンボル（エイリアスあり）
    Single(String, Option<String>),
    /// `.{add, subtract as sub}` — 複数シンボル（(名前, エイリアス) のリスト）
    Multiple(Vec<(String, Option<String>)>),
    /// `.*` — 全シンボル
    All,
}

/// 文
#[derive(Debug, Clone)]
pub enum Stmt {
    /// let x: T = expr
    Let {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Expr,
        is_pub: bool,
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
        is_pub: bool,
        span: Span,
    },
    /// fn name(params) -> T { body }
    Fn {
        name: String,
        type_params: Vec<String>,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Box<Expr>,
        is_pub: bool,
        is_const: bool,
        span: Span,
    },
    /// return expr
    Return(Option<Expr>, Span),
    /// yield expr
    Yield { value: Box<Expr>, span: Span },
    /// 式文
    Expr(Expr),
    /// struct Name { field: Type, ... }
    StructDef {
        name: String,
        generic_params: Vec<String>,
        fields: Vec<(String, TypeAnn)>,
        derives: Vec<String>,
        is_pub: bool,
        span: Span,
    },
    /// impl Name { fn ... }  /  impl Trait for Name { fn ... }
    ImplBlock {
        target: String,
        type_params: Vec<String>,
        target_type_args: Vec<TypeAnn>,
        trait_name: Option<String>,
        methods: Vec<FnDef>,
        operators: Vec<OperatorDef>,
        span: Span,
    },
    /// enum Name { Variant, Variant(Type), Variant { field: Type } }
    EnumDef {
        name: String,
        generic_params: Vec<String>,
        variants: Vec<EnumVariant>,
        derives: Vec<String>,
        is_pub: bool,
        span: Span,
    },
    /// trait Name { fn abstract() -> T \n fn default() { body } }
    TraitDef {
        name: String,
        methods: Vec<TraitMethod>,
        is_pub: bool,
        span: Span,
    },
    /// mixin Name { fn method() { body } }
    MixinDef {
        name: String,
        methods: Vec<FnDef>,
        is_pub: bool,
        span: Span,
    },
    /// impl TraitName for TypeName { fn ... }  または  impl MixinName for TypeName
    ImplTrait {
        trait_name: String,
        target: String,
        methods: Vec<FnDef>,
        span: Span,
    },
    /// data Name { field: Type, ... } validate { ... }
    DataDef {
        name: String,
        generic_params: Vec<String>,
        fields: Vec<(String, TypeAnn)>,
        validate_rules: Vec<ValidateRule>,
        is_pub: bool,
        span: Span,
    },
    /// typestate Name { fields..., states: [...], StateName { fn ... } ..., any { fn ... } }
    TypestateDef {
        name: String,
        fields: Vec<(String, TypeAnn)>,
        states: Vec<TypestateMarker>,
        state_methods: Vec<TypestateState>,
        any_methods: Vec<FnDef>,
        any_block_count: usize,
        derives: Vec<String>,
        generic_params: Vec<String>,
        span: Span,
    },
    /// use ./path/module.symbol [as alias]
    UseDecl {
        path: UsePath,
        symbols: UseSymbols,
        is_pub: bool,
        span: Span,
    },
    /// use raw { ... } — 生 Rust コードの埋め込み（M-6）
    /// `forge run` ではスキップ、`forge build` 時のみ有効
    UseRaw { rust_code: String, span: Span },
    /// when platform.linux { ... } / when feature.debug { ... } / when test { ... } （M-5）
    When {
        condition: WhenCondition,
        body: Vec<Stmt>,
        span: Span,
    },
    /// test "テスト名" { ... } （FT-1）
    TestBlock {
        name: String,
        body: Vec<Stmt>,
        span: Span,
    },
}

/// typestate の各状態定義（状態名とその状態で使えるメソッド）
#[derive(Debug, Clone)]
pub struct TypestateState {
    pub name: String,
    pub methods: Vec<FnDef>,
}

/// typestate の状態マーカー宣言
#[derive(Debug, Clone)]
pub enum TypestateMarker {
    Unit(String),
    Tuple(String, Vec<TypeAnn>),
    Struct(String, Vec<(String, TypeAnn)>),
}

impl TypestateMarker {
    pub fn name(&self) -> &str {
        match self {
            TypestateMarker::Unit(name)
            | TypestateMarker::Tuple(name, _)
            | TypestateMarker::Struct(name, _) => name,
        }
    }
}

/// バリデーションルール: フィールドに対する制約の集合
#[derive(Debug, Clone)]
pub struct ValidateRule {
    pub field: String,
    pub constraints: Vec<Constraint>,
}

/// バリデーション制約の種類
#[derive(Debug, Clone)]
pub enum Constraint {
    /// 文字列長チェック: length(3..20) / length(min: 8) / length(max: 20)
    Length {
        min: Option<usize>,
        max: Option<usize>,
    },
    /// 英数字のみ
    Alphanumeric,
    /// メールフォーマット（@と.を含む簡易チェック）
    EmailFormat,
    /// URLフォーマット
    UrlFormat,
    /// 数値範囲チェック: range(0..150) / range(min: 0)
    Range { min: Option<f64>, max: Option<f64> },
    /// 数字を1文字以上含む
    ContainsDigit,
    /// 大文字を1文字以上含む
    ContainsUppercase,
    /// 小文字を1文字以上含む
    ContainsLowercase,
    /// 空文字列でない
    NotEmpty,
    /// 正規表現マッチ
    Matches(String),
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
    Interpolation { parts: Vec<InterpPart>, span: Span },
    /// 範囲 1..=10 / 0..10
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        span: Span,
    },
    /// リストリテラル [1, 2, 3]
    List(Vec<Expr>, Span),
    /// マップリテラル { key: value, ... }
    MapLiteral {
        pairs: Vec<(Expr, Expr)>,
        span: Span,
    },
    /// セットリテラル { value1, value2, ... }
    SetLiteral { items: Vec<Expr>, span: Span },
    /// ? 演算子
    Question(Box<Expr>, Span),
    /// .await
    Await { expr: Box<Expr>, span: Span },
    /// 再代入（state のみ） x = expr
    Assign {
        name: String,
        value: Box<Expr>,
        span: Span,
    },
    /// インデックス代入（state map のみ） obj[key] = value
    IndexAssign {
        object: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },
    /// struct インスタンス化 Name { field: expr, ... }
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
    /// enum バリアントのインスタンス化 EnumName::Variant / EnumName::Variant(expr) / EnumName::Variant { field: expr }
    EnumInit {
        enum_name: String,
        variant: String,
        data: EnumInitData,
        span: Span,
    },
    /// フィールドへの代入 (state self) self.field = expr
    FieldAssign {
        object: Box<Expr>,
        field: String,
        value: Box<Expr>,
        span: Span,
    },
    /// オプショナルチェーン / null 合体
    OptionalChain {
        object: Box<Expr>,
        chain: ChainKind,
        span: Span,
    },
    NullCoalesce {
        value: Box<Expr>,
        default: Box<Expr>,
        span: Span,
    },
    /// spawn { ... }
    Spawn { body: Box<Expr>, span: Span },
}

/// オプショナルチェーンの種類
#[derive(Debug, Clone)]
pub enum ChainKind {
    Field(String),
    Method { name: String, args: Vec<Expr> },
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
    Range {
        start: Literal,
        end: Literal,
        inclusive: bool,
    },
    /// enum Unit バリアント: Direction::North または単に North
    EnumUnit {
        enum_name: Option<String>,
        variant: String,
    },
    /// enum Tuple バリアント: Shape::Circle(r) または Circle(r)
    EnumTuple {
        enum_name: Option<String>,
        variant: String,
        bindings: Vec<String>,
    },
    /// enum Struct バリアント: Message::Move { x, y } または Move { x, y }
    EnumStruct {
        enum_name: Option<String>,
        variant: String,
        fields: Vec<String>,
    },
}

/// enum バリアント定義
#[derive(Debug, Clone, PartialEq)]
pub enum EnumVariant {
    /// データなし: North
    Unit(String),
    /// タプル: Circle(number)
    Tuple(String, Vec<TypeAnn>),
    /// 名前付きフィールド: Move { x: number, y: number }
    Struct(String, Vec<(String, TypeAnn)>),
}

/// enum インスタンス化時のデータ
#[derive(Debug, Clone)]
pub enum EnumInitData {
    /// データなし
    None,
    /// タプル: Circle(3)
    Tuple(Vec<Expr>),
    /// 名前付き: Move { x: 1, y: 2 }
    Struct(Vec<(String, Expr)>),
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
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnn {
    Number,
    Float,
    String,
    Bool,
    Option(Box<TypeAnn>),                   // T?
    Result(Box<TypeAnn>),                   // T!
    ResultWith(Box<TypeAnn>, Box<TypeAnn>), // T![E]
    List(Box<TypeAnn>),                     // list<T>
    Named(String),                          // ユーザー定義型（Phase 5 以降）
    // G-1-A: ジェネリクス拡張
    Generic {
        name: String,
        args: Vec<TypeAnn>,
    }, // Response<T>, Pair<A, B>
    Map(Box<TypeAnn>, Box<TypeAnn>),        // map<K, V>
    Set(Box<TypeAnn>),                      // set<T>
    OrderedMap(Box<TypeAnn>, Box<TypeAnn>), // ordered_map<K, V>
    OrderedSet(Box<TypeAnn>),               // ordered_set<T>
    Unit,                                   // ()
    Fn {
        params: Vec<TypeAnn>,
        return_type: Box<TypeAnn>,
    }, // T => U
    /// generate<T>
    Generate(Box<TypeAnn>),
    /// Pick/Omit の Keys 引数: "id" | "name" | "email" 形式
    StringLiteralUnion(Vec<String>),
}

/// 関数パラメータ
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub span: Span,
}

/// trait メソッド（抽象 or デフォルト実装）
#[derive(Debug, Clone)]
pub enum TraitMethod {
    /// 抽象メソッド（実装必須）: fn name(params) -> T
    Abstract {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        has_self: bool,
        span: Span,
    },
    /// デフォルト実装: fn name(params) -> T { body }
    Default {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Box<Expr>,
        has_self: bool,
        has_state_self: bool,
        span: Span,
    },
}

/// Operator definitions parsed inside impl blocks
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OperatorKind {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Lt,
    Index,
    Neg,
}

/// Operator definition node
#[derive(Debug, Clone)]
pub struct OperatorDef {
    pub op: OperatorKind,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnn>,
    pub body: Box<Expr>,
    pub has_self: bool,
    pub has_state_self: bool,
    pub span: Span,
}

/// impl ブロック内のメソッド定義
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnn>,
    pub body: Box<Expr>,
    pub has_self: bool,       // `self` または `state self` で宣言されたか
    pub has_state_self: bool, // `state self` で宣言されたか
    pub is_const: bool,
    pub span: Span,
}

/// 二項演算子
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

/// 単項演算子
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg, // -x
    Not, // !x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_stub_compiles() {
        let _module = Module { stmts: vec![] };
    }
}
