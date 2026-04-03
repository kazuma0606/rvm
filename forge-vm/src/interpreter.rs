// forge-vm: ツリーウォーキングインタープリタ
// Phase 2-B 実装

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use forge_compiler::ast::*;
use crate::value::{CapturedEnv, EnumData, NativeFn, Value};

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
            MethodImpl::Native(_)  => write!(f, "Native"),
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
    derives: Vec<String>,
}

/// trait の定義情報
#[derive(Debug, Clone)]
struct TraitInfo {
    /// 抽象メソッド名（実装必須）
    abstract_methods: Vec<String>,
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
    TypeMismatch { expected: String, found: String },
    DivisionByZero,
    IndexOutOfBounds { index: i64, len: usize },
    /// let 変数への再代入
    Immutable(String),
    Custom(String),
    // ── 内部制御フロー ──
    /// return 文による早期脱出（関数呼び出しが補足）
    Return(Value),
    /// ? 演算子の Err 伝播（関数呼び出しが補足）
    PropagateErr(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::UndefinedVariable(n)  => write!(f, "未定義の変数 '{}'", n),
            RuntimeError::TypeMismatch { expected, found } =>
                write!(f, "型エラー: {} を期待しましたが {} でした", expected, found),
            RuntimeError::DivisionByZero        => write!(f, "ゼロ除算"),
            RuntimeError::IndexOutOfBounds { index, len } =>
                write!(f, "インデックス範囲外: {} (長さ: {})", index, len),
            RuntimeError::Immutable(n)          => write!(f, "変数 '{}' は不変です", n),
            RuntimeError::Custom(msg)           => write!(f, "{}", msg),
            RuntimeError::Return(_)             => write!(f, "<return>"),
            RuntimeError::PropagateErr(e)       => write!(f, "<propagate err: {}>", e),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ── スコープ ────────────────────────────────────────────────────────────────

/// バインディング: (値, 可変かどうか)
type Binding = (Value, bool);

// ── インタプリタ ─────────────────────────────────────────────────────────────

pub struct Interpreter {
    /// スコープスタック。scopes[0] = グローバル、scopes.last() = 現在のスコープ
    scopes: Vec<HashMap<String, Binding>>,
    /// 型レジストリ（struct 定義・メソッドを保持）
    type_registry: TypeRegistry,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            scopes: vec![HashMap::new()],
            type_registry: TypeRegistry::default(),
        };
        interp.register_builtins();
        interp
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
            for (k, (v, _)) in scope {
                map.insert(k.clone(), v.clone());
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

        self.define("some", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err(format!("some() takes 1 arg")); }
            Ok(Value::Option(Some(Box::new(args.remove(0)))))
        }), false);

        self.define("ok", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err(format!("ok() takes 1 arg")); }
            Ok(Value::Result(Ok(Box::new(args.remove(0)))))
        }), false);

        self.define("err", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err(format!("err() takes 1 arg")); }
            Ok(Value::Result(Err(args.remove(0).to_string())))
        }), false);

        self.define("print", native!(|args: Vec<Value>| {
            let s = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("{}", s);
            Ok(Value::Unit)
        }), false);

        // println は print と同じ（改行付き）
        self.define("println", native!(|args: Vec<Value>| {
            let s = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
            println!("{}", s);
            Ok(Value::Unit)
        }), false);

        self.define("string", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err("string() takes 1 arg".to_string()); }
            Ok(Value::String(args.remove(0).to_string()))
        }), false);

        self.define("number", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err("number() takes 1 arg".to_string()); }
            match args.remove(0) {
                Value::String(s) => match s.trim().parse::<i64>() {
                    Ok(n)  => Ok(Value::Result(Ok(Box::new(Value::Int(n))))),
                    Err(_) => Ok(Value::Result(Err(format!("\"{}\" を number に変換できません", s)))),
                },
                Value::Float(f) => Ok(Value::Result(Ok(Box::new(Value::Int(f as i64))))),
                Value::Int(n)   => Ok(Value::Result(Ok(Box::new(Value::Int(n))))),
                v => Ok(Value::Result(Err(format!("{} を number に変換できません", v.type_name())))),
            }
        }), false);

        self.define("float", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err("float() takes 1 arg".to_string()); }
            match args.remove(0) {
                Value::String(s) => match s.trim().parse::<f64>() {
                    Ok(f)  => Ok(Value::Result(Ok(Box::new(Value::Float(f))))),
                    Err(_) => Ok(Value::Result(Err(format!("\"{}\" を float に変換できません", s)))),
                },
                Value::Int(n)   => Ok(Value::Result(Ok(Box::new(Value::Float(n as f64))))),
                Value::Float(f) => Ok(Value::Result(Ok(Box::new(Value::Float(f))))),
                v => Ok(Value::Result(Err(format!("{} を float に変換できません", v.type_name())))),
            }
        }), false);

        self.define("len", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err("len() takes 1 arg".to_string()); }
            match args.remove(0) {
                Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::List(list) => Ok(Value::Int(list.borrow().len() as i64)),
                v => Err(format!("len() は string または list を期待しましたが {} でした", v.type_name())),
            }
        }), false);

        self.define("type_of", native!(|mut args: Vec<Value>| {
            if args.len() != 1 { return Err("type_of() takes 1 arg".to_string()); }
            Ok(Value::String(args.remove(0).type_name().to_string()))
        }), false);
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
            Stmt::Fn { name, params, body, .. } => {
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let captured = self.capture_env();
                let closure = Value::Closure {
                    params: param_names,
                    body: body.clone(),
                    env: Rc::clone(&captured),
                };
                // 再帰呼び出しのために自己参照を captured env に追加
                captured.borrow_mut().insert(name.clone(), closure.clone());
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
            Stmt::StructDef { name, fields, derives, .. } => {
                self.eval_struct_def(name.clone(), fields.clone(), derives.clone())
            }
            Stmt::ImplBlock { target, trait_name: _, methods, .. } => {
                self.eval_impl_block(target.clone(), methods.clone())
            }
            Stmt::EnumDef { name, variants, derives, .. } => {
                self.eval_enum_def(name.clone(), variants.clone(), derives.clone())
            }
            Stmt::TraitDef { name, methods, .. } => {
                self.eval_trait_def(name.clone(), methods.clone())
            }
            Stmt::MixinDef { name, methods, .. } => {
                self.eval_mixin_def(name.clone(), methods.clone())
            }
            Stmt::ImplTrait { trait_name, target, methods, .. } => {
                self.eval_impl_trait(trait_name.clone(), target.clone(), methods.clone())
            }
            Stmt::DataDef { name, fields, validate_rules, .. } => {
                self.eval_data_def(name.clone(), fields.clone(), validate_rules.clone())
            }
            Stmt::TypestateDef { name, states, state_methods, .. } => {
                self.eval_typestate_def(name.clone(), states.clone(), state_methods.clone())
            }
        }
    }

    // ── 式の評価 ──────────────────────────────────────────────────────────

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Literal(lit, _)  => Ok(eval_literal(lit)),
            Expr::Ident(name, _)   => self.eval_ident(name),
            Expr::BinOp { op, left, right, .. } => self.eval_binop(op, left, right),
            Expr::UnaryOp { op, operand, .. }   => self.eval_unary(op, operand),
            Expr::If { cond, then_block, else_block, .. } =>
                self.eval_if(cond, then_block, else_block.as_deref()),
            Expr::While { cond, body, .. } => self.eval_while(cond, body),
            Expr::For { var, iter, body, .. } => self.eval_for(var, iter, body),
            Expr::Match { scrutinee, arms, .. } => self.eval_match(scrutinee, arms),
            Expr::Block { stmts, tail, .. } => self.eval_block(stmts, tail.as_deref()),
            Expr::Call { callee, args, .. } => self.eval_call(callee, args),
            Expr::MethodCall { object, method, args, .. } =>
                self.eval_method_call(object, method, args),
            Expr::Closure { params, body, .. } => self.eval_closure(params, body),
            Expr::Question(inner, _) => self.eval_question(inner),
            Expr::Interpolation { parts, .. } => self.eval_interpolation(parts),
            Expr::Range { start, end, inclusive, .. } =>
                self.eval_range(start, end, *inclusive),
            Expr::List(items, _) => self.eval_list(items),
            Expr::Assign { name, value, .. } => {
                let v = self.eval_expr(value)?;
                self.assign(name, v)
            }
            Expr::Index { object, index, .. } => self.eval_index(object, index),
            Expr::Field { object, field, .. } => self.eval_field_access(object, field),
            Expr::StructInit { name, fields, .. } => self.eval_struct_init(name, fields),
            Expr::FieldAssign { object, field, value, .. } => {
                self.eval_field_assign(object, field, value)
            }
            Expr::EnumInit { enum_name, variant, data, .. } => {
                self.eval_enum_init(enum_name, variant, data)
            }
        }
    }

    // ── 各評価メソッド ────────────────────────────────────────────────────

    fn eval_ident(&self, name: &str) -> Result<Value, RuntimeError> {
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
                    Value::Bool(true)  => self.eval_expr(right),
                    _ => Err(type_err("bool", l.type_name())),
                };
            }
            BinOp::Or => {
                let l = self.eval_expr(left)?;
                return match l {
                    Value::Bool(true)  => Ok(Value::Bool(true)),
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
                (Value::Int(a),    Value::Int(b))    => Ok(Value::Int(a.wrapping_add(b))),
                (Value::Float(a),  Value::Float(b))  => Ok(Value::Float(a + b)),
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
            BinOp::Eq  => Ok(Value::Bool(l == r)),
            BinOp::Ne  => Ok(Value::Bool(l != r)),
            BinOp::Lt  => cmp_op(l, r, |a, b| a < b, |a, b| a < b),
            BinOp::Gt  => cmp_op(l, r, |a, b| a > b, |a, b| a > b),
            BinOp::Le  => cmp_op(l, r, |a, b| a <= b, |a, b| a <= b),
            BinOp::Ge  => cmp_op(l, r, |a, b| a >= b, |a, b| a >= b),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, operand: &Expr) -> Result<Value, RuntimeError> {
        let v = self.eval_expr(operand)?;
        match op {
            UnaryOp::Neg => match v {
                Value::Int(n)   => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(type_err("number", v.type_name())),
            },
            UnaryOp::Not => match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(type_err("bool", v.type_name())),
            },
        }
    }

    fn eval_if(&mut self, cond: &Expr, then_block: &Expr, else_block: Option<&Expr>) -> Result<Value, RuntimeError> {
        match self.eval_expr(cond)? {
            Value::Bool(true)  => self.eval_expr(then_block),
            Value::Bool(false) => match else_block {
                Some(e) => self.eval_expr(e),
                None    => Ok(Value::Unit),
            },
            v => Err(type_err("bool", v.type_name())),
        }
    }

    fn eval_while(&mut self, cond: &Expr, body: &Expr) -> Result<Value, RuntimeError> {
        loop {
            match self.eval_expr(cond)? {
                Value::Bool(false) => break,
                Value::Bool(true)  => match self.eval_expr(body) {
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
                None    => Ok(Value::Unit),
            }
        })();
        self.pop_scope();
        result
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value, RuntimeError> {
        let callee_val = self.eval_expr(callee)?;
        let arg_vals: Vec<Value> = args.iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;

        match callee_val {
            Value::Closure { params, body, env } => {
                self.call_closure(&params, &body, &env, arg_vals)
            }
            Value::NativeFunction(NativeFn(f)) => {
                f(arg_vals).map_err(RuntimeError::Custom)
            }
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

        let mut initial: HashMap<String, Binding> = captured.borrow()
            .iter()
            .map(|(k, v)| (k.clone(), (v.clone(), false)))
            .collect();

        for (param, arg) in params.iter().zip(args) {
            initial.insert(param.clone(), (arg, false));
        }
        self.scopes = vec![initial];

        let result = self.eval_expr(body);
        self.scopes = saved;

        match result {
            Ok(v)                              => Ok(v),
            Err(RuntimeError::Return(v))       => Ok(v),
            Err(RuntimeError::PropagateErr(e)) => Ok(Value::Result(Err(e))),
            Err(e)                             => Err(e),
        }
    }

    fn eval_method_call(&mut self, object: &Expr, method: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        // TypeName::method() のような静的メソッド呼び出しを先に処理
        if let Expr::Ident(type_name, _) = object {
            if is_type_name_str(type_name) && self.type_registry.structs.contains_key(type_name.as_str()) {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args.iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                // static メソッド呼び出し: self として Unit を渡す
                return self.eval_struct_static_method(&type_name_cloned, method, arg_vals);
            }
            // enum の静的メソッド呼び出し（Unit バリアントアクセス等）
            if is_type_name_str(type_name) && self.type_registry.enums.contains_key(type_name.as_str()) {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args.iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_enum_static_method(&type_name_cloned, method, arg_vals);
            }
            // typestate の静的メソッド呼び出し（new<State>() = new("StateName") として渡される）
            if is_type_name_str(type_name) && self.type_registry.typestates.contains_key(type_name.as_str()) {
                let type_name_cloned = type_name.clone();
                let arg_vals: Vec<Value> = args.iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;
                return self.eval_typestate_static_method(&type_name_cloned, method, arg_vals);
            }
        }

        let obj = self.eval_expr(object)?;
        let arg_vals: Vec<Value> = args.iter()
            .map(|a| self.eval_expr(a))
            .collect::<Result<_, _>>()?;

        match obj {
            Value::List(items) => self.eval_list_method(items, method, arg_vals),
            Value::Struct { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_struct_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            Value::Enum { ref type_name, .. } => {
                let type_name_cloned = type_name.clone();
                self.eval_enum_method(obj.clone(), &type_name_cloned, method, arg_vals)
            }
            Value::Typestate { ref type_name, ref current_state, .. } => {
                let type_name_cloned = type_name.clone();
                let current_state_cloned = current_state.clone();
                self.eval_typestate_method(obj.clone(), &type_name_cloned, &current_state_cloned, method, arg_vals)
            }
            Value::Closure { .. } | Value::NativeFunction(_) => {
                self.call_value(obj, arg_vals)
            }
            other => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は {} に対して未実装です",
                method, other.type_name()
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
        // instance() は特別処理
        if method == "instance" {
            let is_singleton = self.type_registry.structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Singleton"))
                .unwrap_or(false);

            if is_singleton {
                if let Some(cached) = self.type_registry.singletons.get(type_name).cloned() {
                    return Ok(cached);
                }
                let fields: Vec<(String, TypeAnn)> = self.type_registry.structs
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
                self.type_registry.singletons.insert(type_name.to_string(), instance.clone());
                return Ok(instance);
            }
        }

        // default() / new() は @derive(Default) で有効化
        if method == "default" || method == "new" {
            let has_default = self.type_registry.structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Default"))
                .unwrap_or(false);

            if has_default {
                let fields: Vec<(String, TypeAnn)> = self.type_registry.structs
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

        Err(RuntimeError::Custom(format!(
            "型 '{}' に静的メソッド '{}' は存在しません",
            type_name, method
        )))
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
                        Value::Bool(true)  => out.push(item),
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
                        Value::Option(None)    => {}
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
                        Value::Bool(true)  => out.push(item),
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
                            Value::Bool(true)  => {}
                            Value::Bool(false) => { skipping = false; out.push(item); }
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
                let out = list.iter().enumerate()
                    .map(|(i, v)| mk_list(vec![Value::Int(i as i64), v.clone()]))
                    .collect();
                Ok(mk_list(out))
            }
            "zip" => {
                let other = one_list_arg(method, args)?;
                let a = items.borrow();
                let b = other.borrow();
                let out = a.iter().zip(b.iter())
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
                        Value::Int(n)   => { int_sum += n; float_sum += *n as f64; }
                        Value::Float(n) => { float_sum += n; has_float = true; }
                        v => return Err(type_err("number", v.type_name())),
                    }
                }
                Ok(if has_float { Value::Float(float_sum) } else { Value::Int(int_sum) })
            }
            "count" => {
                Ok(Value::Int(items.borrow().len() as i64))
            }
            "fold" => {
                if args.len() < 2 {
                    return Err(RuntimeError::Custom("fold() は引数が2つ必要です".into()));
                }
                let mut it = args.into_iter();
                let seed = it.next().ok_or_else(|| RuntimeError::Custom("fold: seed missing".into()))?;
                let f    = it.next().ok_or_else(|| RuntimeError::Custom("fold: fn missing".into()))?;
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
                        Value::Bool(true)  => return Ok(Value::Bool(true)),
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
                        Value::Bool(true)  => {}
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
                        Value::Bool(true)  => return Ok(Value::Bool(false)),
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
                Ok(Value::Option(list.get(n as usize).map(|v| Box::new(v.clone()))))
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
                let mut min_key  = self.call_value(f.clone(), vec![min_item.clone()])?;
                for item in list.into_iter().skip(1) {
                    let key = self.call_value(f.clone(), vec![item.clone()])?;
                    if compare_values(&key, &min_key)? == std::cmp::Ordering::Less {
                        min_key  = key;
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
                let mut max_key  = self.call_value(f.clone(), vec![max_item.clone()])?;
                for item in list.into_iter().skip(1) {
                    let key = self.call_value(f.clone(), vec![item.clone()])?;
                    if compare_values(&key, &max_key)? == std::cmp::Ordering::Greater {
                        max_key  = key;
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
            "collect" => {
                Ok(mk_list(items.borrow().clone()))
            }
            other => Err(RuntimeError::Custom(format!(
                "メソッド '{}' は list に対して未実装です",
                other
            ))),
        }
    }

    /// Value（Closure または NativeFunction）を引数付きで呼び出す
    fn call_value(&mut self, f: Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match f {
            Value::Closure { params, body, env } => {
                self.call_closure(&params, &body, &env, args)
            }
            Value::NativeFunction(NativeFn(func)) => {
                func(args).map_err(RuntimeError::Custom)
            }
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
            Value::Result(Ok(v))  => Ok(*v),
            Value::Result(Err(e)) => Err(RuntimeError::PropagateErr(e)),
            v => Err(type_err("result", v.type_name())),
        }
    }

    fn eval_interpolation(&mut self, parts: &[InterpPart]) -> Result<Value, RuntimeError> {
        let mut buf = String::new();
        for part in parts {
            match part {
                InterpPart::Literal(s) => buf.push_str(s),
                InterpPart::Expr(e)    => buf.push_str(&self.eval_expr(e)?.to_string()),
            }
        }
        Ok(Value::String(buf))
    }

    fn eval_range(&mut self, start: &Expr, end: &Expr, inclusive: bool) -> Result<Value, RuntimeError> {
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
            (s, e) => Err(type_err("number..number", &format!("{}..{}", s.type_name(), e.type_name()))),
        }
    }

    fn eval_list(&mut self, items: &[Expr]) -> Result<Value, RuntimeError> {
        let vals: Vec<Value> = items.iter().map(|e| self.eval_expr(e)).collect::<Result<_, _>>()?;
        Ok(Value::List(Rc::new(RefCell::new(vals))))
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
            (o, i) => Err(type_err("list[number]", &format!("{}[{}]", o.type_name(), i.type_name()))),
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
        states: Vec<String>,
        state_methods: Vec<forge_compiler::ast::TypestateState>,
    ) -> Result<Value, RuntimeError> {
        let mut state_infos: HashMap<String, TypestateStateInfo> = HashMap::new();

        for state in &state_methods {
            let mut methods: HashMap<String, TypestateMethodInfo> = HashMap::new();
            for method in &state.methods {
                let (next_state, is_result) = extract_transition_info(&method.return_type);
                methods.insert(method.name.clone(), TypestateMethodInfo {
                    params: method.params.clone(),
                    next_state,
                    is_result,
                });
            }
            state_infos.insert(state.name.clone(), TypestateStateInfo { methods });
        }

        self.type_registry.typestates.insert(name, TypestateInfo { states, state_infos });
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
                Some(v) => return Err(RuntimeError::Custom(format!(
                    "{}::new<State>() の State は文字列を期待しましたが {} でした",
                    type_name, v.type_name()
                ))),
                None => return Err(RuntimeError::Custom(format!(
                    "{}::new<State>() には状態名が必要です", type_name
                ))),
            };

            // 状態が typestate に定義されているか確認
            let valid = self.type_registry.typestates
                .get(type_name)
                .map(|info| info.states.contains(&initial_state))
                .unwrap_or(false);

            if !valid {
                return Err(RuntimeError::Custom(format!(
                    "状態 '{}' は typestate '{}' に定義されていません",
                    initial_state, type_name
                )));
            }

            return Ok(Value::Typestate {
                type_name: type_name.to_string(),
                current_state: initial_state,
                fields: Rc::new(RefCell::new(HashMap::new())),
            });
        }

        Err(RuntimeError::Custom(format!(
            "typestate '{}' に静的メソッド '{}' は存在しません", type_name, method
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
        let method_info = self.type_registry.typestates
            .get(type_name)
            .and_then(|info| info.state_infos.get(current_state))
            .and_then(|state_info| state_info.methods.get(method))
            .cloned();

        match method_info {
            None => {
                // 他の状態に存在するか確認してエラーメッセージを充実させる
                let available_in_states: Vec<String> = self.type_registry.typestates
                    .get(type_name)
                    .map(|info| {
                        info.state_infos.iter()
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
                        current_state, method, available_in_states.join(", ")
                    )));
                }
            }
            Some(info) => {
                // 引数の数チェック
                if args.len() != info.params.len() {
                    return Err(RuntimeError::Custom(format!(
                        "メソッド '{}' は {} 個の引数を期待しましたが {} 個渡されました",
                        method, info.params.len(), args.len()
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
                Some(v) => return Err(format!("validate() は struct でのみ使用可能です (got {})", v.type_name())),
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

            Ok(Value::Result(Ok(Box::new(self_val))))
        }));

        if let Some(info) = self.type_registry.structs.get_mut(type_name) {
            info.methods.insert("validate".to_string(), MethodImpl::Native(native));
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
                    if let Some(Value::Struct { type_name: ref actual_tn, ref fields }) = args.first() {
                        let fields = fields.borrow();
                        let mut sorted: Vec<(&String, &Value)> = fields.iter().collect();
                        sorted.sort_by_key(|(k, _)| k.as_str());
                        let field_str = sorted.iter()
                            .map(|(k, v)| format!("{}: {}", k, v))
                            .collect::<Vec<_>>()
                            .join(", ");
                        Ok(Value::String(format!("{} {{ {} }}", actual_tn, field_str)))
                    } else {
                        Err("display() は struct でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods.insert("display".to_string(), MethodImpl::Native(native));
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
                    info.methods.insert("clone".to_string(), MethodImpl::Native(native));
                }
            }
            "Accessor" => {
                let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                for field_name in field_names {
                    // getter
                    let fn_clone = field_name.clone();
                    let getter_native = NativeFn(Rc::new(move |args: Vec<Value>| {
                        if let Some(Value::Struct { ref fields, .. }) = args.first() {
                            fields.borrow().get(&fn_clone)
                                .cloned()
                                .ok_or_else(|| format!("フィールド '{}' が存在しません", fn_clone))
                        } else {
                            Err("getter は struct でのみ使用可能です".to_string())
                        }
                    }));
                    let getter_name = format!("get_{}", field_name);
                    if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                        info.methods.insert(getter_name, MethodImpl::Native(getter_native));
                    }

                    // setter
                    let fn_clone2 = field_name.clone();
                    let setter_native = NativeFn(Rc::new(move |args: Vec<Value>| {
                        if args.len() < 2 {
                            return Err(format!("set_{}() は2引数必要です", fn_clone2));
                        }
                        if let Value::Struct { ref fields, .. } = args[0] {
                            fields.borrow_mut().insert(fn_clone2.clone(), args[1].clone());
                            Ok(Value::Unit)
                        } else {
                            Err("setter は struct でのみ使用可能です".to_string())
                        }
                    }));
                    let setter_name = format!("set_{}", field_name);
                    if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                        info.methods.insert(setter_name, MethodImpl::Native(setter_native));
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
                    info.methods.insert("eq".to_string(), MethodImpl::Native(native));
                }
            }
            "Hash" => {
                // hash() メソッドを生成: struct のハッシュ値を number として返す
                use std::hash::{Hash, Hasher};
                use std::collections::hash_map::DefaultHasher;
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
                    info.methods.insert("hash".to_string(), MethodImpl::Native(native));
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
                        std::cmp::Ordering::Less    => -1_i64,
                        std::cmp::Ordering::Equal   =>  0_i64,
                        std::cmp::Ordering::Greater =>  1_i64,
                    };
                    Ok(Value::Int(result))
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods.insert("compare".to_string(), MethodImpl::Native(native));
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
        let info = EnumInfo {
            variants,
            derives: derives.clone(),
        };
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
                    self.type_registry.structs.insert(type_name.to_string(), StructInfo {
                        fields: vec![],
                        derives: vec![],
                        methods: HashMap::new(),
                    });
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Enum { .. }) = args.first() {
                        Ok(Value::String(v.to_string()))
                    } else {
                        Err("display() は enum でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods.insert("display".to_string(), MethodImpl::Native(native));
                }
            }
            "Clone" => {
                if !self.type_registry.structs.contains_key(type_name) {
                    self.type_registry.structs.insert(type_name.to_string(), StructInfo {
                        fields: vec![],
                        derives: vec![],
                        methods: HashMap::new(),
                    });
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if let Some(v @ Value::Enum { .. }) = args.first() {
                        Ok(v.deep_clone())
                    } else {
                        Err("clone() は enum でのみ使用可能です".to_string())
                    }
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods.insert("clone".to_string(), MethodImpl::Native(native));
                }
            }
            "Eq" => {
                if !self.type_registry.structs.contains_key(type_name) {
                    self.type_registry.structs.insert(type_name.to_string(), StructInfo {
                        fields: vec![],
                        derives: vec![],
                        methods: HashMap::new(),
                    });
                }
                let native = NativeFn(Rc::new(|args: Vec<Value>| {
                    if args.len() < 2 {
                        return Err("eq() は2引数必要です".to_string());
                    }
                    Ok(Value::Bool(args[0] == args[1]))
                }));
                if let Some(info) = self.type_registry.structs.get_mut(type_name) {
                    info.methods.insert("eq".to_string(), MethodImpl::Native(native));
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
                let vals: Vec<Value> = exprs.iter()
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
        let method_impl = self.type_registry.structs
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
        let variant_exists = self.type_registry.enums
            .get(type_name)
            .map(|info| info.variants.iter().any(|v| match v {
                EnumVariant::Unit(n) => n == method,
                EnumVariant::Tuple(n, _) => n == method,
                EnumVariant::Struct(n, _) => n == method,
            }))
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
            self.type_registry.structs.insert(target.clone(), StructInfo {
                fields: vec![],
                derives: vec![],
                methods: HashMap::new(),
            });
        }
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in methods {
                info.methods.insert(method.name.clone(), MethodImpl::Forge(method));
            }
        }
        Ok(Value::Unit)
    }

    // ── T-3-C: trait / mixin / impl trait サポート ────────────────────────

    fn eval_trait_def(&mut self, name: String, methods: Vec<TraitMethod>) -> Result<Value, RuntimeError> {
        let mut abstract_methods = Vec::new();
        let mut default_methods = HashMap::new();

        for method in methods {
            match method {
                TraitMethod::Abstract { name: method_name, .. } => {
                    abstract_methods.push(method_name);
                }
                TraitMethod::Default {
                    name: method_name,
                    params,
                    return_type,
                    body,
                    has_state_self,
                    span,
                } => {
                    let fn_def = FnDef {
                        name: method_name.clone(),
                        params,
                        return_type,
                        body,
                        has_state_self,
                        span,
                    };
                    default_methods.insert(method_name, fn_def);
                }
            }
        }

        let info = TraitInfo { abstract_methods, default_methods };
        self.type_registry.traits.insert(name, info);
        Ok(Value::Unit)
    }

    fn eval_mixin_def(&mut self, name: String, methods: Vec<FnDef>) -> Result<Value, RuntimeError> {
        let mut method_map = HashMap::new();
        for method in methods {
            method_map.insert(method.name.clone(), method);
        }
        let info = MixinInfo { methods: method_map };
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
            self.type_registry.structs.insert(target.clone(), StructInfo {
                fields: vec![],
                derives: vec![],
                methods: HashMap::new(),
            });
        }

        // 明示的に実装されたメソッドを型に登録（優先度: 直接 impl）
        let explicit_method_names: Vec<String> = methods.iter().map(|m| m.name.clone()).collect();
        if let Some(info) = self.type_registry.structs.get_mut(&target) {
            for method in &methods {
                info.methods.insert(method.name.clone(), MethodImpl::Forge(method.clone()));
            }
        }

        // trait のデフォルト実装を（明示的 impl がない場合のみ）型に登録
        let trait_defaults: Option<HashMap<String, FnDef>> = self.type_registry.traits
            .get(&trait_name)
            .map(|ti| ti.default_methods.clone());

        if let Some(defaults) = trait_defaults {
            if let Some(struct_info) = self.type_registry.structs.get_mut(&target) {
                for (method_name, fn_def) in defaults {
                    if !explicit_method_names.contains(&method_name) {
                        // デフォルト実装を登録（明示的 impl がない場合のみ）
                        struct_info.methods
                            .entry(method_name)
                            .or_insert(MethodImpl::Forge(fn_def));
                    }
                }
            }
        }

        // mixin の場合: デフォルトメソッドを登録（名前衝突チェックあり）
        let mixin_methods: Option<HashMap<String, FnDef>> = self.type_registry.mixins
            .get(&trait_name)
            .map(|mi| mi.methods.clone());

        if let Some(mixin_map) = mixin_methods {
            // 既に他の mixin から同名メソッドが登録されているか確認
            // mixin に由来するメソッドを識別するためにマーキングが必要だが
            // ここではシンプルに: 既存メソッドがある場合にエラーを発生させる
            // ただし、同一の trait/impl で登録したものは除外する
            for (method_name, fn_def) in &mixin_map {
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
                    struct_info.methods.insert(method_name, MethodImpl::Forge(fn_def));
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
            Value::Struct { ref fields, .. } => {
                fields.borrow().get(field)
                    .cloned()
                    .ok_or_else(|| RuntimeError::Custom(
                        format!("フィールド '{}' が存在しません", field)
                    ))
            }
            // Option(Some(struct)) → 中身の struct に対してフィールドアクセスを透過させる
            Value::Option(Some(ref inner)) => {
                match inner.as_ref() {
                    Value::Struct { ref fields, .. } => {
                        fields.borrow().get(field)
                            .cloned()
                            .ok_or_else(|| RuntimeError::Custom(
                                format!("フィールド '{}' が存在しません", field)
                            ))
                    }
                    _ => Err(RuntimeError::Custom(format!(
                        "フィールドアクセスは struct でのみ使用可能です (got option<{}>)", inner.type_name()
                    ))),
                }
            }
            Value::Option(None) => Err(RuntimeError::Custom(
                format!("none に対してフィールド '{}' にアクセスできません", field)
            )),
            _ => Err(RuntimeError::Custom(format!(
                "フィールドアクセスは struct でのみ使用可能です (got {})", obj.type_name()
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
            _ => Err(RuntimeError::Custom(format!(
                "フィールド代入は struct でのみ使用可能です (got {})", obj.type_name()
            ))),
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
            let is_singleton = self.type_registry.structs
                .get(type_name)
                .map(|info| info.derives.iter().any(|d| d == "Singleton"))
                .unwrap_or(false);

            if is_singleton {
                if let Some(cached) = self.type_registry.singletons.get(type_name).cloned() {
                    return Ok(cached);
                }
                // 初回: ゼロ値で struct を作る
                let fields: Vec<(String, TypeAnn)> = self.type_registry.structs
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
                self.type_registry.singletons.insert(type_name.to_string(), instance.clone());
                return Ok(instance);
            }
        }

        // 型レジストリからメソッドを検索
        let method_impl = self.type_registry.structs
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
                initial.insert("self".to_string(), (self_val.clone(), fn_def.has_state_self));

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
        (Value::Int(x),    Value::Int(y))    => Ok(x.cmp(y)),
        (Value::Float(x),  Value::Float(y))  => Ok(x.partial_cmp(y).unwrap_or(Equal)),
        (Value::Int(x),    Value::Float(y))  => Ok((*x as f64).partial_cmp(y).unwrap_or(Equal)),
        (Value::Float(x),  Value::Int(y))    => Ok(x.partial_cmp(&(*y as f64)).unwrap_or(Equal)),
        (Value::String(x), Value::String(y)) => Ok(x.cmp(y)),
        (Value::Struct { fields: fa, .. }, Value::Struct { fields: fb, .. }) => {
            // フィールドをキー順でソートして辞書順比較
            let borrow_a = fa.borrow();
            let borrow_b = fb.borrow();
            let mut keys_a: Vec<&String> = borrow_a.keys().collect();
            keys_a.sort();
            for key in keys_a {
                let va = borrow_a.get(key).ok_or_else(|| RuntimeError::Custom(
                    format!("フィールド '{}' が存在しません", key)
                ))?;
                let vb = borrow_b.get(key).ok_or_else(|| RuntimeError::Custom(
                    format!("比較対象にフィールド '{}' がありません", key)
                ))?;
                let ord = compare_values(va, vb)?;
                if ord != std::cmp::Ordering::Equal {
                    return Ok(ord);
                }
            }
            Ok(std::cmp::Ordering::Equal)
        }
        _ => Err(RuntimeError::Custom(format!(
            "比較できない型: {} と {}", a.type_name(), b.type_name()
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
        Some(TypeAnn::Result(inner)) => {
            match inner.as_ref() {
                TypeAnn::Named(state_name) => (Some(state_name.clone()), true),
                _ => (None, true),
            }
        }
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
                let va = borrow_a.get(key).ok_or_else(|| RuntimeError::Custom(
                    format!("フィールド '{}' が存在しません", key)
                ))?;
                let vb = borrow_b.get(key).ok_or_else(|| RuntimeError::Custom(
                    format!("比較対象にフィールド '{}' がありません", key)
                ))?;
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
    args.into_iter().next()
        .ok_or_else(|| RuntimeError::Custom(format!("{}() は引数が1つ必要です", method)))
}

/// メソッドの第1引数を i64 として取り出す
fn one_int_arg(method: &str, args: Vec<Value>) -> Result<i64, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::Int(n)) => Ok(n),
        Some(v)             => Err(type_err("number", v.type_name())),
        None => Err(RuntimeError::Custom(format!("{}() は引数が1つ必要です", method))),
    }
}

/// メソッドの第1引数を List として取り出す
fn one_list_arg(method: &str, args: Vec<Value>) -> Result<Rc<RefCell<Vec<Value>>>, RuntimeError> {
    match args.into_iter().next() {
        Some(Value::List(lst)) => Ok(lst),
        Some(v)                => Err(type_err("list", v.type_name())),
        None => Err(RuntimeError::Custom(format!("{}() は引数が1つ必要です", method))),
    }
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
            Ok(ord) => if descending { ord.reverse() } else { ord },
            Err(e)  => { sort_err = Some(e); std::cmp::Ordering::Equal }
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
        Some(Value::Option(None)) => return None,  // None なら制約スキップ
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
                if len < *m { return Some("length"); }
            }
            if let Some(m) = max {
                if len > *m { return Some("length"); }
            }
            None
        }
        Constraint::Alphanumeric => {
            match val {
                Value::String(s) if s.chars().all(|c| c.is_alphanumeric()) => None,
                Value::String(_) => Some("alphanumeric"),
                _ => Some("alphanumeric"),
            }
        }
        Constraint::EmailFormat => {
            match val {
                Value::String(s) => {
                    if s.contains('@') && s.contains('.') {
                        None
                    } else {
                        Some("email_format")
                    }
                }
                _ => Some("email_format"),
            }
        }
        Constraint::UrlFormat => {
            match val {
                Value::String(s) => {
                    if s.starts_with("http://") || s.starts_with("https://") {
                        None
                    } else {
                        Some("url_format")
                    }
                }
                _ => Some("url_format"),
            }
        }
        Constraint::Range { min, max } => {
            let n = match val {
                Value::Int(n)   => *n as f64,
                Value::Float(f) => *f,
                _ => return Some("range"),
            };
            if let Some(m) = min {
                if n < *m { return Some("range"); }
            }
            if let Some(m) = max {
                if n > *m { return Some("range"); }
            }
            None
        }
        Constraint::ContainsDigit => {
            match val {
                Value::String(s) if s.chars().any(|c| c.is_ascii_digit()) => None,
                Value::String(_) => Some("contains_digit"),
                _ => Some("contains_digit"),
            }
        }
        Constraint::ContainsUppercase => {
            match val {
                Value::String(s) if s.chars().any(|c| c.is_uppercase()) => None,
                Value::String(_) => Some("contains_uppercase"),
                _ => Some("contains_uppercase"),
            }
        }
        Constraint::ContainsLowercase => {
            match val {
                Value::String(s) if s.chars().any(|c| c.is_lowercase()) => None,
                Value::String(_) => Some("contains_lowercase"),
                _ => Some("contains_lowercase"),
            }
        }
        Constraint::NotEmpty => {
            match val {
                Value::String(s) if !s.is_empty() => None,
                Value::String(_) => Some("not_empty"),
                Value::List(list) if !list.borrow().is_empty() => None,
                Value::List(_) => Some("not_empty"),
                _ => Some("not_empty"),
            }
        }
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
        Literal::Int(n)    => Value::Int(*n),
        Literal::Float(f)  => Value::Float(*f),
        Literal::String(s) => Value::String(s.clone()),
        Literal::Bool(b)   => Value::Bool(*b),
    }
}

fn type_err(expected: &str, found: &str) -> RuntimeError {
    RuntimeError::TypeMismatch {
        expected: expected.to_string(),
        found: found.to_string(),
    }
}

fn int_float_op(
    l: Value, r: Value,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
    sym: &str,
) -> Result<Value, RuntimeError> {
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(int_op(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(a, b))),
        (l, r) => Err(type_err(
            &format!("number {} number", sym),
            &format!("{} {} {}", l.type_name(), sym, r.type_name()),
        )),
    }
}

fn cmp_op(
    l: Value, r: Value,
    int_pred: impl Fn(i64, i64) -> bool,
    float_pred: impl Fn(f64, f64) -> bool,
) -> Result<Value, RuntimeError> {
    match (&l, &r) {
        (Value::Int(a),    Value::Int(b))    => Ok(Value::Bool(int_pred(*a, *b))),
        (Value::Float(a),  Value::Float(b))  => Ok(Value::Bool(float_pred(*a, *b))),
        (Value::Struct { .. }, Value::Struct { .. }) => {
            let ord = compare_values(&l, &r)?;
            // int_pred を ordering に対応させる: (a, b) として -1/0/1 に変換
            let (ai, bi): (i64, i64) = match ord {
                std::cmp::Ordering::Less    => (-1, 0),
                std::cmp::Ordering::Equal   => ( 0, 0),
                std::cmp::Ordering::Greater => ( 1, 0),
            };
            Ok(Value::Bool(int_pred(ai, bi)))
        }
        _ => Err(type_err("number", &format!("{} vs {}", l.type_name(), r.type_name()))),
    }
}

/// パターンマッチング: マッチした場合はバインディングリストを返す
fn match_pattern(pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    match (pattern, value) {
        (Pattern::Wildcard, _) => Some(vec![]),
        (Pattern::Ident(name), v) => Some(vec![(name.clone(), v.clone())]),
        (Pattern::Literal(lit), v) => {
            let lit_val = eval_literal(lit);
            if lit_val == *v { Some(vec![]) } else { None }
        }
        (Pattern::None, Value::Option(None)) => Some(vec![]),
        (Pattern::Some(inner), Value::Option(Some(v))) => match_pattern(inner, v),
        (Pattern::Ok(inner), Value::Result(Ok(v)))     => match_pattern(inner, v),
        (Pattern::Err(inner), Value::Result(Err(e))) => {
            match_pattern(inner, &Value::String(e.clone()))
        }
        (Pattern::Range { start, end, inclusive }, Value::Int(n)) => {
            let s = match start { Literal::Int(i) => *i, _ => return None };
            let e = match end   { Literal::Int(i) => *i, _ => return None };
            let hit = if *inclusive { s <= *n && *n <= e } else { s <= *n && *n < e };
            if hit { Some(vec![]) } else { None }
        }
        // ── enum パターン ─────────────────────────────────────────────────
        (Pattern::EnumUnit { enum_name, variant }, Value::Enum { type_name, variant: val_variant, data: EnumData::Unit }) => {
            // enum_name が Some の場合は型名も確認する
            if let Some(en) = enum_name {
                if en != type_name { return None; }
            }
            if variant == val_variant { Some(vec![]) } else { None }
        }
        (Pattern::EnumTuple { enum_name, variant, bindings }, Value::Enum { type_name, variant: val_variant, data: EnumData::Tuple(items) }) => {
            if let Some(en) = enum_name {
                if en != type_name { return None; }
            }
            if variant != val_variant { return None; }
            if bindings.len() != items.len() { return None; }
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
        (Pattern::EnumStruct { enum_name, variant, fields }, Value::Enum { type_name, variant: val_variant, data: EnumData::Struct(field_map) }) => {
            if let Some(en) = enum_name {
                if en != type_name { return None; }
            }
            if variant != val_variant { return None; }
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
    let module = parse_source(source)
        .map_err(|e| RuntimeError::Custom(e.to_string()))?;
    Interpreter::new().eval(&module)
}

/// 型名かどうかを判定（大文字から始まる識別子）
fn is_type_name_str(name: &str) -> bool {
    name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

/// TypeAnn から型のゼロ値を生成する（@derive(Singleton) の初期化用）
fn zero_value_for_type(ann: &TypeAnn) -> Value {
    match ann {
        TypeAnn::Number => Value::Int(0),
        TypeAnn::Float  => Value::Float(0.0),
        TypeAnn::String => Value::String(String::new()),
        TypeAnn::Bool   => Value::Bool(false),
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
        assert_eq!(run(r#""foo" + "bar""#), Ok(Value::String("foobar".to_string())));
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
        assert_eq!(run("state i = 0; while i < 3 { i = i + 1 }; i"), Ok(Value::Int(3)));
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
        assert_eq!(run("let base = 10; let f = x => x + base; f(5)"), Ok(Value::Int(15)));
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
        assert_eq!(run("string(42)"),   Ok(Value::String("42".to_string())));
        assert_eq!(run("string(true)"), Ok(Value::String("true".to_string())));
    }

    #[test]
    fn test_native_number() {
        assert_eq!(run(r#"number("42")"#),  Ok(Value::Result(Ok(Box::new(Value::Int(42))))));
        // number("abc") → err(...)
        let result = run(r#"number("abc")"#).expect("eval failed");
        assert!(matches!(result, Value::Result(Err(_))));
    }

    #[test]
    fn test_native_float() {
        assert_eq!(run(r#"float("3.14")"#), Ok(Value::Result(Ok(Box::new(Value::Float(3.14))))));
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
        assert_eq!(run(src2), Ok(Value::String("Status::Pending(review)".to_string())));

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
        // バリデーション成功で ok(instance) を返す
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
    ok(r)    => r.get_username()
    err(msg) => msg
}
"#;
        let result = run(src);
        assert_eq!(result, Ok(Value::String("alice".to_string())));
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
    ok(r)    => "valid"
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
        assert!(err_msg.contains("query"), "エラーメッセージに 'query' が含まれていません: {}", err_msg);
    }
}
