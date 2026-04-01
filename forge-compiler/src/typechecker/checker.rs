// forge-compiler: 型チェッカー本体
// Phase 4-A 実装

use std::collections::HashMap;

use crate::ast::*;
use crate::lexer::Span;
use super::types::Type;

// ── エラー型 ─────────────────────────────────────────────────────────────────

/// 型エラー（行番号・カラム付き）
#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.span {
            Some(s) => write!(f, "型エラー [{}:{}]: {}", s.line, s.col, self.message),
            None    => write!(f, "型エラー: {}", self.message),
        }
    }
}

// ── TypeChecker ───────────────────────────────────────────────────────────────

/// 型チェッカー（スコープ付き型環境）
pub struct TypeChecker {
    /// スコープスタック（変数名 → 型）
    env: Vec<HashMap<String, Type>>,
    /// 発見した型エラーの一覧
    pub errors: Vec<TypeError>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self { env: vec![HashMap::new()], errors: Vec::new() }
    }

    // ── スコープ操作 ──────────────────────────────────────────────────────

    fn push_scope(&mut self) { self.env.push(HashMap::new()); }

    fn pop_scope(&mut self) { self.env.pop(); }

    fn define(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.env.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    /// 変数の型を検索する（外側スコープも遡る）
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.env.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    fn add_error(&mut self, message: impl Into<String>, span: Option<Span>) {
        self.errors.push(TypeError { message: message.into(), span });
    }

    // ── モジュール・文 ────────────────────────────────────────────────────

    /// モジュール全体を型チェックする
    pub fn check_module(&mut self, module: &Module) {
        for stmt in &module.stmts {
            self.check_stmt(stmt);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let   { name, type_ann, value, .. } => self.check_binding(name, type_ann, value),
            Stmt::State { name, type_ann, value, .. } => self.check_binding(name, type_ann, value),
            Stmt::Const { name, type_ann, value, .. } => self.check_binding(name, type_ann, value),

            Stmt::Fn { name, params, return_type, body, .. } => {
                let declared_ret = return_type.as_ref().map(Type::from_ann).unwrap_or(Type::Unknown);

                self.push_scope();
                for param in params {
                    let ty = param.type_ann.as_ref().map(Type::from_ann).unwrap_or(Type::Unknown);
                    self.define(&param.name, ty);
                }
                let body_ty = self.infer_expr(body);
                self.pop_scope();

                if declared_ret != Type::Unknown && body_ty != Type::Unknown
                    && declared_ret != body_ty
                {
                    self.add_error(
                        format!(
                            "関数 '{}' の戻り値型が不一致: 宣言 {} / 実際 {}",
                            name, declared_ret, body_ty
                        ),
                        None,
                    );
                }
                let fn_ty = if declared_ret != Type::Unknown { declared_ret } else { body_ty };
                self.define(name, fn_ty);
            }

            Stmt::Return(expr, _) => {
                if let Some(e) = expr { self.infer_expr(e); }
            }

            Stmt::Expr(expr) => { self.infer_expr(expr); }

            // T-1: 型定義は型チェッカーでは現在スキップ（将来対応）
            Stmt::StructDef { .. } | Stmt::ImplBlock { .. } | Stmt::EnumDef { .. } => {}
        }
    }

    /// let / state / const バインディングの型チェック
    fn check_binding(&mut self, name: &str, ann: &Option<TypeAnn>, value: &Expr) {
        let inferred  = self.infer_expr(value);
        let declared  = ann.as_ref().map(Type::from_ann).unwrap_or(Type::Unknown);
        let final_ty  = if declared != Type::Unknown { declared.clone() } else { inferred.clone() };

        if declared != Type::Unknown && inferred != Type::Unknown && declared != inferred {
            self.add_error(
                format!(
                    "型不一致: '{}' は {} と宣言されていますが {} の値が代入されています",
                    name, declared, inferred
                ),
                None,
            );
        }
        self.define(name, final_ty);
    }

    // ── 式の型推論 ────────────────────────────────────────────────────────

    /// 式の型を推論する
    pub fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            // リテラルはそのまま型が決まる
            Expr::Literal(lit, _) => infer_literal(lit),

            Expr::Ident(name, _) => {
                self.lookup(name).cloned().unwrap_or(Type::Unknown)
            }

            Expr::BinOp { op, left, right, span } => {
                self.infer_binop(op, left, right, span)
            }

            Expr::UnaryOp { op, operand, .. } => {
                let ty = self.infer_expr(operand);
                match op {
                    UnaryOp::Neg => ty,
                    UnaryOp::Not => Type::Bool,
                }
            }

            Expr::If { cond, then_block, else_block, .. } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then_block);
                match else_block {
                    Some(e) => {
                        let else_ty = self.infer_expr(e);
                        if then_ty == else_ty { then_ty } else { Type::Unknown }
                    }
                    None => Type::Unit,
                }
            }

            Expr::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.infer_expr(body);
                Type::Unit
            }

            Expr::For { var, iter, body, .. } => {
                let iter_ty = self.infer_expr(iter);
                let elem_ty = match iter_ty {
                    Type::List(t) => *t,
                    _             => Type::Unknown,
                };
                self.push_scope();
                self.define(var, elem_ty);
                self.infer_expr(body);
                self.pop_scope();
                Type::List(Box::new(Type::Unknown))
            }

            Expr::Block { stmts, tail, .. } => {
                self.push_scope();
                for stmt in stmts { self.check_stmt(stmt); }
                let ty = match tail {
                    Some(e) => self.infer_expr(e),
                    None    => Type::Unit,
                };
                self.pop_scope();
                ty
            }

            Expr::Match { scrutinee, arms, .. } => {
                let scrut_ty = self.infer_expr(scrutinee);
                self.check_match_exhaustiveness(&scrut_ty, arms);
                arms.first().map(|arm| self.infer_expr(&arm.body)).unwrap_or(Type::Unit)
            }

            Expr::List(items, _) => {
                let elem = items.first().map(|e| self.infer_expr(e)).unwrap_or(Type::Unknown);
                Type::List(Box::new(elem))
            }

            Expr::Range { .. }         => Type::List(Box::new(Type::Number)),
            Expr::Interpolation { .. } => Type::String,

            Expr::Question(inner, _) => {
                match self.infer_expr(inner) {
                    Type::Result(t) => *t,
                    _               => Type::Unknown,
                }
            }

            // 関数呼び出し・メソッド呼び出し・クロージャは Unknown（Phase 4 では未推論）
            Expr::Call { callee, args, .. } => {
                self.infer_expr(callee);
                for a in args { self.infer_expr(a); }
                Type::Unknown
            }
            Expr::MethodCall { object, args, .. } => {
                self.infer_expr(object);
                for a in args { self.infer_expr(a); }
                Type::Unknown
            }
            Expr::Closure { body, .. } => {
                self.infer_expr(body);
                Type::Unknown
            }
            Expr::Assign { value, .. } => {
                self.infer_expr(value);
                Type::Unit
            }
            _ => Type::Unknown,
        }
    }

    // ── 二項演算の型推論 ──────────────────────────────────────────────────

    fn infer_binop(&mut self, op: &BinOp, left: &Expr, right: &Expr, span: &Span) -> Type {
        let lt = self.infer_expr(left);
        let rt = self.infer_expr(right);

        match op {
            BinOp::Add => {
                // string + string → string
                if lt == Type::String && rt == Type::String { return Type::String; }
                self.numeric_result(&lt, &rt, op, span)
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                self.numeric_result(&lt, &rt, op, span)
            }
            BinOp::Eq | BinOp::Ne => {
                if lt != Type::Unknown && rt != Type::Unknown && lt != rt {
                    self.add_error(
                        format!("比較型不一致: {} と {} を比較できません", lt, rt),
                        Some(span.clone()),
                    );
                }
                Type::Bool
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if lt != Type::Unknown && rt != Type::Unknown && lt != rt {
                    self.add_error(
                        format!("比較型不一致: {} と {} を比較できません", lt, rt),
                        Some(span.clone()),
                    );
                }
                Type::Bool
            }
            BinOp::And | BinOp::Or => Type::Bool,
        }
    }

    fn numeric_result(&mut self, lt: &Type, rt: &Type, op: &BinOp, span: &Span) -> Type {
        match (lt, rt) {
            (Type::Number, Type::Number) => Type::Number,
            (Type::Float,  Type::Float)  => Type::Float,
            (Type::Unknown, _) | (_, Type::Unknown) => Type::Unknown,
            _ => {
                self.add_error(
                    format!("型不一致: {:?} 演算で {} と {} は使えません", op, lt, rt),
                    Some(span.clone()),
                );
                Type::Unknown
            }
        }
    }

    // ── match 網羅性チェック ───────────────────────────────────────────────

    fn check_match_exhaustiveness(&mut self, scrut_ty: &Type, arms: &[MatchArm]) {
        let patterns: Vec<&Pattern> = arms.iter().map(|a| &a.pattern).collect();
        let has_wildcard = patterns.iter().any(|p| matches!(p, Pattern::Wildcard | Pattern::Ident(_)));
        if has_wildcard { return; }

        match scrut_ty {
            Type::Option(_) => {
                if !patterns.iter().any(|p| matches!(p, Pattern::Some(_))) {
                    self.add_error("match が網羅的ではありません: some(_) のアームがありません", None);
                }
                if !patterns.iter().any(|p| matches!(p, Pattern::None)) {
                    self.add_error("match が網羅的ではありません: none のアームがありません", None);
                }
            }
            Type::Result(_) => {
                if !patterns.iter().any(|p| matches!(p, Pattern::Ok(_))) {
                    self.add_error("match が網羅的ではありません: ok(_) のアームがありません", None);
                }
                if !patterns.iter().any(|p| matches!(p, Pattern::Err(_))) {
                    self.add_error("match が網羅的ではありません: err(_) のアームがありません", None);
                }
            }
            _ => {}
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self { Self::new() }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

/// リテラルから型を推論する
fn infer_literal(lit: &Literal) -> Type {
    match lit {
        Literal::Int(_)    => Type::Number,
        Literal::Float(_)  => Type::Float,
        Literal::String(_) => Type::String,
        Literal::Bool(_)   => Type::Bool,
    }
}

/// ソースコードを型チェックしてエラーリストを返す（CLI 用）
pub fn type_check_source(source: &str) -> Vec<TypeError> {
    use crate::parser::parse_source;
    let module = match parse_source(source) {
        Ok(m) => m,
        Err(e) => return vec![TypeError { message: e.to_string(), span: None }],
    };
    let mut checker = TypeChecker::new();
    checker.check_module(&module);
    checker.errors
}

// ── テスト ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    fn checker_for(src: &str) -> TypeChecker {
        let module = parse_source(src).unwrap_or_else(|e| panic!("parse: {}", e));
        let mut tc = TypeChecker::new();
        tc.check_module(&module);
        tc
    }

    // ── Phase 4-A 単体テスト ─────────────────────────────────────────────

    #[test]
    fn test_type_infer_int() {
        let tc = checker_for("let x = 42");
        assert_eq!(tc.lookup("x"), Some(&Type::Number));
    }

    #[test]
    fn test_type_infer_float() {
        let tc = checker_for("let x = 3.14");
        assert_eq!(tc.lookup("x"), Some(&Type::Float));
    }

    #[test]
    fn test_type_check_binop() {
        // 数値 + 文字列 → 型エラーが発生する
        let tc = checker_for(r#"1 + "hello""#);
        assert!(!tc.errors.is_empty(), "型エラーが検出されるはず");
        assert!(tc.errors[0].message.contains("型不一致") || tc.errors[0].message.contains("不一致"),
            "エラーメッセージ: {}", tc.errors[0].message);
    }

    #[test]
    fn test_type_check_fn_return() {
        // 戻り値型が宣言と一致しない場合にエラー
        let src = r#"
fn double(n: number) -> string {
    n * 2
}
"#;
        let tc = checker_for(src);
        assert!(!tc.errors.is_empty(), "型エラーが検出されるはず");
    }

    #[test]
    fn test_type_check_option_match() {
        // none アームなし → 網羅性エラー
        let src = r#"
let v: number? = some(1)
match v {
    some(x) => x
}
"#;
        let tc = checker_for(src);
        let has_none_err = tc.errors.iter().any(|e| e.message.contains("none"));
        assert!(has_none_err, "none アーム欠如のエラーが検出されるはず: {:?}", tc.errors);
    }

    #[test]
    fn test_type_check_result_match() {
        // err アームなし → 網羅性エラー
        let src = r#"
let r: number! = ok(1)
match r {
    ok(v) => v
}
"#;
        let tc = checker_for(src);
        let has_err_err = tc.errors.iter().any(|e| e.message.contains("err"));
        assert!(has_err_err, "err アーム欠如のエラーが検出されるはず: {:?}", tc.errors);
    }
}
