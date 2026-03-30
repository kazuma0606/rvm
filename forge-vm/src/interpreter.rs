// forge-vm: ツリーウォーキングインタープリタ
// Phase 2-B 実装

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use forge_compiler::ast::*;
use crate::value::{CapturedEnv, NativeFn, Value};

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
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self { scopes: vec![HashMap::new()] };
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
            Expr::Field { .. } => Err(RuntimeError::Custom("フィールドアクセスは未実装".to_string())),
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
        let obj = self.eval_expr(object)?;
        let _arg_vals: Vec<Value> = args.iter().map(|a| self.eval_expr(a)).collect::<Result<_, _>>()?;
        // Phase 3 でコレクション API を実装する
        Err(RuntimeError::Custom(format!(
            "メソッド '{}' は {} に対して未実装です (Phase 3 で実装予定)",
            method, obj.type_name()
        )))
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
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

// ── ヘルパー関数 ────────────────────────────────────────────────────────────

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
    match (l, r) {
        (Value::Int(a),   Value::Int(b))   => Ok(Value::Bool(int_pred(a, b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(float_pred(a, b))),
        (l, r) => Err(type_err("number", &format!("{} vs {}", l.type_name(), r.type_name()))),
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
}
