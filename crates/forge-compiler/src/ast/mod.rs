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

/// 組み込みデコレータ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decorator {
    Derive(String),
    Service,
    Repository,
    On { event_type: String },
    Timed { metric: String },
    Validate { target: String, using: String },
}

/// 分割代入パターン (E2-1)
#[derive(Debug, Clone)]
pub enum Pat {
    /// 単純な変数束縛: x
    Ident(String),
    /// ワイルドカード: _
    Wildcard,
    /// タプル風: (a, b, c)
    Tuple(Vec<Pat>),
    /// リストパターン: [a, b, c] （Tuple と同義）
    List(Vec<Pat>),
    /// 残余パターン: ..name
    Rest(String),
}

/// 文
#[derive(Debug, Clone)]
pub enum Stmt {
    /// let x: T = expr  /  let (a, b): T = expr
    Let {
        pat: Pat,
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
        defer_cleanup: Option<String>,
        annotations: Vec<String>,
        decorators: Vec<Decorator>,
        span: Span,
    },
    /// system name(params) { body } — Ember ECS system declaration
    System {
        name: String,
        params: Vec<Param>,
        body: Box<Expr>,
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
        decorators: Vec<Decorator>,
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
    /// app AppName { load ..., provide ..., container { ... }, wire ... }
    App {
        name: String,
        loads: Vec<String>,
        provides: Vec<ProvideDecl>,
        container: Option<Vec<Binding>>,
        wires: Vec<WireDecl>,
        span: Span,
    },
    /// job JobName { input ..., run { ... } }
    Job {
        name: String,
        inputs: Vec<JobInput>,
        body: Box<Expr>,
        span: Span,
    },
    /// run JobName { key: expr, ... }
    RunJob {
        name: String,
        args: Vec<(String, Expr)>,
        span: Span,
    },
    /// event EventName { field: Type, ... }
    Event {
        name: String,
        fields: Vec<(String, TypeAnn)>,
        span: Span,
    },
    /// emit EventName { key: expr, ... }
    Emit {
        event_name: String,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
    /// defer expr / defer { block } （E-7）
    Defer { body: DeferBody, span: Span },
    /// container { bind Trait to Impl }
    ContainerDef { bindings: Vec<Binding>, span: Span },
}

impl Stmt {
    pub fn span(&self) -> &Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::State { span, .. }
            | Stmt::Const { span, .. }
            | Stmt::Fn { span, .. }
            | Stmt::System { span, .. }
            | Stmt::Yield { span, .. }
            | Stmt::StructDef { span, .. }
            | Stmt::ImplBlock { span, .. }
            | Stmt::EnumDef { span, .. }
            | Stmt::TraitDef { span, .. }
            | Stmt::MixinDef { span, .. }
            | Stmt::ImplTrait { span, .. }
            | Stmt::DataDef { span, .. }
            | Stmt::TypestateDef { span, .. }
            | Stmt::UseDecl { span, .. }
            | Stmt::UseRaw { span, .. }
            | Stmt::When { span, .. }
            | Stmt::TestBlock { span, .. }
            | Stmt::App { span, .. }
            | Stmt::Job { span, .. }
            | Stmt::RunJob { span, .. }
            | Stmt::Event { span, .. }
            | Stmt::Emit { span, .. }
            | Stmt::Defer { span, .. }
            | Stmt::ContainerDef { span, .. } => span,
            Stmt::Return(_, span) => span,
            Stmt::Expr(expr) => expr.span(),
        }
    }
}

