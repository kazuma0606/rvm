// forge-vm: ツリーウォーキングインタープリタ
// Phase 2-B 実装

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use crate::value::{CapturedEnv, EnumData, NativeFn, Value};
use forge_compiler::ast::*;
use forge_compiler::deps::DepsManager;
use forge_compiler::loader::{ModForgeExport, ModuleLoader};

/// struct 型のメソッド（Forge 定義 or ネイティブ関数）
#[derive(Clone)]
enum MethodImpl {
    /// Forge スクリプトで定義されたメソッド
    Forge(FnDef),
    /// Rust ネイティブ関数（引数の第1要素が self）
    Native(NativeFn),
}

impl std::fmt::Debug for MethodImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodImpl::Forge(def) => write!(f, "Forge({})", def.name),
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
    /// テストモードフラグ（M-5-C）: `forge test` コマンドで true になる
    pub is_test_mode: bool,
    /// REPL でロード済みのモジュール情報（M-7-A）
    /// モジュールパス → そのモジュールからインポートしたシンボル名リスト
    pub loaded_modules: HashMap<String, Vec<String>>,
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
            is_test_mode: false,
            loaded_modules: HashMap::new(),
        };
        interp.register_builtins();
        interp
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
            is_test_mode: false,
            loaded_modules: HashMap::new(),
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
            is_test_mode: false,
            loaded_modules: HashMap::new(),
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
            is_test_mode: false,
            loaded_modules: HashMap::new(),
        };
        interp.register_builtins();
        interp
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
            Stmt::Let { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.define(name, v, false);
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
                name, params, body, ..
            } => {
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let captured = self.capture_env();
                let closure = Value::Closure {
                    params: param_names,
                    body: body.clone(),
                    env: Rc::clone(&captured),
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
                ..
            } => self.eval_impl_block(target.clone(), methods.clone()),
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
                let has_dep = self
                    .module_loader
                    .as_ref()
                    .map(|l| l.dep_paths.contains_key(crate_name.as_str()))
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
                } else {
                    // forge run では外部クレートのインポートをスキップ（警告なし）
                    // クレート名を DepsManager に記録して forge build 連携に備える
                    self.deps_manager.add(crate_name);
                    Ok(Value::Unit)
                }
            }
            UsePath::Stdlib(_) => {
                // forge run では標準ライブラリはスキップ（警告なし）
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
                        self.eval_directory_use(&sub_path, &UseSymbols::All, depth + 1)?;
                    } else {
                        self.eval_file_use(&sub_path, &UseSymbols::All)?;
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
                Stmt::Let { name, is_pub, .. } if *is_pub => {
                    pub_names.insert(name.clone());
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
            Expr::For {
                var, iter, body, ..
            } => self.eval_for(var, iter, body),
            Expr::Match {
                scrutinee, arms, ..
            } => self.eval_match(scrutinee, arms),
            Expr::Block { stmts, tail, .. } => self.eval_block(stmts, tail.as_deref()),
            Expr::Call { callee, args, .. } => self.eval_call(callee, args),
            Expr::MethodCall {
                object,
                method,
                args,
                ..
            } => self.eval_method_call(object, method, args),
            Expr::Closure { params, body, .. } => self.eval_closure(params, body),
            Expr::Await { expr, .. } => self.eval_expr(expr),
            Expr::Question(inner, _) => self.eval_question(inner),
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
            Expr::StructInit { name, fields, .. } => self.eval_struct_init(name, fields),
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
        // 短絡評価
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

        let l = self.eval_expr(left)?;
        let r = self.eval_expr(right)?;

        match op {
            BinOp::Add => match (l, r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(b))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
                (l, r) => Err(type_err(
                    "number/string + number/string",
                    &format!("{} + {}", l.type_name(), r.type_name()),
                )),
            },
            BinOp::Sub => int_float_op(l, r, i64::wrapping_sub, std::ops::Sub::sub, "-"),
            BinOp::Mul => int_float_op(l, r, i64::wrapping_mul, std::ops::Mul::mul, "*"),
            BinOp::Div => {
                if matches!((&l, &r), (Value::Int(_), Value::Int(0))) {
                    return Err(RuntimeError::DivisionByZero);
                }
                int_float_op(l, r, i64::wrapping_div, std::ops::Div::div, "/")
            }
            BinOp::Rem => int_float_op(l, r, i64::wrapping_rem, std::ops::Rem::rem, "%"),
            BinOp::Eq => Ok(Value::Bool(l == r)),
            BinOp::Ne => Ok(Value::Bool(l != r)),
            BinOp::Lt => cmp_op(l, r, |a, b| a < b, |a, b| a < b),
            BinOp::Gt => cmp_op(l, r, |a, b| a > b, |a, b| a > b),
            BinOp::Le => cmp_op(l, r, |a, b| a <= b, |a, b| a <= b),
            BinOp::Ge => cmp_op(l, r, |a, b| a >= b, |a, b| a >= b),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, operand: &Expr) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(operand)?;
        match op {
            UnaryOp::Neg => match v {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(type_err("number", v.type_name())),
            },
            UnaryOp::Not => match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(type_err("bool", v.type_name())),
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

    fn eval_for(&mut self, var: &str, iter: &Expr, body: &Expr) -> Result<Value, RuntimeError> {
        let iter_val = self.eval_expr(iter)?;
        let items = match iter_val {
            Value::List(list) => list.borrow().clone(),
            v => return Err(type_err("list", v.type_name())),
        };

        let mut results = Vec::new();
        for item in items {
            self.push_scope();
            self.define(var, item, false);
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
        }
        let callee_val = self.eval_expr(callee)?;
        let arg_vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;

        match callee_val {
            Value::Closure { params, body, env } => {
                self.call_closure(&params, &body, &env, arg_vals)
            }
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

        match obj {
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
                other => Err(RuntimeError::Custom(format!(
                    "メソッド '{}' は option に対して未実装です",
                    other
                ))),
            },
            Value::Result(result) => match method {
                "is_ok" => Ok(Value::Bool(result.is_ok())),
                "is_err" => Ok(Value::Bool(result.is_err())),
                other => Err(RuntimeError::Custom(format!(
                    "メソッド '{}' は result に対して未実装です",
                    other
                ))),
            },
            Value::List(items) => self.eval_list_method(items, method, arg_vals),
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
        }
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
            Some(MethodImpl::Forge(fn_def)) => {
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = HashMap::new();
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
    ) -> Result<Value, RuntimeError> {
        match method {
            // ── 変換 ──────────────────────────────────────────────────────
            "map" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::with_capacity(list.len());
                for item in list {
                    out.push(self.call_value(f.clone(), vec![item])?);
                }
                Ok(mk_list(out))
            }
            "filter" => {
                let f = one_fn_arg(method, args)?;
                let list = items.borrow().clone();
                let mut out = Vec::new();
                for item in list {
                    match self.call_value(f.clone(), vec![item.clone()])? {
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
            "count" | "len" => Ok(Value::Int(items.borrow().len() as i64)),
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
            _ => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は string に対して未実装です",
                method
            ))),
        }
    }

    /// Value（Closure または NativeFunction）を引数付きで呼び出す
    fn call_value(&mut self, f: Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match f {
            Value::Closure { params, body, env } => self.call_closure(&params, &body, &env, args),
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
        })
    }

    fn eval_question(&mut self, inner: &Expr) -> Result<Value, RuntimeError> {
        match self.eval_expr(inner)? {
            Value::Result(Ok(v)) => Ok(*v),
            Value::Result(Err(e)) => Err(RuntimeError::PropagateErr(e)),
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
        match (obj, idx) {
            (Value::List(list), Value::Int(i)) => {
                let list = list.borrow();
                let len = list.len();
                if i < 0 || i as usize >= len {
                    Err(RuntimeError::IndexOutOfBounds { index: i, len })
                } else {
                    Ok(list[i as usize].clone())
                }
            }
            (Value::Map(entries), key) => entries
                .into_iter()
                .find(|(entry_key, _)| *entry_key == key)
                .map(|(_, value)| value)
                .ok_or_else(|| RuntimeError::Custom("map に指定したキーが存在しません".into())),
            (o, i) => Err(type_err(
                "list[number] / map[key]",
                &format!("{}[{}]", o.type_name(), i.type_name()),
            )),
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
            Some(MethodImpl::Forge(fn_def)) => {
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = HashMap::new();
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
    ) -> Result<Value, RuntimeError> {
        if !self.type_registry.structs.contains_key(&target) {
            self.type_registry.structs.insert(
                target.clone(),
                StructInfo {
                    fields: vec![],
                    derives: vec![],
                    methods: HashMap::new(),
                },
            );
        }
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in methods {
                info.methods
                    .insert(method.name.clone(), MethodImpl::Forge(method));
            }
        }
        Ok(Value::Unit)
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
                },
            );
        }

        // 明示的に実装されたメソッドを型に登録（優先度: 直接 impl）
        let explicit_method_names: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in &methods {
                info.methods
                    .insert(method.name.clone(), MethodImpl::Forge(method.clone()));
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
                        // デフォルト実装を登録（明示的 impl がない場合のみ）
                        struct_info
                            .methods
                            .entry(method_name)
                            .or_insert(MethodImpl::Forge(fn_def));
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
            // 既に他の mixin から同名メソッドが登録されているか確認
            // mixin に由来するメソッドを識別するためにマーキングが必要だが
            // ここではシンプルに: 既存メソッドがある場合にエラーを発生させる
            // ただし、同一の trait/impl で登録したものは除外する
            for method_name in mixin_map.keys() {
                if let Some(struct_info) = self.type_registry.structs.get(&target) {
                    if struct_info.methods.contains_key(method_name) {
                        // 既存メソッドが存在する場合: 明示的 impl か他の mixin か？
                        // mixin 衝突として扱う（実行時エラー）
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
                        .insert(method_name, MethodImpl::Forge(fn_def));
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

    fn eval_field_access(&mut self, object: &Expr, field: &str) -> Result<Value, RuntimeError> {
        let obj = self.eval_expr(object)?;
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
        object_expr: &Expr,
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
            Some(MethodImpl::Forge(fn_def)) => {
                // self を暗黙引数として束縛してメソッドを呼び出す
                let saved = std::mem::take(&mut self.scopes);
                let mut initial: HashMap<String, Binding> = HashMap::new();

                // グローバルスコープをベースにする
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

// ── テスト ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Value, RuntimeError> {
        eval_source(src)
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

    // ── Phase 2-C tests ───────────────────────────────────────────────────

    #[test]
    fn test_native_print() {
        // print は Value::Unit を返し、副作用として stdout に出力する
        assert_eq!(run("print(42)"), Ok(Value::Unit));
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
}
