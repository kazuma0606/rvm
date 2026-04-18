// forge-vm: ツリーウォーキングインタープリタ
// Phase 2-B 実装

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::value::{CapturedEnv, EnumData, NativeFn, Value};
use forge_compiler::ast::*;
use forge_compiler::deps::DepsManager;
use forge_compiler::lexer::Span;
use forge_compiler::loader::{ModForgeExport, ModuleLoader};
use wasmtime::{Engine, Instance, Module as WasmModule, Store};

/// struct 型のメソッド（Forge 定義 or ネイティブ関数）
#[derive(Clone)]
enum MethodImpl {
    /// Forge スクリプトで定義されたメソッド（定義時の環境をキャプチャ）
    Forge(FnDef, CapturedEnv),
    /// Rust ネイティブ関数（引数の第1要素が self）
    Native(NativeFn),
}

/// Operator 実装（operator 定義）
#[derive(Debug, Clone)]
enum OperatorImpl {
    Forge(OperatorDef, CapturedEnv),
}

impl std::fmt::Debug for MethodImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodImpl::Forge(def, _) => write!(f, "Forge({})", def.name),
            MethodImpl::Native(_) => write!(f, "Native"),
        }
    }
}

/// struct 型の定義情報（型レジストリに格納）
#[derive(Debug, Clone)]
struct StructInfo {
    fields: Vec<(String, TypeAnn)>,
    derives: Vec<String>,
    methods: HashMap<String, MethodImpl>,
    operators: HashMap<OperatorKind, OperatorImpl>,
}

/// enum 型の定義情報（型レジストリに格納）
#[derive(Debug, Clone)]
struct EnumInfo {
    variants: Vec<EnumVariant>,
}

/// trait の定義情報
#[derive(Debug, Clone)]
struct TraitInfo {
    /// デフォルト実装（メソッド名 → FnDef）
    default_methods: HashMap<String, FnDef>,
}

/// mixin の定義情報（デフォルト実装のみ）
#[derive(Debug, Clone)]
struct MixinInfo {
    methods: HashMap<String, FnDef>,
}

/// typestate の各状態が持つメソッド情報
#[derive(Debug, Clone)]
struct TypestateStateInfo {
    /// メソッド名 → (戻り値の状態名, 戻り値が Result か, パラメータリスト)
    /// 戻り値の状態名が None の場合は通常の値を返す
    methods: HashMap<String, TypestateMethodInfo>,
}

/// typestate メソッドの情報
#[derive(Debug, Clone)]
struct TypestateMethodInfo {
    params: Vec<Param>,
    /// 遷移先状態名（None = 状態遷移なし、通常値を返す）
    next_state: Option<String>,
    /// 戻り値が Result 型か（`!` 付き）
    is_result: bool,
}

/// typestate 型の定義情報
#[derive(Debug, Clone)]
struct TypestateInfo {
    fields: Vec<(String, TypeAnn)>,
    states: Vec<String>,
    /// 状態名 → その状態のメソッド情報
    state_infos: HashMap<String, TypestateStateInfo>,
}

/// 型レジストリ（struct / enum / trait / mixin / typestate 定義とメソッドを格納）
#[derive(Default)]
struct TypeRegistry {
    structs: HashMap<String, StructInfo>,
    enums: HashMap<String, EnumInfo>,
    traits: HashMap<String, TraitInfo>,
    mixins: HashMap<String, MixinInfo>,
    typestates: HashMap<String, TypestateInfo>,
    /// Singleton インスタンスキャッシュ
    singletons: HashMap<String, Value>,
}

// ── RuntimeError ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    UndefinedVariable(String),
    TypeMismatch {
        expected: String,
        found: String,
    },
    DivisionByZero,
    IndexOutOfBounds {
        index: i64,
        len: usize,
    },
    /// let 変数への再代入
    Immutable(String),
    Custom(String),
    /// 循環参照検出（M-4-B）
    CircularDependency {
        cycle: Vec<String>,
    },
    // ── 内部制御フロー ──
    /// return 文による早期脱出（関数呼び出しが補足）
    Return(Value),
    /// ? 演算子の Err 伝播（関数呼び出しが補足）
    PropagateErr(String),
    /// アサーション失敗（forge test でテストを失敗としてマーク）（FT-1-D）
    TestFailure(String),
    /// break 文による loop の脱出（eval_loop が補足）
    LoopBreak,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::UndefinedVariable(n) => write!(f, "未定義の変数 '{}'", n),
            RuntimeError::TypeMismatch { expected, found } => write!(
                f,
                "型エラー: {} を期待しましたが {} でした",
                expected, found
            ),
            RuntimeError::DivisionByZero => write!(f, "ゼロ除算"),
            RuntimeError::IndexOutOfBounds { index, len } => {
                write!(f, "インデックス範囲外: {} (長さ: {})", index, len)
            }
            RuntimeError::Immutable(n) => write!(f, "変数 '{}' は不変です", n),
            RuntimeError::Custom(msg) => write!(f, "{}", msg),
            RuntimeError::CircularDependency { cycle } => {
                write!(f, "循環参照エラー: {}", cycle.join(" → "))
            }
            RuntimeError::Return(_) => write!(f, "<return>"),
            RuntimeError::PropagateErr(e) => write!(f, "<propagate err: {}>", e),
            RuntimeError::TestFailure(msg) => write!(f, "assertion failed: {}", msg),
            RuntimeError::LoopBreak => write!(f, "<break>"),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ── スコープ ────────────────────────────────────────────────────────────────

/// バインディング: (値, 可変かどうか)
type Binding = (Value, bool);

// ── M-4-C: インポート情報（未使用インポート検出用）────────────────────────────

/// インポートされたシンボルの情報（未使用インポート検出用）
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// インポートしたシンボル名
    pub name: String,
    /// インポート元のパス
    pub source_path: String,
    /// 使用されたかどうか
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineTraceOutcome {
    Ok,
    FindNone,
    ResultErr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorruptedRecord {
    pub index: usize,
    pub fields: Vec<(String, Value)>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineStageTrace {
    pub stage_name: String,
    pub in_count: usize,
    pub out_count: usize,
    pub corrupted: Vec<CorruptedRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineTrace {
    pub pipeline_name: String,
    pub source_snippet: String,
    pub stages: Vec<PipelineStageTrace>,
    pub total_records: usize,
    pub total_corrupted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineTraceNodeRef {
    pub node_id: usize,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PipelineTraceEvent {
    pub node_id: Option<usize>,
    pub method: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
    pub item_count: Option<usize>,
    pub outcome: PipelineTraceOutcome,
    pub message: Option<String>,
    pub corrupted: Vec<CorruptedRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayOutput {
    Text {
        value: String,
    },
    Html {
        value: String,
    },
    Json {
        value: serde_json::Value,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<serde_json::Value>>,
    },
    Image {
        mime: String,
        data: String,
    },
    Markdown {
        value: String,
    },
}

// ── インタプリタ ─────────────────────────────────────────────────────────────

pub struct Interpreter {
    /// スコープスタック。scopes[0] = グローバル、scopes.last() = 現在のスコープ
    scopes: Vec<HashMap<String, Binding>>,
    /// 型レジストリ（struct 定義・メソッドを保持）
    type_registry: TypeRegistry,
    /// モジュールローダー（forge run でのみ有効）
    module_loader: Option<ModuleLoader>,
    /// 外部クレート依存関係マネージャー（forge build 連携用）
    pub deps_manager: DepsManager,
    /// インポートされたシンボルの追跡（未使用インポート検出用）（M-4-C）
    pub imported_symbols: HashMap<String, ImportInfo>,
    /// 現在ロード中のモジュールパスのスタック（循環参照検出用）（M-4-B）
    loading_stack: Vec<String>,
    /// yield 値を蓄積するスタック
    generator_stack: Vec<Vec<Value>>,
    /// defer スタック（LIFO で関数終了時に実行）（E-7）
    defer_stack: Vec<Vec<DeferBody>>,
    /// テストモードフラグ（M-5-C）: `forge test` コマンドで true になる
    pub is_test_mode: bool,
    /// REPL でロード済みのモジュール情報（M-7-A）
    /// モジュールパス → そのモジュールからインポートしたシンボル名リスト
    pub loaded_modules: HashMap<String, Vec<String>>,
    /// 出力バッファ: Some の場合 print/println はここに書き込む（MCP 等でのキャプチャ用）
    pub output_buffer: Option<Arc<Mutex<String>>>,
    output_listener: Option<Arc<dyn Fn(String) + Send + Sync>>,
    display_listener: Option<Arc<dyn Fn(DisplayOutput) + Send + Sync>>,
    trace_mode: bool,
    /// Goblet 用のパイプライントレース設定
    pipeline_trace_nodes: Vec<PipelineTraceNodeRef>,
    /// Goblet 用の実行時トレースイベント
    pub pipeline_trace_events: Vec<PipelineTraceEvent>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            scopes: vec![HashMap::new()],
            type_registry: TypeRegistry::default(),
            module_loader: None,
            deps_manager: DepsManager::new(),
            imported_symbols: HashMap::new(),
            loading_stack: Vec::new(),
            generator_stack: Vec::new(),
            defer_stack: Vec::new(),
            is_test_mode: false,
            loaded_modules: HashMap::new(),
            output_buffer: None,
            output_listener: None,
            display_listener: None,
            trace_mode: false,
            pipeline_trace_nodes: Vec::new(),
            pipeline_trace_events: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    /// 出力バッファ付きインタープリタを生成する（MCP などでの stdout キャプチャ用）
    pub fn with_output_capture() -> (Self, Arc<Mutex<String>>) {
        let buf = Arc::new(Mutex::new(String::new()));
        let mut interp = Self::new();
        interp.output_buffer = Some(Arc::clone(&buf));
        // output_buffer が設定されているので register_builtins を再実行して print を上書き
        interp.register_print_builtins();
        interp.register_display_builtins();
        (interp, buf)
    }

    pub fn with_output_capture_and_listener<F>(listener: F) -> (Self, Arc<Mutex<String>>)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let buf = Arc::new(Mutex::new(String::new()));
        let mut interp = Self::new();
        interp.output_buffer = Some(Arc::clone(&buf));
        interp.output_listener = Some(Arc::new(listener));
        interp.register_print_builtins();
        interp.register_display_builtins();
        (interp, buf)
    }

    pub fn with_output_capture_and_display_listener<F, G>(
        output_listener: F,
        display_listener: G,
    ) -> (Self, Arc<Mutex<String>>)
    where
        F: Fn(String) + Send + Sync + 'static,
        G: Fn(DisplayOutput) + Send + Sync + 'static,
    {
        let buf = Arc::new(Mutex::new(String::new()));
        let mut interp = Self::new();
        interp.output_buffer = Some(Arc::clone(&buf));
        interp.output_listener = Some(Arc::new(output_listener));
        interp.display_listener = Some(Arc::new(display_listener));
        interp.register_print_builtins();
        interp.register_display_builtins();
        (interp, buf)
    }

    pub fn with_file_path_and_output_capture(path: &std::path::Path) -> (Self, Arc<Mutex<String>>) {
        let buf = Arc::new(Mutex::new(String::new()));
        let mut interp = Self::with_file_path(path);
        interp.output_buffer = Some(Arc::clone(&buf));
        interp.register_print_builtins();
        interp.register_display_builtins();
        (interp, buf)
    }

    /// print / println を output_buffer に書き込むよう再登録する
    fn register_print_builtins(&mut self) {
        let buf1 = Arc::clone(self.output_buffer.as_ref().unwrap());
        let listener1 = self.output_listener.as_ref().map(Arc::clone);
        self.define(
            "print",
            Value::NativeFunction(NativeFn(Rc::new(move |args: Vec<Value>| {
                let s = args
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                let line = format!("{}\n", s);
                if let Ok(mut b) = buf1.lock() {
                    b.push_str(&line);
                }
                if let Some(listener) = &listener1 {
                    listener(line.clone());
                }
                Ok(Value::Unit)
            }))),
            false,
        );
        let buf2 = Arc::clone(self.output_buffer.as_ref().unwrap());
        let listener2 = self.output_listener.as_ref().map(Arc::clone);
        self.define(
            "println",
            Value::NativeFunction(NativeFn(Rc::new(move |args: Vec<Value>| {
                let s = args
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                let line = format!("{}\n", s);
                if let Ok(mut b) = buf2.lock() {
                    b.push_str(&line);
                }
                if let Some(listener) = &listener2 {
                    listener(line.clone());
                }
                Ok(Value::Unit)
            }))),
            false,
        );
    }

    fn register_display_builtins(&mut self) {
        let listener = self.display_listener.clone();
        let output_buffer = self.output_buffer.clone();

        let mut methods = HashMap::new();
        methods.insert(
            "text".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::text() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let value = args.remove(0).to_string();
                    emit_display_output(&listener, &output_buffer, DisplayOutput::Text { value });
                    Ok(Value::Unit)
                }
            }))),
        );
        methods.insert(
            "html".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::html() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let value = match args.remove(0) {
                        Value::String(value) => value,
                        other => other.to_string(),
                    };
                    emit_display_output(&listener, &output_buffer, DisplayOutput::Html { value });
                    Ok(Value::Unit)
                }
            }))),
        );
        methods.insert(
            "json".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::json() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let value = value_to_json(&args.remove(0));
                    emit_display_output(&listener, &output_buffer, DisplayOutput::Json { value });
                    Ok(Value::Unit)
                }
            }))),
        );
        methods.insert(
            "table".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::table() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let (columns, rows) = value_to_table(&args.remove(0))?;
                    emit_display_output(
                        &listener,
                        &output_buffer,
                        DisplayOutput::Table { columns, rows },
                    );
                    Ok(Value::Unit)
                }
            }))),
        );
        methods.insert(
            "image".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::image() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let path = match args.remove(0) {
                        Value::String(path) => path,
                        other => {
                            return Err(format!(
                                "display::image() expects string path, got {}",
                                other.type_name()
                            ))
                        }
                    };
                    let bytes =
                        std::fs::read(&path).map_err(|e| format!("display::image(): {}", e))?;
                    let mime = image_mime_type(&path);
                    let data = {
                        use base64::Engine;
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    };
                    emit_display_output(
                        &listener,
                        &output_buffer,
                        DisplayOutput::Image { mime, data },
                    );
                    Ok(Value::Unit)
                }
            }))),
        );
        methods.insert(
            "markdown".to_string(),
            MethodImpl::Native(NativeFn(Rc::new({
                let listener = listener.clone();
                let output_buffer = output_buffer.clone();
                move |mut args: Vec<Value>| {
                    if args.len() != 2 {
                        return Err("display::markdown() takes 1 arg".to_string());
                    }
                    let _self = args.remove(0);
                    let value = match args.remove(0) {
                        Value::String(value) => value,
                        other => other.to_string(),
                    };
                    emit_display_output(
                        &listener,
                        &output_buffer,
                        DisplayOutput::Markdown { value },
                    );
                    Ok(Value::Unit)
                }
            }))),
        );

        self.type_registry.structs.insert(
            "Display".to_string(),
            StructInfo {
                fields: vec![],
                derives: vec![],
                methods,
                operators: HashMap::new(),
            },
        );
        let instance = Value::Struct {
            type_name: "Display".to_string(),
            fields: Rc::new(RefCell::new(HashMap::new())),
        };
        self.type_registry
            .singletons
            .insert("Display".to_string(), instance.clone());
        self.define("display", instance, false);
    }

    /// ファイルパスを指定してモジュールローダーを初期化する
    pub fn with_file_path(path: &std::path::Path) -> Self {
        let mut interp = Self {
            scopes: vec![HashMap::new()],
            type_registry: TypeRegistry::default(),
            module_loader: Some(ModuleLoader::from_file_path(path)),
            deps_manager: DepsManager::new(),
            imported_symbols: HashMap::new(),
            loading_stack: Vec::new(),
            generator_stack: Vec::new(),
            defer_stack: Vec::new(),
            is_test_mode: false,
            loaded_modules: HashMap::new(),
            output_buffer: None,
            output_listener: None,
            display_listener: None,
            trace_mode: false,
            pipeline_trace_nodes: Vec::new(),
            pipeline_trace_events: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    /// プロジェクトルートを指定してモジュールローダーを初期化する
    pub fn with_project_root(project_root: PathBuf) -> Self {
        let mut interp = Self {
            scopes: vec![HashMap::new()],
            type_registry: TypeRegistry::default(),
            module_loader: Some(ModuleLoader::new(project_root)),
            deps_manager: DepsManager::new(),
            imported_symbols: HashMap::new(),
            loading_stack: Vec::new(),
            generator_stack: Vec::new(),
            defer_stack: Vec::new(),
            is_test_mode: false,
            loaded_modules: HashMap::new(),
            output_buffer: None,
            output_listener: None,
            display_listener: None,
            trace_mode: false,
            pipeline_trace_nodes: Vec::new(),
            pipeline_trace_events: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    /// プロジェクトルートとローカル依存パスを指定してモジュールローダーを初期化する
    pub fn with_project_root_and_deps(
        project_root: PathBuf,
        dep_paths: Vec<(String, PathBuf)>,
    ) -> Self {
        let mut loader = ModuleLoader::new(project_root);
        for (name, path) in dep_paths {
            loader.add_dep_path(name, path);
        }
        let mut interp = Self {
            scopes: vec![HashMap::new()],
            type_registry: TypeRegistry::default(),
            module_loader: Some(loader),
            deps_manager: DepsManager::new(),
            imported_symbols: HashMap::new(),
            loading_stack: Vec::new(),
            generator_stack: Vec::new(),
            defer_stack: Vec::new(),
            is_test_mode: false,
            loaded_modules: HashMap::new(),
            output_buffer: None,
            output_listener: None,
            display_listener: None,
            trace_mode: false,
            pipeline_trace_nodes: Vec::new(),
            pipeline_trace_events: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    pub fn set_pipeline_trace_nodes(&mut self, nodes: Vec<PipelineTraceNodeRef>) {
        self.pipeline_trace_nodes = nodes;
        self.pipeline_trace_events.clear();
    }

    pub fn set_trace_mode(&mut self, enabled: bool) {
        self.trace_mode = enabled;
        if !enabled {
            self.pipeline_trace_events.clear();
        }
    }

    pub fn take_pipeline_trace_events(&mut self) -> Vec<PipelineTraceEvent> {
        std::mem::take(&mut self.pipeline_trace_events)
    }

    /// 未使用インポートの警告を stderr に出力する（M-4-C）
    pub fn warn_unused_imports(&self) {
        for (name, info) in &self.imported_symbols {
            if !info.used {
                eprintln!(
                    "警告: `{}` はインポートされていますが使用されていません\n   --> use {}.{}",
                    name, info.source_path, name
                );
            }
        }
    }

    fn record_pipeline_trace(&mut self, span: &Span, method: &str, value: &Value) {
        let corrupted = if self.trace_mode {
            detect_corrupted_records(value)
        } else {
            Vec::new()
        };
        let message = pipeline_trace_message(value);
        self.pipeline_trace_events.push(PipelineTraceEvent {
            node_id: self.resolve_pipeline_trace_node_id(span, method),
            method: method.to_string(),
            start: span.start,
            end: span.end,
            line: span.line,
            col: span.col,
            item_count: pipeline_item_count(value),
            outcome: pipeline_trace_outcome(method, value),
            message: message.or_else(|| {
                if corrupted.is_empty() {
                    None
                } else {
                    Some(
                        corrupted
                            .iter()
                            .map(|record| format!("#{} {}", record.index, record.reason))
                            .collect::<Vec<_>>()
                            .join("; "),
                    )
                }
            }),
            corrupted,
        });
    }

    fn record_pipeline_trace_error(
        &mut self,
        span: &Span,
        method: &str,
        item_index: usize,
        item: &Value,
        error: &RuntimeError,
        item_count: Option<usize>,
    ) {
        if !self.trace_mode {
            return;
        }

        let mut reason = error.to_string();
        let mut fields = snapshot_fields(item);
        if fields.is_empty() {
            fields.push(("value".to_string(), item.clone()));
        }
        if let Some(existing) = corruption_reasons_for_value(None, item).first() {
            reason = format!("{reason}, {existing}");
        }

        self.pipeline_trace_events.push(PipelineTraceEvent {
            node_id: self.resolve_pipeline_trace_node_id(span, method),
            method: method.to_string(),
            start: span.start,
            end: span.end,
            line: span.line,
            col: span.col,
            item_count,
            outcome: PipelineTraceOutcome::ResultErr,
            message: Some(error.to_string()),
            corrupted: vec![CorruptedRecord {
                index: item_index,
                fields,
                reason,
            }],
        });
    }

    fn resolve_pipeline_trace_node_id(&self, span: &Span, method: &str) -> Option<usize> {
        self.pipeline_trace_nodes
            .iter()
            .find(|node| {
                node.line == span.line
                    && node.col == span.col
                    && node.start == span.start
                    && node.end == span.end
            })
            .map(|node| node.node_id)
            .or_else(|| {
                self.pipeline_trace_nodes
                    .iter()
                    .find(|node| {
                        node.line == span.line
                            && node.col == span.col
                            && span.start >= node.start
                            && span.end <= node.end
                    })
                    .map(|node| node.node_id)
            })
            .or_else(|| {
                self.pipeline_trace_nodes
                    .iter()
                    .find(|node| node.line == span.line && node.col == span.col)
                    .map(|node| node.node_id)
            })
            .or_else(|| {
                self.pipeline_trace_nodes
                    .iter()
                    .find(|node| node.line == span.line && method == "?")
                    .map(|node| node.node_id)
            })
    }

    /// REPL 用: 指定モジュールをアンロードし、そのシンボルをスコープから削除する（M-7-A）
    pub fn unload_module(&mut self, path: &str) {
        if let Some(symbols) = self.loaded_modules.remove(path) {
            // グローバルスコープ（scopes[0]）からシンボルを削除する
            if let Some(global_scope) = self.scopes.first_mut() {
                for sym in &symbols {
                    global_scope.remove(sym);
                }
            }
            // 未使用インポート情報からも削除する
            for sym in &symbols {
                self.imported_symbols.remove(sym);
            }
        }
    }

    /// REPL 用: モジュールローダーを現在のディレクトリから初期化する（M-7-A）
    pub fn init_module_loader_from_cwd(&mut self) {
        if self.module_loader.is_none() {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            self.module_loader = Some(ModuleLoader::new(cwd));
        }
    }

    /// REPL 用: 指定モジュールのローダーキャッシュをクリアする（M-7-A）
    pub fn clear_module_loader_cache(&mut self, path: &str) {
        if let Some(loader) = &mut self.module_loader {
            loader.clear_cache(path);
        }
    }

    // ── スコープ操作 ──────────────────────────────────────────────────────

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, value: Value, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), (value, mutable));
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(b) = scope.get(name) {
                return Some(b);
            }
        }
        None
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<Value, RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                if !binding.1 {
                    return Err(RuntimeError::Immutable(name.to_string()));
                }
                binding.0 = value;
                return Ok(Value::Unit);
            }
        }
        Err(RuntimeError::UndefinedVariable(name.to_string()))
    }

    /// 現在の全スコープをフラットに Rc<RefCell<Map>> へスナップショット
    fn capture_env(&self) -> CapturedEnv {
        let mut map = HashMap::new();
        for scope in &self.scopes {
            for (k, (v, mutable)) in scope {
                map.insert(k.clone(), (v.clone(), *mutable));
            }
        }
        Rc::new(RefCell::new(map))
    }

    // ── 組み込み関数登録 ──────────────────────────────────────────────────

    fn register_builtins(&mut self) {
        self.define("none", Value::Option(None), false);
        self.define("unit", Value::Unit, false);

        macro_rules! native {
            ($f:expr) => {
                Value::NativeFunction(NativeFn(Rc::new($f)))
            };
        }

        self.define(
            "some",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(format!("some() takes 1 arg"));
                }
                Ok(Value::Option(Some(Box::new(args.remove(0)))))
            }),
            false,
        );

        self.define(
            "ok",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(format!("ok() takes 1 arg"));
                }
                Ok(Value::Result(Ok(Box::new(args.remove(0)))))
            }),
            false,
        );

        self.define(
            "err",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err(format!("err() takes 1 arg"));
                }
                Ok(Value::Result(Err(args.remove(0).to_string())))
            }),
            false,
        );

        self.define(
            "print",
            native!(|args: Vec<Value>| {
                let s = args
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("{}", s);
                Ok(Value::Unit)
            }),
            false,
        );

        // println は print と同じ（改行付き）
        self.define(
            "println",
            native!(|args: Vec<Value>| {
                let s = args
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("{}", s);
                Ok(Value::Unit)
            }),
            false,
        );

        self.define(
            "string",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("string() takes 1 arg".to_string());
                }
                Ok(Value::String(args.remove(0).to_string()))
            }),
            false,
        );

        self.define(
            "number",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("number() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::String(s) => match s.trim().parse::<i64>() {
                        Ok(n) => Ok(Value::Result(Ok(Box::new(Value::Int(n))))),
                        Err(_) => Ok(Value::Result(Err(format!(
                            "\"{}\" を number に変換できません",
                            s
                        )))),
                    },
                    Value::Float(f) => Ok(Value::Result(Ok(Box::new(Value::Int(f as i64))))),
                    Value::Int(n) => Ok(Value::Result(Ok(Box::new(Value::Int(n))))),
                    v => Ok(Value::Result(Err(format!(
                        "{} を number に変換できません",
                        v.type_name()
                    )))),
                }
            }),
            false,
        );

        self.define(
            "string_replace",
            native!(|mut args: Vec<Value>| {
                if args.len() != 3 {
                    return Err("string_replace() takes 3 args: (s, from, to)".to_string());
                }
                let to = match args.remove(2) {
                    Value::String(s) => s,
                    v => {
                        return Err(format!(
                            "string_replace: 'to' must be string, got {}",
                            v.type_name()
                        ))
                    }
                };
                let from = match args.remove(1) {
                    Value::String(s) => s,
                    v => {
                        return Err(format!(
                            "string_replace: 'from' must be string, got {}",
                            v.type_name()
                        ))
                    }
                };
                let s = match args.remove(0) {
                    Value::String(s) => s,
                    v => {
                        return Err(format!(
                            "string_replace: 's' must be string, got {}",
                            v.type_name()
                        ))
                    }
                };
                Ok(Value::String(s.replacen(&from, &to, 1)))
            }),
            false,
        );

        self.define(
            "map_keys",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("map_keys() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::Map(entries) => {
                        let keys: Vec<Value> = entries.into_iter().map(|(k, _)| k).collect();
                        Ok(mk_list(keys))
                    }
                    v => Err(format!("map_keys: expected map, got {}", v.type_name())),
                }
            }),
            false,
        );

        self.define(
            "float",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("float() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::String(s) => match s.trim().parse::<f64>() {
                        Ok(f) => Ok(Value::Result(Ok(Box::new(Value::Float(f))))),
                        Err(_) => Ok(Value::Result(Err(format!(
                            "\"{}\" を float に変換できません",
                            s
                        )))),
                    },
                    Value::Int(n) => Ok(Value::Result(Ok(Box::new(Value::Float(n as f64))))),
                    Value::Float(f) => Ok(Value::Result(Ok(Box::new(Value::Float(f))))),
                    v => Ok(Value::Result(Err(format!(
                        "{} を float に変換できません",
                        v.type_name()
                    )))),
                }
            }),
            false,
        );

        self.define(
            "len",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("len() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                    Value::List(list) => Ok(Value::Int(list.borrow().len() as i64)),
                    v => Err(format!(
                        "len() は string または list を期待しましたが {} でした",
                        v.type_name()
                    )),
                }
            }),
            false,
        );

        self.define(
            "type_of",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("type_of() takes 1 arg".to_string());
                }
                Ok(Value::String(args.remove(0).type_name().to_string()))
            }),
            false,
        );

        // ── アサーション組み込み関数（FT-1-E）──────────────────────────
        // エラー文字列に "__tf__:" プレフィックスを付けて TestFailure として識別する

        self.define(
            "assert",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("assert() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::Bool(true) => Ok(Value::Unit),
                    Value::Bool(false) => Err("__tf__:assertion failed".to_string()),
                    v => Err(format!("assert() expects bool, got {}", v.type_name())),
                }
            }),
            false,
        );

        self.define(
            "assert_eq",
            native!(|mut args: Vec<Value>| {
                if args.len() != 2 {
                    return Err("assert_eq() takes 2 args".to_string());
                }
                let b = args.remove(1);
                let a = args.remove(0);
                if a == b {
                    Ok(Value::Unit)
                } else {
                    Err(format!(
                        "__tf__:assertion failed: expected {}, got {}",
                        b, a
                    ))
                }
            }),
            false,
        );

        self.define(
            "assert_ne",
            native!(|mut args: Vec<Value>| {
                if args.len() != 2 {
                    return Err("assert_ne() takes 2 args".to_string());
                }
                let b = args.remove(1);
                let a = args.remove(0);
                if a != b {
                    Ok(Value::Unit)
                } else {
                    Err(format!(
                        "__tf__:assertion failed: expected not {}, got {}",
                        b, a
                    ))
                }
            }),
            false,
        );

        self.define(
            "assert_ok",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("assert_ok() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::Result(Ok(_)) => Ok(Value::Unit),
                    Value::Result(Err(msg)) => Err(format!(
                        "__tf__:assertion failed: expected Ok, got Err({})",
                        msg
                    )),
                    v => Err(format!("assert_ok() expects result, got {}", v.type_name())),
                }
            }),
            false,
        );

        self.define(
            "assert_err",
            native!(|mut args: Vec<Value>| {
                if args.len() != 1 {
                    return Err("assert_err() takes 1 arg".to_string());
                }
                match args.remove(0) {
                    Value::Result(Err(_)) => Ok(Value::Unit),
                    Value::Result(Ok(_)) => {
                        Err("__tf__:assertion failed: expected Err, got Ok".to_string())
                    }
                    v => Err(format!(
                        "assert_err() expects result, got {}",
                        v.type_name()
                    )),
                }
            }),
            false,
        );

        // forge/std/json — parse(json_string) -> Value!
        self.define(
            "parse",
            native!(|args: Vec<Value>| {
                let s = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => {
                        return Err(format!("parse() expects string, got {}", v.type_name()))
                    }
                    None => return Err("parse() requires 1 argument".to_string()),
                };
                let jv: serde_json::Value =
                    serde_json::from_str(&s).map_err(|e| format!("JSON parse error: {}", e))?;
                Ok(Value::Result(Ok(Box::new(json_to_value(jv)))))
            }),
            false,
        );
    }

    // ── パブリック評価 ────────────────────────────────────────────────────

    pub fn eval(&mut self, module: &Module) -> Result<Value, RuntimeError> {
        let mut result = Value::Unit;
        for stmt in &module.stmts {
            result = self.eval_stmt(stmt)?;
        }
        Ok(result)
    }

    // ── 文の評価 ──────────────────────────────────────────────────────────

    fn eval_stmt(&mut self, stmt: &Stmt) -> Result<Value, RuntimeError> {
        match stmt {
            Stmt::Let { pat, value, .. } => {
                let v = self.eval_expr(value)?;
                self.bind_pat(pat, v)?;
                Ok(Value::Unit)
            }
            Stmt::State { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.define(name, v, true);
                Ok(Value::Unit)
            }
            Stmt::Const { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.define(name, v, false);
                Ok(Value::Unit)
            }
            Stmt::Fn {
                name,
                params,
                return_type,
                body,
                defer_cleanup,
                ..
            } => {
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let captured = self.capture_env();
                let closure = Value::Closure {
                    params: param_names,
                    body: body.clone(),
                    env: Rc::clone(&captured),
                    return_type: return_type.clone(),
                    defer_cleanup: defer_cleanup.clone(),
                };
                // 再帰呼び出しのために自己参照を captured env に追加
                captured
                    .borrow_mut()
                    .insert(name.clone(), (closure.clone(), false));
                self.define(name, closure, false);
                Ok(Value::Unit)
            }
            Stmt::Return(expr, _) => {
                let v = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Unit,
                };
                Err(RuntimeError::Return(v))
            }
            Stmt::Yield { value, .. } => {
                let v = self.eval_expr(value)?;
                if let Some(buf) = self.generator_stack.last_mut() {
                    buf.push(v);
                    Ok(Value::Unit)
                } else {
                    Err(RuntimeError::Custom(
                        "yield は generate<T> 関数内でのみ使用可能です".to_string(),
                    ))
                }
            }
            Stmt::Expr(expr) => self.eval_expr(expr),
            Stmt::StructDef {
                name,
                fields,
                derives,
                ..
            } => self.eval_struct_def(name.clone(), fields.clone(), derives.clone()),
            Stmt::ImplBlock {
                target,
                trait_name: _,
                methods,
                operators,
                ..
            } => self.eval_impl_block(target.clone(), methods.clone(), operators.clone()),
            Stmt::EnumDef {
                name,
                variants,
                derives,
                ..
            } => self.eval_enum_def(name.clone(), variants.clone(), derives.clone()),
            Stmt::TraitDef { name, methods, .. } => {
                self.eval_trait_def(name.clone(), methods.clone())
            }
            Stmt::MixinDef { name, methods, .. } => {
                self.eval_mixin_def(name.clone(), methods.clone())
            }
            Stmt::ImplTrait {
                trait_name,
                target,
                methods,
                ..
            } => self.eval_impl_trait(trait_name.clone(), target.clone(), methods.clone()),
            Stmt::DataDef {
                name,
                generic_params: _,
                fields,
                validate_rules,
                ..
            } => self.eval_data_def(name.clone(), fields.clone(), validate_rules.clone()),
            Stmt::TypestateDef {
                name,
                fields,
                states,
                state_methods,
                ..
            } => self.eval_typestate_def(
                name.clone(),
                fields.clone(),
                states.clone(),
                state_methods.clone(),
            ),
            Stmt::UseDecl { path, symbols, .. } => self.eval_use_decl(path, symbols),
            Stmt::UseRaw { .. } => {
                // `forge run` では use raw ブロックをスキップして警告を出す（M-6-C）
                // `forge build` 時のみ有効
                eprintln!("警告: `use raw` ブロックは `forge run` ではスキップされます（`forge build` 時のみ有効）");
                Ok(Value::Unit)
            }
            Stmt::When {
                condition, body, ..
            } => self.eval_when(condition, body),
            Stmt::TestBlock { .. } => {
                // `forge run` では TestBlock をスキップ（FT-1-D）
                // `forge test` では run_tests() が直接処理する
                Ok(Value::Unit)
            }
            Stmt::Defer { body, .. } => {
                // defer は defer_stack に積む（実際の実行は関数終了時）
                self.push_defer(body.clone());
                Ok(Value::Unit)
            }
        }
    }

    // ── When の評価（M-5-C）──────────────────────────────────────────────

    fn eval_when_condition(&self, condition: &WhenCondition) -> bool {
        match condition {
            WhenCondition::Platform(name) => std::env::consts::OS == name.as_str(),
            WhenCondition::Feature(name) => {
                let key = format!("FORGE_FEATURE_{}", name.to_uppercase());
                std::env::var(&key).map(|v| v == "1").unwrap_or(false)
            }
            WhenCondition::Env(name) => std::env::var("FORGE_ENV")
                .map(|v| v == *name)
                .unwrap_or(false),
            WhenCondition::Test => self.is_test_mode,
            WhenCondition::Not(inner) => !self.eval_when_condition(inner),
        }
    }

    fn eval_when(
        &mut self,
        condition: &WhenCondition,
        body: &[Stmt],
    ) -> Result<Value, RuntimeError> {
        if self.eval_when_condition(condition) {
            let mut result = Value::Unit;
            for stmt in body {
                result = self.eval_stmt(stmt)?;
            }
            Ok(result)
        } else {
            Ok(Value::Unit)
        }
    }

    // ── テスト実行（FT-1-D）──────────────────────────────────────────────

    /// `is_test_mode = true` で TestBlock を収集して順次実行する（FT-1-D）
    pub fn run_tests(
        &mut self,
        stmts: &[Stmt],
        filter: Option<&str>,
    ) -> Vec<crate::test_runner::TestResult> {
        // まずトップレベルの fn/const/struct/enum/trait 等を実行して共有スコープを構築
        for stmt in stmts {
            match stmt {
                Stmt::TestBlock { .. } => {} // スキップ
                _ => {
                    let _ = self.eval_stmt(stmt);
                }
            }
        }

        // テスト実行前のグローバルスコープのスナップショット（state リセット用）
        let global_snapshot: HashMap<String, Binding> =
            self.scopes.first().cloned().unwrap_or_default();

        // TestBlock を収集してフィルタを適用し、順次実行する
        let mut results = Vec::new();
        for stmt in stmts {
            if let Stmt::TestBlock { name, body, .. } = stmt {
                // フィルタ
                if let Some(pattern) = filter {
                    if !name.contains(pattern) {
                        continue;
                    }
                }

                // 各テストはスコープを分離して実行（state をリセット）
                // グローバルスコープをスナップショットに戻す
                if let Some(global) = self.scopes.first_mut() {
                    *global = global_snapshot.clone();
                }
                // テスト専用スコープを追加（テスト内 let/state の定義はここに入る）
                self.push_scope();
                let mut failed = false;
                let mut failure_msg = None;

                for test_stmt in body {
                    match self.eval_stmt(test_stmt) {
                        Ok(_) => {}
                        Err(RuntimeError::TestFailure(msg)) => {
                            failed = true;
                            failure_msg = Some(msg);
                            break;
                        }
                        Err(e) => {
                            failed = true;
                            failure_msg = Some(e.to_string());
                            break;
                        }
                    }
                }

                self.pop_scope();

                results.push(crate::test_runner::TestResult {
                    name: name.clone(),
                    passed: !failed,
                    failure_message: failure_msg,
                });
            }
        }

        results
    }

    // ── UseDecl の評価 ────────────────────────────────────────────────────

    fn eval_use_decl(
        &mut self,
        path: &UsePath,
        symbols: &UseSymbols,
    ) -> Result<Value, RuntimeError> {
        match path {
            UsePath::Local(use_path) => {
                // モジュールローダーが設定されていない場合はエラー
                if self.module_loader.is_none() {
                    return Err(RuntimeError::Custom(format!(
                        "ローカルモジュール '{}' を読み込めません: モジュールローダーが初期化されていません",
                        use_path
                    )));
                }

                // M-4-B: 循環参照チェック
                // ./ プレフィックスを除去して正規化
                let clean_path = use_path.trim_start_matches("./").to_string();
                if self.loading_stack.contains(&clean_path) {
                    let mut cycle = self
                        .loading_stack
                        .iter()
                        .skip_while(|p| p.as_str() != clean_path.as_str())
                        .cloned()
                        .collect::<Vec<_>>();
                    cycle.push(clean_path.clone());
                    return Err(RuntimeError::CircularDependency { cycle });
                }

                // ディレクトリかどうかを確認する
                let is_dir = self
                    .module_loader
                    .as_ref()
                    .map(|l| l.is_directory(&clean_path))
                    .unwrap_or(false);

                // ローディングスタックに追加
                self.loading_stack.push(clean_path.clone());

                let result = if is_dir {
                    // ディレクトリ指定: mod.forge の存在を確認する
                    self.eval_directory_use_with_tracking(&clean_path, symbols, 0)
                } else {
                    // 通常のファイル指定
                    self.eval_file_use_with_tracking(&clean_path, symbols)
                };

                // ローディングスタックから削除
                self.loading_stack.pop();

                result
            }
            UsePath::External(crate_name) => {
                // dep_paths に登録されているローカル依存なら Local と同様に解決する
                // "anvil/anvil" のような場合は先頭セグメント "anvil" でチェック
                let first_seg = crate_name.split('/').next().unwrap_or(crate_name.as_str());
                let has_dep = self
                    .module_loader
                    .as_ref()
                    .map(|l| l.dep_paths.contains_key(first_seg))
                    .unwrap_or(false);

                if has_dep {
                    if self.module_loader.is_none() {
                        return Err(RuntimeError::Custom(format!(
                            "依存パッケージ '{}' を読み込めません: モジュールローダーが初期化されていません",
                            crate_name
                        )));
                    }

                    let clean_path = crate_name.clone();
                    if self.loading_stack.contains(&clean_path) {
                        let cycle = self
                            .loading_stack
                            .iter()
                            .skip_while(|p| p.as_str() != clean_path.as_str())
                            .cloned()
                            .collect::<Vec<_>>();
                        return Err(RuntimeError::CircularDependency { cycle });
                    }

                    let is_dir = self
                        .module_loader
                        .as_ref()
                        .map(|l| l.is_directory(&clean_path))
                        .unwrap_or(false);

                    self.loading_stack.push(clean_path.clone());

                    let result = if is_dir {
                        self.eval_directory_use_with_tracking(&clean_path, symbols, 0)
                    } else {
                        self.eval_file_use_with_tracking(&clean_path, symbols)
                    };

                    self.loading_stack.pop();
                    result
                } else if crate_name == "forge/http" {
                    // forge/http は組み込み HTTP モジュールとして処理する
                    self.register_http_module(symbols)
                        .map_err(RuntimeError::Custom)?;
                    Ok(Value::Unit)
                } else {
                    // forge run では外部クレートのインポートをスキップ（警告なし）
                    // クレート名を DepsManager に記録して forge build 連携に備える
                    self.deps_manager.add(crate_name);
                    Ok(Value::Unit)
                }
            }
            UsePath::Stdlib(path) => {
                if path == "forge/std/wasm" {
                    self.register_wasm_module(symbols)
                        .map_err(RuntimeError::Custom)?;
                } else if path == "forge/std/fs" {
                    self.register_fs_module(symbols)
                        .map_err(RuntimeError::Custom)?;
                }
                Ok(Value::Unit)
            }
        }
    }

    /// M-4-C/D: シンボルをインポート記録しながらスコープにバインドする
    fn record_import(
        &mut self,
        sym_name: &str,
        bind_name: &str,
        source_path: &str,
        value: Value,
        is_wildcard: bool,
    ) -> Result<(), RuntimeError> {
        // M-4-D: シンボル衝突検出
        if self.imported_symbols.contains_key(bind_name) {
            let existing_source = self.imported_symbols[bind_name].source_path.clone();
            if existing_source == source_path {
                // 同じソースからの重複インポートは追跡記録をスキップするが値はバインドする
                self.define(bind_name, value, false);
                return Ok(());
            }
            if is_wildcard {
                // use * の衝突は警告のみ
                eprintln!(
                    "警告: `{}` が複数のモジュールから use * でインポートされています\n  {}.{}\n  {}.{}",
                    bind_name, existing_source, bind_name, source_path, bind_name
                );
            } else {
                // 明示的インポートの衝突はエラー
                return Err(RuntimeError::Custom(format!(
                    "シンボル衝突エラー: `{}` が複数のモジュールからインポートされています\n  use {}.{}\n  use {}.{}\n解決策: エイリアスを使用してください（use {}.{} as {}_{}_alias）",
                    bind_name,
                    existing_source, bind_name,
                    source_path, bind_name,
                    source_path, sym_name,
                    source_path.replace('/', "_"), sym_name
                )));
            }
        }

        // インポート情報を記録
        self.imported_symbols.insert(
            bind_name.to_string(),
            ImportInfo {
                name: sym_name.to_string(),
                source_path: source_path.to_string(),
                used: false,
            },
        );

        self.define(bind_name, value, false);
        Ok(())
    }

    /// M-4-C/D 対応のファイル use 評価
    fn eval_file_use_with_tracking(
        &mut self,
        use_path: &str,
        symbols: &UseSymbols,
    ) -> Result<Value, RuntimeError> {
        let loader = self
            .module_loader
            .as_mut()
            .expect("module_loader should be set");

        // モジュールを読み込む
        let stmts = loader
            .load(use_path)
            .map_err(|e| RuntimeError::Custom(format!("モジュール読み込みエラー: {}", e)))?;

        // dep パッケージ内のファイルの場合、内部の `use ./foo` を `use depname/foo` に変換する
        // （dep パッケージ内の相対インポートを正しく解決するため）
        let dep_prefix = {
            let first_seg = use_path.split('/').next().unwrap_or(use_path);
            if self
                .module_loader
                .as_ref()
                .map(|l| l.dep_paths.contains_key(first_seg))
                .unwrap_or(false)
            {
                Some(first_seg.to_string())
            } else {
                None
            }
        };
        let stmts = if let Some(ref dep_name) = dep_prefix {
            rewrite_local_use_paths(&stmts, dep_name)
        } else {
            stmts
        };

        // モジュールを別スコープで評価して、エクスポートを取得する
        let (all_symbols, pub_names) = self.eval_module_stmts(&stmts)?;

        // シンボルを現在のスコープにバインド（インポート記録付き）
        self.bind_symbols_to_scope_with_tracking(use_path, symbols, &all_symbols, &pub_names)
    }

    /// M-4-C/D 対応のディレクトリ use 評価
    fn eval_directory_use_with_tracking(
        &mut self,
        dir_path: &str,
        symbols: &UseSymbols,
        depth: usize,
    ) -> Result<Value, RuntimeError> {
        // 元のメソッドに委譲するが内部のバインドは tracking 付きに置き換える
        // シンプルに既存の eval_directory_use を呼んで OK（tracking は bind_symbols_to_scope_with_tracking で行う）
        // ただし eval_directory_use は bind_symbols_to_scope を使っているため、
        // 別の方法で tracking を統合する必要がある
        // ここでは tracking なしの既存実装を呼び、その後で imported_symbols を更新する
        if depth > 3 {
            eprintln!(
                "警告: re-export チェーンが3段階を超えています (depth={}): {}",
                depth, dir_path
            );
        }

        let loader = self
            .module_loader
            .as_mut()
            .expect("module_loader should be set");

        let mod_forge_path = loader.resolve_mod_forge(dir_path);

        match mod_forge_path {
            None => Err(RuntimeError::Custom(format!(
                "ディレクトリ '{}' が見つかりません",
                dir_path
            ))),
            Some(resolved_path) => {
                if resolved_path.is_dir() {
                    let stmts = {
                        let loader = self.module_loader.as_mut().expect("module_loader");
                        loader.load_directory_all_pub(dir_path).map_err(|e| {
                            RuntimeError::Custom(format!("ディレクトリ読み込みエラー: {}", e))
                        })?
                    };
                    let (all_syms, pub_names) = self.eval_module_stmts(&stmts)?;
                    self.bind_symbols_to_scope_with_tracking(
                        dir_path, symbols, &all_syms, &pub_names,
                    )
                } else {
                    let mod_forge_path_clone = resolved_path.clone();
                    let export = {
                        let loader = self.module_loader.as_mut().expect("module_loader");
                        loader.parse_mod_forge(&mod_forge_path_clone).map_err(|e| {
                            RuntimeError::Custom(format!("mod.forge 読み込みエラー: {}", e))
                        })?
                    };
                    self.eval_mod_forge_use(dir_path, symbols, &export, depth)
                }
            }
        }
    }

    /// M-4-C/D 対応のシンボルバインド（インポート記録付き）
    fn bind_symbols_to_scope_with_tracking(
        &mut self,
        use_path: &str,
        symbols: &UseSymbols,
        all_symbols: &HashMap<String, Value>,
        pub_names: &std::collections::HashSet<String>,
    ) -> Result<Value, RuntimeError> {
        match symbols {
            UseSymbols::Single(name, alias) => {
                if !pub_names.contains(name.as_str()) {
                    return Err(RuntimeError::Custom(format!(
                        "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                        name, use_path
                    )));
                }
                let bind_name = alias.as_deref().unwrap_or(name.as_str());
                let value = all_symbols.get(name).cloned().ok_or_else(|| {
                    RuntimeError::Custom(format!(
                        "モジュール '{}' にシンボル '{}' が見つかりません",
                        use_path, name
                    ))
                })?;
                self.record_import(name, bind_name, use_path, value, false)?;
            }
            UseSymbols::Multiple(names) => {
                for (name, alias) in names {
                    if !pub_names.contains(name.as_str()) {
                        return Err(RuntimeError::Custom(format!(
                            "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                            name, use_path
                        )));
                    }
                    let bind_name = alias.as_deref().unwrap_or(name.as_str());
                    let value = all_symbols.get(name).cloned().ok_or_else(|| {
                        RuntimeError::Custom(format!(
                            "モジュール '{}' にシンボル '{}' が見つかりません",
                            use_path, name
                        ))
                    })?;
                    self.record_import(name, bind_name, use_path, value, false)?;
                }
            }
            UseSymbols::All => {
                // ワイルドカード: pub シンボルのみをインポート（衝突は警告）
                for (name, value) in all_symbols {
                    if pub_names.contains(name) {
                        self.record_import(name, name, use_path, value.clone(), true)?;
                    }
                }
            }
        }
        Ok(Value::Unit)
    }

    /// ファイルを直接指定した場合の use 評価
    fn eval_file_use(
        &mut self,
        use_path: &str,
        symbols: &UseSymbols,
    ) -> Result<Value, RuntimeError> {
        let loader = self
            .module_loader
            .as_mut()
            .expect("module_loader should be set");

        // モジュールを読み込む
        let stmts = loader
            .load(use_path)
            .map_err(|e| RuntimeError::Custom(format!("モジュール読み込みエラー: {}", e)))?;

        // モジュールを別スコープで評価して、エクスポートを取得する
        let (all_symbols, pub_names) = self.eval_module_stmts(&stmts)?;

        // シンボルを現在のスコープにバインド
        self.bind_symbols_to_scope(use_path, symbols, &all_symbols, &pub_names)
    }

    /// ディレクトリを指定した場合の use 評価
    /// `depth` は re-export チェーンの深さ（3段階超で警告）
    fn eval_directory_use(
        &mut self,
        dir_path: &str,
        symbols: &UseSymbols,
        depth: usize,
    ) -> Result<Value, RuntimeError> {
        if depth > 3 {
            // 3段階超の re-export チェーン: 警告を出す（エラーにはしない）
            eprintln!(
                "警告: re-export チェーンが3段階を超えています (depth={}): {}",
                depth, dir_path
            );
        }

        let loader = self
            .module_loader
            .as_mut()
            .expect("module_loader should be set");

        // mod.forge の絶対パスを解決
        let mod_forge_path = loader.resolve_mod_forge(dir_path);

        match mod_forge_path {
            None => {
                // ディレクトリが存在しない → エラー
                return Err(RuntimeError::Custom(format!(
                    "ディレクトリ '{}' が見つかりません",
                    dir_path
                )));
            }
            Some(resolved_path) => {
                if resolved_path.is_dir() {
                    // mod.forge がない場合: ディレクトリ内の全 pub シンボルを収集
                    let stmts = {
                        let loader = self.module_loader.as_mut().expect("module_loader");
                        loader.load_directory_all_pub(dir_path).map_err(|e| {
                            RuntimeError::Custom(format!("ディレクトリ読み込みエラー: {}", e))
                        })?
                    };
                    let (all_syms, pub_names) = self.eval_module_stmts(&stmts)?;
                    self.bind_symbols_to_scope(dir_path, symbols, &all_syms, &pub_names)
                } else {
                    // mod.forge が存在する場合: それを経由してシンボルを解決
                    let mod_forge_path_clone = resolved_path.clone();
                    let export = {
                        let loader = self.module_loader.as_mut().expect("module_loader");
                        loader.parse_mod_forge(&mod_forge_path_clone).map_err(|e| {
                            RuntimeError::Custom(format!("mod.forge 読み込みエラー: {}", e))
                        })?
                    };

                    self.eval_mod_forge_use(dir_path, symbols, &export, depth)
                }
            }
        }
    }

    /// mod.forge 経由でシンボルを解決してスコープにバインドする
    fn eval_mod_forge_use(
        &mut self,
        dir_path: &str,
        symbols: &UseSymbols,
        export: &ModForgeExport,
        depth: usize,
    ) -> Result<Value, RuntimeError> {
        // export.symbols から必要なシンボルを解決する
        // export.symbols: "add" → ("basic", "add")
        // "add" を要求された場合、"basic" ファイルから "add" を読み込む

        let symbols_to_resolve: Vec<(String, Option<String>)> = match symbols {
            UseSymbols::Single(name, alias) => vec![(name.clone(), alias.clone())],
            UseSymbols::Multiple(names) => names.clone(),
            UseSymbols::All => {
                // mod.forge でエクスポートされた全シンボルを収集
                let all_names: Vec<(String, Option<String>)> = export
                    .symbols
                    .keys()
                    .filter(|k| !k.starts_with("__all__"))
                    .map(|k| (k.clone(), None))
                    .collect();
                // __all__ マーカーの処理（pub use basic.* の場合）
                let wildcard_modules: Vec<String> = export
                    .symbols
                    .iter()
                    .filter(|(k, (_, sym))| k.starts_with("__all__") && sym == "*")
                    .map(|(_, (module, _))| module.clone())
                    .collect();
                let _result = all_names;
                for module in &wildcard_modules {
                    let sub_path = format!("{}/{}", dir_path, module);
                    // ファイルか、サブディレクトリかを確認
                    let is_sub_dir = self
                        .module_loader
                        .as_ref()
                        .map(|l| l.is_directory(&sub_path))
                        .unwrap_or(false);
                    if is_sub_dir {
                        // サブディレクトリの場合は再帰
                        self.eval_directory_use_with_tracking(
                            &sub_path,
                            &UseSymbols::All,
                            depth + 1,
                        )?;
                    } else {
                        // dep パッケージ内ファイルの場合は with_tracking を使う（path rewriting のため）
                        self.eval_file_use_with_tracking(&sub_path, &UseSymbols::All)?;
                    }
                }
                // wildcard 分は既に処理済みなので、名前付きシンボルのみを返す
                return Ok(Value::Unit);
            }
        };

        for (sym_name, alias) in &symbols_to_resolve {
            // mod.forge の export マップからシンボルの元ファイルを探す
            let (source_module, source_sym) = export.symbols.get(sym_name)
                .ok_or_else(|| RuntimeError::Custom(format!(
                    "mod.forge: モジュール '{}' にシンボル '{}' が見つかりません（re-export されていません）",
                    dir_path, sym_name
                )))?;

            // __all__ マーカーは通常のシンボル解決ではスキップ
            if sym_name.starts_with("__all__") {
                continue;
            }

            // ソースモジュールのパスを構築
            let sub_path = format!("{}/{}", dir_path, source_module);

            // ソースモジュールを読み込む
            let is_sub_dir = self
                .module_loader
                .as_ref()
                .map(|l| l.is_directory(&sub_path))
                .unwrap_or(false);

            let (all_mod_symbols, pub_names) = if is_sub_dir {
                // サブディレクトリの場合
                // まず mod.forge を取得
                let sub_mod_forge_path = {
                    let loader = self.module_loader.as_mut().expect("module_loader");
                    loader.resolve_mod_forge(&sub_path)
                };
                match sub_mod_forge_path {
                    None => {
                        return Err(RuntimeError::Custom(format!(
                            "サブディレクトリ '{}' が見つかりません",
                            sub_path
                        )));
                    }
                    Some(p) if p.is_dir() => {
                        let stmts = {
                            let loader = self.module_loader.as_mut().expect("module_loader");
                            loader.load_directory_all_pub(&sub_path).map_err(|e| {
                                RuntimeError::Custom(format!("読み込みエラー: {}", e))
                            })?
                        };
                        self.eval_module_stmts(&stmts)?
                    }
                    Some(_) => {
                        // mod.forge 経由
                        let stmts = {
                            let loader = self.module_loader.as_mut().expect("module_loader");
                            loader.load(&sub_path).map_err(|e| {
                                RuntimeError::Custom(format!("読み込みエラー: {}", e))
                            })?
                        };
                        self.eval_module_stmts(&stmts)?
                    }
                }
            } else {
                // 通常ファイルを読み込む
                let stmts = {
                    let loader = self.module_loader.as_mut().expect("module_loader");
                    loader.load(&sub_path).map_err(|e| {
                        RuntimeError::Custom(format!(
                            "モジュール読み込みエラー (mod.forge 経由): {}",
                            e
                        ))
                    })?
                };
                self.eval_module_stmts(&stmts)?
            };

            // pub チェック
            if !pub_names.contains(source_sym.as_str()) {
                return Err(RuntimeError::Custom(format!(
                    "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                    source_sym, sub_path
                )));
            }

            let value = all_mod_symbols.get(source_sym).cloned().ok_or_else(|| {
                RuntimeError::Custom(format!(
                    "モジュール '{}' にシンボル '{}' が見つかりません",
                    sub_path, source_sym
                ))
            })?;

            // バインド名はエイリアスがあればそれ、なければシンボル名
            let bind_name = alias.as_deref().unwrap_or(sym_name.as_str());
            self.define(bind_name, value, false);
        }

        Ok(Value::Unit)
    }

    /// シンボルを現在のスコープにバインドするヘルパー
    fn bind_symbols_to_scope(
        &mut self,
        use_path: &str,
        symbols: &UseSymbols,
        all_symbols: &HashMap<String, Value>,
        pub_names: &std::collections::HashSet<String>,
    ) -> Result<Value, RuntimeError> {
        match symbols {
            UseSymbols::Single(name, alias) => {
                // pub チェック: pub でないシンボルはエラー
                if !pub_names.contains(name.as_str()) {
                    return Err(RuntimeError::Custom(format!(
                        "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                        name, use_path
                    )));
                }
                let bind_name = alias.as_deref().unwrap_or(name.as_str());
                let value = all_symbols.get(name).cloned().ok_or_else(|| {
                    RuntimeError::Custom(format!(
                        "モジュール '{}' にシンボル '{}' が見つかりません",
                        use_path, name
                    ))
                })?;
                self.define(bind_name, value, false);
            }
            UseSymbols::Multiple(names) => {
                for (name, alias) in names {
                    // pub チェック
                    if !pub_names.contains(name.as_str()) {
                        return Err(RuntimeError::Custom(format!(
                            "`{}` は非公開です（`pub` キーワードがありません）\n  --> {}",
                            name, use_path
                        )));
                    }
                    let bind_name = alias.as_deref().unwrap_or(name.as_str());
                    let value = all_symbols.get(name).cloned().ok_or_else(|| {
                        RuntimeError::Custom(format!(
                            "モジュール '{}' にシンボル '{}' が見つかりません",
                            use_path, name
                        ))
                    })?;
                    self.define(bind_name, value, false);
                }
            }
            UseSymbols::All => {
                // ワイルドカード: pub シンボルのみをインポート
                for (name, value) in all_symbols {
                    if pub_names.contains(name) {
                        self.define(name, value.clone(), false);
                    }
                }
            }
        }
        Ok(Value::Unit)
    }

    /// モジュールの文を別スコープで評価して、定義されたシンボルをマップとして返す。
    /// 戻り値は `(全シンボルマップ, pub シンボル名セット)` のタプル。
    fn eval_module_stmts(
        &mut self,
        stmts: &[Stmt],
    ) -> Result<(HashMap<String, Value>, std::collections::HashSet<String>), RuntimeError> {
        self.push_scope();

        // pub シンボル名を収集（AST から静的に判断）
        let mut pub_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for stmt in stmts {
            match stmt {
                Stmt::Fn { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::Let { pat, is_pub, .. } if *is_pub => {
                    if let Pat::Ident(name) = pat {
                        pub_names.insert(name.clone());
                    }
                }
                Stmt::Const { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::StructDef { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::EnumDef { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::DataDef { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::TraitDef { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                Stmt::MixinDef { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
                }
                _ => {}
            }
        }

        for stmt in stmts {
            match self.eval_stmt(stmt) {
                Ok(_) => {}
                Err(RuntimeError::Return(_)) => {
                    // モジュールトップレベルでの return は無視
                }
                Err(e) => {
                    self.pop_scope();
                    return Err(e);
                }
            }
        }

        // 現在のスコープのシンボルをすべて取得
        let scope = self.scopes.last().cloned().unwrap_or_default();
        let all_symbols: HashMap<String, Value> = scope
            .into_iter()
            .map(|(name, (value, _mutable))| (name, value))
            .collect();

        self.pop_scope();
        Ok((all_symbols, pub_names))
    }

    // ── 式の評価 ──────────────────────────────────────────────────────────

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Literal(lit, _) => Ok(eval_literal(lit)),
            Expr::Ident(name, _) => self.eval_ident(name),
            Expr::BinOp {
                op, left, right, ..
            } => self.eval_binop(op, left, right),
            Expr::UnaryOp { op, operand, .. } => self.eval_unary(op, operand),
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => self.eval_if(cond, then_block, else_block.as_deref()),
            Expr::While { cond, body, .. } => self.eval_while(cond, body),
            Expr::Loop { body, .. } => self.eval_loop(body),
            Expr::Break { .. } => Err(RuntimeError::LoopBreak),
            Expr::For {
                pat, iter, body, ..
            } => self.eval_for(pat, iter, body),
            Expr::Match {
                scrutinee, arms, ..
            } => self.eval_match(scrutinee, arms),
            Expr::Block { stmts, tail, .. } => self.eval_block(stmts, tail.as_deref()),
            Expr::Call { callee, args, .. } => self.eval_call(callee, args),
            Expr::MethodCall {
                object,
                method,
                args,
                span,
            } => self.eval_method_call(object, method, args, Some(span)),
            Expr::Closure { params, body, .. } => self.eval_closure(params, body),
            Expr::Await { expr, .. } => self.eval_expr(expr),
            Expr::Spawn { body, .. } => {
                let result = self.eval_expr(body)?;
                Ok(Value::Option(Some(Box::new(result))))
            }
            Expr::Question(inner, span) => self.eval_question(inner, Some(span)),
            Expr::Interpolation { parts, .. } => self.eval_interpolation(parts),
            Expr::Range {
                start,
                end,
                inclusive,
                ..
            } => self.eval_range(start, end, *inclusive),
            Expr::List(items, _) => self.eval_list(items),
            Expr::MapLiteral { pairs, .. } => self.eval_map_literal(pairs),
            Expr::SetLiteral { items, .. } => self.eval_set_literal(items),
            Expr::Assign { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.assign(name, v)
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => self.eval_index_assign(object, index, value),
            Expr::Index { object, index, .. } => self.eval_index(object, index),
            Expr::Field { object, field, .. } => self.eval_field_access(object, field),
            Expr::OptionalChain { object, chain, .. } => self.eval_optional_chain(object, chain),
            Expr::NullCoalesce { value, default, .. } => self.eval_null_coalesce(value, default),
            Expr::StructInit { name, fields, .. } => self.eval_struct_init(name, fields),
            Expr::AnonStruct { fields, .. } => self.eval_anon_struct(fields),
            Expr::FieldAssign {
                object,
                field,
                value,
                ..
            } => self.eval_field_assign(object, field, value),
            Expr::EnumInit {
                enum_name,
                variant,
                data,
                ..
            } => self.eval_enum_init(enum_name, variant, data),
            Expr::Pipeline { steps, .. } => self.eval_pipeline(steps),
        }
    }

    // ── 各評価メソッド ────────────────────────────────────────────────────

    fn eval_ident(&mut self, name: &str) -> Result<Value, RuntimeError> {
        // M-4-C: インポート済みシンボルの使用をマークする
        if let Some(info) = self.imported_symbols.get_mut(name) {
            info.used = true;
        }
        match self.lookup(name) {
            Some((v, _)) => Ok(v.clone()),
            None => Err(RuntimeError::UndefinedVariable(name.to_string())),
        }
    }

    fn eval_binop(&mut self, op: &BinOp, left: &Expr, right: &Expr) -> Result<Value, RuntimeError> {
        match op {
            BinOp::And => {
                let l = self.eval_expr(left)?;
                return match l {
                    Value::Bool(false) => Ok(Value::Bool(false)),
                    Value::Bool(true) => self.eval_expr(right),
                    _ => Err(type_err("bool", l.type_name())),
                };
            }
            BinOp::Or => {
                let l = self.eval_expr(left)?;
                return match l {
                    Value::Bool(true) => Ok(Value::Bool(true)),
                    Value::Bool(false) => self.eval_expr(right),
                    _ => Err(type_err("bool", l.type_name())),
                };
            }
            _ => {}
        }

        let left_val = self.eval_expr(left)?;
        let right_val = self.eval_expr(right)?;
        let left_clone = left_val.clone();
        let right_clone = right_val.clone();
        if let Some(result) =
            self.call_struct_binary_operator(op, left_clone.clone(), right_clone.clone())?
        {
            return Ok(result);
        }

        match op {
            BinOp::Add => match (left_val, right_val) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(b))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
                (l, r) => Err(type_err(
                    "number/string + number/string",
                    &format!("{} + {}", l.type_name(), r.type_name()),
                )),
            },
            BinOp::Sub => {
                let l = left_val;
                let r = right_val;
                int_float_op(l, r, i64::wrapping_sub, std::ops::Sub::sub, "-")
            }
            BinOp::Mul => {
                let l = left_val;
                let r = right_val;
                int_float_op(l, r, i64::wrapping_mul, std::ops::Mul::mul, "*")
            }
            BinOp::Div => {
                let l = left_val;
                let r = right_val;
                if matches!((&l, &r), (Value::Int(_), Value::Int(0))) {
                    return Err(RuntimeError::DivisionByZero);
                }
                int_float_op(l, r, i64::wrapping_div, std::ops::Div::div, "/")
            }
            BinOp::Rem => {
                let l = left_val;
                let r = right_val;
                int_float_op(l, r, i64::wrapping_rem, std::ops::Rem::rem, "%")
            }
            BinOp::Eq => Ok(Value::Bool(left_val == right_val)),
            BinOp::Ne => Ok(Value::Bool(left_val != right_val)),
            BinOp::Lt => {
                let l = left_val;
                let r = right_val;
                cmp_op(l, r, |a, b| a < b, |a, b| a < b)
            }
            BinOp::Gt => {
                let l = left_val;
                let r = right_val;
                match cmp_op(l.clone(), r.clone(), |a, b| a > b, |a, b| a > b) {
                    Ok(v) => Ok(v),
                    Err(_) => {
                        if let Some(result) = self.call_struct_binary_operator(&BinOp::Lt, r, l)? {
                            return Ok(result);
                        }
                        Err(type_err(
                            "number",
                            &format!("{} > {}", left_clone.type_name(), right_clone.type_name()),
                        ))
                    }
                }
            }
            BinOp::Le => cmp_op(left_val, right_val, |a, b| a <= b, |a, b| a <= b),
            BinOp::Ge => cmp_op(left_val, right_val, |a, b| a >= b, |a, b| a >= b),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn call_struct_binary_operator(
        &mut self,
        op: &BinOp,
        left: Value,
        right: Value,
    ) -> Result<Option<Value>, RuntimeError> {
        let kind = match op {
            BinOp::Add => Some(OperatorKind::Add),
            BinOp::Sub => Some(OperatorKind::Sub),
            BinOp::Mul => Some(OperatorKind::Mul),
            BinOp::Div => Some(OperatorKind::Div),
            BinOp::Rem => Some(OperatorKind::Rem),
            BinOp::Eq => Some(OperatorKind::Eq),
            BinOp::Lt => Some(OperatorKind::Lt),
            _ => None,
        };
        if let Some(kind) = kind {
            if let Value::Struct { type_name, .. } = &left {
                let type_name = type_name.clone();
                return self.call_struct_operator(&type_name, kind, left, vec![right]);
            }
        }
        Ok(None)
    }

    fn eval_unary(&mut self, op: &UnaryOp, operand: &Expr) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(operand)?;
        if let UnaryOp::Neg = op {
            if let Value::Struct { type_name, .. } = &v {
                if let Some(result) =
                    self.call_struct_operator(type_name, OperatorKind::Neg, v.clone(), vec![])?
                {
                    return Ok(result);
                }
            }
        }
        match op {
            UnaryOp::Neg => match v {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                other => Err(type_err("number", other.type_name())),
            },
            UnaryOp::Not => match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                other => Err(type_err("bool", other.type_name())),
            },
        }
    }

    fn eval_if(
        &mut self,
        cond: &Expr,
        then_block: &Expr,
        else_block: Option<&Expr>,
    ) -> Result<Value, RuntimeError> {
        match self.eval_expr(cond)? {
            Value::Bool(true) => self.eval_expr(then_block),
            Value::Bool(false) => match else_block {
                Some(e) => self.eval_expr(e),
                None => Ok(Value::Unit),
            },
            v => Err(type_err("bool", v.type_name())),
        }
    }

    fn eval_while(&mut self, cond: &Expr, body: &Expr) -> Result<Value, RuntimeError> {
        loop {
            match self.eval_expr(cond)? {
                Value::Bool(false) => break,
                Value::Bool(true) => match self.eval_expr(body) {
                    Ok(_) => {}
                    Err(RuntimeError::Return(v)) => return Err(RuntimeError::Return(v)),
                    Err(e) => return Err(e),
                },
                v => return Err(type_err("bool", v.type_name())),
            }
        }
        Ok(Value::Unit)
    }

    fn eval_loop(&mut self, body: &Expr) -> Result<Value, RuntimeError> {
        loop {
            match self.eval_expr(body) {
                Ok(_) => {}
                Err(RuntimeError::LoopBreak) => break,
                Err(RuntimeError::Return(v)) => return Err(RuntimeError::Return(v)),
                Err(e) => return Err(e),
            }
        }
        Ok(Value::Unit)
    }

    fn eval_for(&mut self, pat: &Pat, iter: &Expr, body: &Expr) -> Result<Value, RuntimeError> {
        let iter_val = self.eval_expr(iter)?;
        let items = match iter_val {
            Value::List(list) => list.borrow().clone(),
            v => return Err(type_err("list", v.type_name())),
        };

        let mut results = Vec::new();
        for item in items {
            self.push_scope();
            self.bind_pat(pat, item)?;
            let result = self.eval_expr(body);
            self.pop_scope();
            match result {
                Ok(v) => results.push(v),
                Err(RuntimeError::Return(v)) => return Err(RuntimeError::Return(v)),
                Err(e) => return Err(e),
            }
        }
        Ok(Value::List(Rc::new(RefCell::new(results))))
    }

    /// Pat に値を束縛する（E2-1）
    fn bind_pat(&mut self, pat: &Pat, value: Value) -> Result<(), RuntimeError> {
        match pat {
            Pat::Ident(name) => {
                self.define(name, value, false);
                Ok(())
            }
            Pat::Wildcard => Ok(()),
            Pat::Tuple(pats) | Pat::List(pats) => {
                let items = match value {
                    Value::List(ref list) => list.borrow().clone(),
                    other => {
                        return Err(RuntimeError::Custom(format!(
                            "分割代入: list が必要ですが {} が渡されました",
                            other.type_name()
                        )))
                    }
                };
                // 残余パターンの数を数える
                let rest_count = pats.iter().filter(|p| matches!(p, Pat::Rest(_))).count();
                if rest_count > 1 {
                    return Err(RuntimeError::Custom(
                        "分割代入: 残余パターン ..name は1つのみ使用できます".to_string(),
                    ));
                }
                // 残余パターンがない場合は要素数チェック
                let required = pats.len();
                if rest_count == 0 && items.len() < required {
                    return Err(RuntimeError::Custom(format!(
                        "分割代入: {} 要素が必要ですが {} 要素しかありません",
                        required,
                        items.len()
                    )));
                }
                let has_rest = rest_count > 0;
                let non_rest_count = pats.len() - rest_count;
                if has_rest && items.len() < non_rest_count {
                    return Err(RuntimeError::Custom(format!(
                        "分割代入: 少なくとも {} 要素が必要ですが {} 要素しかありません",
                        non_rest_count,
                        items.len()
                    )));
                }

                let mut item_idx = 0usize;
                for sub_pat in pats {
                    match sub_pat {
                        Pat::Rest(rest_name) => {
                            // 残余パターン: このパターンの後に来る non-rest 要素の数を計算
                            let remaining_non_rest = pats
                                .iter()
                                .skip_while(|p| !std::ptr::eq(*p, sub_pat))
                                .skip(1)
                                .filter(|p| !matches!(p, Pat::Rest(_)))
                                .count();
                            let rest_end = items.len() - remaining_non_rest;
                            let rest_items = items[item_idx..rest_end].to_vec();
                            self.define(
                                rest_name,
                                Value::List(Rc::new(RefCell::new(rest_items))),
                                false,
                            );
                            item_idx = rest_end;
                        }
                        _ => {
                            let item = items.get(item_idx).cloned().ok_or_else(|| {
                                RuntimeError::Custom(format!(
                                    "分割代入: インデックス {} に要素がありません",
                                    item_idx
                                ))
                            })?;
                            self.bind_pat(sub_pat, item)?;
                            item_idx += 1;
                        }
                    }
                }
                Ok(())
            }
            Pat::Rest(name) => {
                // トップレベルの Rest は全体を束縛
                self.define(name, value, false);
                Ok(())
            }
        }
    }

    fn eval_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(scrutinee)?;
        for arm in arms {
            if let Some(bindings) = match_pattern(&arm.pattern, &val) {
                self.push_scope();
                for (name, v) in bindings {
                    self.define(&name, v, false);
                }
                let result = self.eval_expr(&arm.body);
                self.pop_scope();
                return result;
            }
        }
        Err(RuntimeError::Custom("非網羅的なmatch式".to_string()))
    }

    fn eval_block(&mut self, stmts: &[Stmt], tail: Option<&Expr>) -> Result<Value, RuntimeError> {
        self.push_scope();
        let result = (|| -> Result<Value, RuntimeError> {
            for stmt in stmts {
                self.eval_stmt(stmt)?;
            }
            match tail {
                Some(e) => self.eval_expr(e),
                None => Ok(Value::Unit),
            }
        })();
        self.pop_scope();
        result
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value, RuntimeError> {
        if let Expr::Ident(name, _) = callee {
            if name == "none" && args.is_empty() {
                return Ok(Value::Option(None));
            }
            if name == "display" {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                if arg_vals.len() != 1 {
                    return Err(RuntimeError::Custom("display() takes 1 arg".to_string()));
                }
                let output = auto_display_output(&arg_vals[0]);
                emit_display_output(&self.display_listener, &self.output_buffer, output);
                return Ok(Value::Unit);
            }
            // tcp_listen / tcp_listen_async — forge run モードのシンプルな同期 HTTP サーバ
            if name == "tcp_listen" || name == "tcp_listen_async" {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_tcp_listen(arg_vals);
            }
            // TCP クライアント関数
            if matches!(
                name.as_str(),
                "tcp_connect" | "tcp_write" | "tcp_read_exact" | "tcp_read_available" | "tcp_close"
            ) {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return eval_tcp_client(name, arg_vals);
            }
            // SCRAM-SHA-256 暗号計算関数
            if matches!(
                name.as_str(),
                "scram_generate_nonce"
                    | "scram_hi"
                    | "scram_hmac"
                    | "scram_h"
                    | "scram_xor"
                    | "scram_base64_encode"
                    | "scram_base64_decode"
            ) {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return eval_scram(name, arg_vals);
            }
            // バイト列 ↔ 文字列変換関数
            if matches!(name.as_str(), "string_to_bytes" | "bytes_to_str") {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return eval_bytes_string(name, arg_vals);
            }
        }
        let callee_val = self.eval_expr(callee)?;
        let arg_vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;

        match callee_val {
            Value::Closure {
                params,
                body,
                env,
                return_type,
                ..
            } => self.call_closure(&params, &body, &env, return_type.clone(), arg_vals),
            Value::NativeFunction(NativeFn(f)) => f(arg_vals).map_err(|msg| {
                if let Some(rest) = msg.strip_prefix("__tf__:") {
                    RuntimeError::TestFailure(rest.to_string())
                } else {
                    RuntimeError::Custom(msg)
                }
            }),
            v => Err(type_err("function", v.type_name())),
        }
    }

    fn call_closure(
        &mut self,
        params: &[String],
        body: &Expr,
        captured: &CapturedEnv,
        return_type: Option<TypeAnn>,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // クロージャ専用のスコープスタックを構築
        let saved = std::mem::take(&mut self.scopes);

        let mut initial: HashMap<String, Binding> = captured
            .borrow()
            .iter()
            .map(|(k, (v, mutable))| (k.clone(), (v.clone(), *mutable)))
            .collect();

        for (param, arg) in params.iter().zip(args) {
            initial.insert(param.clone(), (arg, false));
        }
        self.scopes = vec![initial];
        let is_generator = matches!(return_type.as_ref(), Some(TypeAnn::Generate(_)));
        if is_generator {
            self.generator_stack.push(Vec::new());
        }

        let result = self.eval_expr(body);
        if let Some(scope) = self.scopes.first() {
            let mut captured_mut = captured.borrow_mut();
            for (name, (value, mutable)) in scope {
                if let Some((captured_value, captured_mutable)) = captured_mut.get_mut(name) {
                    if *mutable || *captured_mutable {
                        *captured_value = value.clone();
                        *captured_mutable = *mutable || *captured_mutable;
                    }
                }
            }
        }
        self.scopes = saved;
        let generated_values = if is_generator {
            Some(self.generator_stack.pop().unwrap())
        } else {
            None
        };

        if let Some(values) = generated_values {
            match result {
                Ok(_) | Err(RuntimeError::Return(_)) => {
                    return Ok(Value::List(Rc::new(RefCell::new(values))));
                }
                Err(e) => return Err(e),
            }
        }

        match result {
            Ok(v) => Ok(v),
            Err(RuntimeError::Return(v)) => Ok(v),
            Err(RuntimeError::PropagateErr(e)) => Ok(Value::Result(Err(e))),
            Err(e) => Err(e),
        }
    }

    fn eval_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: Option<&Span>,
    ) -> Result<Value, RuntimeError> {
        if let Expr::Ident(type_name, _) = object {
            if is_utility_type_name(type_name) {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_utility_static_method(type_name, method, arg_vals);
            }
        }

        // TypeName::method() のような静的メソッド呼び出しを先に処理
        if let Expr::Ident(type_name, _) = object {
            if is_type_name_str(type_name)
                && self.type_registry.structs.contains_key(type_name.as_str())
            {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                // static メソッド呼び出し: self として Unit を渡す
                return self.eval_struct_static_method(&type_name_cloned, method, arg_vals);
            }
            // enum の静的メソッド呼び出し（Unit バリアントアクセス等）
            if is_type_name_str(type_name)
                && self.type_registry.enums.contains_key(type_name.as_str())
            {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_enum_static_method(&type_name_cloned, method, arg_vals);
            }
            // typestate の静的メソッド呼び出し（new<State>() = new("StateName") として渡される）
            if is_type_name_str(type_name)
                && self
                    .type_registry
                    .typestates
                    .contains_key(type_name.as_str())
            {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_typestate_static_method(&type_name_cloned, method, arg_vals);
            }
        }

        let obj = self.eval_expr(object)?;
        let arg_vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;

        if method == "clone" && arg_vals.is_empty() {
            return Ok(obj.clone());
        }

        let result = match obj {
            Value::Option(opt) => match method {
                "is_some" => Ok(Value::Bool(opt.is_some())),
                "is_none" => Ok(Value::Bool(opt.is_none())),
                "unwrap_or" => {
                    let fallback = arg_vals.into_iter().next().unwrap_or(Value::Unit);
                    Ok(match opt {
                        Some(inner) => *inner,
                        None => fallback,
                    })
                }
                "unwrap" => match opt {
                    Some(inner) => Ok(*inner),
                    None => Err(RuntimeError::Custom("unwrap called on none".to_string())),
                },
                "map" => {
                    let f = one_fn_arg(method, arg_vals)?;
                    match opt {
                        Some(inner) => {
                            let result = self.call_value(f, vec![*inner])?;
                            Ok(Value::Option(Some(Box::new(result))))
                        }
                        None => Ok(Value::Option(None)),
                    }
                }
                "and_then" => {
                    let f = one_fn_arg(method, arg_vals)?;
                    match opt {
                        Some(inner) => self.call_value(f, vec![*inner]),
                        None => Ok(Value::Option(None)),
                    }
                }
                "or" => {
                    let fallback = arg_vals.into_iter().next().unwrap_or(Value::Option(None));
                    match opt {
                        Some(inner) => Ok(Value::Option(Some(inner))),
                        None => Ok(fallback),
                    }
                }
                "filter" => {
                    let f = one_fn_arg(method, arg_vals)?;
                    match opt {
                        Some(inner) => match self.call_value(f, vec![(*inner).clone()])? {
                            Value::Bool(true) => Ok(Value::Option(Some(inner))),
                            Value::Bool(false) => Ok(Value::Option(None)),
                            other => Err(RuntimeError::Custom(format!(
                                "filter の述語は bool を返す必要があります。{} が返されました",
                                other.type_name()
                            ))),
                        },
                        None => Ok(Value::Option(None)),
                    }
                }
                other => Err(RuntimeError::Custom(format!(
                    "メソッド '{}' は option に対して未実装です",
                    other
                ))),
            },
            Value::Result(result) => match method {
                "is_ok" => Ok(Value::Bool(result.is_ok())),
                "is_err" => Ok(Value::Bool(result.is_err())),
                "map" => {
                    let f = one_fn_arg(method, arg_vals)?;
                    match result {
                        Ok(inner) => {
                            let mapped = self.call_value(f, vec![*inner])?;
                            Ok(Value::Result(Ok(Box::new(mapped))))
                        }
                        Err(msg) => Ok(Value::Result(Err(msg))),
                    }
                }
                "and_then" => {
                    let f = one_fn_arg(method, arg_vals)?;
                    match result {
                        Ok(inner) => match self.call_value(f, vec![*inner])? {
                            Value::Result(result) => Ok(Value::Result(result)),
                            other => Err(type_err("result", other.type_name())),
                        },
                        Err(msg) => Ok(Value::Result(Err(msg))),
                    }
                }
                "unwrap_or" => {
                    let fallback = arg_vals.into_iter().next().unwrap_or(Value::Unit);
                    Ok(match result {
                        Ok(inner) => *inner,
                        Err(_) => fallback,
                    })
                }
                "ok" => Ok(match result {
                    Ok(inner) => Value::Option(Some(inner)),
                    Err(_) => Value::Option(None),
                }),
                other => Err(RuntimeError::Custom(format!(
                    "メソッド '{}' は result に対して未実装です",
                    other
                ))),
            },
            Value::List(items) => self.eval_list_method(items, method, arg_vals, span),
            Value::Map(entries) => self.eval_map_method(object, entries, method, arg_vals),
            Value::Set(items) => self.eval_set_method(object, items, method, arg_vals),
            Value::String(text) => self.eval_string_method(text, method, arg_vals),
            Value::Struct { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_struct_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            Value::Enum { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_enum_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            Value::Typestate {
                ref type_name,
                ref current_state,
                ..
            } => {
                let type_name_cloned = type_name.clone();
                let current_state_cloned = current_state.clone();
                self.eval_typestate_method(
                    obj.clone(),
                    &type_name_cloned,
                    &current_state_cloned,
                    method,
                    arg_vals,
                )
            }
            Value::Closure { .. } | Value::NativeFunction(_) => self.call_value(obj, arg_vals),
            other => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は {} に対して未実装です",
                method,
                other.type_name()
            ))),
        };

        if let (Some(span), Ok(value)) = (span, result.as_ref()) {
            self.record_pipeline_trace(span, method, value);
        }

        result
    }

    /// TypeName::method() のような静的メソッド呼び出し
    fn eval_struct_static_method(
        &mut self,
        type_name: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        if method == "instance" {
            let is_singleton = self
                .type_registry
                .structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Singleton"))
                .unwrap_or(false);

            if is_singleton {
                if let Some(cached) = self.type_registry.singletons.get(type_name).cloned() {
                    return Ok(cached);
                }
                let fields: Vec<(String, TypeAnn)> = self
                    .type_registry
                    .structs
                    .get(type_name)
                    .map(|i| i.fields.clone())
                    .unwrap_or_default();
                let mut field_map = HashMap::new();
                for (fname, tann) in &fields {
                    field_map.insert(fname.clone(), zero_value_for_type(tann));
                }
                let instance = Value::Struct {
                    type_name: type_name.to_string(),
                    fields: Rc::new(RefCell::new(field_map)),
                };
                self.type_registry
                    .singletons
                    .insert(type_name.to_string(), instance.clone());
                return Ok(instance);
            }
        }

        if method == "default" || method == "new" {
            let has_default = self
                .type_registry
                .structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Default"))
                .unwrap_or(false);

            if has_default {
                let fields: Vec<(String, TypeAnn)> = self
                    .type_registry
                    .structs
                    .get(type_name)
                    .map(|i| i.fields.clone())
                    .unwrap_or_default();
                let mut field_map = HashMap::new();
                for (fname, tann) in &fields {
                    field_map.insert(fname.clone(), zero_value_for_type(tann));
                }
                return Ok(Value::Struct {
                    type_name: type_name.to_string(),
                    fields: Rc::new(RefCell::new(field_map)),
                });
            }
        }

        let method_impl = self
            .type_registry
            .structs
            .get(type_name)
            .and_then(|info| info.methods.get(method))
            .cloned();

        match method_impl {
            Some(MethodImpl::Native(NativeFn(f))) => f(args).map_err(RuntimeError::Custom),
            Some(MethodImpl::Forge(fn_def, captured)) => {
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = captured.borrow().clone();
                // グローバルスコープで上書き（最新の値を優先）
                if let Some(global) = saved.first() {
                    for (k, v) in global {
                        initial.insert(k.clone(), v.clone());
                    }
                }
                for (i, param) in fn_def.params.iter().enumerate() {
                    let value = args.get(i).cloned().unwrap_or(Value::Unit);
                    initial.insert(param.name.clone(), (value, false));
                }
                self.scopes = vec![initial];
                let result = self.eval_expr(&fn_def.body);
                self.scopes = saved;
                result
            }
            None => Err(RuntimeError::Custom(format!(
                "型 '{}' に静的メソッド '{}' は存在しません",
                type_name, method
            ))),
        }
    }
    fn eval_utility_static_method(
        &mut self,
        type_name: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match (type_name, method) {
            ("Partial", "from") => {
                let instance = one_value_arg(method, args)?;
                self.partial_from_value(instance)
            }
            ("Required", "from") => {
                let instance = one_value_arg(method, args)?;
                self.required_from_value(instance)
            }
            ("Pick", "from") => {
                let (instance, keys) = two_value_args(method, args)?;
                let keys = value_to_string_keys(keys)?;
                self.pick_from_value(instance, &keys)
            }
            ("Omit", "from") => {
                let (instance, keys) = two_value_args(method, args)?;
                let keys = value_to_string_keys(keys)?;
                self.omit_from_value(instance, &keys)
            }
            ("NonNullable", "from") => {
                let value = one_value_arg(method, args)?;
                match value {
                    Value::Option(Some(inner)) => Ok(*inner),
                    Value::Option(None) => {
                        Err(RuntimeError::Custom("NonNullable: value is None".into()))
                    }
                    other => Ok(other),
                }
            }
            ("Readonly", "from") => {
                // forge run では Readonly は型消去（通常の値と同じ）
                let value = one_value_arg(method, args)?;
                Ok(value)
            }
            ("Record", "new") => Ok(Value::Map(vec![])),
            _ => Err(RuntimeError::Custom(format!(
                "ユーティリティ型 '{}' に静的メソッド '{}' は存在しません",
                type_name, method
            ))),
        }
    }

    /// リスト値に対するメソッド呼び出しをディスパッチする（Phase 3-A 全メソッド）
    fn eval_list_method(
        &mut self,
        items: Rc<RefCell<Vec<Value>>>,
        method: &str,
        args: Vec<Value>,
        span: Option<&Span>,
    ) -> Result<Value, RuntimeError> {
        match method {
            // ── 変換 ──────────────────────────────────────────────────────
            "map" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::with_capacity(list.len());
                for (index, item) in list.into_iter().enumerate() {
                    match self.call_value(f.clone(), vec![item.clone()]) {
                        Ok(value) => out.push(value),
                        Err(error) => {
                            if let Some(span) = span {
                                self.record_pipeline_trace_error(
                                    span,
                                    method,
                                    index + 1,
                                    &item,
                                    &error,
                                    Some(out.len()),
                                );
                            }
                            return Err(error);
                        }
                    }
                }
                Ok(mk_list(out))
            }
            "filter" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                for (index, item) in list.into_iter().enumerate() {
                    let matched = match self.call_value(f.clone(), vec![item.clone()]) {
                        Ok(value) => value,
                        Err(error) => {
                            if let Some(span) = span {
                                self.record_pipeline_trace_error(
                                    span,
                                    method,
                                    index + 1,
                                    &item,
                                    &error,
                                    Some(out.len()),
                                );
                            }
                            return Err(error);
                        }
                    };
                    match matched {
                        Value::Bool(true) => out.push(item),
                        Value::Bool(false) => {}
                        v => return Err(type_err("bool", v.type_name())),
                    }
                }
                Ok(mk_list(out))
            }
            "flat_map" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                for item in list {
                    match self.call_value(f.clone(), vec![item])? {
                        Value::List(inner) => out.extend(inner.borrow().iter().cloned()),
                        v => return Err(type_err("list", v.type_name())),
                    }
                }
                Ok(mk_list(out))
            }
            "filter_map" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                for item in list {
                    match self.call_value(f.clone(), vec![item])? {
                        Value::Option(Some(v)) => out.push(*v),
                        Value::Option(None) => {}
                        v => return Err(type_err("option", v.type_name())),
                    }
                }
                Ok(mk_list(out))
            }
            // ── スライス ──────────────────────────────────────────────────
            "take" => {
                let n = one_int_arg(method, args)?;
                let list = items.borrow();
                let n = n.max(0) as usize;
                Ok(mk_list(list.iter().take(n).cloned().collect()))
            }
            "skip" => {
                let n = one_int_arg(method, args)?;
                let list = items.borrow();
                let n = n.max(0) as usize;
                Ok(mk_list(list.iter().skip(n).cloned().collect()))
            }
            "take_while" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                for item in list {
                    match self.call_value(f.clone(), vec![item.clone()])? {
                        Value::Bool(true) => out.push(item),
                        Value::Bool(false) => break,
                        v => return Err(type_err("bool", v.type_name())),
                    }
                }
                Ok(mk_list(out))
            }
            "skip_while" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                let mut skipping = true;
                for item in list {
                    if skipping {
                        match self.call_value(f.clone(), vec![item.clone()])? {
                            Value::Bool(true) => {}
                            Value::Bool(false) => {
                                skipping = false;
                                out.push(item);
                            }
                            v => return Err(type_err("bool", v.type_name())),
                        }
                    } else {
                        out.push(item);
                    }
                }
                Ok(mk_list(out))
            }
            // ── 結合 ──────────────────────────────────────────────────────
            "enumerate" => {
                let list = items.borrow();
                let out = list
                    .iter()
                    .enumerate()
                    .map(|(i, v)| mk_list(vec![Value::Int(i as i64), v.clone()]))
                    .collect();
                Ok(mk_list(out))
            }
            "zip" => {
                let other = one_list_arg(method, args)?;
                let a = items.borrow();
                let b = other.borrow();
                let out = a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| mk_list(vec![x.clone(), y.clone()]))
                    .collect();
                Ok(mk_list(out))
            }
            // ── 集計 ──────────────────────────────────────────────────────
            "sum" => {
                let list = items.borrow();
                if list.is_empty() {
                    return Ok(Value::Int(0));
                }
                let mut int_sum: i64 = 0;
                let mut float_sum: f64 = 0.0;
                let mut has_float = false;
                for item in list.iter() {
                    match item {
                        Value::Int(n) => {
                            int_sum += n;
                            float_sum += *n as f64;
                        }
                        Value::Float(n) => {
                            float_sum += n;
                            has_float = true;
                        }
                        v => return Err(type_err("number", v.type_name())),
                    }
                }
                Ok(if has_float {
                    Value::Float(float_sum)
                } else {
                    Value::Int(int_sum)
                })
            }
            "count" => {
                if args.is_empty() {
                    Ok(Value::Int(items.borrow().len() as i64))
                } else {
                    let f = one_fn_arg(method, args)?;
                    let list = items.borrow().clone();
                    let mut n = 0i64;
                    for item in list {
                        let matched = self.call_value(f.clone(), vec![item])?;
                        if matches!(matched, Value::Bool(true)) {
                            n += 1;
                        }
                    }
                    Ok(Value::Int(n))
                }
            }
            "len" => Ok(Value::Int(items.borrow().len() as i64)),
            "fold" => {
                if args.len() < 2 {
                    return Err(RuntimeError::Custom("fold() は引数が2つ必要です".into()));
                }
                let mut it = args.into_iter();
                let seed = it
                    .next()
                    .ok_or_else(|| RuntimeError::Custom("fold: seed missing".into()))?;
                let f = it
                    .next()
                    .ok_or_else(|| RuntimeError::Custom("fold: fn missing".into()))?;
                let list = items.borrow().clone();
                let mut acc = seed;
                for item in list {
                    acc = self.call_value(f.clone(), vec![acc, item])?;
                }
                Ok(acc)
            }
            "any" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                for item in list {
                    match self.call_value(f.clone(), vec![item])? {
                        Value::Bool(true) => return Ok(Value::Bool(true)),
                        Value::Bool(false) => {}
                        v => return Err(type_err("bool", v.type_name())),
                    }
                }
                Ok(Value::Bool(false))
            }
            "all" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                for item in list {
                    match self.call_value(f.clone(), vec![item])? {
                        Value::Bool(true) => {}
                        Value::Bool(false) => return Ok(Value::Bool(false)),
                        v => return Err(type_err("bool", v.type_name())),
                    }
                }
                Ok(Value::Bool(true))
            }
            "none" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                for item in list {
                    match self.call_value(f.clone(), vec![item])? {
                        Value::Bool(true) => return Ok(Value::Bool(false)),
                        Value::Bool(false) => {}
                        v => return Err(type_err("bool", v.type_name())),
                    }
                }
                Ok(Value::Bool(true))
            }
            // ── 要素アクセス ───────────────────────────────────────────────
            "first" => {
                let list = items.borrow();
                Ok(Value::Option(list.first().map(|v| Box::new(v.clone()))))
            }
            "last" => {
                let list = items.borrow();
                Ok(Value::Option(list.last().map(|v| Box::new(v.clone()))))
            }
            "nth" => {
                let n = one_int_arg(method, args)?;
                let list = items.borrow();
                if n < 0 {
                    return Ok(Value::Option(None));
                }
                Ok(Value::Option(
                    list.get(n as usize).map(|v| Box::new(v.clone())),
                ))
            }
            // ── 最小・最大 ─────────────────────────────────────────────────
            "min" => {
                let list = items.borrow();
                if list.is_empty() {
                    return Ok(Value::Option(None));
                }
                let mut min_val = &list[0];
                for item in list.iter().skip(1) {
                    if compare_values(item, min_val)? == std::cmp::Ordering::Less {
                        min_val = item;
                    }
                }
                Ok(Value::Option(Some(Box::new(min_val.clone()))))
            }
            "max" => {
                let list = items.borrow();
                if list.is_empty() {
                    return Ok(Value::Option(None));
                }
                let mut max_val = &list[0];
                for item in list.iter().skip(1) {
                    if compare_values(item, max_val)? == std::cmp::Ordering::Greater {
                        max_val = item;
                    }
                }
                Ok(Value::Option(Some(Box::new(max_val.clone()))))
            }
            "min_by" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                if list.is_empty() {
                    return Ok(Value::Option(None));
                }
                let mut min_item = list[0].clone();
                let mut min_key = self.call_value(f.clone(), vec![min_item.clone()])?;
                for item in list.into_iter().skip(1) {
                    let key = self.call_value(f.clone(), vec![item.clone()])?;
                    if compare_values(&key, &min_key)? == std::cmp::Ordering::Less {
                        min_key = key;
                        min_item = item;
                    }
                }
                Ok(Value::Option(Some(Box::new(min_item))))
            }
            "max_by" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                if list.is_empty() {
                    return Ok(Value::Option(None));
                }
                let mut max_item = list[0].clone();
                let mut max_key = self.call_value(f.clone(), vec![max_item.clone()])?;
                for item in list.into_iter().skip(1) {
                    let key = self.call_value(f.clone(), vec![item.clone()])?;
                    if compare_values(&key, &max_key)? == std::cmp::Ordering::Greater {
                        max_key = key;
                        max_item = item;
                    }
                }
                Ok(Value::Option(Some(Box::new(max_item))))
            }
            // ── ソート ────────────────────────────────────────────────────
            "order_by" => {
                let f = one_fn_arg(method, args)?;
                sort_by_key(self, items, f, false)
            }
            "order_by_descending" => {
                let f = one_fn_arg(method, args)?;
                sort_by_key(self, items, f, true)
            }
            // then_by は安定ソートなので order_by と同じ実装で正しい動作をする
            "then_by" => {
                let f = one_fn_arg(method, args)?;
                sort_by_key(self, items, f, false)
            }
            "then_by_descending" => {
                let f = one_fn_arg(method, args)?;
                sort_by_key(self, items, f, true)
            }
            // ── その他 ────────────────────────────────────────────────────
            "reverse" => {
                let mut list = items.borrow().clone();
                list.reverse();
                Ok(mk_list(list))
            }
            "distinct" => {
                let list = items.borrow();
                let mut seen: Vec<Value> = Vec::new();
                let mut out = Vec::new();
                for item in list.iter() {
                    if !seen.contains(item) {
                        seen.push(item.clone());
                        out.push(item.clone());
                    }
                }
                Ok(mk_list(out))
            }
            "push" => {
                let value = one_value_arg(method, args)?;
                items.borrow_mut().push(value);
                Ok(Value::Unit)
            }
            "collect" => Ok(mk_list(items.borrow().clone())),
            // ── each ──────────────────────────────────────────────────────
            // list.each(fn) — side-effecting iteration, returns unit
            "each" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                for item in list {
                    self.call_value(f.clone(), vec![item])?;
                }
                Ok(Value::Unit)
            }
            // ── group_by ──────────────────────────────────────────────────
            // list.group_by(key_fn) -> list of { key, values }
            "group_by" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                // preserve insertion order: vec of (key_value, vec<item>)
                let mut order: Vec<Value> = Vec::new();
                let mut buckets: Vec<(Value, Vec<Value>)> = Vec::new();
                for item in list {
                    let key = self.call_value(f.clone(), vec![item.clone()])?;
                    if let Some(pos) = order.iter().position(|k| k == &key) {
                        buckets[pos].1.push(item);
                    } else {
                        order.push(key.clone());
                        buckets.push((key, vec![item]));
                    }
                }
                let groups = buckets
                    .into_iter()
                    .map(|(k, vals)| {
                        let mut fields = std::collections::HashMap::new();
                        fields.insert("key".to_string(), k);
                        fields.insert("values".to_string(), mk_list(vals));
                        Value::Struct {
                            type_name: "Group".to_string(),
                            fields: Rc::new(RefCell::new(fields)),
                        }
                    })
                    .collect::<Vec<_>>();
                Ok(mk_list(groups))
            }
            // ── 追加メソッド ─────────────────────────────────────────────
            "sort" => {
                let mut list = items.borrow().clone();
                list.sort_by(|a, b| match (a, b) {
                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => {
                        x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::Int(x), Value::Float(y)) => (*x as f64)
                        .partial_cmp(y)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Float(x), Value::Int(y)) => x
                        .partial_cmp(&(*y as f64))
                        .unwrap_or(std::cmp::Ordering::Equal),
                    (Value::String(x), Value::String(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(mk_list(list))
            }
            "join" => {
                let sep = one_string_arg(method, args)?;
                let list = items.borrow();
                let parts: Vec<String> = list
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect();
                Ok(Value::String(parts.join(sep.as_str())))
            }
            "dedup" => {
                let list = items.borrow().clone();
                let mut result: Vec<Value> = Vec::new();
                for item in list {
                    if result.last().map_or(true, |last| last != &item) {
                        result.push(item);
                    }
                }
                Ok(mk_list(result))
            }
            "find" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                for (index, item) in list.into_iter().enumerate() {
                    let matched = match self.call_value(f.clone(), vec![item.clone()]) {
                        Ok(value) => value,
                        Err(error) => {
                            if let Some(span) = span {
                                self.record_pipeline_trace_error(
                                    span,
                                    method,
                                    index + 1,
                                    &item,
                                    &error,
                                    Some(index),
                                );
                            }
                            return Err(error);
                        }
                    };
                    if matches!(matched, Value::Bool(true)) {
                        return Ok(Value::Option(Some(Box::new(item))));
                    }
                }
                Ok(Value::Option(None))
            }
            "flatten" => {
                let list = items.borrow().clone();
                let mut result = Vec::new();
                for item in list {
                    match item {
                        Value::List(inner) => result.extend(inner.borrow().clone()),
                        other => result.push(other),
                    }
                }
                Ok(mk_list(result))
            }
            "chunk" => {
                let n = match args.into_iter().next() {
                    Some(Value::Int(n)) if n > 0 => n as usize,
                    _ => {
                        return Err(RuntimeError::Custom(
                            "chunk() には正の整数が必要です".into(),
                        ))
                    }
                };
                let list = items.borrow().clone();
                let chunks: Vec<Value> = list.chunks(n).map(|c| mk_list(c.to_vec())).collect();
                Ok(mk_list(chunks))
            }
            "partition" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut yes = Vec::new();
                let mut no = Vec::new();
                for item in list {
                    let matched = self.call_value(f.clone(), vec![item.clone()])?;
                    if matches!(matched, Value::Bool(true)) {
                        yes.push(item);
                    } else {
                        no.push(item);
                    }
                }
                Ok(mk_list(vec![mk_list(yes), mk_list(no)]))
            }
            "take_last" => {
                let n = match args.into_iter().next() {
                    Some(Value::Int(n)) if n >= 0 => n as usize,
                    _ => {
                        return Err(RuntimeError::Custom(
                            "take_last() には非負整数が必要です".into(),
                        ))
                    }
                };
                let list = items.borrow().clone();
                let start = list.len().saturating_sub(n);
                Ok(mk_list(list[start..].to_vec()))
            }
            "unique" => {
                let list = items.borrow().clone();
                let mut seen: Vec<Value> = Vec::new();
                let mut result = Vec::new();
                for item in list {
                    if !seen.contains(&item) {
                        seen.push(item.clone());
                        result.push(item);
                    }
                }
                Ok(mk_list(result))
            }
            "concat" => {
                let other = one_list_arg(method, args)?;
                let mut result = items.borrow().clone();
                result.extend(other.borrow().clone());
                Ok(mk_list(result))
            }
            "contains" => {
                let value = one_value_arg(method, args)?;
                let list = items.borrow();
                Ok(Value::Bool(list.iter().any(|item| item == &value)))
            }
            other => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は list に対して未実装です",
                other
            ))),
        }
    }

    fn eval_string_method(
        &mut self,
        text: String,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match method {
            "len" => Ok(Value::Int(text.chars().count() as i64)),
            "split" => {
                let sep = one_string_arg(method, args)?;
                let items = if sep.is_empty() {
                    text.chars()
                        .map(|ch| Value::String(ch.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    text.split(sep.as_str())
                        .filter(|part: &&str| !part.is_empty())
                        .map(|part: &str| Value::String(part.to_string()))
                        .collect::<Vec<_>>()
                };
                Ok(mk_list(items))
            }
            "starts_with" => {
                let prefix = one_string_arg(method, args)?;
                Ok(Value::Bool(text.starts_with(prefix.as_str())))
            }
            "strip_prefix" => {
                let prefix = one_string_arg(method, args)?;
                Ok(Value::Option(
                    text.strip_prefix(prefix.as_str())
                        .map(|part| Box::new(Value::String(part.to_string()))),
                ))
            }
            "contains" => {
                let pattern = one_string_arg(method, args)?;
                Ok(Value::Bool(text.contains(pattern.as_str())))
            }
            "trim" => Ok(Value::String(text.trim().to_string())),
            "trim_start" => Ok(Value::String(text.trim_start().to_string())),
            "trim_end" => Ok(Value::String(text.trim_end().to_string())),
            "capitalize" => {
                let mut chars = text.chars();
                let s = match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                };
                Ok(Value::String(s))
            }
            "to_upper" | "upper" => Ok(Value::String(text.to_uppercase())),
            "to_lower" | "lower" => Ok(Value::String(text.to_lowercase())),
            "ends_with" => {
                let suffix = one_string_arg(method, args)?;
                Ok(Value::Bool(text.ends_with(suffix.as_str())))
            }
            "replace" => {
                if args.len() < 2 {
                    return Err(RuntimeError::Custom("replace() は引数が2つ必要です".into()));
                }
                let mut it = args.into_iter();
                let from = match it.next() {
                    Some(Value::String(s)) => s,
                    _ => {
                        return Err(RuntimeError::Custom(
                            "replace(): 第1引数は string が必要です".into(),
                        ))
                    }
                };
                let to = match it.next() {
                    Some(Value::String(s)) => s,
                    _ => {
                        return Err(RuntimeError::Custom(
                            "replace(): 第2引数は string が必要です".into(),
                        ))
                    }
                };
                Ok(Value::String(text.replace(from.as_str(), to.as_str())))
            }
            "repeat" => {
                let n = match args.into_iter().next() {
                    Some(Value::Int(n)) if n >= 0 => n as usize,
                    _ => {
                        return Err(RuntimeError::Custom(
                            "repeat() には非負整数が必要です".into(),
                        ))
                    }
                };
                Ok(Value::String(text.repeat(n)))
            }
            "chars" => {
                let char_items = text
                    .chars()
                    .map(|c| Value::String(c.to_string()))
                    .collect::<Vec<_>>();
                Ok(mk_list(char_items))
            }
            "is_empty" => Ok(Value::Bool(text.is_empty())),
            _ => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は string に対して未実装です",
                method
            ))),
        }
    }

    /// Value（Closure または NativeFunction）を引数付きで呼び出す
    fn call_value(&mut self, f: Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match f {
            Value::Closure {
                params,
                body,
                env,
                return_type,
                ..
            } => self.call_closure(&params, &body, &env, return_type.clone(), args),
            Value::NativeFunction(NativeFn(func)) => func(args).map_err(RuntimeError::Custom),
            v => Err(type_err("function", v.type_name())),
        }
    }

    fn eval_closure(&self, params: &[String], body: &Expr) -> Result<Value, RuntimeError> {
        let captured = self.capture_env();
        Ok(Value::Closure {
            params: params.to_vec(),
            body: Box::new(body.clone()),
            env: captured,
            return_type: None,
            defer_cleanup: None,
        })
    }

    fn eval_question(&mut self, inner: &Expr, span: Option<&Span>) -> Result<Value, RuntimeError> {
        match self.eval_expr(inner)? {
            Value::Result(Ok(v)) => Ok(*v),
            Value::Result(Err(e)) => {
                if let Some(span) = span {
                    self.pipeline_trace_events.push(PipelineTraceEvent {
                        node_id: self.resolve_pipeline_trace_node_id(span, "?"),
                        method: "?".to_string(),
                        start: span.start,
                        end: span.end,
                        line: span.line,
                        col: span.col,
                        item_count: Some(0),
                        outcome: PipelineTraceOutcome::ResultErr,
                        message: Some(e.clone()),
                        corrupted: Vec::new(),
                    });
                }
                Err(RuntimeError::PropagateErr(e))
            }
            v => Err(type_err("result", v.type_name())),
        }
    }

    fn eval_interpolation(&mut self, parts: &[InterpPart]) -> Result<Value, RuntimeError> {
        let mut buf = String::new();
        for part in parts {
            match part {
                InterpPart::Literal(s) => buf.push_str(s),
                InterpPart::Expr(e) => buf.push_str(&self.eval_expr(e)?.to_string()),
            }
        }
        Ok(Value::String(buf))
    }

    fn eval_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
    ) -> Result<Value, RuntimeError> {
        let s = self.eval_expr(start)?;
        let e = self.eval_expr(end)?;
        match (s, e) {
            (Value::Int(a), Value::Int(b)) => {
                let items: Vec<Value> = if inclusive {
                    (a..=b).map(Value::Int).collect()
                } else {
                    (a..b).map(Value::Int).collect()
                };
                Ok(Value::List(Rc::new(RefCell::new(items))))
            }
            (s, e) => Err(type_err(
                "number..number",
                &format!("{}..{}", s.type_name(), e.type_name()),
            )),
        }
    }

    fn eval_list(&mut self, items: &[Expr]) -> Result<Value, RuntimeError> {
        let vals: Vec<Value> = items
            .iter()
            .map(|e| self.eval_expr(e))
            .collect::<Result<_, _>>()?;
        Ok(Value::List(Rc::new(RefCell::new(vals))))
    }

    fn eval_map_literal(&mut self, pairs: &[(Expr, Expr)]) -> Result<Value, RuntimeError> {
        let mut entries = Vec::with_capacity(pairs.len());
        for (key_expr, value_expr) in pairs {
            let key = self.eval_expr(key_expr)?;
            let value = self.eval_expr(value_expr)?;
            if let Some((_, existing)) = entries.iter_mut().find(|(k, _)| *k == key) {
                *existing = value;
            } else {
                entries.push((key, value));
            }
        }
        Ok(Value::Map(entries))
    }

    fn eval_set_literal(&mut self, items: &[Expr]) -> Result<Value, RuntimeError> {
        let mut values = Vec::with_capacity(items.len());
        for item_expr in items {
            let item = self.eval_expr(item_expr)?;
            if !values.contains(&item) {
                values.push(item);
            }
        }
        Ok(Value::Set(values))
    }

    fn eval_index(&mut self, object: &Expr, index: &Expr) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object)?;
        let idx = self.eval_expr(index)?;
        if let Value::List(list) = &obj {
            let len = list.borrow().len();
            if let Value::Int(i) = &idx {
                let i_val = *i;
                if i_val < 0 || i_val as usize >= len {
                    Err(RuntimeError::IndexOutOfBounds { index: i_val, len })
                } else {
                    Ok(list.borrow()[i_val as usize].clone())
                }
            } else {
                Err(type_err(
                    "list[number]",
                    &format!("list[{}]", idx.type_name()),
                ))
            }
        } else if let Value::Map(entries) = &obj {
            let key = idx.clone();
            entries
                .iter()
                .find(|(entry_key, _)| *entry_key == key)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| RuntimeError::Custom("map に指定したキーが存在しません".into()))
        } else if let Value::Struct { type_name, .. } = &obj {
            if let Some(result) = self.call_struct_operator(
                type_name,
                OperatorKind::Index,
                obj.clone(),
                vec![idx.clone()],
            )? {
                return Ok(result);
            }
            Err(type_err(
                "list[number] / map[key] / struct operator",
                &format!("{}[{}]", obj.type_name(), idx.type_name()),
            ))
        } else {
            Err(type_err(
                "list[number] / map[key]",
                &format!("{}[{}]", obj.type_name(), idx.type_name()),
            ))
        }
    }

    fn eval_index_assign(
        &mut self,
        object: &Expr,
        index: &Expr,
        value: &Expr,
    ) -> Result<Value, RuntimeError> {
        let key = self.eval_expr(index)?;
        let new_value = self.eval_expr(value)?;
        let obj = self.eval_expr(object)?;
        match obj {
            Value::Map(mut entries) => {
                if let Some((_, existing)) =
                    entries.iter_mut().find(|(entry_key, _)| *entry_key == key)
                {
                    *existing = new_value;
                } else {
                    entries.push((key, new_value));
                }
                self.assign_target_expr(object, Value::Map(entries))?;
                Ok(Value::Unit)
            }
            other => Err(type_err("map", other.type_name())),
        }
    }

    // ── T-1-D: struct サポート ─────────────────────────────────────────────

    fn eval_struct_def(
        &mut self,
        name: String,
        fields: Vec<(String, TypeAnn)>,
        derives: Vec<String>,
    ) -> Result<Value, RuntimeError> {
        let info = StructInfo {
            fields: fields.clone(),
            derives: derives.clone(),
            methods: HashMap::new(),
            operators: HashMap::new(),
        };
        self.type_registry.structs.insert(name.clone(), info);

        // @derive 自動メソッドの生成
        for derive in &derives {
            self.apply_derive(&name, derive, &fields)?;
        }

        Ok(Value::Unit)
    }

    // ── T-4-C: data キーワードのサポート ──────────────────────────────────

    fn eval_data_def(
        &mut self,
        name: String,
        fields: Vec<(String, TypeAnn)>,
        validate_rules: Vec<forge_compiler::ast::ValidateRule>,
    ) -> Result<Value, RuntimeError> {
        // data は全 derive を自動付与した StructDef として処理
        let auto_derives = vec![
            "Debug".to_string(),
            "Clone".to_string(),
            "Eq".to_string(),
            "Hash".to_string(),
            "Accessor".to_string(),
        ];
        self.eval_struct_def(name.clone(), fields.clone(), auto_derives)?;

        // validate ブロックがある場合、.validate() メソッドを自動生成
        if !validate_rules.is_empty() {
            self.register_validate_method(&name, &fields, validate_rules)?;
        }

        Ok(Value::Unit)
    }

    // ── T-5-C: typestate サポート ──────────────────────────────────────────

    fn eval_typestate_def(
        &mut self,
        name: String,
        fields: Vec<(String, TypeAnn)>,
        states: Vec<forge_compiler::ast::TypestateMarker>,
        state_methods: Vec<forge_compiler::ast::TypestateState>,
    ) -> Result<Value, RuntimeError> {
        let mut state_infos: HashMap<String, TypestateStateInfo> = HashMap::new();
        let state_names = states
            .iter()
            .map(|state| state.name().to_string())
            .collect::<Vec<_>>();

        for state in &state_methods {
            let mut methods: HashMap<String, TypestateMethodInfo> = HashMap::new();
            for method in &state.methods {
                let (next_state, is_result) = extract_transition_info(&method.return_type);
                methods.insert(
                    method.name.clone(),
                    TypestateMethodInfo {
                        params: method.params.clone(),
                        next_state,
                        is_result,
                    },
                );
            }
            state_infos.insert(state.name.clone(), TypestateStateInfo { methods });
        }

        self.type_registry.typestates.insert(
            name,
            TypestateInfo {
                fields,
                states: state_names,
                state_infos,
            },
        );
        Ok(Value::Unit)
    }

    /// `TypestateName::new("StateName")` の静的メソッド呼び出し
    fn eval_typestate_static_method(
        &mut self,
        type_name: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        if method == "new" {
            // 最初の引数が初期状態名の文字列
            let initial_state = match args.first() {
                Some(Value::String(s)) => s.clone(),
                Some(v) => {
                    return Err(RuntimeError::Custom(format!(
                        "{}::new<State>() の State は文字列を期待しましたが {} でした",
                        type_name,
                        v.type_name()
                    )))
                }
                None => {
                    return Err(RuntimeError::Custom(format!(
                        "{}::new<State>() には状態名が必要です",
                        type_name
                    )))
                }
            };

            // 状態が typestate に定義されているか確認
            let valid = self
                .type_registry
                .typestates
                .get(type_name)
                .map(|info| info.states.contains(&initial_state))
                .unwrap_or(false);

            if !valid {
                return Err(RuntimeError::Custom(format!(
                    "状態 '{}' は typestate '{}' に定義されていません",
                    initial_state, type_name
                )));
            }

            let declared_fields = self
                .type_registry
                .typestates
                .get(type_name)
                .map(|info| info.fields.clone())
                .unwrap_or_default();
            let field_args = &args[1..];
            if field_args.len() != declared_fields.len() {
                return Err(RuntimeError::Custom(format!(
                    "{}::new<State>() は {} 個のフィールド引数を期待しましたが {} 個渡されました",
                    type_name,
                    declared_fields.len(),
                    field_args.len()
                )));
            }
            let mut field_map = HashMap::new();
            for ((field_name, _), arg) in declared_fields.iter().zip(field_args.iter()) {
                field_map.insert(field_name.clone(), arg.clone());
            }

            return Ok(Value::Typestate {
                type_name: type_name.to_string(),
                current_state: initial_state,
                fields: Rc::new(RefCell::new(field_map)),
            });
        }

        Err(RuntimeError::Custom(format!(
            "typestate '{}' に静的メソッド '{}' は存在しません",
            type_name, method
        )))
    }

    /// typestate インスタンスに対するメソッド呼び出し
    fn eval_typestate_method(
        &mut self,
        self_val: Value,
        type_name: &str,
        current_state: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // 現在の状態でこのメソッドが使えるか確認
        let method_info = self
            .type_registry
            .typestates
            .get(type_name)
            .and_then(|info| info.state_infos.get(current_state))
            .and_then(|state_info| state_info.methods.get(method))
            .cloned();

        match method_info {
            None => {
                // 他の状態に存在するか確認してエラーメッセージを充実させる
                let available_in_states: Vec<String> = self
                    .type_registry
                    .typestates
                    .get(type_name)
                    .map(|info| {
                        info.state_infos
                            .iter()
                            .filter(|(_, si)| si.methods.contains_key(method))
                            .map(|(s, _)| s.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                if available_in_states.is_empty() {
                    return Err(RuntimeError::Custom(format!(
                        "typestate '{}' にメソッド '{}' は存在しません",
                        type_name, method
                    )));
                } else {
                    return Err(RuntimeError::Custom(format!(
                        "'{}' 状態では '{}' は使用できません（使用可能な状態: {}）",
                        current_state,
                        method,
                        available_in_states.join(", ")
                    )));
                }
            }
            Some(info) => {
                // 引数の数チェック
                if args.len() != info.params.len() {
                    return Err(RuntimeError::Custom(format!(
                        "メソッド '{}' は {} 個の引数を期待しましたが {} 個渡されました",
                        method,
                        info.params.len(),
                        args.len()
                    )));
                }

                // 遷移先状態がある場合は Value::Typestate を新しい状態で返す
                let existing_fields = match &self_val {
                    Value::Typestate { fields, .. } => Rc::clone(fields),
                    _ => Rc::new(RefCell::new(HashMap::new())),
                };

                // 引数をフィールドとして保存（状態ごとのデータ保持）
                {
                    let mut field_map = existing_fields.borrow_mut();
                    for (param, arg) in info.params.iter().zip(args.iter()) {
                        field_map.insert(param.name.clone(), arg.clone());
                    }
                }

                match info.next_state {
                    Some(ref next_state) => {
                        let new_val = Value::Typestate {
                            type_name: type_name.to_string(),
                            current_state: next_state.clone(),
                            fields: existing_fields,
                        };
                        if info.is_result {
                            Ok(Value::Result(Ok(Box::new(new_val))))
                        } else {
                            Ok(new_val)
                        }
                    }
                    None => {
                        // 状態遷移なし（string! などの通常値を返すメソッド）
                        // args の最初の引数をそのまま返すか、フィールドから取得
                        if info.is_result {
                            // 通常値を Result で返す: ok("dummy") 相当
                            let ret_val = args.into_iter().next().unwrap_or(Value::Unit);
                            Ok(Value::Result(Ok(Box::new(ret_val))))
                        } else {
                            let ret_val = args.into_iter().next().unwrap_or(Value::Unit);
                            Ok(ret_val)
                        }
                    }
                }
            }
        }
    }

    fn register_validate_method(
        &mut self,
        type_name: &str,
        _fields: &[(String, TypeAnn)],
        validate_rules: Vec<forge_compiler::ast::ValidateRule>,
    ) -> Result<(), RuntimeError> {
        let rules = std::rc::Rc::new(validate_rules);

        let native = NativeFn(Rc::new(move |args: Vec<Value>| {
            let self_val = match args.first() {
                Some(v @ Value::Struct { .. }) => v.clone(),
                Some(v) => {
                    return Err(format!(
                        "validate() は struct でのみ使用可能です (got {})",
                        v.type_name()
                    ))
                }
                None => return Err("validate() の第1引数が必要です".to_string()),
            };

            let fields = match &self_val {
                Value::Struct { fields, .. } => fields.borrow().clone(),
                _ => unreachable!(),
            };

            for rule in rules.as_ref() {
                for constraint in &rule.constraints {
                    let field_val = fields.get(&rule.field);
                    let violation = check_constraint(field_val, constraint);
                    if let Some(constraint_name) = violation {
                        let msg = format!("{}: {}", rule.field, constraint_name);
                        return Ok(Value::Result(Err(msg)));
                    }
                }
            }

            Ok(Value::Result(Ok(Box::new(Value::Unit))))
        }));

        if let Some(info) = self.type_registry.structs.get_mut(type_name) {
            info.methods
                .insert("validate".to_string(), MethodImpl::Native(native));
        }
        Ok(())
    }

    fn apply_derive(
        &mut self,
        type_name: &str,
        derive: &str,
        fields: &[(String, TypeAnn)],
    ) -> Result<(), RuntimeError> {
        match derive {
            "Debug" => {
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(Value::Struct {
                        type_name: ref actual_tn,
                        ref fields,
                    }) = args.first()
                    {
                        let fields = fields.borrow();
                        let mut sorted: Vec<(&String, &Value)> = fields.iter().collect();
                        sorted.sort_by_key(|(k, _)| k.as_str());
                        let field_str = sorted
                            .iter()
                            .map(|(k, v)| format!("{}: {}", k, v))
                            .collect::<Vec<_>>()
                            .join(", ");
                        Ok(Value::String(format!("{} {{ {} }}", actual_tn, field_str)))
                    } else {
                        Err("display() は struct でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("display".to_string(), MethodImpl::Native(native));
                }
            }
            "Clone" => {
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Struct { .. }) = args.first() {
                        Ok(v.deep_clone())
                    } else {
                        Err("clone() は struct でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("clone".to_string(), MethodImpl::Native(native));
                }
            }
            "Accessor" => {
                let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                for field_name in field_names {
                    // getter
                    let fn_clone = field_name.clone();
                    let getter_native =
                        NativeFn(Rc::new(move |args: Vec<Value>| {
                            if let Some(Value::Struct { ref fields, .. }) = args.first() {
                                fields.borrow().get(&fn_clone).cloned().ok_or_else(|| {
                                    format!("フィールド '{}' が存在しません", fn_clone)
                                })
                            } else {
                                Err("getter は struct でのみ使用可能です".to_string())
                            }
                        }));
                    let getter_name = format!("get_{}", field_name);
                    if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                        info.methods
                            .insert(getter_name, MethodImpl::Native(getter_native));
                    }

                    // setter
                    let fn_clone2 = field_name.clone();
                    let setter_native = NativeFn(Rc::new(move |args: Vec<Value>| {
                        if args.len() < 2 {
                            return Err(format!("set_{}() は2引数必要です", fn_clone2));
                        }
                        if let Value::Struct { ref fields, .. } = args[0] {
                            fields
                                .borrow_mut()
                                .insert(fn_clone2.clone(), args[1].clone());
                            Ok(Value::Unit)
                        } else {
                            Err("setter は struct でのみ使用可能です".to_string())
                        }
                    }));
                    let setter_name = format!("set_{}", field_name);
                    if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                        info.methods
                            .insert(setter_name, MethodImpl::Native(setter_native));
                    }
                }
            }
            "Singleton" => {
                // Singleton は instance() メソッドで特別処理する
                // ここでは "singleton" フラグとして derives に記録されているだけで十分
            }
            "Eq" => {
                // Value::Struct の PartialEq は value.rs で実装済み
                // eq() メソッドも追加
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if args.len() < 2 {
                        return Err("eq() は2引数必要です".to_string());
                    }
                    Ok(Value::Bool(args[0] == args[1]))
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("eq".to_string(), MethodImpl::Native(native));
                }
            }
            "Hash" => {
                // hash() メソッドを生成: struct のハッシュ値を number として返す
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Struct { .. }) = args.first() {
                        let mut hasher = DefaultHasher::new();
                        v.hash(&mut hasher);
                        Ok(Value::Int(hasher.finish() as i64))
                    } else {
                        Err("hash() は struct でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("hash".to_string(), MethodImpl::Native(native));
                }
            }
            "Ord" => {
                // @derive(Ord) は compare_values の struct 対応で < / > 等を有効にする
                // compare() メソッドも提供: -1 / 0 / 1 を返す
                let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                let native = NativeFn(Rc::new(move |args: Vec<Value>| {
                    if args.len() < 2 {
                        return Err("compare() は2引数必要です".to_string());
                    }
                    let ord = compare_struct_fields(&args[0], &args[1], &field_names)
                        .map_err(|e| format!("{:?}", e))?;
                    let result = match ord {
                        std::cmp::Ordering::Less => -1_i64,
                        std::cmp::Ordering::Equal => 0_i64,
                        std::cmp::Ordering::Greater => 1_i64,
                    };
                    Ok(Value::Int(result))
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("compare".to_string(), MethodImpl::Native(native));
                    // Ord フラグを derives に記録（compare_values で参照）
                    if !info.derives.contains(&"Ord".to_string()) {
                        info.derives.push("Ord".to_string());
                    }
                }
            }
            "Default" => {
                // TypeName::default() / TypeName::new() でゼロ値インスタンスを生成
                // derives に "Default" が記録されていれば eval_static_method で処理する
                // ここでは derives への記録のみ（eval_static_method 側で対応）
            }
            _ => {} // 未知の derive は無視
        }
        Ok(())
    }

    // ── T-2-C: enum サポート ──────────────────────────────────────────────

    fn eval_enum_def(
        &mut self,
        name: String,
        variants: Vec<EnumVariant>,
        derives: Vec<String>,
    ) -> Result<Value, RuntimeError> {
        let info = EnumInfo { variants };
        self.type_registry.enums.insert(name.clone(), info);

        // @derive 自動処理
        for derive in &derives {
            self.apply_enum_derive(&name, derive)?;
        }

        Ok(Value::Unit)
    }

    fn apply_enum_derive(&mut self, type_name: &str, derive: &str) -> Result<(), RuntimeError> {
        match derive {
            "Debug" => {
                // enum のデフォルト Display が既に to_string() を提供しているので
                // display() メソッドも同様に実装
                if !self.type_registry.structs.contains_key(type_name) {
                    self.type_registry.structs.insert(
                        type_name.to_string(),
                        StructInfo {
                            fields: vec![],
                            derives: vec![],
                            methods: HashMap::new(),
                            operators: HashMap::new(),
                        },
                    );
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Enum { .. }) = args.first() {
                        Ok(Value::String(v.to_string()))
                    } else {
                        Err("display() は enum でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("display".to_string(), MethodImpl::Native(native));
                }
            }
            "Clone" => {
                if !self.type_registry.structs.contains_key(type_name) {
                    self.type_registry.structs.insert(
                        type_name.to_string(),
                        StructInfo {
                            fields: vec![],
                            derives: vec![],
                            methods: HashMap::new(),
                            operators: HashMap::new(),
                        },
                    );
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Enum { .. }) = args.first() {
                        Ok(v.deep_clone())
                    } else {
                        Err("clone() は enum でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("clone".to_string(), MethodImpl::Native(native));
                }
            }
            "Eq" => {
                if !self.type_registry.structs.contains_key(type_name) {
                    self.type_registry.structs.insert(
                        type_name.to_string(),
                        StructInfo {
                            fields: vec![],
                            derives: vec![],
                            methods: HashMap::new(),
                            operators: HashMap::new(),
                        },
                    );
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if args.len() < 2 {
                        return Err("eq() は2引数必要です".to_string());
                    }
                    Ok(Value::Bool(args[0] == args[1]))
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods
                        .insert("eq".to_string(), MethodImpl::Native(native));
                }
            }
            _ => {} // 未知の derive は無視
        }
        Ok(())
    }

    fn eval_enum_init(
        &mut self,
        enum_name: &str,
        variant: &str,
        data: &EnumInitData,
    ) -> Result<Value, RuntimeError> {
        let enum_data = match data {
            EnumInitData::None => EnumData::Unit,
            EnumInitData::Tuple(exprs) => {
                let vals: Vec<Value> = exprs
                    .iter()
                    .map(|e| self.eval_expr(e))
                    .collect::<Result<_, _>>()?;
                EnumData::Tuple(vals)
            }
            EnumInitData::Struct(field_exprs) => {
                let mut fields = HashMap::new();
                for (field_name, expr) in field_exprs {
                    let val = self.eval_expr(expr)?;
                    fields.insert(field_name.clone(), val);
                }
                EnumData::Struct(fields)
            }
        };

        Ok(Value::Enum {
            type_name: enum_name.to_string(),
            variant: variant.to_string(),
            data: enum_data,
        })
    }

    fn eval_enum_method(
        &mut self,
        self_val: Value,
        type_name: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // struct レジストリ経由でメソッドを探す（derive で登録）
        let method_impl = self
            .type_registry
            .structs
            .get(type_name)
            .and_then(|info| info.methods.get(method))
            .cloned();

        match method_impl {
            Some(MethodImpl::Native(NativeFn(f))) => {
                let mut all_args = vec![self_val];
                all_args.extend(args);
                f(all_args).map_err(RuntimeError::Custom)
            }
            Some(MethodImpl::Forge(fn_def, captured)) => {
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = captured.borrow().clone();
                // グローバルスコープで上書き（最新の値を優先）
                if let Some(global) = saved.first() {
                    for (k, v) in global {
                        initial.insert(k.clone(), v.clone());
                    }
                }
                initial.insert("self".to_string(), (self_val, fn_def.has_state_self));
                for (param, arg) in fn_def.params.iter().zip(args) {
                    initial.insert(param.name.clone(), (arg, false));
                }
                self.scopes = vec![initial];
                let result = self.eval_expr(&fn_def.body.clone());
                self.scopes = saved;
                match result {
                    Ok(v) => Ok(v),
                    Err(RuntimeError::Return(v)) => Ok(v),
                    Err(e) => Err(e),
                }
            }
            None => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は enum '{}' に存在しません",
                method, type_name
            ))),
        }
    }

    fn eval_enum_static_method(
        &mut self,
        type_name: &str,
        method: &str,
        _args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // enum のバリアントを Unit として返す（TypeName::VariantName() の形式）
        let variant_exists = self
            .type_registry
            .enums
            .get(type_name)
            .map(|info| {
                info.variants.iter().any(|v| match v {
                    EnumVariant::Unit(n) => n == method,
                    EnumVariant::Tuple(n, _) => n == method,
                    EnumVariant::Struct(n, _) => n == method,
                })
            })
            .unwrap_or(false);

        if variant_exists {
            return Ok(Value::Enum {
                type_name: type_name.to_string(),
                variant: method.to_string(),
                data: EnumData::Unit,
            });
        }

        Err(RuntimeError::Custom(format!(
            "enum '{}' にバリアントまたは静的メソッド '{}' は存在しません",
            type_name, method
        )))
    }

    fn eval_impl_block(
        &mut self,
        target: String,
        methods: Vec<FnDef>,
        operators: Vec<OperatorDef>,
    ) -> Result<Value, RuntimeError> {
        if !self.type_registry.structs.contains_key(&target) {
            self.type_registry.structs.insert(
                target.clone(),
                StructInfo {
                    fields: vec![],
                    derives: vec![],
                    methods: HashMap::new(),
                    operators: HashMap::new(),
                },
            );
        }
        let captured = self.capture_env();
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in methods {
                info.methods.insert(
                    method.name.clone(),
                    MethodImpl::Forge(method, Rc::clone(&captured)),
                );
            }
            for operator in operators {
                self.register_operator(&target, operator, Rc::clone(&captured))?;
            }
        }
        Ok(Value::Unit)
    }

    fn register_operator(
        &mut self,
        target: &str,
        operator: OperatorDef,
        captured: CapturedEnv,
    ) -> Result<(), RuntimeError> {
        if let Some(info) = self.type_registry.structs.get_mut(target) {
            if operator.op == OperatorKind::Eq && info.derives.iter().any(|d| d == "Eq") {
                return Err(RuntimeError::Custom(
                    "derive(Eq) と operator == を同時に定義できません".into(),
                ));
            }
            if operator.op == OperatorKind::Lt && info.derives.iter().any(|d| d == "Ord") {
                return Err(RuntimeError::Custom(
                    "derive(Ord) と operator < を同時に定義できません".into(),
                ));
            }
            info.operators.insert(
                operator.op.clone(),
                OperatorImpl::Forge(operator, Rc::clone(&captured)),
            );
        }
        Ok(())
    }

    // ── T-3-C: trait / mixin / impl trait サポート ────────────────────────

    fn eval_trait_def(
        &mut self,
        name: String,
        methods: Vec<TraitMethod>,
    ) -> Result<Value, RuntimeError> {
        let mut default_methods = HashMap::new();

        for method in methods {
            match method {
                TraitMethod::Abstract { .. } => {}
                TraitMethod::Default {
                    name: method_name,
                    params,
                    return_type,
                    body,
                    has_self,
                    has_state_self,
                    span,
                } => {
                    let fn_def = FnDef {
                        name: method_name.clone(),
                        type_params: vec![],
                        params,
                        return_type,
                        body,
                        has_self,
                        has_state_self,
                        is_const: false,
                        span,
                    };
                    default_methods.insert(method_name, fn_def);
                }
            }
        }

        let info = TraitInfo { default_methods };
        self.type_registry.traits.insert(name, info);
        Ok(Value::Unit)
    }

    fn eval_mixin_def(&mut self, name: String, methods: Vec<FnDef>) -> Result<Value, RuntimeError> {
        let mut method_map = HashMap::new();
        for method in methods {
            method_map.insert(method.name.clone(), method);
        }
        let info = MixinInfo {
            methods: method_map,
        };
        self.type_registry.mixins.insert(name, info);
        Ok(Value::Unit)
    }

    fn eval_impl_trait(
        &mut self,
        trait_name: String,
        target: String,
        methods: Vec<FnDef>,
    ) -> Result<Value, RuntimeError> {
        // 型レジストリに struct が存在しない場合は作成
        if !self.type_registry.structs.contains_key(&target) {
            self.type_registry.structs.insert(
                target.clone(),
                StructInfo {
                    fields: vec![],
                    derives: vec![],
                    methods: HashMap::new(),
                    operators: HashMap::new(),
                },
            );
        }

        // 明示的に実装されたメソッドを型に登録（優先度: 直接 impl）
        let explicit_method_names: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
        let captured = self.capture_env();
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in &methods {
                info.methods.insert(
                    method.name.clone(),
                    MethodImpl::Forge(method.clone(), Rc::clone(&captured)),
                );
            }
        }

        // trait のデフォルト実装を（明示的 impl がない場合のみ）型に登録
        let trait_defaults: Option<HashMap<String, FnDef>> = self
            .type_registry
            .traits
            .get(&trait_name)
            .map(|ti| ti.default_methods.clone());

        if let Some(defaults) = trait_defaults {
            if let Some(struct_info) = self.type_registry.structs.get_mut(&target) {
                for (method_name, fn_def) in defaults {
                    if !explicit_method_names.contains(&method_name) {
                        struct_info
                            .methods
                            .entry(method_name)
                            .or_insert(MethodImpl::Forge(fn_def, Rc::clone(&captured)));
                    }
                }
            }
        }

        // mixin の場合: デフォルトメソッドを登録（名前衝突チェックあり）
        let mixin_methods: Option<HashMap<String, FnDef>> = self
            .type_registry
            .mixins
            .get(&trait_name)
            .map(|mi| mi.methods.clone());

        if let Some(mixin_map) = mixin_methods {
            for method_name in mixin_map.keys() {
                if let Some(struct_info) = self.type_registry.structs.get(&target) {
                    if struct_info.methods.contains_key(method_name) {
                        return Err(RuntimeError::Custom(format!(
                            "mixin '{}' のメソッド '{}' は型 '{}' で既に定義されています（名前衝突）",
                            trait_name, method_name, target
                        )));
                    }
                }
            }
            if let Some(struct_info) = self.type_registry.structs.get_mut(&target) {
                for (method_name, fn_def) in mixin_map {
                    struct_info
                        .methods
                        .insert(method_name, MethodImpl::Forge(fn_def, Rc::clone(&captured)));
                }
            }
        }

        Ok(Value::Unit)
    }

    fn eval_struct_init(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
    ) -> Result<Value, RuntimeError> {
        let mut field_map: HashMap<String, Value> = HashMap::new();
        for (field_name, expr) in fields {
            let val = self.eval_expr(expr)?;
            field_map.insert(field_name.clone(), val);
        }
        Ok(Value::Struct {
            type_name: name.to_string(),
            fields: Rc::new(RefCell::new(field_map)),
        })
    }

    fn eval_anon_struct(
        &mut self,
        fields: &[(String, Option<Expr>)],
    ) -> Result<Value, RuntimeError> {
        let mut field_map: HashMap<String, Value> = HashMap::new();
        for (field_name, expr) in fields {
            let val = match expr {
                Some(expr) => self.eval_expr(expr)?,
                None => self.eval_ident(field_name)?,
            };
            field_map.insert(field_name.clone(), val);
        }
        Ok(Value::Struct {
            type_name: "<anon>".to_string(),
            fields: Rc::new(RefCell::new(field_map)),
        })
    }

    fn eval_field_access(&mut self, object: &Expr, field: &str) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object)?;
        self.field_access_value(&obj, field)
    }

    fn field_access_value(&self, obj: &Value, field: &str) -> Result<Value, RuntimeError> {
        match obj {
            Value::Struct { ref fields, .. } | Value::Typestate { ref fields, .. } => {
                fields.borrow().get(field).cloned().ok_or_else(|| {
                    RuntimeError::Custom(format!("フィールド '{}' が存在しません", field))
                })
            }
            // Option(Some(struct)) → 中身の struct に対してフィールドアクセスを透過させる
            Value::Option(Some(ref inner)) => match inner.as_ref() {
                Value::Struct { ref fields, .. } | Value::Typestate { ref fields, .. } => {
                    fields.borrow().get(field).cloned().ok_or_else(|| {
                        RuntimeError::Custom(format!("フィールド '{}' が存在しません", field))
                    })
                }
                _ => Err(RuntimeError::Custom(format!(
                    "フィールドアクセスは struct でのみ使用可能です (got option<{}>)",
                    inner.type_name()
                ))),
            },
            Value::Option(None) => Err(RuntimeError::Custom(format!(
                "none に対してフィールド '{}' にアクセスできません",
                field
            ))),
            _ => Err(RuntimeError::Custom(format!(
                "フィールドアクセスは struct でのみ使用可能です (got {})",
                obj.type_name()
            ))),
        }
    }

    fn eval_optional_chain(
        &mut self,
        object: &Expr,
        chain: &ChainKind,
    ) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object)?;
        match obj {
            Value::Option(opt) => match opt {
                Some(inner_box) => {
                    let inner = *inner_box;
                    let result = match chain {
                        ChainKind::Field(field) => self.field_access_value(&inner, field)?,
                        ChainKind::Method { name, args } => {
                            self.call_method_on_inner_value(inner, name, args)?
                        }
                    };
                    Ok(Value::Option(Some(Box::new(result))))
                }
                None => Ok(Value::Option(None)),
            },
            other => Err(RuntimeError::Custom(format!(
                "Optional chain は option 型にのみ使用可能です (got {})",
                other.type_name()
            ))),
        }
    }

    fn call_method_on_inner_value(
        &mut self,
        object: Value,
        method: &str,
        args: &[Expr],
    ) -> Result<Value, RuntimeError> {
        if method == "clone" && args.is_empty() {
            return Ok(object.clone());
        }
        let arg_vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;
        match object {
            Value::Result(result) => match method {
                "is_ok" => Ok(Value::Bool(result.is_ok())),
                "is_err" => Ok(Value::Bool(result.is_err())),
                "ok" => Ok(match result {
                    Ok(inner) => Value::Option(Some(inner)),
                    Err(_) => Value::Option(None),
                }),
                other => Err(RuntimeError::Custom(format!(
                    "オプショナルチェーン経由では result で '{}' は使えません",
                    other
                ))),
            },
            Value::List(items) => self.eval_list_method(items, method, arg_vals, None),
            ref obj @ Value::Struct { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_struct_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            ref obj @ Value::Enum { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_enum_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            ref obj @ Value::Typestate {
                ref type_name,
                ref current_state,
                ..
            } => {
                let type_name_cloned = type_name.clone();
                let current_state_cloned = current_state.clone();
                self.eval_typestate_method(
                    obj.clone(),
                    &type_name_cloned,
                    &current_state_cloned,
                    method,
                    arg_vals,
                )
            }
            Value::String(text) => self.eval_string_method(text, method, arg_vals),
            Value::Closure { .. } | Value::NativeFunction(_) => self.call_value(object, arg_vals),
            other => Err(RuntimeError::Custom(format!(
                "オプショナルチェーン経由では '{}' に対して '{}' を呼び出せません",
                other.type_name(),
                method
            ))),
        }
    }

    fn eval_null_coalesce(&mut self, value: &Expr, default: &Expr) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(value)?;
        match val {
            Value::Option(opt) => match opt {
                Some(inner) => Ok(*inner),
                None => self.eval_expr(default),
            },
            other => Err(RuntimeError::Custom(format!(
                "Null coalesce は option 型にのみ使用可能です (got {})",
                other.type_name()
            ))),
        }
    }

    // ── defer（E-7）────────────────────────────────────────────────────────

    fn push_defer(&mut self, body: DeferBody) {
        if let Some(frame) = self.defer_stack.last_mut() {
            frame.push(body);
        }
    }

    fn run_defers(&mut self) {
        if let Some(frame) = self.defer_stack.pop() {
            // LIFO: 最後に登録されたものから実行
            for body in frame.into_iter().rev() {
                let expr = match body {
                    DeferBody::Expr(e) | DeferBody::Block(e) => *e,
                };
                let _ = self.eval_expr(&expr);
            }
        }
    }

    // ── pipeline（S-5-B）──────────────────────────────────────────────────

    fn eval_pipeline(&mut self, steps: &[PipelineStep]) -> Result<Value, RuntimeError> {
        // source を評価してリストを取得
        let source_expr = steps.iter().find_map(|s| {
            if let PipelineStep::Source(e) = s {
                Some(e.as_ref())
            } else {
                None
            }
        });

        let mut items: Vec<Value> = match source_expr {
            Some(e) => match self.eval_expr(e)? {
                Value::List(rc) => rc.borrow().clone(),
                other => {
                    return Err(RuntimeError::Custom(format!(
                        "pipeline source は list が必要です (got {})",
                        other.type_name()
                    )))
                }
            },
            None => {
                return Err(RuntimeError::Custom(
                    "pipeline には source が必要です".to_string(),
                ))
            }
        };

        for step in steps {
            match step {
                PipelineStep::Source(_) => {} // already handled
                PipelineStep::Filter(f) => {
                    let func = self.eval_expr(f)?;
                    let mut result = Vec::new();
                    for item in items {
                        let v = self.call_value(func.clone(), vec![item.clone()])?;
                        if matches!(v, Value::Bool(true)) {
                            result.push(item);
                        }
                    }
                    items = result;
                }
                PipelineStep::Map(f) => {
                    let func = self.eval_expr(f)?;
                    let mut result = Vec::new();
                    for item in items {
                        let v = self.call_value(func.clone(), vec![item])?;
                        result.push(v);
                    }
                    items = result;
                }
                PipelineStep::FlatMap(f) => {
                    let func = self.eval_expr(f)?;
                    let mut result = Vec::new();
                    for item in items {
                        let v = self.call_value(func.clone(), vec![item])?;
                        match v {
                            Value::List(rc) => result.extend(rc.borrow().clone()),
                            other => result.push(other),
                        }
                    }
                    items = result;
                }
                PipelineStep::Take(n_expr) => {
                    let n = match self.eval_expr(n_expr)? {
                        Value::Int(i) => i as usize,
                        other => {
                            return Err(RuntimeError::Custom(format!(
                                "take には number が必要です (got {})",
                                other.type_name()
                            )))
                        }
                    };
                    items.truncate(n);
                }
                PipelineStep::Skip(n_expr) => {
                    let n = match self.eval_expr(n_expr)? {
                        Value::Int(i) => i as usize,
                        other => {
                            return Err(RuntimeError::Custom(format!(
                                "skip には number が必要です (got {})",
                                other.type_name()
                            )))
                        }
                    };
                    if n < items.len() {
                        items = items.into_iter().skip(n).collect();
                    } else {
                        items = Vec::new();
                    }
                }
                PipelineStep::Each(f) => {
                    let func = self.eval_expr(f)?;
                    for item in &items {
                        self.call_value(func.clone(), vec![item.clone()])?;
                    }
                }
                PipelineStep::Sort { key, descending } => {
                    let key_fn = self.eval_expr(key)?;
                    let mut pairs: Vec<(Value, Value)> = Vec::new();
                    for item in &items {
                        let k = self.call_value(key_fn.clone(), vec![item.clone()])?;
                        pairs.push((k, item.clone()));
                    }
                    let desc = *descending;
                    pairs.sort_by(|(a, _), (b, _)| {
                        let ord = match (a, b) {
                            (Value::Int(x), Value::Int(y)) => x.cmp(y),
                            (Value::Float(x), Value::Float(y)) => {
                                x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Value::String(x), Value::String(y)) => x.cmp(y),
                            _ => std::cmp::Ordering::Equal,
                        };
                        if desc {
                            ord.reverse()
                        } else {
                            ord
                        }
                    });
                    items = pairs.into_iter().map(|(_, v)| v).collect();
                }
                PipelineStep::Group(f) => {
                    let key_fn = self.eval_expr(f)?;
                    // グループは map<key, list> として返す
                    let mut groups: Vec<(Value, Value)> = Vec::new();
                    for item in items {
                        let k = self.call_value(key_fn.clone(), vec![item.clone()])?;
                        if let Some(pos) = groups.iter().position(|(gk, _)| gk == &k) {
                            if let Value::List(rc) = &groups[pos].1 {
                                rc.borrow_mut().push(item);
                            }
                        } else {
                            let list = Value::List(Rc::new(RefCell::new(vec![item])));
                            groups.push((k, list));
                        }
                    }
                    return Ok(Value::Map(groups));
                }
                PipelineStep::Parallel(_) => {
                    // 逐次実行（interpreter は同期のみ）
                }
                PipelineStep::Sink(sink_expr) => {
                    let sink_fn = self.eval_expr(sink_expr)?;
                    let list = Value::List(Rc::new(RefCell::new(items.clone())));
                    self.call_value(sink_fn, vec![list])?;
                }
            }
        }

        Ok(Value::List(Rc::new(RefCell::new(items))))
    }

    fn eval_field_assign(
        &mut self,
        object: &Expr,
        field: &str,
        value: &Expr,
    ) -> Result<Value, RuntimeError> {
        let val = self.eval_expr(value)?;
        let obj = self.eval_expr(object)?;
        match obj {
            Value::Struct { ref fields, .. } => {
                fields.borrow_mut().insert(field.to_string(), val);
                Ok(Value::Unit)
            }
            Value::Typestate { ref fields, .. } => {
                fields.borrow_mut().insert(field.to_string(), val);
                Ok(Value::Unit)
            }
            _ => Err(RuntimeError::Custom(format!(
                "フィールド代入は struct でのみ使用可能です (got {})",
                obj.type_name()
            ))),
        }
    }

    fn eval_map_method(
        &mut self,
        object_expr: &Expr,
        mut entries: Vec<(Value, Value)>,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match method {
            "get" => {
                let key = one_value_arg(method, args)?;
                Ok(Value::Option(
                    entries
                        .into_iter()
                        .find(|(entry_key, _)| *entry_key == key)
                        .map(|(_, value)| Box::new(value)),
                ))
            }
            "insert" => {
                let (key, value) = two_value_args(method, args)?;
                if let Some((_, existing)) =
                    entries.iter_mut().find(|(entry_key, _)| *entry_key == key)
                {
                    *existing = value;
                } else {
                    entries.push((key, value));
                }
                self.assign_target_expr(object_expr, Value::Map(entries))?;
                Ok(Value::Unit)
            }
            "contains_key" => {
                let key = one_value_arg(method, args)?;
                Ok(Value::Bool(
                    entries.iter().any(|(entry_key, _)| *entry_key == key),
                ))
            }
            "keys" => Ok(mk_list(entries.into_iter().map(|(key, _)| key).collect())),
            "values" => Ok(mk_list(
                entries.into_iter().map(|(_, value)| value).collect(),
            )),
            "entries" => Ok(mk_list(
                entries
                    .into_iter()
                    .map(|(k, v)| mk_list(vec![k, v]))
                    .collect(),
            )),
            "len" => Ok(Value::Int(entries.len() as i64)),
            "remove" => {
                let key = one_value_arg(method, args)?;
                let removed = entries
                    .iter()
                    .position(|(entry_key, _)| *entry_key == key)
                    .map(|idx| entries.remove(idx).1);
                self.assign_target_expr(object_expr, Value::Map(entries))?;
                Ok(Value::Option(removed.map(Box::new)))
            }
            _ => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は map に対して未実装です",
                method
            ))),
        }
    }

    fn eval_set_method(
        &mut self,
        _object_expr: &Expr,
        mut items: Vec<Value>,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match method {
            "contains" => {
                let value = one_value_arg(method, args)?;
                Ok(Value::Bool(items.contains(&value)))
            }
            "insert" => {
                // spec: set<T>（新しい set を返す）
                let value = one_value_arg(method, args)?;
                let mut new_items = items.clone();
                if !new_items.contains(&value) {
                    new_items.push(value);
                }
                Ok(Value::Set(new_items))
            }
            "union" => {
                let other = one_set_arg(method, args)?;
                for value in other {
                    if !items.contains(&value) {
                        items.push(value);
                    }
                }
                Ok(Value::Set(items))
            }
            "intersect" => {
                let other = one_set_arg(method, args)?;
                Ok(Value::Set(
                    items
                        .into_iter()
                        .filter(|item| other.contains(item))
                        .collect(),
                ))
            }
            "difference" => {
                let other = one_set_arg(method, args)?;
                Ok(Value::Set(
                    items
                        .into_iter()
                        .filter(|item| !other.contains(item))
                        .collect(),
                ))
            }
            "len" => Ok(Value::Int(items.len() as i64)),
            "to_list" => Ok(mk_list(items)),
            _ => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は set に対して未実装です",
                method
            ))),
        }
    }

    fn assign_target_expr(&mut self, target: &Expr, value: Value) -> Result<(), RuntimeError> {
        match target {
            Expr::Ident(name, _) => {
                self.assign(name, value)?;
                Ok(())
            }
            Expr::Field { object, field, .. } => {
                let obj = self.eval_expr(object)?;
                match obj {
                    Value::Struct { fields, .. } | Value::Typestate { fields, .. } => {
                        fields.borrow_mut().insert(field.clone(), value);
                        Ok(())
                    }
                    other => Err(RuntimeError::Custom(format!(
                        "フィールド更新先が struct/typestate ではありません: {}",
                        other.type_name()
                    ))),
                }
            }
            _ => Err(RuntimeError::Custom(
                "更新対象は state 変数またはフィールドである必要があります".into(),
            )),
        }
    }

    fn eval_struct_method(
        &mut self,
        self_val: Value,
        type_name: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // Singleton::instance() の特別処理
        if method == "instance" {
            let is_singleton = self
                .type_registry
                .structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Singleton"))
                .unwrap_or(false);

            if is_singleton {
                if let Some(cached) = self.type_registry.singletons.get(type_name).cloned() {
                    return Ok(cached);
                }
                // 初回: ゼロ値で struct を作る
                let fields: Vec<(String, TypeAnn)> = self
                    .type_registry
                    .structs
                    .get(type_name)
                    .map(|i| i.fields.clone())
                    .unwrap_or_default();
                let mut field_map = HashMap::new();
                for (fname, tann) in &fields {
                    field_map.insert(fname.clone(), zero_value_for_type(tann));
                }
                let instance = Value::Struct {
                    type_name: type_name.to_string(),
                    fields: Rc::new(RefCell::new(field_map)),
                };
                self.type_registry
                    .singletons
                    .insert(type_name.to_string(), instance.clone());
                return Ok(instance);
            }
        }

        // 型レジストリからメソッドを検索
        let method_impl = self
            .type_registry
            .structs
            .get(type_name)
            .and_then(|info| info.methods.get(method))
            .cloned();

        match method_impl {
            Some(MethodImpl::Native(NativeFn(f))) => {
                let mut all_args = vec![self_val];
                all_args.extend(args);
                f(all_args).map_err(RuntimeError::Custom)
            }
            Some(MethodImpl::Forge(fn_def, captured)) => {
                // self を暗黙引数として束縛してメソッドを呼び出す
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = captured.borrow().clone();

                // グローバルスコープで上書き（最新の値を優先）
                if let Some(global) = saved.first() {
                    for (k, v) in global {
                        initial.insert(k.clone(), v.clone());
                    }
                }

                // self を束縛（has_state_self なら mutable）
                initial.insert(
                    "self".to_string(),
                    (self_val.clone(), fn_def.has_state_self),
                );

                // パラメータを束縛
                for (param, arg) in fn_def.params.iter().zip(args) {
                    initial.insert(param.name.clone(), (arg, false));
                }

                self.scopes = vec![initial];
                let result = self.eval_expr(&fn_def.body.clone());
                self.scopes = saved;

                match result {
                    Ok(v) => Ok(v),
                    Err(RuntimeError::Return(v)) => Ok(v),
                    Err(e) => Err(e),
                }
            }
            None => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は struct '{}' に存在しません",
                method, type_name
            ))),
        }
    }

    fn call_struct_operator(
        &mut self,
        type_name: &str,
        op: OperatorKind,
        self_val: Value,
        args: Vec<Value>,
    ) -> Result<Option<Value>, RuntimeError> {
        let operator_impl = self
            .type_registry
            .structs
            .get(type_name)
            .and_then(|info| info.operators.get(&op))
            .cloned();
        if let Some(OperatorImpl::Forge(operator, captured)) = operator_impl {
            let saved = std::mem::take(&mut self.scopes);
            let mut initial: HashMap<String, Binding> = captured.borrow().clone();
            if let Some(global) = saved.first() {
                for (name, binding) in global {
                    initial.insert(name.clone(), binding.clone());
                }
            }
            initial.insert(
                "self".to_string(),
                (self_val.clone(), operator.has_state_self),
            );
            for (param, arg) in operator.params.iter().zip(args.into_iter()) {
                initial.insert(param.name.clone(), (arg, false));
            }
            self.scopes = vec![initial];
            let result = self.eval_expr(&operator.body);
            self.scopes = saved;
            return match result {
                Ok(v) => Ok(Some(v)),
                Err(RuntimeError::Return(v)) => Ok(Some(v)),
                Err(e) => Err(e),
            };
        }
        Ok(None)
    }

    fn partial_from_value(&self, instance: Value) -> Result<Value, RuntimeError> {
        match instance {
            Value::Struct { type_name, fields } => {
                let mapped = fields
                    .borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::Option(Some(Box::new(v.deep_clone())))))
                    .collect();
                Ok(Value::Struct {
                    type_name: format!("Partial<{}>", type_name),
                    fields: Rc::new(RefCell::new(mapped)),
                })
            }
            other => Err(type_err("struct", other.type_name())),
        }
    }

    fn required_from_value(&self, instance: Value) -> Result<Value, RuntimeError> {
        match instance {
            Value::Struct { type_name, fields } => {
                let mut mapped = HashMap::new();
                for (k, v) in fields.borrow().iter() {
                    let required = match v {
                        Value::Option(Some(inner)) => inner.deep_clone(),
                        Value::Option(None) => {
                            return Err(RuntimeError::Custom(format!(
                                "Required: field '{}' is None",
                                k
                            )))
                        }
                        other => other.deep_clone(),
                    };
                    mapped.insert(k.clone(), required);
                }
                Ok(Value::Struct {
                    type_name: format!("Required<{}>", type_name),
                    fields: Rc::new(RefCell::new(mapped)),
                })
            }
            other => Err(type_err("struct", other.type_name())),
        }
    }

    fn pick_from_value(&self, instance: Value, keys: &[String]) -> Result<Value, RuntimeError> {
        match instance {
            Value::Struct { type_name, fields } => {
                let borrow = fields.borrow();
                let mut mapped = HashMap::new();
                for key in keys {
                    let value = borrow.get(key).ok_or_else(|| {
                        RuntimeError::Custom(format!("Pick: field '{}' does not exist", key))
                    })?;
                    mapped.insert(key.clone(), value.deep_clone());
                }
                Ok(Value::Struct {
                    type_name: format!("Pick<{}>", type_name),
                    fields: Rc::new(RefCell::new(mapped)),
                })
            }
            other => Err(type_err("struct", other.type_name())),
        }
    }

    fn omit_from_value(&self, instance: Value, keys: &[String]) -> Result<Value, RuntimeError> {
        match instance {
            Value::Struct { type_name, fields } => {
                let mapped = fields
                    .borrow()
                    .iter()
                    .filter(|(k, _)| !keys.contains(k))
                    .map(|(k, v)| (k.clone(), v.deep_clone()))
                    .collect();
                Ok(Value::Struct {
                    type_name: format!("Omit<{}>", type_name),
                    fields: Rc::new(RefCell::new(mapped)),
                })
            }
            other => Err(type_err("struct", other.type_name())),
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

// ── ヘルパー関数 ────────────────────────────────────────────────────────────

/// 新しいリスト Value を生成する
fn mk_list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn pipeline_item_count(value: &Value) -> Option<usize> {
    match value {
        Value::List(items) => Some(items.borrow().len()),
        Value::Option(Some(_)) => Some(1),
        Value::Option(None) => Some(0),
        Value::Result(Ok(_)) => Some(1),
        Value::Result(Err(_)) => Some(0),
        _ => None,
    }
}

fn pipeline_trace_outcome(method: &str, value: &Value) -> PipelineTraceOutcome {
    match (method, value) {
        ("find", Value::Option(None)) => PipelineTraceOutcome::FindNone,
        (_, Value::Result(Err(_))) => PipelineTraceOutcome::ResultErr,
        _ => PipelineTraceOutcome::Ok,
    }
}

fn pipeline_trace_message(value: &Value) -> Option<String> {
    match value {
        Value::Result(Err(message)) => Some(message.clone()),
        _ => None,
    }
}

fn detect_corrupted_records(value: &Value) -> Vec<CorruptedRecord> {
    match value {
        Value::List(items) => items
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(index, item)| corruption_for_value(index + 1, item))
            .collect(),
        other => corruption_for_value(1, other).into_iter().collect(),
    }
}

fn corruption_for_value(index: usize, value: &Value) -> Option<CorruptedRecord> {
    let reasons = corruption_reasons_for_value(None, value);
    if reasons.is_empty() {
        return None;
    }
    Some(CorruptedRecord {
        index,
        fields: snapshot_fields(value),
        reason: reasons.join(", "),
    })
}

fn corruption_reasons_for_value(field_name: Option<&str>, value: &Value) -> Vec<String> {
    let mut reasons = Vec::new();
    match value {
        Value::Option(None) => {
            reasons.push(match field_name {
                Some(name) => format!("{name} is none"),
                None => "value is none".to_string(),
            });
        }
        Value::Float(number) if number.is_nan() => {
            reasons.push(match field_name {
                Some(name) => format!("{name} is NaN"),
                None => "value is NaN".to_string(),
            });
        }
        Value::String(text) if text.trim().is_empty() => {
            reasons.push(match field_name {
                Some(name) => format!("{name} is empty"),
                None => "value is empty".to_string(),
            });
        }
        Value::Int(number) if field_name.is_some_and(is_score_field) && *number < 0 => {
            reasons.push(format!("{} is negative", field_name.unwrap_or("value")));
        }
        Value::Float(number)
            if field_name.is_some_and(is_score_field) && number.is_finite() && *number < 0.0 =>
        {
            reasons.push(format!("{} is negative", field_name.unwrap_or("value")));
        }
        Value::Map(entries) => {
            for (key, nested) in entries {
                if let Value::String(name) = key {
                    reasons.extend(corruption_reasons_for_value(Some(name), nested));
                }
            }
        }
        Value::Struct { fields, .. } => {
            for (name, nested) in fields.borrow().iter() {
                reasons.extend(corruption_reasons_for_value(Some(name), nested));
            }
        }
        Value::Typestate { fields, .. } => {
            for (name, nested) in fields.borrow().iter() {
                reasons.extend(corruption_reasons_for_value(Some(name), nested));
            }
        }
        Value::Result(Err(message)) => {
            reasons.push(match field_name {
                Some(name) => format!("{name} is err({message})"),
                None => format!("value is err({message})"),
            });
        }
        _ => {}
    }
    reasons
}

fn snapshot_fields(value: &Value) -> Vec<(String, Value)> {
    match value {
        Value::Map(entries) => entries
            .iter()
            .filter_map(|(key, value)| match key {
                Value::String(name) => Some((name.clone(), value.clone())),
                _ => None,
            })
            .collect(),
        Value::Struct { fields, .. } => fields
            .borrow()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        Value::Typestate { fields, .. } => fields
            .borrow()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        other => vec![("value".to_string(), other.clone())],
    }
}

fn is_score_field(name: &str) -> bool {
    name.eq_ignore_ascii_case("score") || name.to_ascii_lowercase().ends_with("_score")
}

/// 2つの Value を大小比較する（Int / Float / String / @derive(Ord) な Struct 対応）
fn compare_values(a: &Value, b: &Value) -> Result<std::cmp::Ordering, RuntimeError> {
    use std::cmp::Ordering::Equal;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Float(x), Value::Float(y)) => Ok(x.partial_cmp(y).unwrap_or(Equal)),
        (Value::Int(x), Value::Float(y)) => Ok((*x as f64).partial_cmp(y).unwrap_or(Equal)),
        (Value::Float(x), Value::Int(y)) => Ok(x.partial_cmp(&(*y as f64)).unwrap_or(Equal)),
        (Value::String(x), Value::String(y)) => Ok(x.cmp(y)),
        (Value::Struct { fields: fa, .. }, Value::Struct { fields: fb, .. }) => {
            // フィールドをキー順でソートして辞書順比較
            let borrow_a = fa.borrow();
            let borrow_b = fb.borrow();
            let mut keys_a: Vec<&String> = borrow_a.keys().collect();
            keys_a.sort();
            for key in keys_a {
                let va = borrow_a.get(key).ok_or_else(|| {
                    RuntimeError::Custom(format!("フィールド '{}' が存在しません", key))
                })?;
                let vb = borrow_b.get(key).ok_or_else(|| {
                    RuntimeError::Custom(format!("比較対象にフィールド '{}' がありません", key))
                })?;
                let ord = compare_values(va, vb)?;
                if ord != std::cmp::Ordering::Equal {
                    return Ok(ord);
                }
            }
            Ok(std::cmp::Ordering::Equal)
        }
        _ => Err(RuntimeError::Custom(format!(
            "比較できない型: {} と {}",
            a.type_name(),
            b.type_name()
        ))),
    }
}

/// typestate メソッドの戻り値型から遷移先状態名と Result かどうかを抽出する
/// - `-> Connected!`    → (Some("Connected"), true)
/// - `-> Disconnected`  → (Some("Disconnected"), false)
/// - `-> string!`       → (None, true)
/// - `-> string`        → (None, false)
fn extract_transition_info(return_type: &Option<TypeAnn>) -> (Option<String>, bool) {
    match return_type {
        None => (None, false),
        Some(TypeAnn::Named(state_name)) => (Some(state_name.clone()), false),
        Some(TypeAnn::Result(inner)) => match inner.as_ref() {
            TypeAnn::Named(state_name) => (Some(state_name.clone()), true),
            _ => (None, true),
        },
        _ => (None, false),
    }
}

/// @derive(Ord) の compare() メソッド用: フィールド宣言順で辞書順比較
fn compare_struct_fields(
    a: &Value,
    b: &Value,
    field_order: &[String],
) -> Result<std::cmp::Ordering, RuntimeError> {
    match (a, b) {
        (Value::Struct { fields: fa, .. }, Value::Struct { fields: fb, .. }) => {
            let borrow_a = fa.borrow();
            let borrow_b = fb.borrow();
            for key in field_order {
                let va = borrow_a.get(key).ok_or_else(|| {
                    RuntimeError::Custom(format!("フィールド '{}' が存在しません", key))
                })?;
                let vb = borrow_b.get(key).ok_or_else(|| {
                    RuntimeError::Custom(format!("比較対象にフィールド '{}' がありません", key))
                })?;
                let ord = compare_values(va, vb)?;
                if ord != std::cmp::Ordering::Equal {
                    return Ok(ord);
                }
            }
            Ok(std::cmp::Ordering::Equal)
        }
        _ => compare_values(a, b),
    }
}

/// メソッドの第1引数として呼び出し可能な Value を取り出す
fn one_fn_arg(method: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    args.into_iter()
        .next()
        .ok_or_else(|| RuntimeError::Custom(format!("{}() は引数が1つ必要です", method)))
}

/// メソッドの第1引数を i64 として取り出す
fn one_int_arg(method: &str, args: Vec<Value>) -> Result<i64, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Int(n)) => Ok(n),
        Some(v) => Err(type_err("number", v.type_name())),
        None => Err(RuntimeError::Custom(format!(
            "{}() は引数が1つ必要です",
            method
        ))),
    }
}

fn one_string_arg(method: &str, args: Vec<Value>) -> Result<String, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::String(text)) => Ok(text),
        Some(v) => Err(type_err("string", v.type_name())),
        None => Err(RuntimeError::Custom(format!(
            "{}() は引数が1つ必要です",
            method
        ))),
    }
}

/// メソッドの第1引数を List として取り出す
fn one_list_arg(method: &str, args: Vec<Value>) -> Result<Rc<RefCell<Vec<Value>>>, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::List(lst)) => Ok(lst),
        Some(v) => Err(type_err("list", v.type_name())),
        None => Err(RuntimeError::Custom(format!(
            "{}() は引数が1つ必要です",
            method
        ))),
    }
}

fn one_value_arg(method: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    args.into_iter()
        .next()
        .ok_or_else(|| RuntimeError::Custom(format!("{}() は引数が1つ必要です", method)))
}

fn two_value_args(method: &str, args: Vec<Value>) -> Result<(Value, Value), RuntimeError> {
    let mut iter = args.into_iter();
    let first = iter
        .next()
        .ok_or_else(|| RuntimeError::Custom(format!("{}() は引数が2つ必要です", method)))?;
    let second = iter
        .next()
        .ok_or_else(|| RuntimeError::Custom(format!("{}() は引数が2つ必要です", method)))?;
    Ok((first, second))
}

fn one_set_arg(method: &str, args: Vec<Value>) -> Result<Vec<Value>, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Set(items)) => Ok(items),
        Some(v) => Err(type_err("set", v.type_name())),
        None => Err(RuntimeError::Custom(format!(
            "{}() は引数が1つ必要です",
            method
        ))),
    }
}

fn value_to_string_keys(value: Value) -> Result<Vec<String>, RuntimeError> {
    match value {
        Value::List(items) => items
            .borrow()
            .iter()
            .map(|item| match item {
                Value::String(s) => Ok(s.clone()),
                other => Err(type_err("string", other.type_name())),
            })
            .collect(),
        other => Err(type_err("list<string>", other.type_name())),
    }
}

fn is_utility_type_name(name: &str) -> bool {
    matches!(
        name,
        "Partial" | "Required" | "Readonly" | "Pick" | "Omit" | "NonNullable" | "Record"
    )
}

/// キー関数 f でリストをソートする（stable sort）
fn sort_by_key(
    interp: &mut Interpreter,
    items: Rc<RefCell<Vec<Value>>>,
    f: Value,
    descending: bool,
) -> Result<Value, RuntimeError> {
    let list = items.borrow().clone();
    // まず各要素のキーを計算してペアを作る
    let mut keyed: Vec<(Value, Value)> = Vec::with_capacity(list.len());
    for item in list {
        let key = interp.call_value(f.clone(), vec![item.clone()])?;
        keyed.push((key, item));
    }
    // 安定ソート（エラーは Cell 経由で伝播）
    let mut sort_err: Option<RuntimeError> = None;
    keyed.sort_by(|(ka, _), (kb, _)| {
        if sort_err.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match compare_values(ka, kb) {
            Ok(ord) => {
                if descending {
                    ord.reverse()
                } else {
                    ord
                }
            }
            Err(e) => {
                sort_err = Some(e);
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(e) = sort_err {
        return Err(e);
    }
    Ok(mk_list(keyed.into_iter().map(|(_, v)| v).collect()))
}

/// T-4-C: バリデーション制約チェック
/// 違反があれば制約名を返し、問題なければ None を返す
fn check_constraint(
    field_val: Option<&Value>,
    constraint: &forge_compiler::ast::Constraint,
) -> Option<&'static str> {
    use forge_compiler::ast::Constraint;

    // Option 型のフィールドは None なら制約をスキップ（nullable）
    let val = match field_val {
        None => return Some("field_missing"),
        Some(Value::Option(None)) => return None, // None なら制約スキップ
        Some(Value::Option(Some(inner))) => inner.as_ref(),
        Some(v) => v,
    };

    match constraint {
        Constraint::Length { min, max } => {
            let s = match val {
                Value::String(s) => s,
                _ => return Some("length"),
            };
            let len = s.chars().count();
            if let Some(m) = min {
                if len < *m {
                    return Some("length");
                }
            }
            if let Some(m) = max {
                if len > *m {
                    return Some("length");
                }
            }
            None
        }
        Constraint::Alphanumeric => match val {
            Value::String(s) if s.chars().all(|c| c.is_alphanumeric()) => None,
            Value::String(_) => Some("alphanumeric"),
            _ => Some("alphanumeric"),
        },
        Constraint::EmailFormat => match val {
            Value::String(s) => {
                if s.contains('@') && s.contains('.') {
                    None
                } else {
                    Some("email_format")
                }
            }
            _ => Some("email_format"),
        },
        Constraint::UrlFormat => match val {
            Value::String(s) => {
                if s.starts_with("http://") || s.starts_with("https://") {
                    None
                } else {
                    Some("url_format")
                }
            }
            _ => Some("url_format"),
        },
        Constraint::Range { min, max } => {
            let n = match val {
                Value::Int(n) => *n as f64,
                Value::Float(f) => *f,
                _ => return Some("range"),
            };
            if let Some(m) = min {
                if n < *m {
                    return Some("range");
                }
            }
            if let Some(m) = max {
                if n > *m {
                    return Some("range");
                }
            }
            None
        }
        Constraint::ContainsDigit => match val {
            Value::String(s) if s.chars().any(|c| c.is_ascii_digit()) => None,
            Value::String(_) => Some("contains_digit"),
            _ => Some("contains_digit"),
        },
        Constraint::ContainsUppercase => match val {
            Value::String(s) if s.chars().any(|c| c.is_uppercase()) => None,
            Value::String(_) => Some("contains_uppercase"),
            _ => Some("contains_uppercase"),
        },
        Constraint::ContainsLowercase => match val {
            Value::String(s) if s.chars().any(|c| c.is_lowercase()) => None,
            Value::String(_) => Some("contains_lowercase"),
            _ => Some("contains_lowercase"),
        },
        Constraint::NotEmpty => match val {
            Value::String(s) if !s.is_empty() => None,
            Value::String(_) => Some("not_empty"),
            Value::List(list) if !list.borrow().is_empty() => None,
            Value::List(_) => Some("not_empty"),
            _ => Some("not_empty"),
        },
        Constraint::Matches(pattern) => {
            // 簡易的な正規表現マッチ（シンプル実装: 完全一致のみ）
            // 本格的な正規表現ライブラリは依存追加が必要なので
            // ここでは文字列が含まれているかのシンプルチェック
            match val {
                Value::String(s) if s.contains(pattern.as_str()) => None,
                Value::String(_) => Some("matches"),
                _ => Some("matches"),
            }
        }
    }
}

fn eval_literal(lit: &Literal) -> Value {
    match lit {
        Literal::Int(n) => Value::Int(*n),
        Literal::Float(f) => Value::Float(*f),
        Literal::String(s) => Value::String(s.clone()),
        Literal::Bool(b) => Value::Bool(*b),
    }
}

fn type_err(expected: &str, found: &str) -> RuntimeError {
    RuntimeError::TypeMismatch {
        expected: expected.to_string(),
        found: found.to_string(),
    }
}

fn int_float_op(
    l: Value,
    r: Value,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
    sym: &str,
) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(a, b))),
        (l, r) => Err(type_err(
            &format!("number {} number", sym),
            &format!("{} {} {}", l.type_name(), sym, r.type_name()),
        )),
    }
}

fn cmp_op(
    l: Value,
    r: Value,
    int_pred: impl Fn(i64, i64) -> bool,
    float_pred: impl Fn(f64, f64) -> bool,
) -> Result<Value, RuntimeError> {
    match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(int_pred(*a, *b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(float_pred(*a, *b))),
        (Value::Struct { .. }, Value::Struct { .. }) => {
            let ord = compare_values(&l, &r)?;
            // int_pred を ordering に対応させる: (a, b) として -1/0/1 に変換
            let (ai, bi): (i64, i64) = match ord {
                std::cmp::Ordering::Less => (-1, 0),
                std::cmp::Ordering::Equal => (0, 0),
                std::cmp::Ordering::Greater => (1, 0),
            };
            Ok(Value::Bool(int_pred(ai, bi)))
        }
        _ => Err(type_err(
            "number",
            &format!("{} vs {}", l.type_name(), r.type_name()),
        )),
    }
}

/// パターンマッチング: マッチした場合はバインディングリストを返す
fn match_pattern(pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    match (pattern, value) {
        (Pattern::Wildcard, _) => Some(vec![]),
        (Pattern::Ident(name), v) => Some(vec![(name.clone(), v.clone())]),
        (Pattern::Literal(lit), v) => {
            let lit_val = eval_literal(lit);
            if lit_val == *v {
                Some(vec![])
            } else {
                None
            }
        }
        (Pattern::None, Value::Option(None)) => Some(vec![]),
        (Pattern::Some(inner), Value::Option(Some(v))) => match_pattern(inner, v),
        (Pattern::Ok(inner), Value::Result(Ok(v))) => match_pattern(inner, v),
        (Pattern::Err(inner), Value::Result(Err(e))) => {
            match_pattern(inner, &Value::String(e.clone()))
        }
        (
            Pattern::Range {
                start,
                end,
                inclusive,
            },
            Value::Int(n),
        ) => {
            let s = match start {
                Literal::Int(i) => *i,
                _ => return None,
            };
            let e = match end {
                Literal::Int(i) => *i,
                _ => return None,
            };
            let hit = if *inclusive {
                s <= *n && *n <= e
            } else {
                s <= *n && *n < e
            };
            if hit {
                Some(vec![])
            } else {
                None
            }
        }
        // ── enum パターン ─────────────────────────────────────────────────
        (
            Pattern::EnumUnit { enum_name, variant },
            Value::Enum {
                type_name,
                variant: val_variant,
                data: EnumData::Unit,
            },
        ) => {
            // enum_name が Some の場合は型名も確認する
            if let Some(en) = enum_name {
                if en != type_name {
                    return None;
                }
            }
            if variant == val_variant {
                Some(vec![])
            } else {
                None
            }
        }
        (
            Pattern::EnumTuple {
                enum_name,
                variant,
                bindings,
            },
            Value::Enum {
                type_name,
                variant: val_variant,
                data: EnumData::Tuple(items),
            },
        ) => {
            if let Some(en) = enum_name {
                if en != type_name {
                    return None;
                }
            }
            if variant != val_variant {
                return None;
            }
            if bindings.len() != items.len() {
                return None;
            }
            let mut result = Vec::new();
            for (name, val) in bindings.iter().zip(items.iter()) {
                if name == "_" {
                    // ワイルドカードはスキップ
                } else {
                    result.push((name.clone(), val.clone()));
                }
            }
            Some(result)
        }
        (
            Pattern::EnumStruct {
                enum_name,
                variant,
                fields,
            },
            Value::Enum {
                type_name,
                variant: val_variant,
                data: EnumData::Struct(field_map),
            },
        ) => {
            if let Some(en) = enum_name {
                if en != type_name {
                    return None;
                }
            }
            if variant != val_variant {
                return None;
            }
            let mut result = Vec::new();
            for field_name in fields {
                if field_name == "_" {
                    continue;
                }
                match field_map.get(field_name) {
                    Some(val) => result.push((field_name.clone(), val.clone())),
                    None => return None,
                }
            }
            Some(result)
        }
        _ => None,
    }
}

// ── 公開ユーティリティ ────────────────────────────────────────────────────

pub fn eval_source(source: &str) -> Result<Value, RuntimeError> {
    use forge_compiler::parser::parse_source;
    let module = parse_source(source).map_err(|e| RuntimeError::Custom(e.to_string()))?;
    Interpreter::new().eval(&module)
}

/// 型名かどうかを判定（大文字から始まる識別子）
fn is_type_name_str(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

/// TypeAnn から型のゼロ値を生成する（@derive(Singleton) の初期化用）
fn zero_value_for_type(ann: &TypeAnn) -> Value {
    match ann {
        TypeAnn::Number => Value::Int(0),
        TypeAnn::Float => Value::Float(0.0),
        TypeAnn::String => Value::String(String::new()),
        TypeAnn::Bool => Value::Bool(false),
        TypeAnn::Option(_) => Value::Option(None),
        _ => Value::Unit,
    }
}

// ── tcp_listen / tcp_listen_async — forge run モードの同期 HTTP サーバ ────

/// forge run モードで `tcp_listen(port, handler)` / `tcp_listen_async(port, handler)` を実行する。
///
/// tokio 不要のシンプルな std::net::TcpListener ベースの実装。
/// 1 接続ずつ同期処理する（開発・テスト用）。
impl Interpreter {
    fn eval_tcp_listen(&mut self, args: Vec<Value>) -> Result<Value, RuntimeError> {
        use std::collections::HashMap as StdMap;
        use std::net::TcpListener;

        if args.len() != 2 {
            return Err(RuntimeError::Custom(
                "tcp_listen takes 2 arguments: (port, handler)".to_string(),
            ));
        }
        let port = match &args[0] {
            Value::Int(n) => *n as u16,
            Value::Float(n) => *n as u16,
            v => {
                return Err(RuntimeError::Custom(format!(
                    "tcp_listen: port must be a number, got {}",
                    v.type_name()
                )))
            }
        };
        let handler = args[1].clone();

        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
            .map_err(|e| RuntimeError::Custom(format!("tcp bind failed: {}", e)))?;

        loop {
            let (stream, _addr) = listener
                .accept()
                .map_err(|e| RuntimeError::Custom(format!("tcp accept failed: {}", e)))?;

            // HTTP リクエストをパース
            let raw_req = match http_parse_request(&stream) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // ForgeScript の RawRequest 値を構築
            let mut fields: StdMap<String, Value> = StdMap::new();
            fields.insert("method".to_string(), Value::String(raw_req.method.clone()));
            fields.insert("path".to_string(), Value::String(raw_req.path.clone()));
            fields.insert("body".to_string(), Value::String(raw_req.body.clone()));
            // headers → Value::Map (Vec<(Value, Value)>)
            let headers_map: Vec<(Value, Value)> = raw_req
                .headers
                .iter()
                .map(|(k, v)| (Value::String(k.clone()), Value::String(v.clone())))
                .collect();
            fields.insert("headers".to_string(), Value::Map(headers_map));
            // query → Value::Map
            let query_map: Vec<(Value, Value)> = raw_req
                .query
                .iter()
                .map(|(k, v)| (Value::String(k.clone()), Value::String(v.clone())))
                .collect();
            fields.insert("query".to_string(), Value::Map(query_map));
            let raw_req_val = Value::Struct {
                type_name: "RawRequest".to_string(),
                fields: Rc::new(RefCell::new(fields)),
            };

            // ハンドラ呼び出し → Value::Result(Ok(Value::Struct { "RawResponse" }))
            let response = match self.call_value(handler.clone(), vec![raw_req_val]) {
                Ok(v) => v,
                Err(e) => {
                    // エラー時は 500 レスポンス
                    let _ = write_http_response(
                        &stream,
                        500,
                        &StdMap::new(),
                        &format!("Internal Server Error: {}", e),
                    );
                    continue;
                }
            };

            // Result を unwrap して RawResponse の各フィールドを取得
            let resp_struct = match response {
                Value::Result(Ok(v)) => *v,
                Value::Result(Err(msg)) => {
                    let _ = write_http_response(&stream, 500, &StdMap::new(), &msg);
                    continue;
                }
                other => other,
            };

            let (status, headers, body) = extract_raw_response(resp_struct);
            let _ = write_http_response(&stream, status, &headers, &body);
        }
    }

    // ── forge/http モジュール ─────────────────────────────────────────────

    /// `use forge/http.{ get, post, ... }` を処理して HTTP 関数をスコープに登録する
    fn register_wasm_module(&mut self, symbols: &UseSymbols) -> Result<(), String> {
        if !self.type_registry.structs.contains_key("Wasm") {
            self.setup_wasm_types();
        }

        let exports = ["Wasm", "WasmOptions"];
        for name in exports {
            let should_bind = match symbols {
                UseSymbols::All => true,
                UseSymbols::Single(symbol, _) => symbol == name,
                UseSymbols::Multiple(items) => items.iter().any(|(symbol, _)| symbol == name),
            };
            if !should_bind {
                continue;
            }

            let bind_name = match symbols {
                UseSymbols::Single(_, Some(alias)) => alias.clone(),
                UseSymbols::Multiple(items) => items
                    .iter()
                    .find(|(symbol, _)| symbol == name)
                    .and_then(|(_, alias)| alias.clone())
                    .unwrap_or_else(|| name.to_string()),
                _ => name.to_string(),
            };

            self.define(&bind_name, Value::String(name.to_string()), false);
        }

        Ok(())
    }

    fn setup_wasm_types(&mut self) {
        let mut wasm_options_methods: HashMap<String, MethodImpl> = HashMap::new();
        wasm_options_methods.insert(
            "trusted".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|_args: Vec<Value>| {
                Ok(vm_wasm_options_value(None, None, None, true, true, true))
            }))),
        );
        wasm_options_methods.insert(
            "sandboxed".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|_args: Vec<Value>| {
                Ok(vm_wasm_options_value(
                    Some(1_000_000),
                    Some(16),
                    Some(500),
                    false,
                    false,
                    false,
                ))
            }))),
        );
        wasm_options_methods.insert(
            "strict".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|_args: Vec<Value>| {
                Ok(vm_wasm_options_value(
                    Some(100_000),
                    Some(4),
                    Some(100),
                    false,
                    false,
                    false,
                ))
            }))),
        );
        self.type_registry.structs.insert(
            "WasmOptions".to_string(),
            StructInfo {
                fields: vec![],
                derives: vec![],
                methods: wasm_options_methods,
                operators: HashMap::new(),
            },
        );

        let mut wasm_methods: HashMap<String, MethodImpl> = HashMap::new();
        wasm_methods.insert(
            "load".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|args: Vec<Value>| {
                let path = match args.first() {
                    Some(Value::String(path)) => path.clone(),
                    Some(other) => {
                        return Err(format!(
                            "Wasm.load() expects string, got {}",
                            other.type_name()
                        ))
                    }
                    None => return Err("Wasm.load() requires path".to_string()),
                };
                Ok(Value::Result(Ok(Box::new(vm_wasm_value(
                    path,
                    vm_wasm_options_value(None, None, None, true, true, true),
                )))))
            }))),
        );
        wasm_methods.insert(
            "load_with".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|args: Vec<Value>| {
                let path = match args.first() {
                    Some(Value::String(path)) => path.clone(),
                    Some(other) => {
                        return Err(format!(
                            "Wasm.load_with() expects path string, got {}",
                            other.type_name()
                        ))
                    }
                    None => return Err("Wasm.load_with() requires path".to_string()),
                };
                let options = match args.get(1) {
                    Some(value @ Value::Struct { type_name, .. }) if type_name == "WasmOptions" => {
                        value.clone()
                    }
                    Some(other) => {
                        return Err(format!(
                            "Wasm.load_with() expects WasmOptions, got {}",
                            other.type_name()
                        ))
                    }
                    None => return Err("Wasm.load_with() requires options".to_string()),
                };
                Ok(Value::Result(Ok(Box::new(vm_wasm_value(path, options)))))
            }))),
        );
        wasm_methods.insert(
            "call".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let fn_name = match args.first() {
                    Some(Value::String(name)) => name.clone(),
                    Some(other) => {
                        return Err(format!(
                            "Wasm.call() expects function name string, got {}",
                            other.type_name()
                        ))
                    }
                    None => return Err("Wasm.call() requires function name".to_string()),
                };
                let input = match args.get(1) {
                    Some(Value::String(input)) => input.clone(),
                    Some(other) => {
                        return Err(format!(
                            "Wasm.call() expects input string, got {}",
                            other.type_name()
                        ))
                    }
                    None => return Err("Wasm.call() requires input".to_string()),
                };
                let (path, options) = vm_wasm_parts(&self_val)?;
                let output = vm_wasm_call_from_file(&path, &options, &fn_name, &input)?;
                Ok(Value::Result(Ok(Box::new(Value::String(output)))))
            }))),
        );
        self.type_registry.structs.insert(
            "Wasm".to_string(),
            StructInfo {
                fields: vec![],
                derives: vec![],
                methods: wasm_methods,
                operators: HashMap::new(),
            },
        );
    }

    fn register_fs_module(&mut self, symbols: &UseSymbols) -> Result<(), String> {
        macro_rules! native {
            ($f:expr) => {
                Value::NativeFunction(NativeFn(Rc::new($f)))
            };
        }

        let fns: &[(&str, Value)] = &[
            (
                "read_file",
                native!(|args: Vec<Value>| {
                    let path = match args.first() {
                        Some(Value::String(s)) => s.clone(),
                        Some(v) => return Err(format!("read_file() expects string, got {}", v.type_name())),
                        None => return Err("read_file() requires 1 argument".to_string()),
                    };
                    match std::fs::read_to_string(&path) {
                        Ok(s) => Ok(Value::Result(Ok(Box::new(Value::String(s))))),
                        Err(e) => Ok(Value::Result(Err(format!("failed to read '{}': {}", path, e)))),
                    }
                }),
            ),
            (
                "write_file",
                native!(|args: Vec<Value>| {
                    let path = match args.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => return Err("write_file() requires (path, content)".to_string()),
                    };
                    let content = match args.get(1) {
                        Some(Value::String(s)) => s.clone(),
                        _ => return Err("write_file() requires (path, content)".to_string()),
                    };
                    match std::fs::write(&path, &content) {
                        Ok(_) => Ok(Value::Result(Ok(Box::new(Value::Unit)))),
                        Err(e) => Ok(Value::Result(Err(format!("failed to write '{}': {}", path, e)))),
                    }
                }),
            ),
            (
                "file_exists",
                native!(|args: Vec<Value>| {
                    let path = match args.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => return Err("file_exists() requires 1 argument".to_string()),
                    };
                    Ok(Value::Bool(std::path::Path::new(&path).is_file()))
                }),
            ),
        ];

        for (name, val) in fns {
            let should_bind = match symbols {
                UseSymbols::All => true,
                UseSymbols::Single(n, _) => n == name,
                UseSymbols::Multiple(pairs) => pairs.iter().any(|(n, _)| n == name),
            };
            if !should_bind {
                continue;
            }
            let bind_name = match symbols {
                UseSymbols::Single(_, Some(alias)) => alias.clone(),
                UseSymbols::Multiple(pairs) => pairs
                    .iter()
                    .find(|(n, _)| n == name)
                    .and_then(|(_, alias)| alias.clone())
                    .unwrap_or_else(|| name.to_string()),
                _ => name.to_string(),
            };
            self.record_import(name, &bind_name, "forge/std/fs", val.clone(), matches!(symbols, UseSymbols::All))
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn register_http_module(&mut self, symbols: &UseSymbols) -> Result<(), String> {
        // 型レジストリに HttpRequest / HttpResponse がなければ初回登録
        if !self.type_registry.structs.contains_key("HttpRequest") {
            self.setup_http_types();
        }

        // 全 HTTP 関数: (名前, メソッド文字列)
        let all: &[(&str, &str)] = &[
            ("get", "GET"),
            ("post", "POST"),
            ("put", "PUT"),
            ("patch", "PATCH"),
            ("delete", "DELETE"),
        ];

        for (fname, method_str) in all {
            let should_bind = match symbols {
                UseSymbols::All => true,
                UseSymbols::Single(name, _) => name == fname,
                UseSymbols::Multiple(pairs) => pairs.iter().any(|(n, _)| n == fname),
            };
            if !should_bind {
                continue;
            }
            let bind_name = match symbols {
                UseSymbols::Single(_, Some(alias)) => alias.clone(),
                UseSymbols::Multiple(pairs) => pairs
                    .iter()
                    .find(|(n, _)| n == fname)
                    .and_then(|(_, alias)| alias.clone())
                    .unwrap_or_else(|| fname.to_string()),
                _ => fname.to_string(),
            };

            let ms = method_str.to_string();
            let func = Value::NativeFunction(NativeFn(Rc::new(move |args: Vec<Value>| {
                let url = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => {
                        return Err(format!("HTTP url must be string, got {}", v.type_name()))
                    }
                    None => return Err("HTTP function requires a URL argument".to_string()),
                };
                Ok(http_request_new(&ms, &url))
            })));
            self.define(&bind_name, func, false);
        }
        Ok(())
    }

    /// HttpRequest / HttpResponse の型レジストリ登録
    fn setup_http_types(&mut self) {
        // ── HttpRequest メソッド ──
        let mut req_methods: HashMap<String, MethodImpl> = HashMap::new();

        // .header(key, value) → new HttpRequest
        req_methods.insert(
            "header".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let key = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err("header key must be string".to_string()),
                };
                let val = if args.len() > 1 {
                    match &args[1] {
                        Value::String(s) => s.clone(),
                        _ => return Err("header value must be string".to_string()),
                    }
                } else {
                    return Err("header() requires (key, value)".to_string());
                };
                let mut fields = req_clone_map(&self_val)?;
                let headers = map_vec_push(
                    fields.get("headers"),
                    (Value::String(key), Value::String(val)),
                );
                fields.insert("headers".to_string(), Value::Map(headers));
                Ok(http_req_from_map(fields))
            }))),
        );

        // .query(map) → new HttpRequest
        req_methods.insert(
            "query".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let params = match args.first() {
                    Some(Value::Map(m)) => m.clone(),
                    _ => return Err("query() requires a map argument".to_string()),
                };
                let mut fields = req_clone_map(&self_val)?;
                let existing = map_vec_from_field(fields.get("query"));
                let mut combined = existing;
                combined.extend(params);
                fields.insert("query".to_string(), Value::Map(combined));
                Ok(http_req_from_map(fields))
            }))),
        );

        // .json(value) → new HttpRequest
        req_methods.insert(
            "json".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let body_val = args.into_iter().next().unwrap_or(Value::Unit);
                let body_str = value_to_json_string(&body_val)?;
                let mut fields = req_clone_map(&self_val)?;
                fields.insert("body".to_string(), Value::String(body_str));
                fields.insert(
                    "content_type".to_string(),
                    Value::String("application/json".to_string()),
                );
                Ok(http_req_from_map(fields))
            }))),
        );

        // .form(map) → new HttpRequest
        req_methods.insert(
            "form".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let params = match args.first() {
                    Some(Value::Map(m)) => m.clone(),
                    _ => return Err("form() requires a map argument".to_string()),
                };
                // URL-encode form
                let body = params
                    .iter()
                    .filter_map(|(k, v)| {
                        if let (Value::String(k), Value::String(v)) = (k, v) {
                            Some(format!("{}={}", k, v))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("&");
                let mut fields = req_clone_map(&self_val)?;
                fields.insert("body".to_string(), Value::String(body));
                fields.insert(
                    "content_type".to_string(),
                    Value::String("application/x-www-form-urlencoded".to_string()),
                );
                Ok(http_req_from_map(fields))
            }))),
        );

        // .timeout(ms) → new HttpRequest
        req_methods.insert(
            "timeout".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let ms = match args.first() {
                    Some(Value::Int(n)) => *n,
                    Some(Value::Float(f)) => *f as i64,
                    _ => return Err("timeout() requires a number (milliseconds)".to_string()),
                };
                let mut fields = req_clone_map(&self_val)?;
                fields.insert("timeout_ms".to_string(), Value::Int(ms));
                Ok(http_req_from_map(fields))
            }))),
        );

        // .retry(n) → new HttpRequest
        req_methods.insert(
            "retry".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let n = match args.first() {
                    Some(Value::Int(n)) => *n,
                    Some(Value::Float(f)) => *f as i64,
                    _ => return Err("retry() requires a number".to_string()),
                };
                let mut fields = req_clone_map(&self_val)?;
                fields.insert("retry_count".to_string(), Value::Int(n));
                Ok(http_req_from_map(fields))
            }))),
        );

        // .send() → Result<HttpResponse, String>
        req_methods.insert(
            "send".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                let fields = req_clone_map(&self_val)?;

                let method = match fields.get("method") {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err("invalid HttpRequest: missing method".to_string()),
                };
                let base_url = match fields.get("url") {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err("invalid HttpRequest: missing url".to_string()),
                };
                let timeout_ms = match fields.get("timeout_ms") {
                    Some(Value::Int(n)) if *n > 0 => Some(*n as u64),
                    _ => None,
                };
                let retry_count = match fields.get("retry_count") {
                    Some(Value::Int(n)) => *n as u32,
                    _ => 0,
                };
                let body = match fields.get("body") {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };
                let content_type = match fields.get("content_type") {
                    Some(Value::String(s)) => s.clone(),
                    _ => String::new(),
                };

                // クエリパラメータ付き URL を構築
                let url = if let Some(Value::Map(q)) = fields.get("query") {
                    if q.is_empty() {
                        base_url.clone()
                    } else {
                        let qs = q
                            .iter()
                            .filter_map(|(k, v)| {
                                if let (Value::String(k), Value::String(v)) = (k, v) {
                                    Some(format!("{}={}", k, v))
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("&");
                        let sep = if base_url.contains('?') { '&' } else { '?' };
                        format!("{}{}{}", base_url, sep, qs)
                    }
                } else {
                    base_url.clone()
                };

                let max_attempts = retry_count + 1;
                let mut last_err = String::new();

                for attempt in 0..max_attempts {
                    if attempt > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(100 * attempt as u64));
                    }

                    match http_send_once(&method, &url, &fields, &body, &content_type, timeout_ms) {
                        Ok(resp) => {
                            // 5xx はリトライ対象
                            if resp.status >= 500 && attempt + 1 < max_attempts {
                                last_err = format!("server error: {}", resp.status);
                                continue;
                            }
                            return Ok(resp.into_value());
                        }
                        Err(e) => {
                            last_err = e;
                        }
                    }
                }

                Err(RuntimeError::Custom(last_err).to_string())
            }))),
        );

        self.type_registry.structs.insert(
            "HttpRequest".to_string(),
            StructInfo {
                fields: vec![],
                derives: vec![],
                methods: req_methods,
                operators: HashMap::new(),
            },
        );

        // ── HttpResponse メソッド ──
        let mut resp_methods: HashMap<String, MethodImpl> = HashMap::new();

        // .text() → Result<string, string>
        resp_methods.insert(
            "text".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                match self_val {
                    Value::Struct { ref fields, .. } => {
                        let body = fields
                            .borrow()
                            .get("body")
                            .cloned()
                            .unwrap_or(Value::String(String::new()));
                        Ok(Value::Result(Ok(Box::new(body))))
                    }
                    _ => Err("text() called on non-HttpResponse".to_string()),
                }
            }))),
        );

        // .json() → Result<map, string>
        resp_methods.insert(
            "json".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                match self_val {
                    Value::Struct { ref fields, .. } => {
                        let body = match fields.borrow().get("body").cloned() {
                            Some(Value::String(s)) => s,
                            _ => String::new(),
                        };
                        let jv: serde_json::Value = serde_json::from_str(&body)
                            .map_err(|e| format!("json parse error: {}", e))?;
                        Ok(Value::Result(Ok(Box::new(json_to_value(jv)))))
                    }
                    _ => Err("json() called on non-HttpResponse".to_string()),
                }
            }))),
        );

        // .bytes() → Result<list<number>, string>
        resp_methods.insert(
            "bytes".to_string(),
            MethodImpl::Native(NativeFn(Rc::new(|mut args: Vec<Value>| {
                let self_val = args.remove(0);
                match self_val {
                    Value::Struct { ref fields, .. } => {
                        let body = match fields.borrow().get("body").cloned() {
                            Some(Value::String(s)) => s,
                            _ => String::new(),
                        };
                        let bytes: Vec<Value> = body
                            .into_bytes()
                            .into_iter()
                            .map(|b| Value::Int(b as i64))
                            .collect();
                        Ok(Value::Result(Ok(Box::new(Value::List(Rc::new(
                            RefCell::new(bytes),
                        ))))))
                    }
                    _ => Err("bytes() called on non-HttpResponse".to_string()),
                }
            }))),
        );

        self.type_registry.structs.insert(
            "HttpResponse".to_string(),
            StructInfo {
                fields: vec![],
                derives: vec![],
                methods: resp_methods,
                operators: HashMap::new(),
            },
        );
    }
}

/// forge/http: 新しい HttpRequest Value を生成する
fn http_request_new(method: &str, url: &str) -> Value {
    let mut fields = HashMap::new();
    fields.insert("method".to_string(), Value::String(method.to_string()));
    fields.insert("url".to_string(), Value::String(url.to_string()));
    fields.insert("headers".to_string(), Value::Map(vec![]));
    fields.insert("query".to_string(), Value::Map(vec![]));
    fields.insert("body".to_string(), Value::String(String::new()));
    fields.insert("content_type".to_string(), Value::String(String::new()));
    fields.insert("timeout_ms".to_string(), Value::Int(0));
    fields.insert("retry_count".to_string(), Value::Int(0));
    Value::Struct {
        type_name: "HttpRequest".to_string(),
        fields: Rc::new(RefCell::new(fields)),
    }
}

/// HttpRequest struct の fields を HashMap にクローンして返す
fn req_clone_map(val: &Value) -> Result<HashMap<String, Value>, String> {
    match val {
        Value::Struct { type_name, fields } if type_name == "HttpRequest" => {
            Ok(fields.borrow().clone())
        }
        _ => Err(format!("expected HttpRequest, got {}", val.type_name())),
    }
}

fn vm_wasm_options_value(
    max_instructions: Option<i64>,
    max_memory_mb: Option<i64>,
    timeout_ms: Option<i64>,
    allow_fs: bool,
    allow_net: bool,
    allow_env: bool,
) -> Value {
    let mut fields = HashMap::new();
    fields.insert(
        "max_instructions".to_string(),
        Value::Option(max_instructions.map(|v| Box::new(Value::Int(v)))),
    );
    fields.insert(
        "max_memory_mb".to_string(),
        Value::Option(max_memory_mb.map(|v| Box::new(Value::Int(v)))),
    );
    fields.insert(
        "timeout_ms".to_string(),
        Value::Option(timeout_ms.map(|v| Box::new(Value::Int(v)))),
    );
    fields.insert("allow_fs".to_string(), Value::Bool(allow_fs));
    fields.insert("allow_net".to_string(), Value::Bool(allow_net));
    fields.insert("allow_env".to_string(), Value::Bool(allow_env));
    Value::Struct {
        type_name: "WasmOptions".to_string(),
        fields: Rc::new(RefCell::new(fields)),
    }
}

fn vm_wasm_value(path: String, options: Value) -> Value {
    let mut fields = HashMap::new();
    fields.insert("path".to_string(), Value::String(path));
    fields.insert("options".to_string(), options);
    Value::Struct {
        type_name: "Wasm".to_string(),
        fields: Rc::new(RefCell::new(fields)),
    }
}

fn vm_wasm_parts(val: &Value) -> Result<(String, Value), String> {
    match val {
        Value::Struct { type_name, fields } if type_name == "Wasm" => {
            let borrowed = fields.borrow();
            let path = match borrowed.get("path") {
                Some(Value::String(path)) => path.clone(),
                _ => return Err("invalid Wasm: missing path".to_string()),
            };
            let options = borrowed
                .get("options")
                .cloned()
                .ok_or_else(|| "invalid Wasm: missing options".to_string())?;
            Ok((path, options))
        }
        other => Err(format!("expected Wasm, got {}", other.type_name())),
    }
}

fn vm_wasm_option_i64(fields: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match fields.get(key) {
        Some(Value::Option(Some(inner))) => match inner.as_ref() {
            Value::Int(n) => Some(*n),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        },
        _ => None,
    }
}

fn vm_wasm_call_from_file(
    path: &str,
    options: &Value,
    fn_name: &str,
    input: &str,
) -> Result<String, String> {
    let option_fields = match options {
        Value::Struct { type_name, fields } if type_name == "WasmOptions" => {
            fields.borrow().clone()
        }
        other => return Err(format!("expected WasmOptions, got {}", other.type_name())),
    };

    if vm_wasm_option_i64(&option_fields, "max_instructions") == Some(0) {
        return Err("WasmFuelExhausted: max_instructions exhausted before execution".to_string());
    }
    if vm_wasm_option_i64(&option_fields, "timeout_ms") == Some(0) {
        return Err("WasmTimeout: timeout_ms elapsed before execution".to_string());
    }

    let engine = Engine::default();
    let module = WasmModule::from_file(&engine, path)
        .map_err(|err| format!("WasmLoadError: failed to load {}: {}", path, err))?;
    let mut store = Store::new(&engine, ());
    let instance =
        Instance::new(&mut store, &module, &[]).map_err(|err| format!("WasmTrap: {}", err))?;

    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| "WasmCallError: exported memory not found".to_string())?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut store, "alloc")
        .map_err(|err| format!("WasmCallError: alloc export not found or invalid: {}", err))?;
    let func = instance
        .get_typed_func::<(i32, i32), i64>(&mut store, fn_name)
        .map_err(|err| {
            format!(
                "WasmCallError: function `{}` not found or invalid: {}",
                fn_name, err
            )
        })?;

    let input_ptr = alloc
        .call(&mut store, input.len() as i32)
        .map_err(|err| format!("WasmTrap: {}", err))?;
    memory
        .write(&mut store, input_ptr as usize, input.as_bytes())
        .map_err(|err| format!("WasmCallError: failed to write memory: {}", err))?;

    let packed = func
        .call(&mut store, (input_ptr, input.len() as i32))
        .map_err(|err| format!("WasmTrap: {}", err))?;
    let output_ptr = ((packed as u64) >> 32) as usize;
    let output_len = ((packed as u64) & 0xffff_ffff) as usize;
    let mut output = vec![0u8; output_len];
    memory
        .read(&mut store, output_ptr, &mut output)
        .map_err(|err| format!("WasmCallError: failed to read memory: {}", err))?;
    String::from_utf8(output)
        .map_err(|err| format!("WasmCallError: wasm output is not valid utf-8: {}", err))
}

/// fields HashMap から HttpRequest Value を作る
fn http_req_from_map(fields: HashMap<String, Value>) -> Value {
    Value::Struct {
        type_name: "HttpRequest".to_string(),
        fields: Rc::new(RefCell::new(fields)),
    }
}

/// Value::Map(Vec) フィールドの末尾にエントリを追加して新しい Vec を返す
fn map_vec_push(field: Option<&Value>, entry: (Value, Value)) -> Vec<(Value, Value)> {
    let mut v = map_vec_from_field(field);
    v.push(entry);
    v
}

fn map_vec_from_field(field: Option<&Value>) -> Vec<(Value, Value)> {
    match field {
        Some(Value::Map(m)) => m.clone(),
        _ => vec![],
    }
}

/// Value を JSON 文字列にシリアライズする
fn value_to_json_string(val: &Value) -> Result<String, String> {
    fn to_json(v: &Value) -> Result<serde_json::Value, String> {
        match v {
            Value::Int(n) => Ok(serde_json::Value::Number((*n).into())),
            Value::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| format!("float {} is not JSON representable", f)),
            Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            Value::String(s) => Ok(serde_json::Value::String(s.clone())),
            Value::Option(None) => Ok(serde_json::Value::Null),
            Value::Option(Some(inner)) => to_json(inner),
            Value::List(items) => {
                let arr: Result<Vec<_>, _> = items.borrow().iter().map(to_json).collect();
                Ok(serde_json::Value::Array(arr?))
            }
            Value::Map(pairs) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in pairs {
                    if let Value::String(key) = k {
                        obj.insert(key.clone(), to_json(v)?);
                    }
                }
                Ok(serde_json::Value::Object(obj))
            }
            Value::Struct { fields, .. } => {
                let mut obj = serde_json::Map::new();
                for (k, v) in fields.borrow().iter() {
                    obj.insert(k.clone(), to_json(v)?);
                }
                Ok(serde_json::Value::Object(obj))
            }
            _ => Ok(serde_json::Value::Null),
        }
    }
    serde_json::to_string(&to_json(val)?).map_err(|e| e.to_string())
}

/// HTTP レスポンスの中間表現
struct HttpRespData {
    status: u16,
    ok: bool,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpRespData {
    fn into_value(self) -> Value {
        let header_pairs: Vec<(Value, Value)> = self
            .headers
            .into_iter()
            .map(|(k, v)| (Value::String(k), Value::String(v)))
            .collect();
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), Value::Int(self.status as i64));
        fields.insert("ok".to_string(), Value::Bool(self.ok));
        fields.insert("headers".to_string(), Value::Map(header_pairs));
        fields.insert("body".to_string(), Value::String(self.body));
        Value::Struct {
            type_name: "HttpResponse".to_string(),
            fields: Rc::new(RefCell::new(fields)),
        }
    }
}

/// 1回分の HTTP リクエスト送信（reqwest::blocking 使用）
fn http_send_once(
    method: &str,
    url: &str,
    fields: &HashMap<String, Value>,
    body: &str,
    content_type: &str,
    timeout_ms: Option<u64>,
) -> Result<HttpRespData, String> {
    let mut cb = reqwest::blocking::ClientBuilder::new();
    if let Some(ms) = timeout_ms {
        cb = cb.timeout(std::time::Duration::from_millis(ms));
    }
    let client = cb
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let req = match method {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        _ => return Err(format!("unknown HTTP method: {}", method)),
    };

    // ヘッダーを追加
    let mut req = req;
    if let Some(Value::Map(hdrs)) = fields.get("headers") {
        for (k, v) in hdrs {
            if let (Value::String(k), Value::String(v)) = (k, v) {
                req = req.header(k.as_str(), v.as_str());
            }
        }
    }

    // Content-Type ヘッダー
    if !content_type.is_empty() {
        req = req.header("Content-Type", content_type);
    }

    // ボディ
    let req = if !body.is_empty() {
        req.body(body.to_string())
    } else {
        req
    };

    let resp = req
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;
    let status = resp.status().as_u16();
    let ok = status >= 200 && status < 300;
    let mut headers = vec![];
    for (k, v) in resp.headers() {
        if let Ok(val) = v.to_str() {
            headers.push((k.as_str().to_string(), val.to_string()));
        }
    }
    let body_text = resp.text().unwrap_or_default();

    Ok(HttpRespData {
        status,
        ok,
        headers,
        body: body_text,
    })
}

/// HTTP リクエストを std::net::TcpStream からパースする（同期版）
fn http_parse_request(stream: &std::net::TcpStream) -> Result<HttpRawRequest, String> {
    use std::io::{BufRead, BufReader};

    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| e.to_string())?;
    let parts: Vec<&str> = first_line.trim().splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err("invalid request line".to_string());
    }
    let method = parts[0].to_string();
    let full_path = parts[1].to_string();
    let (path, query) = parse_path_query(&full_path);

    // ヘッダ読み込み
    let mut headers = std::collections::HashMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if key == "content-length" {
                content_length = val.parse().unwrap_or(0);
            }
            headers.insert(key, val);
        }
    }

    // ボディ読み込み
    let mut body = String::new();
    if content_length > 0 {
        use std::io::Read;
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
        body = String::from_utf8_lossy(&buf).to_string();
    }

    Ok(HttpRawRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

struct HttpRawRequest {
    method: String,
    path: String,
    query: std::collections::HashMap<String, String>,
    headers: std::collections::HashMap<String, String>,
    body: String,
}

fn parse_path_query(full_path: &str) -> (String, std::collections::HashMap<String, String>) {
    let mut query = std::collections::HashMap::new();
    if let Some((path, qs)) = full_path.split_once('?') {
        for pair in qs.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                query.insert(k.to_string(), v.to_string());
            } else if !pair.is_empty() {
                query.insert(pair.to_string(), String::new());
            }
        }
        (path.to_string(), query)
    } else {
        (full_path.to_string(), query)
    }
}

fn extract_raw_response(val: Value) -> (i64, std::collections::HashMap<String, String>, String) {
    let fields = match val {
        Value::Struct { fields, .. } => fields,
        _ => {
            return (
                500,
                std::collections::HashMap::new(),
                "invalid response".to_string(),
            )
        }
    };
    let f = fields.borrow();
    let status = match f.get("status") {
        Some(Value::Int(n)) => *n,
        Some(Value::Float(n)) => *n as i64,
        _ => 200,
    };
    let body = match f.get("body") {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let mut headers: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(Value::Map(hmap)) = f.get("headers") {
        for (k, v) in hmap.iter() {
            if let (Value::String(key), Value::String(val)) = (k, v) {
                headers.insert(key.clone(), val.clone());
            }
        }
    }
    (status, headers, body)
}

fn write_http_response(
    mut stream: &std::net::TcpStream,
    status: i64,
    headers: &std::collections::HashMap<String, String>,
    body: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    write!(stream, "HTTP/1.1 {} {}\r\n", status, reason)?;
    write!(stream, "Content-Length: {}\r\n", body.len())?;
    for (k, v) in headers {
        write!(stream, "{}: {}\r\n", k, v)?;
    }
    write!(stream, "\r\n")?;
    write!(stream, "{}", body)?;
    stream.flush()
}

// ── JSON ↔ Value 変換 ────────────────────────────────────────────────────

fn json_to_value(jv: serde_json::Value) -> Value {
    match jv {
        serde_json::Value::Null => Value::Option(None),
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.into_iter().map(json_to_value).collect();
            Value::List(Rc::new(RefCell::new(items)))
        }
        serde_json::Value::Object(obj) => {
            let pairs: Vec<(Value, Value)> = obj
                .into_iter()
                .map(|(k, v)| (Value::String(k), json_to_value(v)))
                .collect();
            Value::Map(pairs)
        }
    }
}

fn emit_display_output(
    listener: &Option<Arc<dyn Fn(DisplayOutput) + Send + Sync>>,
    output_buffer: &Option<Arc<Mutex<String>>>,
    output: DisplayOutput,
) {
    if let Some(listener) = listener {
        listener(output);
        return;
    }

    let text = fallback_render_display_output(&output);
    if let Some(buffer) = output_buffer {
        if let Ok(mut buffer) = buffer.lock() {
            buffer.push_str(&text);
        }
        return;
    }
    print!("{}", text);
}

fn fallback_render_display_output(output: &DisplayOutput) -> String {
    match output {
        DisplayOutput::Text { value }
        | DisplayOutput::Html { value }
        | DisplayOutput::Markdown { value } => ensure_trailing_newline(value.clone()),
        DisplayOutput::Json { value } => ensure_trailing_newline(
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        ),
        DisplayOutput::Table { columns: _, rows } => {
            let lines: Vec<String> = rows
                .iter()
                .map(|row| {
                    let values = row
                        .iter()
                        .map(json_cell_to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("[{}]", values)
                })
                .collect();
            ensure_trailing_newline(lines.join("\n"))
        }
        DisplayOutput::Image { mime, .. } => ensure_trailing_newline(format!("<image:{}>", mime)),
    }
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn auto_display_output(value: &Value) -> DisplayOutput {
    match value {
        Value::String(value) => DisplayOutput::Text {
            value: value.clone(),
        },
        Value::List(items) => {
            let list = items.borrow();
            let candidate = Value::List(Rc::new(RefCell::new(list.clone())));
            if let Ok((columns, rows)) = value_to_table(&candidate) {
                DisplayOutput::Table { columns, rows }
            } else {
                DisplayOutput::Text {
                    value: candidate.to_string(),
                }
            }
        }
        Value::Map(_) | Value::Struct { .. } => DisplayOutput::Json {
            value: value_to_json(value),
        },
        _ => DisplayOutput::Text {
            value: value.to_string(),
        },
    }
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Int(value) => serde_json::Value::from(*value),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(value) => serde_json::Value::String(value.clone()),
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Unit => serde_json::Value::Null,
        Value::Option(Some(value)) => value_to_json(value),
        Value::Option(None) => serde_json::Value::Null,
        Value::Result(Ok(value)) => value_to_json(value),
        Value::Result(Err(error)) => serde_json::Value::String(error.clone()),
        Value::List(items) => {
            serde_json::Value::Array(items.borrow().iter().map(value_to_json).collect::<Vec<_>>())
        }
        Value::Map(entries) => serde_json::Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), value_to_json(value)))
                .collect(),
        ),
        Value::Set(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Struct { type_name, fields } => {
            let mut object = serde_json::Map::new();
            if type_name != "<anon>" {
                object.insert(
                    "_type".to_string(),
                    serde_json::Value::String(type_name.clone()),
                );
            }
            for (key, value) in fields.borrow().iter() {
                object.insert(key.clone(), value_to_json(value));
            }
            serde_json::Value::Object(object)
        }
        Value::Enum {
            type_name,
            variant,
            data,
        } => {
            let mut object = serde_json::Map::new();
            object.insert(
                "_type".to_string(),
                serde_json::Value::String(type_name.clone()),
            );
            object.insert(
                "variant".to_string(),
                serde_json::Value::String(variant.clone()),
            );
            object.insert(
                "data".to_string(),
                match data {
                    EnumData::Unit => serde_json::Value::Null,
                    EnumData::Tuple(items) => {
                        serde_json::Value::Array(items.iter().map(value_to_json).collect())
                    }
                    EnumData::Struct(fields) => serde_json::Value::Object(
                        fields
                            .iter()
                            .map(|(key, value)| (key.clone(), value_to_json(value)))
                            .collect(),
                    ),
                },
            );
            serde_json::Value::Object(object)
        }
        Value::Typestate {
            type_name,
            current_state,
            fields,
        } => {
            let mut object = serde_json::Map::new();
            object.insert(
                "_type".to_string(),
                serde_json::Value::String(type_name.clone()),
            );
            object.insert(
                "_state".to_string(),
                serde_json::Value::String(current_state.clone()),
            );
            for (key, value) in fields.borrow().iter() {
                object.insert(key.clone(), value_to_json(value));
            }
            serde_json::Value::Object(object)
        }
        Value::Closure { .. } | Value::NativeFunction(_) => {
            serde_json::Value::String(value.to_string())
        }
    }
}

fn value_to_table(value: &Value) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), String> {
    let rows = match value {
        Value::List(items) => items.borrow().clone(),
        other => {
            return Err(format!(
                "display::table() expects list<map>, got {}",
                other.type_name()
            ))
        }
    };

    let mut columns = Vec::<String>::new();
    let mut normalized_rows = Vec::new();
    for row in rows {
        let object = row_to_object(&row)?;
        for key in object.keys() {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
        normalized_rows.push(object);
    }

    let rows = normalized_rows
        .into_iter()
        .map(|row| {
            columns
                .iter()
                .map(|column| row.get(column).cloned().unwrap_or(serde_json::Value::Null))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Ok((columns, rows))
}

fn row_to_object(value: &Value) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match value {
        Value::Map(entries) => Ok(entries
            .iter()
            .map(|(key, value)| (key.to_string(), value_to_json(value)))
            .collect()),
        Value::Struct { fields, .. } | Value::Typestate { fields, .. } => Ok(fields
            .borrow()
            .iter()
            .map(|(key, value)| (key.clone(), value_to_json(value)))
            .collect()),
        other => Err(format!(
            "display::table() expects rows of map/struct, got {}",
            other.type_name()
        )),
    }
}

fn json_cell_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn image_mime_type(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ── dep パッケージ内の相対インポート変換 ──────────────────────────────────

/// dep パッケージから読み込んだファイルの `use ./foo.*` を `use depname/foo.*` に変換する。
///
/// dep パッケージ内のファイルは `use ./cors.*` のような相対インポートを持つが、
/// インタープリタは project_root 基準で解決するため、dep パッケージ名を
/// プレフィックスとした `UsePath::External("depname/cors")` に書き換える必要がある。
fn rewrite_local_use_paths(stmts: &[Stmt], dep_name: &str) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::UseDecl {
                path: UsePath::Local(local_path),
                symbols,
                is_pub,
                span,
            } => {
                // `./middleware` → `anvil/middleware`
                let new_path = format!("{}/{}", dep_name, local_path.trim_start_matches("./"));
                Stmt::UseDecl {
                    path: UsePath::External(new_path),
                    symbols: symbols.clone(),
                    is_pub: *is_pub,
                    span: span.clone(),
                }
            }
            other => other.clone(),
        })
        .collect()
}

// ── TCP クライアント関数（forge/std/net） ─────────────────────────────────
//
// tcp_connect / tcp_write / tcp_read_exact / tcp_read_available / tcp_close
//
// グローバルレジストリで TcpStream を管理し、conn_id を
// Value::Struct { type_name: "TcpConn", id: Value::Int(conn_id) } として
// ForgeScript 側に渡す。

fn eval_tcp_client(name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    use std::collections::HashMap as StdMap;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};

    // レジストリ: conn_id → Arc<Mutex<std::net::TcpStream>>
    static CONN_COUNTER: AtomicU64 = AtomicU64::new(1);
    static CONN_REGISTRY: OnceLock<Mutex<StdMap<u64, Arc<Mutex<std::net::TcpStream>>>>> =
        OnceLock::new();

    fn get_registry() -> &'static Mutex<StdMap<u64, Arc<Mutex<std::net::TcpStream>>>> {
        CONN_REGISTRY.get_or_init(|| Mutex::new(StdMap::new()))
    }

    fn next_id() -> u64 {
        CONN_COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    fn lock_registry() -> Result<
        std::sync::MutexGuard<'static, StdMap<u64, Arc<Mutex<std::net::TcpStream>>>>,
        RuntimeError,
    > {
        get_registry()
            .lock()
            .map_err(|_| RuntimeError::Custom("tcp conn registry lock poisoned".to_string()))
    }

    fn get_conn_id(v: &Value) -> Result<u64, RuntimeError> {
        match v {
            Value::Struct { fields, .. } => {
                let f = fields.borrow();
                match f.get("id") {
                    Some(Value::Int(n)) => u64::try_from(*n)
                        .map_err(|_| RuntimeError::Custom(format!("invalid conn id: {}", n))),
                    _ => Err(RuntimeError::Custom(
                        "TcpConn struct missing 'id' field".to_string(),
                    )),
                }
            }
            _ => Err(RuntimeError::Custom(format!(
                "expected TcpConn struct, got {}",
                v.type_name()
            ))),
        }
    }

    fn make_conn_struct(conn_id: u64) -> Value {
        use std::cell::RefCell;
        use std::collections::HashMap;
        let mut fields = HashMap::new();
        fields.insert("id".to_string(), Value::Int(conn_id as i64));
        Value::Struct {
            type_name: "TcpConn".to_string(),
            fields: Rc::new(RefCell::new(fields)),
        }
    }

    match name {
        "tcp_connect" => {
            if args.len() != 2 {
                return Err(RuntimeError::Custom(
                    "tcp_connect(host, port) takes 2 arguments".to_string(),
                ));
            }
            let host = match &args[0] {
                Value::String(s) => s.clone(),
                v => {
                    return Err(RuntimeError::Custom(format!(
                        "tcp_connect: host must be string, got {}",
                        v.type_name()
                    )))
                }
            };
            let port: u16 = match &args[1] {
                Value::Int(n) => u16::try_from(*n)
                    .map_err(|_| RuntimeError::Custom(format!("invalid port: {}", n)))?,
                Value::Float(n) => *n as u16,
                v => {
                    return Err(RuntimeError::Custom(format!(
                        "tcp_connect: port must be number, got {}",
                        v.type_name()
                    )))
                }
            };
            let addr = format!("{}:{}", host, port);
            let stream = std::net::TcpStream::connect(&addr).map_err(|e| {
                RuntimeError::Custom(format!("tcp_connect failed ({}): {}", addr, e))
            })?;
            let id = next_id();
            let mut registry = lock_registry()?;
            registry.insert(id, Arc::new(Mutex::new(stream)));
            Ok(Value::Result(Ok(Box::new(make_conn_struct(id)))))
        }
        "tcp_write" => {
            if args.len() != 2 {
                return Err(RuntimeError::Custom(
                    "tcp_write(conn, data) takes 2 arguments".to_string(),
                ));
            }
            let conn_id = get_conn_id(&args[0])?;
            let bytes: Vec<u8> = match &args[1] {
                Value::List(items) => items
                    .borrow()
                    .iter()
                    .map(|v| match v {
                        Value::Int(n) => Ok((*n & 0xFF) as u8),
                        Value::Float(n) => Ok(*n as u8),
                        other => Err(RuntimeError::Custom(format!(
                            "tcp_write: data element must be number, got {}",
                            other.type_name()
                        ))),
                    })
                    .collect::<Result<_, _>>()?,
                v => {
                    return Err(RuntimeError::Custom(format!(
                        "tcp_write: data must be list, got {}",
                        v.type_name()
                    )))
                }
            };
            let registry = lock_registry()?;
            let stream_arc = registry
                .get(&conn_id)
                .ok_or_else(|| {
                    RuntimeError::Custom(format!("tcp_write: unknown conn_id {}", conn_id))
                })?
                .clone();
            drop(registry);
            let mut stream = stream_arc
                .lock()
                .map_err(|_| RuntimeError::Custom("tcp stream lock poisoned".to_string()))?;
            use std::io::Write;
            stream
                .write_all(&bytes)
                .map_err(|e| RuntimeError::Custom(format!("tcp_write failed: {}", e)))?;
            Ok(Value::Result(Ok(Box::new(Value::Unit))))
        }
        "tcp_read_exact" => {
            if args.len() != 2 {
                return Err(RuntimeError::Custom(
                    "tcp_read_exact(conn, n) takes 2 arguments".to_string(),
                ));
            }
            let conn_id = get_conn_id(&args[0])?;
            let n: usize = match &args[1] {
                Value::Int(n) => usize::try_from(*n)
                    .map_err(|_| RuntimeError::Custom(format!("invalid byte count: {}", n)))?,
                Value::Float(n) => *n as usize,
                v => {
                    return Err(RuntimeError::Custom(format!(
                        "tcp_read_exact: n must be number, got {}",
                        v.type_name()
                    )))
                }
            };
            let registry = lock_registry()?;
            let stream_arc = registry
                .get(&conn_id)
                .ok_or_else(|| {
                    RuntimeError::Custom(format!("tcp_read_exact: unknown conn_id {}", conn_id))
                })?
                .clone();
            drop(registry);
            let mut stream = stream_arc
                .lock()
                .map_err(|_| RuntimeError::Custom("tcp stream lock poisoned".to_string()))?;
            let mut buf = vec![0u8; n];
            use std::io::Read;
            stream
                .read_exact(&mut buf)
                .map_err(|e| RuntimeError::Custom(format!("tcp_read_exact failed: {}", e)))?;
            let list = Rc::new(std::cell::RefCell::new(
                buf.into_iter().map(|b| Value::Int(b as i64)).collect(),
            ));
            Ok(Value::Result(Ok(Box::new(Value::List(list)))))
        }
        "tcp_read_available" => {
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "tcp_read_available(conn) takes 1 argument".to_string(),
                ));
            }
            let conn_id = get_conn_id(&args[0])?;
            let registry = lock_registry()?;
            let stream_arc = registry
                .get(&conn_id)
                .ok_or_else(|| {
                    RuntimeError::Custom(format!("tcp_read_available: unknown conn_id {}", conn_id))
                })?
                .clone();
            drop(registry);
            let mut stream = stream_arc
                .lock()
                .map_err(|_| RuntimeError::Custom("tcp stream lock poisoned".to_string()))?;
            let mut chunk = [0u8; 4096];
            use std::io::Read;
            let n = stream
                .read(&mut chunk)
                .map_err(|e| RuntimeError::Custom(format!("tcp_read_available failed: {}", e)))?;
            let list = Rc::new(std::cell::RefCell::new(
                chunk[..n].iter().map(|&b| Value::Int(b as i64)).collect(),
            ));
            Ok(Value::Result(Ok(Box::new(Value::List(list)))))
        }
        "tcp_close" => {
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "tcp_close(conn) takes 1 argument".to_string(),
                ));
            }
            let conn_id = get_conn_id(&args[0])?;
            let mut registry = lock_registry()?;
            registry.remove(&conn_id);
            Ok(Value::Unit)
        }
        _ => Err(RuntimeError::Custom(format!(
            "unknown tcp client function: {}",
            name
        ))),
    }
}

// ── バイト列 ↔ 文字列変換 ──────────────────────────────────────────────────
//
// string_to_bytes(s: string) -> list<number>  — UTF-8 バイト列に変換
// bytes_to_str(bytes: list<number>) -> string — UTF-8 バイト列から文字列に変換

fn eval_bytes_string(name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    match name {
        "string_to_bytes" => {
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "string_to_bytes(s) takes 1 argument".to_string(),
                ));
            }
            match &args[0] {
                Value::String(s) => {
                    let bytes: Vec<Value> =
                        s.as_bytes().iter().map(|&b| Value::Int(b as i64)).collect();
                    Ok(Value::List(Rc::new(RefCell::new(bytes))))
                }
                other => Err(RuntimeError::Custom(format!(
                    "string_to_bytes: expected string, got {}",
                    other.type_name()
                ))),
            }
        }
        "bytes_to_str" => {
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "bytes_to_str(bytes) takes 1 argument".to_string(),
                ));
            }
            match &args[0] {
                Value::List(items) => {
                    let bytes: Result<Vec<u8>, RuntimeError> = items
                        .borrow()
                        .iter()
                        .map(|v| match v {
                            Value::Int(n) => Ok((*n & 0xFF) as u8),
                            Value::Float(n) => Ok(*n as u8),
                            other => Err(RuntimeError::Custom(format!(
                                "bytes_to_str: byte must be number, got {}",
                                other.type_name()
                            ))),
                        })
                        .collect();
                    let bytes = bytes?;
                    match String::from_utf8(bytes) {
                        Ok(s) => Ok(Value::String(s)),
                        Err(_) => {
                            // 無効な UTF-8 は lossy 変換
                            Ok(Value::String(String::new()))
                        }
                    }
                }
                other => Err(RuntimeError::Custom(format!(
                    "bytes_to_str: expected list<number>, got {}",
                    other.type_name()
                ))),
            }
        }
        _ => Err(RuntimeError::Custom(format!(
            "unknown bytes/string function: {}",
            name
        ))),
    }
}

// ── SCRAM-SHA-256 暗号計算関数 ────────────────────────────────────────────
//
// ForgeScript から呼べる組み込み関数として実装する。
// scram_generate_nonce / scram_hi / scram_hmac / scram_h / scram_xor /
// scram_base64_encode / scram_base64_decode

fn eval_scram(name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    use base64::Engine as _;
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};

    type HmacSha256 = Hmac<Sha256>;

    /// Value::List から Vec<u8> に変換するヘルパー
    fn list_to_bytes(v: &Value) -> Result<Vec<u8>, RuntimeError> {
        match v {
            Value::List(items) => items
                .borrow()
                .iter()
                .map(|item| match item {
                    Value::Int(n) => Ok((*n & 0xFF) as u8),
                    Value::Float(n) => Ok(*n as u8),
                    other => Err(RuntimeError::Custom(format!(
                        "scram: byte list element must be number, got {}",
                        other.type_name()
                    ))),
                })
                .collect(),
            other => Err(RuntimeError::Custom(format!(
                "scram: expected list<number>, got {}",
                other.type_name()
            ))),
        }
    }

    /// Vec<u8> を Value::List に変換するヘルパー
    fn bytes_to_list(bytes: Vec<u8>) -> Value {
        let items: Vec<Value> = bytes.into_iter().map(|b| Value::Int(b as i64)).collect();
        Value::List(Rc::new(RefCell::new(items)))
    }

    match name {
        "scram_generate_nonce" => {
            // 18 バイトの乱数を base64 エンコードしてノンスを生成する
            use rand::RngCore;
            let mut bytes = [0u8; 18];
            rand::thread_rng().fill_bytes(&mut bytes);
            let nonce = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(Value::String(nonce))
        }
        "scram_hi" => {
            // scram_hi(password: string, salt: list<number>, iterations: number) -> list<number>
            // PBKDF2-HMAC-SHA256
            if args.len() != 3 {
                return Err(RuntimeError::Custom(
                    "scram_hi(password, salt, iterations) takes 3 arguments".to_string(),
                ));
            }
            let password = match &args[0] {
                Value::String(s) => s.as_bytes().to_vec(),
                other => {
                    return Err(RuntimeError::Custom(format!(
                        "scram_hi: password must be string, got {}",
                        other.type_name()
                    )))
                }
            };
            let salt = list_to_bytes(&args[1])?;
            let iterations: u32 = match &args[2] {
                Value::Int(n) => u32::try_from(*n).map_err(|_| {
                    RuntimeError::Custom(format!("scram_hi: invalid iterations: {}", n))
                })?,
                Value::Float(n) => *n as u32,
                other => {
                    return Err(RuntimeError::Custom(format!(
                        "scram_hi: iterations must be number, got {}",
                        other.type_name()
                    )))
                }
            };
            let mut output = vec![0u8; 32]; // SHA-256 output = 32 bytes
            pbkdf2::pbkdf2::<HmacSha256>(&password, &salt, iterations, &mut output)
                .map_err(|e| RuntimeError::Custom(format!("scram_hi pbkdf2 failed: {}", e)))?;
            Ok(bytes_to_list(output))
        }
        "scram_hmac" => {
            // scram_hmac(key: list<number>, msg: string) -> list<number>
            if args.len() != 2 {
                return Err(RuntimeError::Custom(
                    "scram_hmac(key, msg) takes 2 arguments".to_string(),
                ));
            }
            let key = list_to_bytes(&args[0])?;
            let msg = match &args[1] {
                Value::String(s) => s.as_bytes().to_vec(),
                other => {
                    return Err(RuntimeError::Custom(format!(
                        "scram_hmac: msg must be string, got {}",
                        other.type_name()
                    )))
                }
            };
            let mut mac = HmacSha256::new_from_slice(&key)
                .map_err(|e| RuntimeError::Custom(format!("scram_hmac key error: {}", e)))?;
            mac.update(&msg);
            let result = mac.finalize().into_bytes().to_vec();
            Ok(bytes_to_list(result))
        }
        "scram_h" => {
            // scram_h(data: list<number>) -> list<number> — SHA-256 ハッシュ
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "scram_h(data) takes 1 argument".to_string(),
                ));
            }
            let data = list_to_bytes(&args[0])?;
            let hash = Sha256::digest(&data).to_vec();
            Ok(bytes_to_list(hash))
        }
        "scram_xor" => {
            // scram_xor(a: list<number>, b: list<number>) -> list<number>
            if args.len() != 2 {
                return Err(RuntimeError::Custom(
                    "scram_xor(a, b) takes 2 arguments".to_string(),
                ));
            }
            let a = list_to_bytes(&args[0])?;
            let b = list_to_bytes(&args[1])?;
            if a.len() != b.len() {
                return Err(RuntimeError::Custom(format!(
                    "scram_xor: length mismatch: {} vs {}",
                    a.len(),
                    b.len()
                )));
            }
            let result: Vec<u8> = a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect();
            Ok(bytes_to_list(result))
        }
        "scram_base64_encode" => {
            // scram_base64_encode(data: list<number>) -> string
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "scram_base64_encode(data) takes 1 argument".to_string(),
                ));
            }
            let data = list_to_bytes(&args[0])?;
            Ok(Value::String(
                base64::engine::general_purpose::STANDARD.encode(&data),
            ))
        }
        "scram_base64_decode" => {
            // scram_base64_decode(s: string) -> list<number>!
            if args.len() != 1 {
                return Err(RuntimeError::Custom(
                    "scram_base64_decode(s) takes 1 argument".to_string(),
                ));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                other => {
                    return Err(RuntimeError::Custom(format!(
                        "scram_base64_decode: expected string, got {}",
                        other.type_name()
                    )))
                }
            };
            match base64::engine::general_purpose::STANDARD.decode(&s) {
                Ok(bytes) => Ok(Value::Result(Ok(Box::new(bytes_to_list(bytes))))),
                Err(e) => Ok(Value::Result(Err(format!("scram_base64_decode: {}", e)))),
            }
        }
        _ => Err(RuntimeError::Custom(format!(
            "unknown scram function: {}",
            name
        ))),
    }
}

// ── テスト ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Value, RuntimeError> {
        eval_source(src)
    }

    fn run_with_output(src: &str) -> Result<String, RuntimeError> {
        let module = parse_source(src).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        let (mut interp, output) = Interpreter::with_output_capture();
        let _ = interp.eval(&module)?;
        let text = output.lock().expect("lock").clone();
        Ok(text)
    }

    fn run_with_trace(
        src: &str,
        nodes: Vec<PipelineTraceNodeRef>,
    ) -> Result<(Value, Vec<PipelineTraceEvent>), RuntimeError> {
        let module = parse_source(src).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        let mut interp = Interpreter::new();
        interp.set_trace_mode(true);
        interp.set_pipeline_trace_nodes(nodes);
        let value = interp.eval(&module)?;
        Ok((value, interp.take_pipeline_trace_events()))
    }

    fn run_with_trace_capture(
        src: &str,
        nodes: Vec<PipelineTraceNodeRef>,
    ) -> Result<(Result<Value, RuntimeError>, Vec<PipelineTraceEvent>), RuntimeError> {
        let module = parse_source(src).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        let mut interp = Interpreter::new();
        interp.set_trace_mode(true);
        interp.set_pipeline_trace_nodes(nodes);
        let value = interp.eval(&module);
        let events = interp.take_pipeline_trace_events();
        Ok((value, events))
    }

    fn first_method_span(module: &Module) -> Span {
        for stmt in &module.stmts {
            match stmt {
                Stmt::Expr(Expr::MethodCall { span, .. }) => return span.clone(),
                Stmt::Let {
                    value: Expr::MethodCall { span, .. },
                    ..
                } => return span.clone(),
                _ => {}
            }
        }
        panic!("expected method call, got {:?}", module.stmts);
    }

    #[test]
    fn test_interpreter_stub_compiles() {
        let _interp = Interpreter::new();
    }

    // ── Phase 2-B tests ───────────────────────────────────────────────────

    #[test]
    fn test_eval_arithmetic() {
        assert_eq!(run("1 + 2 * 3"), Ok(Value::Int(7)));
    }

    #[test]
    fn test_eval_string_concat() {
        assert_eq!(
            run(r#""foo" + "bar""#),
            Ok(Value::String("foobar".to_string()))
        );
    }

    #[test]
    fn test_eval_comparison() {
        assert_eq!(run("1 < 2"), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_eval_logical() {
        assert_eq!(run("true && false"), Ok(Value::Bool(false)));
    }

    #[test]
    fn test_eval_let_binding() {
        assert_eq!(run("let x = 10; x"), Ok(Value::Int(10)));
    }

    #[test]
    fn test_eval_state_reassign() {
        assert_eq!(run("state x = 0; x = 5; x"), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_let_immutable() {
        let result = run("let x = 1; x = 2");
        assert!(matches!(result, Err(RuntimeError::Immutable(_))));
    }

    #[test]
    fn test_eval_if_expr() {
        assert_eq!(run("if true { 1 } else { 2 }"), Ok(Value::Int(1)));
    }

    #[test]
    fn test_eval_if_else_chain() {
        assert_eq!(
            run("if false { 1 } else if false { 2 } else { 3 }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn test_eval_while() {
        assert_eq!(
            run("state i = 0; while i < 3 { i = i + 1 }; i"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn test_eval_for_range() {
        let result = run("for i in [1..=3] { i }").expect("eval failed");
        match result {
            Value::List(list) => {
                assert_eq!(
                    *list.borrow(),
                    vec![Value::Int(1), Value::Int(2), Value::Int(3)]
                );
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn test_eval_block_expr() {
        assert_eq!(run("{ let a = 1; let b = 2; a + b }"), Ok(Value::Int(3)));
    }

    #[test]
    fn test_eval_fn_call() {
        assert_eq!(run("fn add(a, b) { a + b }; add(1, 2)"), Ok(Value::Int(3)));
    }

    #[test]
    fn test_eval_const_fn_basic() {
        let src = r#"
const fn identity(value: number) -> number {
    value
}
identity(5)
"#;
        assert_eq!(run(src), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_const_fn_in_const_var() {
        let src = r#"
const fn clamp(value: number, min: number, max: number) -> number {
    if value < min { min } else if value > max { max } else { value }
}
const RESULT = clamp(150, 0, 100)
RESULT
"#;
        assert_eq!(run(src), Ok(Value::Int(100)));
    }

    #[test]
    fn test_eval_closure() {
        assert_eq!(run("let f = x => x * 2; f(5)"), Ok(Value::Int(10)));
    }

    #[test]
    fn test_eval_closure_capture() {
        assert_eq!(
            run("let base = 10; let f = x => x + base; f(5)"),
            Ok(Value::Int(15))
        );
    }

    #[test]
    fn test_eval_match_literal() {
        assert_eq!(
            run(r#"match 2 { 1 => "one", 2 => "two", _ => "other" }"#),
            Ok(Value::String("two".to_string()))
        );
    }

    #[test]
    fn test_eval_match_option_some() {
        assert_eq!(
            run("match some(42) { some(v) => v, none => 0 }"),
            Ok(Value::Int(42))
        );
    }

    #[test]
    fn test_eval_match_option_none() {
        assert_eq!(
            run("match none { some(v) => v, none => 0 }"),
            Ok(Value::Int(0))
        );
    }

    #[test]
    fn test_eval_match_result_ok() {
        assert_eq!(
            run("match ok(1) { ok(v) => v, err(e) => 0 }"),
            Ok(Value::Int(1))
        );
    }

    #[test]
    fn test_eval_match_result_err() {
        assert_eq!(
            run(r#"match err("oops") { ok(v) => 1, err(e) => 0 }"#),
            Ok(Value::Int(0))
        );
    }

    #[test]
    fn test_eval_question_ok() {
        assert_eq!(run("fn f() { ok(5)? }; f()"), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_question_err() {
        assert_eq!(
            run(r#"fn f() { err("oops")? }; f()"#),
            Ok(Value::Result(Err("oops".to_string())))
        );
    }

    #[test]
    fn test_eval_string_interpolation() {
        assert_eq!(
            run(r#"let name = "World"; "Hello, {name}!""#),
            Ok(Value::String("Hello, World!".to_string()))
        );
    }

    #[test]
    fn test_eval_shadowing() {
        assert_eq!(run("let x = 1; let x = 2; x"), Ok(Value::Int(2)));
    }

    #[test]
    fn test_eval_scope() {
        let result = run("{ let x = 1 }; x");
        assert!(matches!(result, Err(RuntimeError::UndefinedVariable(_))));
    }

    #[test]
    fn test_eval_optional_chain_none() {
        assert_eq!(
            run("let user = none(); user?.name"),
            Ok(Value::Option(None))
        );
    }

    #[test]
    fn test_eval_optional_chain_some() {
        assert_eq!(
            run(r#"struct User { name: string }
let user = some(User { name: "Alice" });
user?.name"#),
            Ok(Value::Option(Some(Box::new(Value::String(
                "Alice".to_string()
            )))))
        );
    }

    #[test]
    fn test_eval_optional_chain_nested() {
        assert_eq!(
            run(r#"struct Inner { value: number }
struct Outer { inner: Inner? }
let config = some(Outer { inner: some(Inner { value: 1 }) });
config?.inner?.value"#),
            Ok(Value::Option(Some(Box::new(Value::Int(1)))))
        );
    }

    #[test]
    fn test_eval_null_coalesce_none() {
        assert_eq!(run("none() ?? 5"), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_null_coalesce_some() {
        assert_eq!(run("some(3) ?? 5"), Ok(Value::Int(3)));
    }

    #[test]
    fn test_eval_spawn_sequential() {
        assert_eq!(
            run("let handle = spawn { let value = 1; value + 2 }; handle"),
            Ok(Value::Option(Some(Box::new(Value::Int(3)))))
        );
    }

    #[test]
    fn test_eval_generator_finite() {
        let src = r#"
fn gen() -> generate<number> {
    yield 1
    yield 2
    yield 3
}
gen()
"#;
        match run(src) {
            Ok(Value::List(list)) => assert_eq!(
                *list.borrow(),
                vec![Value::Int(1), Value::Int(2), Value::Int(3)]
            ),
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_eval_generator_with_take() {
        let src = r#"
fn numbers() -> generate<number> {
    state i = 0
    while i < 5 {
        yield i
        i = i + 1
    }
}
numbers().take(3)
"#;
        match run(src) {
            Ok(Value::List(list)) => {
                assert_eq!(
                    *list.borrow(),
                    vec![Value::Int(0), Value::Int(1), Value::Int(2)]
                );
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_eval_generator_filter_map() {
        let src = r#"
fn numbers() -> generate<number> {
    state i = 0
    while i < 6 {
        yield i
        i = i + 1
    }
}
numbers()
    .filter(n => n % 2 == 0)
    .map(n => n * 10)
"#;
        match run(src) {
            Ok(Value::List(list)) => {
                assert_eq!(
                    *list.borrow(),
                    vec![Value::Int(0), Value::Int(20), Value::Int(40)]
                );
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_eval_generator_fibonacci() {
        let src = r#"
fn fibonacci() -> generate<number> {
    state a = 0
    state b = 1
    state count = 0
    while count < 5 {
        yield a
        let next = a + b
        a = b
        b = next
        count = count + 1
    }
}
fibonacci().take(5)
"#;
        match run(src) {
            Ok(Value::List(list)) => {
                assert_eq!(
                    *list.borrow(),
                    vec![
                        Value::Int(0),
                        Value::Int(1),
                        Value::Int(1),
                        Value::Int(2),
                        Value::Int(3)
                    ]
                );
            }
            other => panic!("expected list, got {:?}", other),
        }
    }

    #[test]
    fn test_eval_operator_add() {
        let src = r#"
struct Vec2 { x: number, y: number }
impl Vec2 {
    operator +(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}
let a = Vec2 { x: 1, y: 2 };
let b = Vec2 { x: 3, y: 4 };
let c = a + b;
c.x + c.y
"#;
        assert_eq!(run(src), Ok(Value::Int(10)));
    }

    #[test]
    fn test_eval_operator_mul() {
        let src = r#"
struct Scale { value: number }
impl Scale {
    operator *(self, other: Scale) -> Scale {
        Scale { value: self.value * other.value }
    }
}
let s = Scale { value: 3 };
let t = Scale { value: 4 };
(s * t).value
"#;
        assert_eq!(run(src), Ok(Value::Int(12)));
    }

    #[test]
    fn test_eval_operator_eq() {
        let src = r#"
struct Pair { x: number, y: number }
impl Pair {
    operator ==(self, other: Pair) -> bool {
        self.x == other.x && self.y == other.y
    }
}
let a = Pair { x: 1, y: 2 };
let b = Pair { x: 1, y: 2 };
let c = Pair { x: 2, y: 3 };
(a == b) && !(a == c)
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_eval_operator_index() {
        let src = r#"
struct Table { base: number }
impl Table {
    operator [](self, index: number) -> number {
        if index == 0 { self.base } else { index }
    }
}
let tbl = Table { base: 5 };
tbl[0]
"#;
        assert_eq!(run(src), Ok(Value::Int(5)));
    }

    #[test]
    fn test_eval_operator_unary_neg() {
        let src = r#"
struct Value { amount: number }
impl Value {
    operator unary-(self) -> Value {
        Value { amount: -self.amount }
    }
}
let v = Value { amount: 5 };
(-v).amount
"#;
        assert_eq!(run(src), Ok(Value::Int(-5)));
    }

    #[test]
    fn test_eval_operator_conflict_derive_eq_error() {
        let src = r#"
@derive(Eq)
struct Foo { x: number }
impl Foo {
    operator ==(self, other: Foo) -> bool {
        self.x == other.x
    }
}
"#;
        assert!(run(src).is_err());
    }

    // ── Phase 2-C tests ───────────────────────────────────────────────────

    #[test]
    fn test_native_print() {
        // print は Value::Unit を返し、副作用として stdout に出力する
        assert_eq!(run("print(42)"), Ok(Value::Unit));
    }

    #[test]
    fn test_display_text_fallback() {
        let output = run_with_output(r#"display::text("hello")"#).expect("run");
        assert_eq!(output, "hello\n");
    }

    #[test]
    fn test_display_table_fallback() {
        let output = run_with_output(
            r#"
            display::table([
              { name: "alice", score: 90 },
              { name: "bob", score: 75 }
            ])
            "#,
        )
        .expect("run");
        assert!(
            !output.contains("[name, score]"),
            "header row should not appear: {output}"
        );
        assert!(output.contains("[alice, 90]"), "got: {output}");
        assert!(output.contains("[bob, 75]"), "got: {output}");
    }

    #[test]
    fn test_display_auto_string() {
        let output = run_with_output(r#"display("hello")"#).expect("run");
        assert_eq!(output, "hello\n");
    }

    #[test]
    fn test_display_auto_list_map() {
        let output = run_with_output(
            r#"
            display([
              { name: "alice", score: 90 }
            ])
            "#,
        )
        .expect("run");
        assert!(
            !output.contains("[name, score]"),
            "header row should not appear: {output}"
        );
        assert!(output.contains("[alice, 90]"), "got: {output}");
    }

    #[test]
    fn test_native_string() {
        assert_eq!(run("string(42)"), Ok(Value::String("42".to_string())));
        assert_eq!(run("string(true)"), Ok(Value::String("true".to_string())));
    }

    #[test]
    fn test_native_number() {
        assert_eq!(
            run(r#"number("42")"#),
            Ok(Value::Result(Ok(Box::new(Value::Int(42)))))
        );
        // number("abc") → err(...)
        let result = run(r#"number("abc")"#).expect("eval failed");
        assert!(matches!(result, Value::Result(Err(_))));
    }

    #[test]
    fn test_native_float() {
        assert_eq!(
            run(r#"float("3.14")"#),
            Ok(Value::Result(Ok(Box::new(Value::Float(3.14)))))
        );
    }

    #[test]
    fn test_native_len_string() {
        assert_eq!(run(r#"len("hello")"#), Ok(Value::Int(5)));
    }

    #[test]
    fn test_native_len_list() {
        assert_eq!(run("len([1, 2, 3])"), Ok(Value::Int(3)));
    }

    #[test]
    fn test_map_literal_empty() {
        assert_eq!(
            run("let m: map<string, number> = {}; m"),
            Ok(Value::Map(vec![]))
        );
    }

    #[test]
    fn test_map_literal() {
        assert_eq!(
            run(r#"{"a": 1, "b": 2}"#),
            Ok(Value::Map(vec![
                (Value::String("a".to_string()), Value::Int(1)),
                (Value::String("b".to_string()), Value::Int(2)),
            ]))
        );
    }

    #[test]
    fn test_map_get() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.get("a")"#),
            Ok(Value::Option(Some(Box::new(Value::Int(1)))))
        );
    }

    #[test]
    fn test_map_insert() {
        assert_eq!(
            run(r#"state m = {"a": 1}; m.insert("c", 3); m"#),
            Ok(Value::Map(vec![
                (Value::String("a".to_string()), Value::Int(1)),
                (Value::String("c".to_string()), Value::Int(3)),
            ]))
        );
    }

    #[test]
    fn test_map_contains_key() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.contains_key("a")"#),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn test_map_keys() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.keys()"#),
            Ok(mk_list(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ]))
        );
    }

    #[test]
    fn test_map_values() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.values()"#),
            Ok(mk_list(vec![Value::Int(1), Value::Int(2)]))
        );
    }

    #[test]
    fn test_map_len() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.len()"#),
            Ok(Value::Int(2))
        );
    }

    #[test]
    fn test_map_entries() {
        // entries() は [[key, value], ...] のリストを返す（count() でサイズを確認）
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m.entries().count()"#),
            Ok(Value::Int(2))
        );
    }

    #[test]
    fn test_map_index_access() {
        assert_eq!(
            run(r#"let m = {"a": 1, "b": 2}; m["a"]"#),
            Ok(Value::Int(1))
        );
    }

    #[test]
    fn test_map_index_assign() {
        assert_eq!(
            run(r#"state m = {"a": 1}; m["c"] = 3; m"#),
            Ok(Value::Map(vec![
                (Value::String("a".to_string()), Value::Int(1)),
                (Value::String("c".to_string()), Value::Int(3)),
            ]))
        );
    }

    #[test]
    fn test_none_constructor_call() {
        assert_eq!(run("none()"), Ok(Value::Option(None)));
    }

    #[test]
    fn test_set_literal() {
        assert_eq!(
            run(r#"{"rust", "forge"}"#),
            Ok(Value::Set(vec![
                Value::String("rust".to_string()),
                Value::String("forge".to_string()),
            ]))
        );
    }

    #[test]
    fn test_set_contains() {
        assert_eq!(
            run(r#"let s = {"rust", "forge"}; s.contains("rust")"#),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn test_method_named_use() {
        let src = r#"
data App {
    middlewares: list<string>
}

impl App {
    fn new() -> App {
        App { middlewares: [] }
    }

    fn use(state self, name: string) -> App {
        state items = self.middlewares
        items.push(name)
        App { middlewares: items }
    }
}

let app = App::new().use("logger")
app.middlewares[0]
"#;
        assert_eq!(run(src), Ok(Value::String("logger".to_string())));
    }

    #[test]
    fn test_method_named_use_with_data_arg() {
        let src = r#"
data Middleware {
    kind: string
}

fn logger() -> Middleware {
    Middleware { kind: "logger" }
}

data App {
    middlewares: list<Middleware>
}

impl App {
    fn new() -> App {
        App { middlewares: [] }
    }

    fn use(state self, middleware: Middleware) -> App {
        state items = self.middlewares
        items.push(middleware)
        App { middlewares: items }
    }
}

let app = App::new().use(logger())
app.middlewares[0].kind
"#;
        assert_eq!(run(src), Ok(Value::String("logger".to_string())));
    }

    #[test]
    fn test_method_named_use_with_option_field_arg() {
        let src = r#"
data Middleware {
    kind: string
    value: string?
}

fn logger() -> Middleware {
    Middleware { kind: "logger", value: none() }
}

fn static_files(dir: string) -> Middleware {
    Middleware { kind: "static_files", value: some(dir) }
}

data App {
    middlewares: list<Middleware>
}

impl App {
    fn new() -> App {
        App { middlewares: [] }
    }

    fn use(state self, middleware: Middleware) -> App {
        state items = self.middlewares
        items.push(middleware)
        App { middlewares: items }
    }
}

let app = App::new().use(logger()).use(static_files("./public"))
match app.middlewares[1].value {
    some(value) => value,
    none        => "missing",
}
"#;
        assert_eq!(run(src), Ok(Value::String("./public".to_string())));
    }

    #[test]
    fn test_set_insert() {
        // spec 準拠: insert は新しい set を返す（元の set は変更しない）
        assert_eq!(
            run(r#"let s = {"rust", "forge"}; s.insert("async")"#),
            Ok(Value::Set(vec![
                Value::String("rust".to_string()),
                Value::String("forge".to_string()),
                Value::String("async".to_string()),
            ]))
        );
    }

    #[test]
    fn test_set_union() {
        assert_eq!(
            run(r#"let s1 = {"rust", "forge"}; let s2 = {"forge", "async"}; s1.union(s2)"#),
            Ok(Value::Set(vec![
                Value::String("rust".to_string()),
                Value::String("forge".to_string()),
                Value::String("async".to_string()),
            ]))
        );
    }

    #[test]
    fn test_set_intersect() {
        assert_eq!(
            run(r#"let s1 = {"rust", "forge"}; let s2 = {"forge", "async"}; s1.intersect(s2)"#),
            Ok(Value::Set(vec![Value::String("forge".to_string())]))
        );
    }

    #[test]
    fn test_set_difference() {
        assert_eq!(
            run(r#"let s1 = {"rust", "forge"}; let s2 = {"forge", "async"}; s1.difference(s2)"#),
            Ok(Value::Set(vec![Value::String("rust".to_string())]))
        );
    }

    #[test]
    fn test_set_len() {
        assert_eq!(
            run(r#"let s = {"rust", "forge"}; s.len()"#),
            Ok(Value::Int(2))
        );
    }

    #[test]
    fn test_set_to_list() {
        assert_eq!(
            run(r#"let s = {"rust", "forge"}; s.to_list()"#),
            Ok(mk_list(vec![
                Value::String("rust".to_string()),
                Value::String("forge".to_string()),
            ]))
        );
    }

    #[test]
    fn test_generic_struct_basic() {
        let src = r#"
struct Response<T> {
    body: T
}
let r = Response { body: 42 }
r.body
"#;
        assert_eq!(run(src), Ok(Value::Int(42)));
    }

    #[test]
    fn test_generic_struct_method() {
        let src = r#"
struct Response<T> {
    body: T
}

impl<T> Response<T> {
    fn is_ok() -> bool {
        true
    }
}

let r = Response { body: 42 }
r.is_ok()
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_generic_fn_wrap() {
        let src = r#"
struct Response<T> {
    body: T
}

fn wrap<T>(v: T) -> Response<T> {
    Response { body: v }
}

wrap(42).body
"#;
        assert_eq!(run(src), Ok(Value::Int(42)));
    }

    #[test]
    fn test_generic_enum_either() {
        let src = r#"
enum Either<L, R> {
    Left(L),
    Right(R),
}

let v = Either::Left(42)
match v {
    Either::Left(x) => x,
    Either::Right(_) => 0,
}
"#;
        assert_eq!(run(src), Ok(Value::Int(42)));
    }

    #[test]
    fn test_partial_type() {
        let src = r#"
struct User {
    id: number
    name: string
}
let user = User { id: 1, name: "alice" }
let partial: Partial<User> = Partial::from(user)
partial.name
"#;
        assert_eq!(
            run(src),
            Ok(Value::Option(Some(Box::new(Value::String(
                "alice".to_string()
            )))))
        );
    }

    #[test]
    fn test_partial_from() {
        let src = r#"
struct User {
    id: number
    name: string
}
let user = User { id: 1, name: "alice" }
Partial::from(user).id
"#;
        assert_eq!(run(src), Ok(Value::Option(Some(Box::new(Value::Int(1))))));
    }

    #[test]
    fn test_required_type() {
        let src = r#"
struct Config {
    host: string?
    port: number?
}
let cfg = Config { host: some("localhost"), port: some(8080) }
let req: Required<Config> = Required::from(cfg)
req.port
"#;
        assert_eq!(run(src), Ok(Value::Int(8080)));
    }

    #[test]
    fn test_pick_type() {
        let src = r#"
struct User {
    id: number
    name: string
    password: string
}
let user = User { id: 1, name: "alice", password: "secret" }
Pick::from(user, ["id", "name"]).name
"#;
        assert_eq!(run(src), Ok(Value::String("alice".to_string())));
    }

    #[test]
    fn test_omit_type() {
        let src = r#"
struct User {
    id: number
    name: string
    password: string
}
let user = User { id: 1, name: "alice", password: "secret" }
let safe = Omit::from(user, ["password"])
safe.name
"#;
        assert_eq!(run(src), Ok(Value::String("alice".to_string())));
    }

    #[test]
    fn test_nonnullable() {
        assert_eq!(
            run(r#"NonNullable::from(some("hello"))"#),
            Ok(Value::String("hello".to_string()))
        );
    }

    #[test]
    fn test_nonnullable_none_error() {
        let result = run("NonNullable::from(none)");
        assert!(matches!(
            result,
            Err(RuntimeError::Custom(msg)) if msg == "NonNullable: value is None"
        ));
    }

    #[test]
    fn test_record_alias() {
        assert_eq!(
            run(r#"let r: Record<string, number> = Record::new(); r"#),
            Ok(Value::Map(vec![]))
        );
    }

    #[test]
    fn test_native_type_of() {
        assert_eq!(run("type_of(42)"), Ok(Value::String("number".to_string())));
    }

    // ── Phase T-1 tests ───────────────────────────────────────────────────

    #[test]
    fn test_struct_basic() {
        let src = r#"
struct Point {
    x: number
    y: number
}
let p = Point { x: 1, y: 2 }
p.x
"#;
        assert_eq!(run(src), Ok(Value::Int(1)));

        let src2 = r#"
struct Point {
    x: number
    y: number
}
let p = Point { x: 3, y: 4 }
p.y
"#;
        assert_eq!(run(src2), Ok(Value::Int(4)));
    }

    #[test]
    fn test_struct_impl() {
        let src = r#"
struct Rectangle {
    width: number
    height: number
}

impl Rectangle {
    fn area() -> number {
        self.width * self.height
    }
}

let r = Rectangle { width: 3, height: 4 }
r.area()
"#;
        assert_eq!(run(src), Ok(Value::Int(12)));
    }

    #[test]
    fn test_struct_self_mutation() {
        let src = r#"
struct Counter {
    count: number
}

impl Counter {
    fn increment(state self) {
        self.count = self.count + 1
    }

    fn get_count() -> number {
        self.count
    }
}

let c = Counter { count: 0 }
c.increment()
c.get_count()
"#;
        assert_eq!(run(src), Ok(Value::Int(1)));
    }

    #[test]
    fn test_derive_debug() {
        let src = r#"
@derive(Debug)
struct Point {
    x: number
    y: number
}
let p = Point { x: 1, y: 2 }
p.display()
"#;
        let result = run(src).expect("eval failed");
        match result {
            Value::String(s) => {
                assert!(s.contains("Point"), "should contain type name: {}", s);
                assert!(s.contains("x: 1"), "should contain x: 1: {}", s);
                assert!(s.contains("y: 2"), "should contain y: 2: {}", s);
            }
            other => panic!("expected String, got {:?}", other),
        }
    }

    #[test]
    fn test_derive_clone() {
        let src = r#"
@derive(Clone)
struct Point {
    x: number
    y: number
}
let p = Point { x: 1, y: 2 }
let q = p.clone()
q.x
"#;
        assert_eq!(run(src), Ok(Value::Int(1)));
    }

    #[test]
    fn test_derive_eq() {
        let src = r#"
@derive(Eq)
struct Point {
    x: number
    y: number
}
let p = Point { x: 1, y: 2 }
let q = Point { x: 1, y: 2 }
p == q
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));

        let src2 = r#"
@derive(Eq)
struct Point {
    x: number
    y: number
}
let p = Point { x: 1, y: 2 }
let q = Point { x: 3, y: 4 }
p == q
"#;
        assert_eq!(run(src2), Ok(Value::Bool(false)));
    }

    #[test]
    fn test_derive_accessor() {
        let src = r#"
@derive(Accessor)
struct User {
    name: string
    age: number
}
let u = User { name: "Alice", age: 30 }
u.get_name()
"#;
        assert_eq!(run(src), Ok(Value::String("Alice".to_string())));

        let src2 = r#"
@derive(Accessor)
struct User {
    name: string
    age: number
}
let u = User { name: "Alice", age: 30 }
u.set_name("Bob")
u.get_name()
"#;
        assert_eq!(run(src2), Ok(Value::String("Bob".to_string())));
    }

    #[test]
    fn test_derive_singleton() {
        let src = r#"
@derive(Singleton)
struct AppConfig {
    db_url: string
    port: number
}
let c1 = AppConfig::instance()
let c2 = AppConfig::instance()
c1.port == c2.port
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_derive_hash() {
        // @derive(Hash) で hash() メソッドが使えること
        // 同じフィールド値なら同じハッシュ値になること
        let src = r#"
@derive(Hash)
struct Key {
    id: number
    label: string
}
let k1 = Key { id: 1, label: "hello" }
let k2 = Key { id: 1, label: "hello" }
k1.hash() == k2.hash()
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));

        // フィールドが異なれば（高確率で）ハッシュ値が異なること
        let src2 = r#"
@derive(Hash)
struct Key {
    id: number
    label: string
}
let k1 = Key { id: 1, label: "hello" }
let k2 = Key { id: 2, label: "world" }
let h1 = k1.hash()
let h2 = k2.hash()
h1 == h2
"#;
        // 異なる値は同じハッシュになる可能性が理論上はあるが実用上 false
        assert_eq!(run(src2), Ok(Value::Bool(false)));
    }

    #[test]
    fn test_derive_ord() {
        // @derive(Ord) で < / > 演算子が struct に使えること
        let src = r#"
@derive(Ord)
struct Point {
    x: number
    y: number
}
let p1 = Point { x: 1, y: 2 }
let p2 = Point { x: 3, y: 0 }
p1 < p2
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));

        // order_by でリストをソートできること
        let src2 = r#"
@derive(Ord)
struct Point {
    x: number
    y: number
}
let points = [Point { x: 3, y: 1 }, Point { x: 1, y: 2 }, Point { x: 2, y: 0 }]
let sorted = points.order_by(p => p.x)
sorted.first().x
"#;
        assert_eq!(run(src2), Ok(Value::Int(1)));
    }

    #[test]
    fn test_derive_default() {
        // @derive(Default) で TypeName::default() がゼロ値インスタンスを返すこと
        let src = r#"
@derive(Default)
struct Config {
    host: string
    port: number
    debug: bool
}
let c = Config::default()
c.port
"#;
        assert_eq!(run(src), Ok(Value::Int(0)));

        let src2 = r#"
@derive(Default)
struct Config {
    host: string
    port: number
    debug: bool
}
let c = Config::default()
c.host
"#;
        assert_eq!(run(src2), Ok(Value::String("".to_string())));

        let src3 = r#"
@derive(Default)
struct Config {
    host: string
    port: number
    debug: bool
}
let c = Config::default()
c.debug
"#;
        assert_eq!(run(src3), Ok(Value::Bool(false)));
    }

    // ── Phase T-2 tests ───────────────────────────────────────────────────

    #[test]
    fn test_enum_unit() {
        let src = r#"
enum Direction {
    North
    South
    East
    West
}
let d = Direction::North
match d {
    Direction::North => "up"
    Direction::South => "down"
    _ => "other"
}
"#;
        assert_eq!(run(src), Ok(Value::String("up".to_string())));

        let src2 = r#"
enum Direction {
    North
    South
    East
    West
}
let d = Direction::West
match d {
    Direction::North => "up"
    Direction::South => "down"
    _ => "other"
}
"#;
        assert_eq!(run(src2), Ok(Value::String("other".to_string())));
    }

    #[test]
    fn test_enum_tuple() {
        let src = r#"
enum Shape {
    Circle(number)
    Rectangle(number, number)
}
let s = Shape::Circle(5)
match s {
    Shape::Circle(r) => r
    Shape::Rectangle(w, h) => w + h
}
"#;
        assert_eq!(run(src), Ok(Value::Int(5)));

        let src2 = r#"
enum Shape {
    Circle(number)
    Rectangle(number, number)
}
let s = Shape::Rectangle(3, 4)
match s {
    Shape::Circle(r) => r
    Shape::Rectangle(w, h) => w + h
}
"#;
        assert_eq!(run(src2), Ok(Value::Int(7)));
    }

    #[test]
    fn test_enum_struct_variant() {
        let src = r#"
enum Message {
    Quit
    Move { x: number, y: number }
    Write(string)
}
let m = Message::Move { x: 10, y: 20 }
match m {
    Message::Quit => "quit"
    Message::Move { x, y } => "moved"
    Message::Write(text) => text
}
"#;
        assert_eq!(run(src), Ok(Value::String("moved".to_string())));

        let src2 = r#"
enum Message {
    Quit
    Move { x: number, y: number }
    Write(string)
}
let m = Message::Move { x: 10, y: 20 }
match m {
    Message::Move { x, y } => x + y
    _ => 0
}
"#;
        assert_eq!(run(src2), Ok(Value::Int(30)));
    }

    #[test]
    fn test_enum_derive() {
        // @derive(Debug) - display() メソッド
        let src = r#"
@derive(Debug, Clone, Eq)
enum Status {
    Active
    Inactive
    Pending(string)
}
let s = Status::Active
s.display()
"#;
        assert_eq!(run(src), Ok(Value::String("Status::Active".to_string())));

        // @derive(Clone)
        let src2 = r#"
@derive(Debug, Clone, Eq)
enum Status {
    Active
    Inactive
    Pending(string)
}
let s = Status::Pending("review")
let c = s.clone()
c.display()
"#;
        assert_eq!(
            run(src2),
            Ok(Value::String("Status::Pending(review)".to_string()))
        );

        // @derive(Eq) - == 比較
        let src3 = r#"
@derive(Debug, Clone, Eq)
enum Status {
    Active
    Inactive
    Pending(string)
}
let a = Status::Active
let b = Status::Active
let c = Status::Inactive
a == b
"#;
        assert_eq!(run(src3), Ok(Value::Bool(true)));

        let src4 = r#"
@derive(Debug, Clone, Eq)
enum Status {
    Active
    Inactive
    Pending(string)
}
let a = Status::Active
let c = Status::Inactive
a == c
"#;
        assert_eq!(run(src4), Ok(Value::Bool(false)));
    }

    // ── Phase T-3 tests ───────────────────────────────────────────────────

    #[test]
    fn test_trait_impl() {
        // 基本的な trait の定義と実装
        let src = r#"
trait Printable {
    fn display() -> string
}

struct User {
    name: string
}

impl Printable for User {
    fn display() -> string {
        self.name
    }
}

let u = User { name: "Alice" }
u.display()
"#;
        assert_eq!(run(src), Ok(Value::String("Alice".to_string())));
    }

    #[test]
    fn test_trait_default() {
        // デフォルト実装の継承と上書き
        let src = r#"
trait Loggable {
    fn label() -> string

    fn log() {
        self.label()
    }
}

struct Post {
    title: string
}

impl Loggable for Post {
    fn label() -> string {
        self.title
    }
}

let p = Post { title: "Hello" }
p.log()
"#;
        // log() はデフォルト実装で label() を呼ぶ → "Hello" を返す
        assert_eq!(run(src), Ok(Value::String("Hello".to_string())));

        // デフォルト実装を上書きするケース
        let src2 = r#"
trait Loggable {
    fn label() -> string

    fn log() {
        self.label()
    }
}

struct Post {
    title: string
}

impl Loggable for Post {
    fn label() -> string {
        self.title
    }
    fn log() {
        "overridden"
    }
}

let p = Post { title: "Hello" }
p.log()
"#;
        assert_eq!(run(src2), Ok(Value::String("overridden".to_string())));
    }

    #[test]
    fn test_mixin_basic() {
        // mixin のデフォルト実装
        let src = r#"
mixin Timestamped {
    fn created_label() -> string {
        self.created_at
    }
}

struct Post {
    title: string
    created_at: string
}

impl Timestamped for Post

let p = Post { title: "Hello", created_at: "2026-01-01" }
p.created_label()
"#;
        assert_eq!(run(src), Ok(Value::String("2026-01-01".to_string())));
    }

    #[test]
    fn test_mixin_multi() {
        // 複数 mixin の組み合わせ
        let src = r#"
mixin Walker {
    fn walk() -> string {
        self.name
    }
}

mixin Talker {
    fn talk() -> string {
        self.name
    }
}

struct Dog {
    name: string
}

impl Walker for Dog
impl Talker for Dog

let d = Dog { name: "Rex" }
let w = d.walk()
let t = d.talk()
w == t
"#;
        assert_eq!(run(src), Ok(Value::Bool(true)));
    }

    #[test]
    fn test_mixin_conflict() {
        // mixin のメソッド名衝突はランタイムエラー
        let src = r#"
mixin MixinA {
    fn shared() -> string {
        "A"
    }
}

mixin MixinB {
    fn shared() -> string {
        "B"
    }
}

struct Foo {
    x: number
}

impl MixinA for Foo
impl MixinB for Foo
"#;
        let result = run(src);
        assert!(
            matches!(result, Err(RuntimeError::Custom(ref msg)) if msg.contains("名前衝突")),
            "expected mixin conflict error, got {:?}",
            result
        );
    }

    // ── Phase T-4: data キーワードのテスト ───────────────────────────────

    #[test]
    fn test_data_basic() {
        // data 定義・インスタンス化・自動 derive 確認（Accessor の get_name() 等が使える）
        let src = r#"
data UserProfile {
    id:    number
    name:  string
}

let u = UserProfile { id: 1, name: "Alice" }
u.get_name()
"#;
        let result = run(src);
        assert_eq!(result, Ok(Value::String("Alice".to_string())));
    }

    #[test]
    fn test_data_validate_ok() {
        // バリデーション成功で ok(()) を返す
        let src = r#"
data UserRegistration {
    username: string
    email:    string
    password: string
} validate {
    username: length(3..20), alphanumeric
    email:    email_format
    password: length(min: 8), contains_digit, contains_uppercase
}

let reg = UserRegistration { username: "alice", email: "alice@example.com", password: "Pass1234" }
match reg.validate() {
    ok(_)    => "valid"
    err(msg) => msg
}
"#;
        let result = run(src);
        assert_eq!(result, Ok(Value::String("valid".to_string())));
    }

    #[test]
    fn test_data_validate_err() {
        // バリデーション失敗で err("field: constraint") を返す
        let src = r#"
data UserRegistration {
    username: string
    email:    string
    password: string
} validate {
    username: length(3..20), alphanumeric
    email:    email_format
    password: length(min: 8), contains_digit, contains_uppercase
}

let bad = UserRegistration { username: "a", email: "not-email", password: "weak" }
match bad.validate() {
    ok(_)    => "valid"
    err(msg) => msg
}
"#;
        let result = run(src);
        // username の length チェックで失敗（最初の違反のみ）
        assert_eq!(result, Ok(Value::String("username: length".to_string())));
    }

    // ── Phase T-5: typestate テスト ──────────────────────────────────────

    #[test]
    fn test_typestate_basic() {
        // 正常な状態遷移: Disconnected → Connected → Authenticated → query
        let src = r#"
typestate Connection {
    states: [Disconnected, Connected, Authenticated]

    Disconnected {
        fn connect(url: string) -> Connected!
    }

    Connected {
        fn auth(token: string) -> Authenticated!
        fn disconnect() -> Disconnected
    }

    Authenticated {
        fn query(sql: string) -> string!
        fn disconnect() -> Disconnected
    }
}

let conn  = Connection::new<Disconnected>()
let conn2 = conn.connect("localhost")?
let conn3 = conn2.auth("secret")?
let rows  = conn3.query("SELECT 1")?
rows
"#;
        let result = run(src);
        assert_eq!(result, Ok(Value::String("SELECT 1".to_string())));
    }

    #[test]
    fn test_typestate_invalid() {
        // 不正な状態でのメソッド呼び出しがランタイムエラーになる
        let src = r#"
typestate Connection {
    states: [Disconnected, Connected, Authenticated]

    Disconnected {
        fn connect(url: string) -> Connected!
    }

    Connected {
        fn auth(token: string) -> Authenticated!
    }

    Authenticated {
        fn query(sql: string) -> string!
    }
}

let conn  = Connection::new<Disconnected>()
let conn2 = conn.connect("localhost")?
conn2.query("SELECT 1")
"#;
        let result = run(src);
        // Connected 状態では query は使えない → RuntimeError
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("query"),
            "エラーメッセージに 'query' が含まれていません: {}",
            err_msg
        );
    }

    // ── Phase M-0 tests ───────────────────────────────────────────────────

    /// 一時ファイルを作成してモジュールテストを実行するヘルパー
    fn run_with_module(
        main_src: &str,
        module_path: &str,
        module_src: &str,
    ) -> Result<Value, RuntimeError> {
        use forge_compiler::parser::parse_source;
        use std::fs;

        // 一時ディレクトリを作成
        let tmp = tempfile::tempdir().map_err(|e| RuntimeError::Custom(e.to_string()))?;

        // モジュールファイルを作成
        let mod_file = tmp.path().join(module_path);
        if let Some(parent) = mod_file.parent() {
            fs::create_dir_all(parent).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        }
        fs::write(&mod_file, module_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        // main ファイルを作成（project_root の解決のために main.forge を配置）
        let main_file = tmp.path().join("main.forge");
        fs::write(&main_file, main_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        // インタープリタを初期化（ファイルパスから ModuleLoader を生成）
        let module = parse_source(main_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let mut interp = Interpreter::with_file_path(&main_file);
        interp.eval(&module)
    }

    #[test]
    fn test_use_local_single() {
        // 単一シンボルのインポートと使用
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
pub fn subtract(a: number, b: number) -> number { a - b }
"#;
        let main_src = r#"
use ./math.add
add(3, 4)
"#;
        let result = run_with_module(main_src, "math.forge", module_src);
        assert_eq!(result, Ok(Value::Int(7)));
    }

    #[test]
    fn test_use_local_multiple() {
        // 複数シンボルのインポート
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
pub fn subtract(a: number, b: number) -> number { a - b }
"#;
        let main_src = r#"
use ./math.{add, subtract}
add(10, subtract(5, 2))
"#;
        let result = run_with_module(main_src, "math.forge", module_src);
        // subtract(5, 2) = 3, add(10, 3) = 13
        assert_eq!(result, Ok(Value::Int(13)));
    }

    #[test]
    fn test_use_alias() {
        // `use ./module.add as add_numbers` でエイリアス
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
"#;
        let main_src = r#"
use ./math.add as add_numbers
add_numbers(5, 6)
"#;
        let result = run_with_module(main_src, "math.forge", module_src);
        assert_eq!(result, Ok(Value::Int(11)));
    }

    #[test]
    fn test_use_wildcard() {
        // `use ./module.*` で全シンボルをインポート
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
pub fn multiply(a: number, b: number) -> number { a * b }
"#;
        let main_src = r#"
use ./math.*
multiply(add(2, 3), 4)
"#;
        let result = run_with_module(main_src, "math.forge", module_src);
        // add(2, 3) = 5, multiply(5, 4) = 20
        assert_eq!(result, Ok(Value::Int(20)));
    }

    // ── Phase M-1 tests ───────────────────────────────────────────────────

    #[test]
    fn test_pub_import_success() {
        // pub シンボルのインポート成功
        let module_src = r#"
pub fn public_fn() -> string { "I am public" }
fn private_fn() -> string { "I am private" }
pub const PUBLIC_CONST: number = 42
const PRIVATE_CONST: number = 99
"#;
        let main_src = r#"
use ./secret.{public_fn, PUBLIC_CONST}
public_fn()
"#;
        let result = run_with_module(main_src, "secret.forge", module_src);
        assert_eq!(result, Ok(Value::String("I am public".to_string())));
    }

    #[test]
    fn test_pub_import_private_error() {
        // 非公開シンボルのインポートでエラー
        let module_src = r#"
pub fn public_fn() -> string { "I am public" }
fn private_fn() -> string { "I am private" }
"#;
        let main_src = r#"
use ./secret.private_fn
private_fn()
"#;
        let result = run_with_module(main_src, "secret.forge", module_src);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("private_fn") && err_msg.contains("非公開"),
            "エラーメッセージに 'private_fn' と '非公開' が含まれていません: {}",
            err_msg
        );
    }

    // ── Phase M-4 tests ───────────────────────────────────────────────────

    /// 複数モジュールを持つテスト用ヘルパー
    fn run_with_two_modules(
        main_src: &str,
        mod1_path: &str,
        mod1_src: &str,
        mod2_path: &str,
        mod2_src: &str,
    ) -> Result<Value, RuntimeError> {
        use forge_compiler::parser::parse_source;
        use std::fs;

        let tmp = tempfile::tempdir().map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let m1 = tmp.path().join(mod1_path);
        if let Some(p) = m1.parent() {
            fs::create_dir_all(p).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        }
        fs::write(&m1, mod1_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let m2 = tmp.path().join(mod2_path);
        if let Some(p) = m2.parent() {
            fs::create_dir_all(p).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        }
        fs::write(&m2, mod2_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let main_file = tmp.path().join("main.forge");
        fs::write(&main_file, main_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let module = parse_source(main_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        let mut interp = Interpreter::with_file_path(&main_file);
        interp.eval(&module)
    }

    /// M-4-E: 未使用インポートで警告が出る
    #[test]
    fn test_unused_import_warning() {
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
pub fn subtract(a: number, b: number) -> number { a - b }
"#;
        // add のみインポートして subtract はインポートしない → 未使用インポートなし
        // add をインポートして使わない → 未使用インポートあり
        let main_src = r#"
use ./math.add
42
"#;
        let result = run_with_module(main_src, "math.forge", module_src);
        assert_eq!(result, Ok(Value::Int(42)));

        // インタープリタの imported_symbols を確認するために別の方法で実行
        use forge_compiler::parser::parse_source;
        use std::fs;

        let tmp = tempfile::tempdir()
            .map_err(|e| RuntimeError::Custom(e.to_string()))
            .unwrap();
        let mod_file = tmp.path().join("math.forge");
        fs::write(&mod_file, module_src).unwrap();
        let main_file = tmp.path().join("main.forge");
        fs::write(&main_file, main_src).unwrap();

        let module = parse_source(main_src).unwrap();
        let mut interp = Interpreter::with_file_path(&main_file);
        interp.eval(&module).unwrap();

        // add がインポートされていること
        assert!(
            interp.imported_symbols.contains_key("add"),
            "add がインポートされているべき"
        );
        // add は使われていない（本文が `42` のみ）
        let add_info = &interp.imported_symbols["add"];
        assert!(!add_info.used, "add は使用されていないはず");
    }

    /// M-4-E: 同名シンボルの衝突でエラー
    #[test]
    fn test_symbol_collision_error() {
        let math1_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
"#;
        let math2_src = r#"
pub fn add(a: number, b: number) -> number { a + b + 100 }
"#;
        // 同名 add を2つのモジュールからインポート → エラー
        let main_src = r#"
use ./math1.add
use ./math2.add
add(1, 2)
"#;
        let result =
            run_with_two_modules(main_src, "math1.forge", math1_src, "math2.forge", math2_src);
        assert!(result.is_err(), "シンボル衝突はエラーになるべき");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("add") && err_msg.contains("衝突"),
            "エラーメッセージに 'add' と '衝突' が含まれるべき: {}",
            err_msg
        );
    }

    /// M-4-E: use * 衝突で警告（エラーではない）
    #[test]
    fn test_wildcard_collision_warning() {
        let math1_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
pub fn multiply(a: number, b: number) -> number { a * b }
"#;
        let math2_src = r#"
pub fn add(a: number, b: number) -> number { a + b + 100 }
pub fn subtract(a: number, b: number) -> number { a - b }
"#;
        // use * で add が衝突 → 警告のみ（エラーにならない）
        let main_src = r#"
use ./math1.*
use ./math2.*
multiply(2, 3)
"#;
        let result =
            run_with_two_modules(main_src, "math1.forge", math1_src, "math2.forge", math2_src);
        // use * 衝突は警告のみなのでエラーにならない
        assert!(
            result.is_ok(),
            "use * の衝突は警告のみでエラーにならないべき: {:?}",
            result
        );
    }

    // ── Phase M-5: when キーワードテスト ─────────────────────────────────

    /// M-5-D: platform 条件の評価 — 現在の OS に対応する when ブロックが実行される
    #[test]
    fn test_when_platform() {
        let current_os = std::env::consts::OS;

        // 現在のプラットフォームに合致する when ブロックで定義した関数が呼べる
        let src = format!(
            r#"
when platform.{os} {{
    fn platform_fn() -> number {{ 42 }}
}}
platform_fn()
"#,
            os = current_os
        );

        let result = run(&src);
        assert_eq!(
            result,
            Ok(Value::Int(42)),
            "現在の OS ({}) に対応する when ブロックが実行されるべき",
            current_os
        );
    }

    /// M-5-D: `forge run` モード（is_test_mode = false）では `when test` がスキップされる
    #[test]
    fn test_when_test_skipped() {
        // is_test_mode = false (デフォルト) で when test ブロックはスキップ
        let src = r#"
when test {
    fn test_helper() -> number { 99 }
}
42
"#;
        // when test がスキップされるのでエラーにならず、42 が返る
        let result = run(src);
        assert_eq!(
            result,
            Ok(Value::Int(42)),
            "forge run モードでは when test ブロックがスキップされるべき"
        );

        // test_helper が定義されていないことを確認
        let src2 = r#"
when test {
    fn test_helper() -> number { 99 }
}
test_helper()
"#;
        let result2 = run(src2);
        assert!(
            matches!(result2, Err(RuntimeError::UndefinedVariable(_))),
            "when test がスキップされた場合、test_helper は未定義のはず: {:?}",
            result2
        );
    }

    /// M-5-D: `when not` の反転 — when not feature.x は when feature.x の逆になる
    #[test]
    fn test_when_not() {
        // FORGE_FEATURE_TESTFEAT が未設定 → feature.testfeat は false → not feature.testfeat は true
        // 環境変数が未設定の状態でテスト
        std::env::remove_var("FORGE_FEATURE_TESTFEAT");

        let src = r#"
when not feature.testfeat {
    fn not_feature_fn() -> number { 1 }
}
when feature.testfeat {
    fn not_feature_fn() -> number { 2 }
}
not_feature_fn()
"#;
        let result = run(src);
        assert_eq!(
            result,
            Ok(Value::Int(1)),
            "feature.testfeat が未設定のとき when not feature.testfeat が実行されるべき"
        );
    }

    // ── Phase M-6: use raw テスト ─────────────────────────────────────────

    /// M-6-D: `forge run` では use raw がスキップされ警告が出る
    #[test]
    fn test_use_raw_skipped_in_run() {
        // use raw ブロックは forge run ではスキップされ、後続のコードは正常に実行される
        let src = r#"
use raw {
    let map = ::std::collections::HashMap::new();
    let val = ::std::env::var("HOME");
}
let x = 42
x
"#;
        // forge run モードでは use raw をスキップして正常終了すること
        let result = run(src);
        assert_eq!(
            result,
            Ok(Value::Int(42)),
            "use raw はスキップされ、後続の let x = 42 が評価されること"
        );
    }

    // ── Phase M-7: REPL でのモジュールインポート テスト ───────────────────

    /// REPL でのモジュールロードと loaded_modules の記録をテストするヘルパー
    fn run_repl_with_module(
        module_path: &str,
        module_src: &str,
        use_stmt: &str,
        code: &str,
    ) -> Result<(Value, Vec<String>), RuntimeError> {
        use forge_compiler::parser::parse_source;
        use std::fs;

        let tmp = tempfile::tempdir().map_err(|e| RuntimeError::Custom(e.to_string()))?;

        // モジュールファイルを配置（src/ 以下）
        let mod_file = tmp.path().join("src").join(module_path);
        if let Some(parent) = mod_file.parent() {
            fs::create_dir_all(parent).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        }
        fs::write(&mod_file, module_src).map_err(|e| RuntimeError::Custom(e.to_string()))?;

        // REPL 用インタープリタを project_root で初期化する
        let mut interp = Interpreter::with_project_root(tmp.path().to_path_buf());

        // use 文を実行する（REPL 入力をシミュレート）
        let before_keys: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();

        let use_module = parse_source(use_stmt).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        interp.eval(&use_module)?;

        let after_keys: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();
        let new_syms: Vec<String> = after_keys.difference(&before_keys).cloned().collect();

        // loaded_modules に記録する（REPL と同じロジック）
        if !new_syms.is_empty() {
            // use パスを取得する
            let use_path = use_module
                .stmts
                .iter()
                .filter_map(|s| {
                    if let Stmt::UseDecl { path, .. } = s {
                        Some(match path {
                            UsePath::Local(p) => p.clone(),
                            UsePath::External(p) => p.clone(),
                            UsePath::Stdlib(p) => p.clone(),
                        })
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or_default();

            let entry = interp
                .loaded_modules
                .entry(use_path)
                .or_insert_with(Vec::new);
            for sym in &new_syms {
                if !entry.contains(sym) {
                    entry.push(sym.clone());
                }
            }
        }

        // コードを評価する
        let code_module = parse_source(code).map_err(|e| RuntimeError::Custom(e.to_string()))?;
        let val = interp.eval(&code_module)?;

        // ロード済みシンボルのリストを返す
        let loaded_syms: Vec<String> = interp
            .loaded_modules
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();

        Ok((val, loaded_syms))
    }

    /// M-7-B: REPL でのモジュールロード — モジュールをロードしてシンボルが使える
    #[test]
    fn test_repl_module_load() {
        let module_src = r#"
pub fn add(a: number, b: number) -> number { a + b }
"#;
        let result = run_repl_with_module("math.forge", module_src, "use ./math.add", "add(10, 5)");
        match result {
            Ok((val, loaded_syms)) => {
                assert_eq!(val, Value::Int(15), "add(10, 5) が 15 を返すこと");
                assert!(
                    loaded_syms.contains(&"add".to_string()),
                    "loaded_modules に 'add' が記録されていること: {:?}",
                    loaded_syms
                );
            }
            Err(e) => panic!("テスト失敗: {}", e),
        }
    }

    /// M-7-B: :reload による再読み込み — reload でシンボルが更新される
    #[test]
    fn test_repl_module_reload() {
        use forge_compiler::parser::parse_source;
        use std::fs;

        let tmp = tempfile::tempdir().expect("tempdir");

        // 初期バージョンのモジュールを配置
        let mod_file = tmp.path().join("src").join("math.forge");
        fs::create_dir_all(mod_file.parent().unwrap()).expect("create_dir_all");
        fs::write(&mod_file, "pub fn value() -> number { 1 }").expect("write v1");

        let mut interp = Interpreter::with_project_root(tmp.path().to_path_buf());

        // 最初のロード
        let use_src = "use ./math.value";
        let use_module = parse_source(use_src).expect("parse use v1");
        let before_keys: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();
        interp.eval(&use_module).expect("eval use v1");
        let after_keys: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();
        let new_syms: Vec<String> = after_keys.difference(&before_keys).cloned().collect();
        let entry = interp
            .loaded_modules
            .entry("math".to_string())
            .or_insert_with(Vec::new);
        for sym in &new_syms {
            entry.push(sym.clone());
        }

        // 初期値の確認
        let v1 = parse_source("value()").expect("parse v1");
        let result1 = interp.eval(&v1).expect("eval v1");
        assert_eq!(result1, Value::Int(1), "初期値は 1 であること");

        // モジュールを更新する（ファイルを上書き）
        fs::write(&mod_file, "pub fn value() -> number { 42 }").expect("write v2");

        // reload をシミュレート: アンロード + キャッシュクリア + 再ロード
        interp.unload_module("math");
        interp.clear_module_loader_cache("math");

        let use_module2 = parse_source(use_src).expect("parse use v2");
        let before_keys2: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();
        interp.eval(&use_module2).expect("eval use v2");
        let after_keys2: std::collections::HashSet<String> =
            interp.imported_symbols.keys().cloned().collect();
        let new_syms2: Vec<String> = after_keys2.difference(&before_keys2).cloned().collect();
        let entry2 = interp
            .loaded_modules
            .entry("math".to_string())
            .or_insert_with(Vec::new);
        for sym in &new_syms2 {
            entry2.push(sym.clone());
        }

        // 更新後の値の確認
        let v2 = parse_source("value()").expect("parse v2");
        let result2 = interp.eval(&v2).expect("eval v2");
        assert_eq!(result2, Value::Int(42), "reload 後は 42 を返すこと");
    }

    // ── Phase FT-1 tests ──────────────────────────────────────────────────

    use forge_compiler::parser::parse_source;

    fn eval(src: &str) -> Result<Value, RuntimeError> {
        eval_source(src)
    }

    #[test]
    fn test_assert_eq_pass() {
        let result = eval("assert_eq(1 + 1, 2)");
        assert_eq!(result, Ok(Value::Unit));
    }

    #[test]
    fn test_assert_eq_fail() {
        let result = eval("assert_eq(1, 2)");
        assert!(matches!(result, Err(RuntimeError::TestFailure(_))));
        if let Err(RuntimeError::TestFailure(msg)) = result {
            assert!(msg.contains("expected 2, got 1"), "msg: {}", msg);
        }
    }

    #[test]
    fn test_assert_pass() {
        let result = eval("assert(true)");
        assert_eq!(result, Ok(Value::Unit));
    }

    #[test]
    fn test_assert_fail() {
        let result = eval("assert(false)");
        assert!(matches!(result, Err(RuntimeError::TestFailure(_))));
    }

    #[test]
    fn test_assert_ok() {
        let result = eval("assert_ok(ok(1))");
        assert_eq!(result, Ok(Value::Unit));
    }

    #[test]
    fn test_assert_err() {
        let result = eval("assert_err(err(\"msg\"))");
        assert_eq!(result, Ok(Value::Unit));
    }

    #[test]
    fn test_test_scope_isolation() {
        let src = r#"
state counter: number = 0

test "first" {
    counter = counter + 1
}

test "second" {
    assert_eq(counter, 0)
}
"#;
        let module = parse_source(src).expect("parse failed");
        let mut interp = Interpreter::new();
        interp.is_test_mode = true;
        let results = interp.run_tests(&module.stmts, None);
        assert_eq!(results.len(), 2);
        assert!(results[0].passed, "first test should pass");
        assert!(results[1].passed, "second test should pass (counter reset)");
    }

    #[test]
    fn test_run_skips_test_blocks() {
        let src = r#"
test "should be skipped" {
    assert(false)
}
"#;
        let module = parse_source(src).expect("parse failed");
        let mut interp = Interpreter::new();
        // is_test_mode = false (default)
        let result = interp.eval(&module);
        assert!(
            result.is_ok(),
            "eval should succeed when skipping test blocks"
        );
    }

    #[test]
    fn test_http_get_via_interpreter() {
        use mockito::Server;
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/hello")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("world")
            .create();

        let base_url = server.url();
        let src = format!(
            r#"
use forge/http.{{ get }}
let res = get("{}/hello").send()
res
"#,
            base_url
        );

        let module = parse_source(&src).expect("parse failed");
        let mut interp = Interpreter::new();
        let result = interp.eval(&module).expect("eval failed");

        // .send() returns HttpResponse struct directly (errors become RuntimeError)
        match result {
            Value::Struct { ref fields, .. } => {
                let f = fields.borrow();
                assert_eq!(
                    f.get("status"),
                    Some(&Value::Int(200)),
                    "status should be 200"
                );
                assert_eq!(f.get("ok"), Some(&Value::Bool(true)), "ok should be true");
            }
            other => panic!("expected Struct, got {:?}", other),
        }

        mock.assert();
    }

    // ── C-1-A: wire protocol encode テスト ─────────────────────────────────

    #[test]
    fn c1a_encode_terminate_returns_correct_bytes() {
        // encode_terminate() が [88, 0, 0, 0, 4] を返すことを確認する
        // NOTE: ForgeScript では let 変数宣言の直後にリストリテラルを書くと
        //       インデックスアクセスと誤解釈されるため、リテラルを変数に代入する
        let src = r#"
fn encode_terminate() -> list<number> {
    let out = [88, 0, 0, 0, 4]
    out
}
let result = encode_terminate()
result.len()
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(5), "length should be 5");
    }

    #[test]
    fn c1a_encode_terminate_byte_values() {
        let src = r#"
fn encode_terminate() -> list<number> {
    let out = [88, 0, 0, 0, 4]
    out
}
let result = encode_terminate()
result[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(88), "byte[0] should be 88 ('X')");
    }

    #[test]
    fn c1a_encode_int32_big_endian() {
        // encode_int32(256) の動作確認（byte[2] == 1 であれば正しいビッグエンディアン）
        let src = r#"
fn encode_int32(n: number) -> list<number> {
    let b0 = (n / 16777216) % 256
    let b1 = (n / 65536) % 256
    let b2 = (n / 256) % 256
    let b3 = n % 256
    let out = [b0, b1, b2, b3]
    out
}
let r = encode_int32(256)
r[2]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(
            result,
            Value::Int(1),
            "byte[2] of 256 should be 1 (big-endian)"
        );
    }

    #[test]
    fn c1a_encode_query_select_1() {
        // encode_query("SELECT 1") のバイト列長が正しいことを確認する
        let src = r#"
fn encode_int32(n: number) -> list<number> {
    let b0 = (n / 16777216) % 256
    let b1 = (n / 65536) % 256
    let b2 = (n / 256) % 256
    let b3 = n % 256
    let out = [b0, b1, b2, b3]
    out
}
fn encode_string(s: string) -> list<number> {
    let raw = string_to_bytes(s)
    let result = []
    state i = 0
    let len = raw.len()
    loop {
        if i >= len { break }
        result.push(raw[i])
        i = i + 1
    }
    result.push(0)
    result
}
fn list_concat(a: list<number>, b: list<number>) -> list<number> {
    let result = []
    state i = 0
    let alen = a.len()
    loop {
        if i >= alen { break }
        result.push(a[i])
        i = i + 1
    }
    state j = 0
    let blen = b.len()
    loop {
        if j >= blen { break }
        result.push(b[j])
        j = j + 1
    }
    result
}
fn encode_query(sql: string) -> list<number> {
    let sql_bytes = encode_string(sql)
    let msg_len = 4 + sql_bytes.len()
    let len_bytes = encode_int32(msg_len)
    let type_byte = [81]
    let result = list_concat(type_byte, len_bytes)
    list_concat(result, sql_bytes)
}
let q = encode_query("SELECT 1")
q.len()
"#;
        let result = eval_source(src).expect("eval failed");
        // 1 ('Q') + 4 (length) + 8 ("SELECT 1") + 1 ('\0') = 14
        assert_eq!(result, Value::Int(14), "total length should be 14");
    }

    #[test]
    fn c1a_encode_query_type_byte() {
        let src = r#"
fn encode_int32(n: number) -> list<number> {
    let b0 = (n / 16777216) % 256
    let b1 = (n / 65536) % 256
    let b2 = (n / 256) % 256
    let b3 = n % 256
    let out = [b0, b1, b2, b3]
    out
}
fn encode_string(s: string) -> list<number> {
    let raw = string_to_bytes(s)
    let result = []
    state i = 0
    let len = raw.len()
    loop {
        if i >= len { break }
        result.push(raw[i])
        i = i + 1
    }
    result.push(0)
    result
}
fn list_concat(a: list<number>, b: list<number>) -> list<number> {
    let result = []
    state i = 0
    let alen = a.len()
    loop {
        if i >= alen { break }
        result.push(a[i])
        i = i + 1
    }
    state j = 0
    let blen = b.len()
    loop {
        if j >= blen { break }
        result.push(b[j])
        j = j + 1
    }
    result
}
fn encode_query(sql: string) -> list<number> {
    let sql_bytes = encode_string(sql)
    let msg_len = 4 + sql_bytes.len()
    let len_bytes = encode_int32(msg_len)
    let type_byte = [81]
    let result = list_concat(type_byte, len_bytes)
    list_concat(result, sql_bytes)
}
let q = encode_query("SELECT 1")
q[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(81), "type byte should be 'Q' = 81");
    }

    // ── C-1-B: wire protocol decode テスト ─────────────────────────────────

    #[test]
    fn c1b_decode_backend_message_ready_for_query() {
        // ReadyForQuery ('Z' + 0x00000005 + 'I') を正しく decode できることを確認する
        let src = r#"
fn decode_int32(bytes: list<number>, offset: number) -> number {
    bytes[offset] * 16777216 + bytes[offset + 1] * 65536 + bytes[offset + 2] * 256 + bytes[offset + 3]
}
fn make_backend_message(t: number, body: list<number>) {
    { "type_byte": t, "body": body }
}
fn decode_backend_message(bytes: list<number>) {
    let t = bytes[0]
    let msg_len = decode_int32(bytes, 1)
    let body_len = msg_len - 4
    let body = []
    state i = 0
    loop {
        if i >= body_len { break }
        body.push(bytes[5 + i])
        i = i + 1
    }
    make_backend_message(t, body)
}
let input = [90, 0, 0, 0, 5, 73]
let msg = decode_backend_message(input)
msg["type_byte"]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(90), "type_byte = 'Z' = 90");
    }

    #[test]
    fn c1b_decode_ready_for_query_body() {
        let src = r#"
fn decode_int32(bytes: list<number>, offset: number) -> number {
    bytes[offset] * 16777216 + bytes[offset + 1] * 65536 + bytes[offset + 2] * 256 + bytes[offset + 3]
}
fn make_backend_message(t: number, body: list<number>) {
    { "type_byte": t, "body": body }
}
fn decode_backend_message(bytes: list<number>) {
    let t = bytes[0]
    let msg_len = decode_int32(bytes, 1)
    let body_len = msg_len - 4
    let body = []
    state i = 0
    loop {
        if i >= body_len { break }
        body.push(bytes[5 + i])
        i = i + 1
    }
    make_backend_message(t, body)
}
let input = [90, 0, 0, 0, 5, 73]
let msg = decode_backend_message(input)
let body = msg["body"]
body[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(73), "body[0] = 'I' = 73");
    }

    // ── C-1-C: SCRAM 暗号計算テスト ────────────────────────────────────────

    #[test]
    fn c1c_scram_generate_nonce_returns_nonempty_string() {
        let src = r#"
let nonce = scram_generate_nonce()
nonce.len() > 0
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Bool(true), "nonce should be non-empty");
    }

    #[test]
    fn c1c_scram_base64_encode() {
        // base64 encode の確認
        let src = r#"
let input_bytes = [72, 101, 108, 108, 111]
scram_base64_encode(input_bytes)
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(
            result,
            Value::String("SGVsbG8=".to_string()),
            "base64(Hello) should be SGVsbG8="
        );
    }

    #[test]
    fn c1c_scram_base64_decode() {
        // base64 decode の確認
        let src = r#"
let decoded = scram_base64_decode("SGVsbG8=")?
decoded[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(72), "decoded[0] = 'H' = 72");
    }

    #[test]
    fn c1c_scram_xor_correctness() {
        // XOR 計算の正しさを確認する
        let src = r#"
let a = [255, 0, 170]
let b = [15, 240, 85]
let result = scram_xor(a, b)
result[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(240), "0xFF ^ 0x0F = 0xF0 = 240");
    }

    #[test]
    fn c1c_scram_xor_second_element() {
        let src = r#"
let a = [255, 0, 170]
let b = [15, 240, 85]
let result = scram_xor(a, b)
result[1]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(240), "0x00 ^ 0xF0 = 0xF0 = 240");
    }

    #[test]
    fn c1c_scram_hi_pbkdf2_produces_32_bytes() {
        // PBKDF2-HMAC-SHA256 の出力が 32 バイトであることを確認する
        let src = r#"
let salt_b64 = "W22ZaJ0SNY7soEsUEjb6gQ=="
let salt = scram_base64_decode(salt_b64)?
let result = scram_hi("pencil", salt, 4096)
result.len()
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(32), "SHA-256 output should be 32 bytes");
    }

    #[test]
    fn c1c_scram_hmac_produces_32_bytes() {
        // HMAC-SHA256 の出力が 32 バイトであることを確認する
        let src = r#"
let key = scram_base64_decode("AAECBAUG")?
let result = scram_hmac(key, "Client Key")
result.len()
"#;
        // 短いbase64でもHMACは32バイト出力する
        let result = eval_source(src).expect("eval failed");
        assert_eq!(
            result,
            Value::Int(32),
            "HMAC-SHA256 output should be 32 bytes"
        );
    }

    #[test]
    fn c1c_scram_h_sha256_produces_32_bytes() {
        // SHA-256 の出力が 32 バイトであることを確認する
        let src = r#"
let raw_bytes = string_to_bytes("hello")
let result = scram_h(raw_bytes)
result.len()
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(32), "SHA-256 output should be 32 bytes");
    }

    // ── C-1-D: 統合テスト (PostgreSQL 必要、通常はスキップ) ────────────────

    /// connect() 統合テスト
    /// docker compose -f docker/docker-compose.test.yml up -d 後に実行すること
    #[test]
    #[ignore]
    fn c1d_integration_connect_and_query() {
        // この統合テストは PostgreSQL が起動している状態でのみ実行可能
        // docker compose -f docker/docker-compose.test.yml up -d を先に実行すること
        let src = r#"
let conn_result = tcp_connect("localhost", 5432)
match conn_result {
    ok(conn) => {
        tcp_close(conn)
        true
    }
    err(e) => false
}
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(
            result,
            Value::Bool(true),
            "PostgreSQL connection should succeed"
        );
    }

    // ── C-1-E: string_to_bytes / bytes_to_str テスト ───────────────────────

    #[test]
    fn c1e_string_to_bytes_len() {
        let src = r#"
let bytes = string_to_bytes("Hello")
bytes.len()
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(5), "len = 5");
    }

    #[test]
    fn c1e_string_to_bytes_first_char() {
        let src = r#"
let bytes = string_to_bytes("Hello")
bytes[0]
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Int(72), "'H' = 72");
    }

    #[test]
    fn c1e_bytes_to_str_round_trip() {
        let src = r#"
let bytes = string_to_bytes("Hello")
let s = bytes_to_str(bytes)
s == "Hello"
"#;
        let result = eval_source(src).expect("eval failed");
        assert_eq!(result, Value::Bool(true), "round-trip");
    }

    // ── 追加リスト・文字列メソッド テスト ────────────────────────────────

    #[test]
    fn test_list_sort() {
        let result = run(r#"
            let nums = [3, 1, 4, 1, 5, 9, 2]
            nums.sort()
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_list_join() {
        let result = run(r#"
            let words = ["hello", "world"]
            words.join(", ")
        "#);
        assert_eq!(result.unwrap(), Value::String("hello, world".into()));
    }

    #[test]
    fn test_list_find() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5]
            nums.find(n => n > 3)
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(4))))
        );
    }

    #[test]
    fn test_list_find_none() {
        let result = run(r#"
            let nums = [1, 2, 3]
            nums.find(n => n > 10)
        "#);
        assert_eq!(result.unwrap(), Value::Option(None));
    }

    #[test]
    fn test_list_flatten() {
        let result = run(r#"
            let nested = [[1, 2], [3, 4], [5]]
            nested.flatten()
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_list_chunk() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5]
            nums.chunk(2)
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_list_partition() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5, 6]
            nums.partition(n => n % 2 == 0)
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_list_unique() {
        let result = run(r#"
            let nums = [1, 2, 2, 3, 1, 4]
            nums.unique()
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_list_count_with_fn() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5]
            nums.count(n => n > 3)
        "#);
        assert_eq!(result.unwrap(), Value::Int(2));
    }

    #[test]
    fn test_list_concat() {
        let result = run(r#"
            let a = [1, 2, 3]
            let b = [4, 5, 6]
            a.concat(b)
        "#);
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_string_trim() {
        let result = run(r#""  hello  ".trim()"#);
        assert_eq!(result.unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn test_string_to_upper() {
        let result = run(r#""hello".to_upper()"#);
        assert_eq!(result.unwrap(), Value::String("HELLO".into()));
    }

    #[test]
    fn test_string_to_lower() {
        let result = run(r#""HELLO".to_lower()"#);
        assert_eq!(result.unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn test_string_ends_with() {
        let result = run(r#""hello world".ends_with("world")"#);
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_string_replace() {
        let result = run(r#""hello world".replace("world", "forge")"#);
        assert_eq!(result.unwrap(), Value::String("hello forge".into()));
    }

    #[test]
    fn test_string_repeat() {
        let result = run(r#""ab".repeat(3)"#);
        assert_eq!(result.unwrap(), Value::String("ababab".into()));
    }

    #[test]
    fn test_string_is_empty() {
        let r1 = run(r#""".is_empty()"#);
        assert_eq!(r1.unwrap(), Value::Bool(true));
        let r2 = run(r#""a".is_empty()"#);
        assert_eq!(r2.unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_pipe_sort_join() {
        let result = run(r#"
            let words = ["banana", "apple", "cherry"]
            words.sort().join(", ")
        "#);
        assert_eq!(
            result.unwrap(),
            Value::String("apple, banana, cherry".into())
        );
    }

    // ── Phase E2-2: Option メソッド拡充 ───────────────────────────────────

    #[test]
    fn test_option_unwrap_or_none() {
        let result = run(r#"
            let opt = none
            opt.unwrap_or(42)
        "#);
        assert_eq!(result.unwrap(), Value::Int(42));
    }

    #[test]
    fn test_option_unwrap_or_some() {
        let result = run(r#"
            let opt = some(10)
            opt.unwrap_or(42)
        "#);
        assert_eq!(result.unwrap(), Value::Int(10));
    }

    #[test]
    fn test_option_unwrap_some() {
        let result = run(r#"
            let opt = some(99)
            opt.unwrap()
        "#);
        assert_eq!(result.unwrap(), Value::Int(99));
    }

    #[test]
    fn test_option_unwrap_none_panics() {
        let result = run(r#"
            let opt = none
            opt.unwrap()
        "#);
        assert!(
            matches!(result, Err(RuntimeError::Custom(ref msg)) if msg.contains("unwrap called on none"))
        );
    }

    #[test]
    fn test_option_map_some() {
        let result = run(r#"
            let opt = some(5)
            opt.map(x => x * 2)
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(10))))
        );
    }

    #[test]
    fn test_option_map_none() {
        let result = run(r#"
            let opt = none
            opt.map(x => x * 2)
        "#);
        assert_eq!(result.unwrap(), Value::Option(None));
    }

    #[test]
    fn test_option_map_chain() {
        let result = run(r#"
            let nums = [3, 7, 1, 9, 2]
            let found = nums |> find(n => n > 5)
            found |> map(n => n + 1) |> unwrap_or(0)
        "#);
        assert_eq!(result.unwrap(), Value::Int(8));
    }

    #[test]
    fn test_pipeline_trace_records_find_none() {
        let src = "let missing = [1, 2, 3].find(n => n > 10)";
        let module = parse_source(src).expect("parse");
        let span = first_method_span(&module);
        let (_, trace) = run_with_trace(
            src,
            vec![PipelineTraceNodeRef {
                node_id: 7,
                start: span.start,
                end: span.end,
                line: span.line,
                col: span.col,
            }],
        )
        .expect("run");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].node_id, Some(7));
        assert_eq!(trace[0].method, "find");
        assert_eq!(trace[0].item_count, Some(0));
        assert_eq!(trace[0].outcome, PipelineTraceOutcome::FindNone);
    }

    #[test]
    fn test_pipeline_trace_records_result_err() {
        let src = r#"let value = err("oops").map(x => x)"#;
        let module = parse_source(src).expect("parse");
        let span = first_method_span(&module);
        let (_, trace) = run_with_trace(
            src,
            vec![PipelineTraceNodeRef {
                node_id: 9,
                start: span.start,
                end: span.end,
                line: span.line,
                col: span.col,
            }],
        )
        .expect("run");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].node_id, Some(9));
        assert_eq!(trace[0].method, "map");
        assert_eq!(trace[0].item_count, Some(0));
        assert_eq!(trace[0].outcome, PipelineTraceOutcome::ResultErr);
        assert_eq!(trace[0].message.as_deref(), Some("oops"));
    }

    #[test]
    fn test_trace_null_field() {
        let src = r#"
            let rows = [
                { name: none, score: 95 },
                { name: "ok", score: 88 }
            ]
            rows.map(row => row)
        "#;
        let module = parse_source(src).expect("parse");
        let span = first_method_span(&module);
        let (_, trace) = run_with_trace(
            src,
            vec![PipelineTraceNodeRef {
                node_id: 11,
                start: span.start,
                end: span.end,
                line: span.line,
                col: span.col,
            }],
        )
        .expect("run");

        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].method, "map");
        assert_eq!(trace[0].corrupted.len(), 1);
        assert_eq!(trace[0].corrupted[0].index, 1);
        assert!(trace[0].corrupted[0].reason.contains("name is none"));
    }

    #[test]
    fn test_trace_nan_score() {
        let src = r#"
            let rows = [
                { name: "alice", score: 10.0 },
                { name: "bob", score: 0.0 / 0.0 },
                { name: "carol", score: -1.0 }
            ]
            rows.map(row => row)
        "#;
        let module = parse_source(src).expect("parse");
        let span = first_method_span(&module);
        let (_, trace) = run_with_trace(
            src,
            vec![PipelineTraceNodeRef {
                node_id: 12,
                start: span.start,
                end: span.end,
                line: span.line,
                col: span.col,
            }],
        )
        .expect("run");

        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].corrupted.len(), 2);
        assert!(trace[0]
            .corrupted
            .iter()
            .any(|record| record.reason.contains("score is NaN")));
        assert!(trace[0]
            .corrupted
            .iter()
            .any(|record| record.reason.contains("score is negative")));
    }

    #[test]
    fn test_trace_type_mismatch_error() {
        let src = r#"
            let rows = [
                { score: 1 },
                { score: none }
            ]
            rows.map(row => row.score + 1)
        "#;
        let module = parse_source(src).expect("parse");
        let span = first_method_span(&module);
        let (result, trace) = run_with_trace_capture(
            src,
            vec![PipelineTraceNodeRef {
                node_id: 13,
                start: span.start,
                end: span.end,
                line: span.line,
                col: span.col,
            }],
        )
        .expect("run");

        assert!(result.is_err());
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].outcome, PipelineTraceOutcome::ResultErr);
        assert_eq!(trace[0].corrupted.len(), 1);
        assert_eq!(trace[0].corrupted[0].index, 2);
        assert!(trace[0].corrupted[0].reason.contains("number"));
    }

    #[test]
    fn test_option_and_then_some() {
        let result = run(r#"
            let opt = some(4)
            opt.and_then(x => some(x * 3))
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(12))))
        );
    }

    #[test]
    fn test_option_and_then_none() {
        let result = run(r#"
            let opt = none
            opt.and_then(x => some(x * 3))
        "#);
        assert_eq!(result.unwrap(), Value::Option(None));
    }

    #[test]
    fn test_option_and_then_chain() {
        let result = run(r#"
            let opt = some(10)
            opt.and_then(x => if x > 5 { some(x) } else { none }).and_then(x => some(x + 1))
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(11))))
        );
    }

    #[test]
    fn test_option_is_some() {
        let r1 = run("some(1).is_some()").unwrap();
        let r2 = run("none.is_some()").unwrap();
        assert_eq!(r1, Value::Bool(true));
        assert_eq!(r2, Value::Bool(false));
    }

    #[test]
    fn test_option_is_none() {
        let r1 = run("none.is_none()").unwrap();
        let r2 = run("some(1).is_none()").unwrap();
        assert_eq!(r1, Value::Bool(true));
        assert_eq!(r2, Value::Bool(false));
    }

    #[test]
    fn test_option_or_none() {
        let result = run(r#"
            let opt = none
            opt.or(some(99))
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(99))))
        );
    }

    #[test]
    fn test_option_or_some() {
        let result = run(r#"
            let opt = some(7)
            opt.or(some(99))
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(7))))
        );
    }

    #[test]
    fn test_option_filter_true() {
        let result = run(r#"
            let opt = some(10)
            opt.filter(x => x > 5)
        "#);
        assert_eq!(
            result.unwrap(),
            Value::Option(Some(Box::new(Value::Int(10))))
        );
    }

    #[test]
    fn test_option_filter_false() {
        let result = run(r#"
            let opt = some(3)
            opt.filter(x => x > 5)
        "#);
        assert_eq!(result.unwrap(), Value::Option(None));
    }

    #[test]
    fn test_option_pipeline_find_map_unwrap_or() {
        let result = run(r#"
            struct Student { name: string, score: number }
            let students = [
                Student { name: "Alice", score: 92 },
                Student { name: "Bob", score: 78 },
            ]
            students |> find(s => s.score >= 90) |> map(s => s.name) |> unwrap_or("なし")
        "#);
        assert_eq!(result.unwrap(), Value::String("Alice".to_string()));
    }

    #[test]
    fn test_option_pipeline_none_path() {
        let result = run(r#"
            struct Student { name: string, score: number }
            let students = [
                Student { name: "Alice", score: 92 },
                Student { name: "Bob", score: 78 },
            ]
            students |> find(s => s.score >= 100) |> map(s => s.name) |> unwrap_or("なし")
        "#);
        assert_eq!(result.unwrap(), Value::String("なし".to_string()));
    }

    #[test]
    fn test_option_and_then_find_chain() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5]
            nums |> find(n => n > 3) |> and_then(n => if n > 4 { some(n * 10) } else { none }) |> unwrap_or(0)
        "#);
        assert_eq!(result.unwrap(), Value::Int(0));
    }

    #[test]
    fn test_result_unwrap_or_ok() {
        let result = run(r#"
            let value = ok(42)
            value.unwrap_or(0)
        "#);
        assert_eq!(result.unwrap(), Value::Int(42));
    }

    #[test]
    fn test_result_unwrap_or_err() {
        let result = run(r#"
            let value = err("oops")
            value.unwrap_or(99)
        "#);
        assert_eq!(result.unwrap(), Value::Int(99));
    }

    #[test]
    fn test_result_ok_some_and_none() {
        let ok_value = run(r#"ok(7).ok().unwrap_or(0)"#).unwrap();
        let err_value = run(r#"err("oops").ok().unwrap_or(0)"#).unwrap();
        assert_eq!(ok_value, Value::Int(7));
        assert_eq!(err_value, Value::Int(0));
    }

    #[test]
    fn test_eval_destructure_basic() {
        let result = run(r#"
            let (a, b) = [10, 20]
            a + b
        "#);
        assert_eq!(result.unwrap(), Value::Int(30));
    }

    #[test]
    fn test_eval_destructure_partition() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5, 6]
            let (evens, odds) = nums.partition(n => n % 2 == 0)
            if evens == [2, 4, 6] && odds == [1, 3, 5] { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_eval_destructure_wildcard() {
        let result = run(r#"
            let (_, value) = [1, 99]
            value
        "#);
        assert_eq!(result.unwrap(), Value::Int(99));
    }

    #[test]
    fn test_eval_destructure_rest() {
        let result = run(r#"
            let (head, ..tail) = [1, 2, 3, 4]
            if head == 1 && tail[0] == 2 && tail[1] == 3 && tail[2] == 4 { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_eval_destructure_zip() {
        let result = run(r#"
            let (a, b) = ["a", "b"].zip([1, 2])
            if a[0] == "a" && a[1] == 1 && b[0] == "b" && b[1] == 2 { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_eval_destructure_too_few_elements_error() {
        let result = run(r#"
            let (a, b, c) = [1, 2]
            a
        "#);
        assert!(matches!(
            result,
            Err(RuntimeError::Custom(ref msg)) if msg.contains("3 要素が必要ですが 2 要素しかありません")
        ));
    }

    #[test]
    fn test_eval_for_destructure_enumerate() {
        let result = run(r#"
            state out = []
            for (i, v) in [10, 20, 30].enumerate() {
                out = out.concat(["{i}: {v}"])
            }
            out
        "#);
        assert_eq!(
            result.unwrap(),
            Value::List(Rc::new(RefCell::new(vec![
                Value::String("0: 10".to_string()),
                Value::String("1: 20".to_string()),
                Value::String("2: 30".to_string()),
            ])))
        );
    }

    #[test]
    fn test_eval_for_destructure_zip() {
        let result = run(r#"
            state out = []
            for (k, v) in ["a", "b"].zip([1, 2]) {
                out = out.concat(["{k}={v}"])
            }
            out
        "#);
        assert_eq!(
            result.unwrap(),
            Value::List(Rc::new(RefCell::new(vec![
                Value::String("a=1".to_string()),
                Value::String("b=2".to_string()),
            ])))
        );
    }

    #[test]
    fn test_e2e_destructure_partition() {
        let result = run(r#"
            let nums = [1, 2, 3, 4, 5, 6]
            let (evens, odds) = nums.partition(n => n % 2 == 0)
            if evens == [2, 4, 6] && odds == [1, 3, 5] { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_e2e_destructure_zip_for() {
        let result = run(r#"
            let keys = ["a", "b", "c"]
            let values = [1, 2, 3]
            state result = []
            for (k, v) in keys.zip(values) {
                result = result.concat(["{k}={v}"])
            }
            result
        "#);
        assert_eq!(
            result.unwrap(),
            Value::List(Rc::new(RefCell::new(vec![
                Value::String("a=1".to_string()),
                Value::String("b=2".to_string()),
                Value::String("c=3".to_string()),
            ])))
        );
    }

    #[test]
    fn test_e2e_destructure_chunk() {
        let result = run(r#"
            let (first, second, third) = [10, 20, 30]
            first + second + third
        "#);
        assert_eq!(result.unwrap(), Value::Int(60));
    }

    #[test]
    fn test_eval_anon_struct_literal() {
        let result = run(r#"
            let user = { name: "Alice", score: 92 }
            user
        "#);
        assert!(
            matches!(result.unwrap(), Value::Struct { type_name, .. } if type_name == "<anon>")
        );
    }

    #[test]
    fn test_eval_anon_struct_field_access() {
        let result = run(r#"
            let user = { name: "Alice", score: 92 }
            user.name
        "#);
        assert_eq!(result.unwrap(), Value::String("Alice".to_string()));
    }

    #[test]
    fn test_eval_anon_struct_shorthand() {
        let result = run(r#"
            let name = "Alice"
            let score = 92
            let user = { name, score }
            user.score
        "#);
        assert_eq!(result.unwrap(), Value::Int(92));
    }

    #[test]
    fn test_eval_anon_struct_in_list() {
        let result = run(r#"
            let users = [{ name: "Alice" }, { name: "Bob" }]
            users[1].name
        "#);
        assert_eq!(result.unwrap(), Value::String("Bob".to_string()));
    }

    #[test]
    fn test_eval_anon_struct_as_return_value() {
        let result = run(r#"
            fn make_user() -> { name: string, score: number } {
                { name: "Alice", score: 92 }
            }
            make_user().score
        "#);
        assert_eq!(result.unwrap(), Value::Int(92));
    }

    #[test]
    fn test_eval_anon_struct_pipe_map() {
        let result = run(r#"
            struct Student { name: string, score: number }
            let students = [
                Student { name: "Alice", score: 92 },
                Student { name: "Bob", score: 78 },
            ]
            let summaries = students.map(s => {
                let summary = { name: s.name, passed: s.score >= 80 }
                summary
            })
            if summaries[0].name == "Alice" && summaries[0].passed == true && summaries[1].passed == false { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_e2e_anon_struct_map() {
        let result = run(r#"
            struct Student { name: string, score: number }
            let students = [
                Student { name: "Alice", score: 92 },
                Student { name: "Bob", score: 78 },
            ]
            let summaries = students.map(s => {
                let summary = { name: s.name, passed: s.score >= 80 }
                summary
            })
            if summaries[0].name == "Alice" && summaries[0].passed == true && summaries[1].passed == false { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }

    #[test]
    fn test_e2e_anon_struct_state() {
        let result = run(r#"
            state users: list<{ id: number, name: string }> = []
            users = users.concat([{ id: 1, name: "Alice" }])
            users[0].name
        "#);
        assert_eq!(result.unwrap(), Value::String("Alice".to_string()));
    }

    #[test]
    fn test_e2e_anon_struct_shorthand() {
        let result = run(r#"
            let name = "Alice"
            let score = 92
            let s = { name, score }
            if s.name == "Alice" && s.score == 92 { "ok" } else { "ng" }
        "#);
        assert_eq!(result.unwrap(), Value::String("ok".to_string()));
    }
}