/// defer の本体種別（E-7）
#[derive(Debug, Clone)]
pub enum DeferBody {
    /// defer expr
    Expr(Box<Expr>),
    /// defer { ... }
    Block(Box<Expr>),
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

#[derive(Debug, Clone)]
pub struct Binding {
    pub trait_name: String,
    pub implementation: Expr,
}

#[derive(Debug, Clone)]
pub struct ProvideDecl {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WireDecl {
    pub job_name: String,
    pub bindings: Vec<(String, String)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct JobInput {
    pub name: String,
    pub type_ann: TypeAnn,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobInputSource {
    Cli,
    Di,
}

impl TypeAnn {
    pub fn is_job_cli_input_type(&self) -> bool {
        match self {
            TypeAnn::Number | TypeAnn::Float | TypeAnn::String | TypeAnn::Bool => true,
            TypeAnn::Option(inner) => inner.is_job_cli_input_type(),
            _ => false,
        }
    }
}

impl JobInput {
    pub fn source(&self) -> JobInputSource {
        if self.type_ann.is_job_cli_input_type() {
            JobInputSource::Cli
        } else {
            JobInputSource::Di
        }
    }
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
    /// loop 文 (break で脱出可能)
    Loop { body: Box<Expr>, span: Span },
    /// break 文 (loop の脱出)
    Break { span: Span },
    /// for 式
    For {
        pat: Pat,
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
    AnonStruct {
        fields: Vec<(String, Option<Expr>)>,
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
    /// pipeline { source ... filter ... map ... sink ... } （S-5-B）
    Pipeline {
        steps: Vec<PipelineStep>,
        span: Span,
    },
    /// container { bind Trait to Impl }
    ContainerLiteral { bindings: Vec<Binding>, span: Span },
}

impl Expr {
    pub fn span(&self) -> &Span {
        match self {
            Expr::Literal(_, span)
            | Expr::Ident(_, span)
            | Expr::Loop { span, .. }
            | Expr::Break { span }
            | Expr::List(_, span)
            | Expr::Question(_, span)
            | Expr::Await { span, .. }
            | Expr::SetLiteral { span, .. }
            | Expr::Interpolation { span, .. }
            | Expr::BinOp { span, .. }
            | Expr::UnaryOp { span, .. }
            | Expr::If { span, .. }
            | Expr::While { span, .. }
            | Expr::For { span, .. }
            | Expr::Match { span, .. }
            | Expr::Block { span, .. }
            | Expr::Call { span, .. }
            | Expr::MethodCall { span, .. }
            | Expr::Field { span, .. }
            | Expr::Index { span, .. }
            | Expr::Closure { span, .. }
            | Expr::Range { span, .. }
            | Expr::MapLiteral { span, .. }
            | Expr::Assign { span, .. }
            | Expr::IndexAssign { span, .. }
            | Expr::StructInit { span, .. }
            | Expr::AnonStruct { span, .. }
            | Expr::EnumInit { span, .. }
            | Expr::FieldAssign { span, .. }
            | Expr::OptionalChain { span, .. }
            | Expr::NullCoalesce { span, .. }
            | Expr::Spawn { span, .. }
            | Expr::Pipeline { span, .. }
            | Expr::ContainerLiteral { span, .. } => span,
        }
    }
}

/// pipeline ステップ（S-5-B）
#[derive(Debug, Clone)]
pub enum PipelineStep {
    Source(Box<Expr>),
    Filter(Box<Expr>),
    Map(Box<Expr>),
    FlatMap(Box<Expr>),
    Group(Box<Expr>),
    Sort { key: Box<Expr>, descending: bool },
    Take(Box<Expr>),
    Skip(Box<Expr>),
    Each(Box<Expr>),
    Sink(Box<Expr>),
    Parallel(Box<Expr>),
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
    AnonStruct(Vec<(String, TypeAnn)>),
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
    use crate::lexer::Span;

    #[test]
    fn test_ast_stub_compiles() {
        let _module = Module { stmts: vec![] };
    }

    #[test]
    fn test_job_input_source_cli_and_di() {
        let span = Span {
            file: "<test>".to_string(),
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        };
        let cli_input = JobInput {
            name: "path".to_string(),
            type_ann: TypeAnn::String,
            default: None,
            span: span.clone(),
        };
        let optional_cli_input = JobInput {
            name: "since".to_string(),
            type_ann: TypeAnn::Option(Box::new(TypeAnn::String)),
            default: None,
            span: span.clone(),
        };
        let di_input = JobInput {
            name: "notifier".to_string(),
            type_ann: TypeAnn::Named("Notifier".to_string()),
            default: None,
            span,
        };

        assert_eq!(cli_input.source(), JobInputSource::Cli);
        assert_eq!(optional_cli_input.source(), JobInputSource::Cli);
        assert_eq!(di_input.source(), JobInputSource::Di);
    }

    #[test]
    fn test_app_ast_nodes_compile() {
        let span = Span {
            file: "<test>".to_string(),
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        };
        let stmt = Stmt::App {
            name: "Production".to_string(),
            loads: vec!["jobs/*".to_string()],
            provides: vec![ProvideDecl {
                name: "db".to_string(),
                value: Expr::Ident("connect".to_string(), span.clone()),
                span: span.clone(),
            }],
            container: Some(vec![Binding {
                trait_name: "Notifier".to_string(),
                implementation: Expr::Ident("SlackNotifier".to_string(), span.clone()),
            }]),
            wires: vec![WireDecl {
                job_name: "ImportUsers".to_string(),
                bindings: vec![("notifier".to_string(), "Notifier".to_string())],
                span: span.clone(),
            }],
            span: span.clone(),
        };

        assert_eq!(stmt.span(), &span);
    }

    #[test]
    fn test_event_emit_ast_nodes_compile() {
        let span = Span {
            file: "<test>".to_string(),
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        };
        let event_stmt = Stmt::Event {
            name: "RowInvalid".to_string(),
            fields: vec![
                ("row".to_string(), TypeAnn::Number),
                ("field".to_string(), TypeAnn::String),
                ("message".to_string(), TypeAnn::String),
            ],
            span: span.clone(),
        };
        assert_eq!(event_stmt.span(), &span);

        let emit_stmt = Stmt::Emit {
            event_name: "RowInvalid".to_string(),
            fields: vec![
                ("row".to_string(), Expr::Literal(Literal::Int(42), span.clone())),
                ("field".to_string(), Expr::Literal(Literal::String("email".to_string()), span.clone())),
            ],
            span: span.clone(),
        };
        assert_eq!(emit_stmt.span(), &span);
    }
}
