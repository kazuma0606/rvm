// forge-compiler: 型チェッカー本体
// Phase 4-A 実装

use std::collections::HashMap;

use super::types::Type;
use crate::ast::*;
use crate::lexer::Span;

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
            None => write!(f, "型エラー: {}", self.message),
        }
    }
}

// ── TypeChecker ───────────────────────────────────────────────────────────────

/// 型チェッカー（スコープ付き型環境）
pub struct TypeChecker {
    /// スコープスタック（変数名 → 型）
    env: Vec<HashMap<String, Type>>,
    /// job 名 → input 一覧
    jobs: HashMap<String, Vec<JobInput>>,
    /// event 名 → フィールド一覧
    events: HashMap<String, Vec<(String, TypeAnn)>>,
    /// 発見した型エラーの一覧
    pub errors: Vec<TypeError>,
    generator_stack: Vec<Option<Type>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            env: vec![HashMap::new()],
            jobs: HashMap::new(),
            events: HashMap::new(),
            errors: Vec::new(),
            generator_stack: Vec::new(),
        }
    }

    // ── スコープ操作 ──────────────────────────────────────────────────────

    fn push_scope(&mut self) {
        self.env.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.env.pop();
    }

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

    fn push_generator(&mut self, ty: Option<Type>) {
        self.generator_stack.push(ty);
    }

    fn pop_generator(&mut self) {
        self.generator_stack.pop();
    }

    fn current_generator_item(&self) -> Option<&Type> {
        self.generator_stack.last().and_then(|opt| opt.as_ref())
    }

    fn add_error(&mut self, message: impl Into<String>, span: Option<Span>) {
        self.errors.push(TypeError {
            message: message.into(),
            span,
        });
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
            Stmt::Let {
                pat,
                type_ann,
                value,
                ..
            } => {
                // Pat::Ident の場合のみ型チェック（分割代入は型チェックをスキップ）
                if let Pat::Ident(name) = pat {
                    self.check_binding(name, type_ann, value);
                } else {
                    self.infer_expr(value);
                }
            }
            Stmt::State {
                name,
                type_ann,
                value,
                ..
            } => self.check_binding(name, type_ann, value),
            Stmt::Const {
                name,
                type_ann,
                value,
                ..
            } => self.check_binding(name, type_ann, value),

            Stmt::Fn {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                let declared_ret = return_type
                    .as_ref()
                    .map(Type::from_ann)
                    .unwrap_or(Type::Unknown);

                let generator_item = return_type.as_ref().and_then(|ty| {
                    if let TypeAnn::Generate(inner) = ty {
                        Some(Type::from_ann(inner))
                    } else {
                        None
                    }
                });

                self.push_generator(generator_item.clone());

                self.push_scope();
                for param in params {
                    let ty = param
                        .type_ann
                        .as_ref()
                        .map(Type::from_ann)
                        .unwrap_or(Type::Unknown);
                    self.define(&param.name, ty);
                }
                let body_ty = self.infer_expr(body);
                self.pop_scope();
                self.pop_generator();

                if declared_ret != Type::Unknown
                    && body_ty != Type::Unknown
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
                let fn_ty = if declared_ret != Type::Unknown {
                    declared_ret
                } else {
                    body_ty
                };
                self.define(name, fn_ty);
            }

            Stmt::System {
                name, params, body, ..
            } => {
                self.push_scope();
                for param in params {
                    let ty = param
                        .type_ann
                        .as_ref()
                        .map(Type::from_ann)
                        .unwrap_or(Type::Unknown);
                    self.define(&param.name, ty);
                }
                let body_ty = self.infer_expr(body);
                self.pop_scope();
                self.define(name, body_ty);
            }

            Stmt::Job {
                name, inputs, body, ..
            } => {
                self.jobs.insert(name.clone(), inputs.clone());
                self.define(name, Type::Unknown);
                self.push_scope();
                for input in inputs {
                    let declared_ty = Type::from_ann(&input.type_ann);
                    self.define(&input.name, declared_ty.clone());
                    if let Some(default) = &input.default {
                        let default_ty = self.infer_expr(default);
                        if declared_ty != Type::Unknown
                            && default_ty != Type::Unknown
                            && declared_ty != default_ty
                        {
                            self.add_error(
                                format!(
                                    "job '{}' の input '{}' の default 型が不一致: 宣言 {} / 実際 {}",
                                    name, input.name, declared_ty, default_ty
                                ),
                                Some(input.span.clone()),
                            );
                        }
                    }
                }
                self.infer_expr(body);
                self.pop_scope();
            }

            Stmt::RunJob { name, args, span } => {
                let Some(inputs) = self.jobs.get(name).cloned() else {
                    self.add_error(
                        format!("未定義の job '{}' を実行しています", name),
                        Some(span.clone()),
                    );
                    return;
                };

                let mut provided = HashMap::new();
                for (arg_name, arg_expr) in args {
                    let Some(input) = inputs.iter().find(|input| input.name == *arg_name) else {
                        self.add_error(
                            format!(
                                "job '{}' に input '{}' は定義されていません",
                                name, arg_name
                            ),
                            Some(span.clone()),
                        );
                        continue;
                    };

                    let actual = self.infer_expr(arg_expr);
                    let expected = Type::from_ann(&input.type_ann);
                    if expected != Type::Unknown && actual != Type::Unknown && expected != actual {
                        self.add_error(
                            format!(
                                "job '{}' の input '{}' の型が不一致: expected {} / actual {}",
                                name, arg_name, expected, actual
                            ),
                            Some(span.clone()),
                        );
                    }
                    provided.insert(arg_name.clone(), actual);
                }

                for input in &inputs {
                    if input.source() == JobInputSource::Cli
                        && input.default.is_none()
                        && !provided.contains_key(&input.name)
                    {
                        self.add_error(
                            format!(
                                "job '{}' の必須 input '{}' が不足しています",
                                name, input.name
                            ),
                            Some(span.clone()),
                        );
                    }
                }
            }

            Stmt::Return(expr, _) => {
                if let Some(e) = expr {
                    self.infer_expr(e);
                }
            }
            Stmt::Yield { value, span } => {
                let value_ty = self.infer_expr(value);
                if let Some(expected) = self.current_generator_item() {
                    if expected != &Type::Unknown
                        && value_ty != Type::Unknown
                        && expected != &value_ty
                    {
                        self.add_error(
                            format!(
                                "yield の値が generate の要素型と一致しません: {} / {}",
                                expected, value_ty
                            ),
                            Some(span.clone()),
                        );
                    }
                } else {
                    self.add_error(
                        "yield は generate<T> 関数内でのみ使用可能です",
                        Some(span.clone()),
                    );
                }
            }

            Stmt::Defer { body, .. } => {
                self.check_defer_body(body);
            }
            Stmt::Expr(expr) => {
                self.infer_expr(expr);
            }

            // T-1: 型定義は型チェッカーでは現在スキップ（将来対応）
            Stmt::StructDef { .. } | Stmt::ImplBlock { .. } | Stmt::EnumDef { .. } => {}
            // T-3: trait / mixin / impl trait も現在スキップ
            Stmt::TraitDef { .. } | Stmt::MixinDef { .. } | Stmt::ImplTrait { .. } => {}
            // T-4: data キーワードも現在スキップ
            Stmt::DataDef { .. } => {}
            // T-5: typestate も現在スキップ
            Stmt::TypestateDef { .. } => {}
            // M-0: use 宣言は型チェッカーでは現在スキップ
            Stmt::UseDecl { .. } => {}
            // M-6: use raw ブロックは型チェッカーでスキップ
            Stmt::UseRaw { .. } => {}
            // M-5: when 文は型チェッカーでは現在スキップ
            Stmt::When { .. } => {}
            // FT-1: test ブロックは型チェッカーでは現在スキップ
            Stmt::TestBlock { .. } => {}
            Stmt::App {
                loads,
                provides,
                container,
                wires,
                span,
                ..
            } => {
                for provide in provides {
                    self.infer_expr(&provide.value);
                }

                for load in loads {
                    if !is_valid_app_load_pattern(load) {
                        self.add_error(
                            format!("無効な load pattern '{}'", load),
                            Some(span.clone()),
                        );
                    }
                }

                for wire in wires {
                    let Some(bindings) = container.as_ref() else {
                        self.add_error(
                            format!(
                                "wire '{}' は container bindings なしでは service を解決できません",
                                wire.job_name
                            ),
                            Some(wire.span.clone()),
                        );
                        continue;
                    };

                    for (_, service_name) in &wire.bindings {
                        if !bindings
                            .iter()
                            .any(|binding| binding.trait_name == *service_name)
                        {
                            self.add_error(
                                format!(
                                    "wire '{}' が未知の service '{}' を参照しています",
                                    wire.job_name, service_name
                                ),
                                Some(wire.span.clone()),
                            );
                        }
                    }
                }
            }
            // DI-4: container 定義は型チェッカーでは現在スキップ
            Stmt::ContainerDef { .. } => {}

            Stmt::Event { name, fields, .. } => {
                self.events.insert(name.clone(), fields.clone());
                self.define(name, Type::Unknown);
            }

            Stmt::Emit { event_name, fields, span } => {
                let Some(decl_fields) = self.events.get(event_name).cloned() else {
                    self.add_error(
                        format!("未定義のイベント '{}' を emit しています", event_name),
                        Some(span.clone()),
                    );
                    return;
                };

                // 過不足チェック
                for (field_name, _) in &decl_fields {
                    if !fields.iter().any(|(n, _)| n == field_name) {
                        self.add_error(
                            format!(
                                "emit '{}' に必須フィールド '{}' が不足しています",
                                event_name, field_name
                            ),
                            Some(span.clone()),
                        );
                    }
                }
                for (field_name, field_expr) in fields {
                    if !decl_fields.iter().any(|(n, _)| n == field_name) {
                        self.add_error(
                            format!(
                                "emit '{}' に未定義フィールド '{}' があります",
                                event_name, field_name
                            ),
                            Some(span.clone()),
                        );
                    } else {
                        self.infer_expr(field_expr);
                    }
                }
            }
        }
    }

    /// let / state / const バインディングの型チェック
    fn check_binding(&mut self, name: &str, ann: &Option<TypeAnn>, value: &Expr) {
        let inferred = self.infer_expr(value);
        let declared = ann.as_ref().map(Type::from_ann).unwrap_or(Type::Unknown);
        let final_ty = if declared != Type::Unknown {
            declared.clone()
        } else {
            inferred.clone()
        };

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

    fn check_defer_body(&mut self, body: &DeferBody) {
        match body {
            DeferBody::Expr(expr) | DeferBody::Block(expr) => {
                self.infer_expr(expr);
            }
        }
    }

    // ── 式の型推論 ────────────────────────────────────────────────────────

    /// 式の型を推論する
    pub fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            // リテラルはそのまま型が決まる
            Expr::Literal(lit, _) => infer_literal(lit),

            Expr::Ident(name, _) => self.lookup(name).cloned().unwrap_or(Type::Unknown),

            Expr::BinOp {
                op,
                left,
                right,
                span,
            } => self.infer_binop(op, left, right, span),

            Expr::UnaryOp { op, operand, .. } => {
                let ty = self.infer_expr(operand);
                match op {
                    UnaryOp::Neg => ty,
                    UnaryOp::Not => Type::Bool,
                }
            }

            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then_block);
                match else_block {
                    Some(e) => {
                        let else_ty = self.infer_expr(e);
                        if then_ty == else_ty {
                            then_ty
                        } else {
                            Type::Unknown
                        }
                    }
                    None => Type::Unit,
                }
            }

            Expr::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.infer_expr(body);
                Type::Unit
            }

            Expr::For {
                pat, iter, body, ..
            } => {
                let iter_ty = self.infer_expr(iter);
                let elem_ty = match iter_ty {
                    Type::List(t) => *t,
                    _ => Type::Unknown,
                };
                self.push_scope();
                // Pat::Ident のみ型定義（分割代入パターンは型チェックをスキップ）
                if let Pat::Ident(var) = pat {
                    self.define(var, elem_ty);
                }
                self.infer_expr(body);
                self.pop_scope();
                Type::List(Box::new(Type::Unknown))
            }

            Expr::Block { stmts, tail, .. } => {
                self.push_scope();
                for stmt in stmts {
                    self.check_stmt(stmt);
                }
                let ty = match tail {
                    Some(e) => self.infer_expr(e),
                    None => Type::Unit,
                };
                self.pop_scope();
                ty
            }

            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrut_ty = self.infer_expr(scrutinee);
                self.check_match_exhaustiveness(&scrut_ty, arms);
                arms.first()
                    .map(|arm| self.infer_expr(&arm.body))
                    .unwrap_or(Type::Unit)
            }

            Expr::List(items, _) => {
                let elem = items
                    .first()
                    .map(|e| self.infer_expr(e))
                    .unwrap_or(Type::Unknown);
                Type::List(Box::new(elem))
            }

            Expr::Range { .. } => Type::List(Box::new(Type::Number)),
            Expr::Interpolation { .. } => Type::String,

            Expr::Question(inner, _) => match self.infer_expr(inner) {
                Type::Result(t) => *t,
                _ => Type::Unknown,
            },

            // 関数呼び出し・メソッド呼び出し・クロージャは Unknown（Phase 4 では未推論）
            Expr::Call { callee, args, .. } => {
                self.infer_expr(callee);
                for a in args {
                    self.infer_expr(a);
                }
                Type::Unknown
            }
            Expr::MethodCall { object, args, .. } => {
                self.infer_expr(object);
                for a in args {
                    self.infer_expr(a);
                }
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
            Expr::Spawn { body, .. } => {
                self.infer_expr(body);
                Type::Unknown
            }
            Expr::Pipeline { steps, .. } => {
                for step in steps {
                    match step {
                        PipelineStep::Source(e)
                        | PipelineStep::Filter(e)
                        | PipelineStep::Map(e)
                        | PipelineStep::FlatMap(e)
                        | PipelineStep::Group(e)
                        | PipelineStep::Take(e)
                        | PipelineStep::Skip(e)
                        | PipelineStep::Each(e)
                        | PipelineStep::Sink(e)
                        | PipelineStep::Parallel(e) => {
                            self.infer_expr(e);
                        }
                        PipelineStep::Sort { key, .. } => {
                            self.infer_expr(key);
                        }
                    }
                }
                Type::Unknown
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
                if lt == Type::String && rt == Type::String {
                    return Type::String;
                }
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
            (Type::Float, Type::Float) => Type::Float,
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
        let has_wildcard = patterns
            .iter()
            .any(|p| matches!(p, Pattern::Wildcard | Pattern::Ident(_)));
        if has_wildcard {
            return;
        }

        match scrut_ty {
            Type::Option(_) => {
                if !patterns.iter().any(|p| matches!(p, Pattern::Some(_))) {
                    self.add_error(
                        "match が網羅的ではありません: some(_) のアームがありません",
                        None,
                    );
                }
                if !patterns.iter().any(|p| matches!(p, Pattern::None)) {
                    self.add_error(
                        "match が網羅的ではありません: none のアームがありません",
                        None,
                    );
                }
            }
            Type::Result(_) => {
                if !patterns.iter().any(|p| matches!(p, Pattern::Ok(_))) {
                    self.add_error(
                        "match が網羅的ではありません: ok(_) のアームがありません",
                        None,
                    );
                }
                if !patterns.iter().any(|p| matches!(p, Pattern::Err(_))) {
                    self.add_error(
                        "match が網羅的ではありません: err(_) のアームがありません",
                        None,
                    );
                }
            }
            _ => {}
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

/// リテラルから型を推論する
fn infer_literal(lit: &Literal) -> Type {
    match lit {
        Literal::Int(_) => Type::Number,
        Literal::Float(_) => Type::Float,
        Literal::String(_) => Type::String,
        Literal::Bool(_) => Type::Bool,
    }
}

fn is_valid_app_load_pattern(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.' | '*'))
}

/// ソースコードを型チェックしてエラーリストを返す（CLI 用）
pub fn type_check_source(source: &str) -> Vec<TypeError> {
    use crate::parser::parse_source;
    let module = match parse_source(source) {
        Ok(m) => m,
        Err(e) => {
            return vec![TypeError {
                message: e.to_string(),
                span: None,
            }]
        }
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
        let tc = checker_for(r#"1 + "hello""#);
        assert!(!tc.errors.is_empty(), "型エラーが検出されるはず");
        assert!(
            tc.errors[0].message.contains("型不一致") || tc.errors[0].message.contains("不一致"),
            "エラーメッセージ: {}",
            tc.errors[0].message
        );
    }

    #[test]
    fn test_type_check_fn_return() {
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
        let src = r#"
let v: number? = some(1)
match v {
    some(x) => x
}
"#;
        let tc = checker_for(src);
        let has_none_err = tc.errors.iter().any(|e| e.message.contains("none"));
        assert!(
            has_none_err,
            "none アーム欠如のエラーが検出されるはず: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_result_match() {
        let src = r#"
let r: number! = ok(1)
match r {
    ok(v) => v
}
"#;
        let tc = checker_for(src);
        let has_err_err = tc.errors.iter().any(|e| e.message.contains("err"));
        assert!(
            has_err_err,
            "err アーム欠如のエラーが検出されるはず: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_run_job_arg_mismatch() {
        let src = r#"
job ImportUsers {
    input path: string
    run { path }
}

run ImportUsers {
    path: 42
}
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors.iter().any(|e| e.message.contains("input 'path'")
                && e.message.contains("expected string / actual number")),
            "job input 型不一致エラーが必要: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_run_job_missing_required_input() {
        let src = r#"
job ImportUsers {
    input path: string
    input dry_run: bool = true
    run { path }
}

run ImportUsers
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors
                .iter()
                .any(|e| e.message.contains("必須 input 'path' が不足")),
            "job 必須 input 欠如エラーが必要: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_run_job_allows_missing_di_input() {
        let src = r#"
job ImportUsers {
    input path: string
    input notifier: Notifier
    run { path }
}

run ImportUsers {
    path: "users.csv"
}
"#;
        let tc = checker_for(src);
        assert!(
            !tc.errors
                .iter()
                .any(|e| e.message.contains("input 'notifier'")),
            "DI input は app/wire で補完される前提なので型チェッカーで必須扱いしない: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_app_wire_requires_container_binding() {
        let src = r#"
app Production {
    wire ImportUsers {
        notifier: Notifier
    }
}
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors.iter().any(|e| {
                e.message.contains("container bindings") && e.message.contains("ImportUsers")
            }),
            "wire と container の不整合エラーが必要: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_app_invalid_load_pattern() {
        let src = r#"
app Production {
    load "jobs/[bad]"
}
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors
                .iter()
                .any(|e| e.message.contains("load pattern") && e.message.contains("[bad]")),
            "無効な load パターンエラーが必要: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_emit_unknown_event() {
        let src = r#"
emit RowInvalid {
    row: 42,
    message: "bad",
}
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors
                .iter()
                .any(|e| e.message.contains("RowInvalid")),
            "未定義イベントエラーが必要: {:?}",
            tc.errors
        );
    }

    #[test]
    fn test_type_check_emit_missing_field() {
        let src = r#"
event RowInvalid {
    row: number
    field: string
    message: string
}

emit RowInvalid {
    row: 42,
    message: "bad",
}
"#;
        let tc = checker_for(src);
        assert!(
            tc.errors
                .iter()
                .any(|e| e.message.contains("field") && e.message.contains("不足")),
            "フィールド不足エラーが必要: {:?}",
            tc.errors
        );
    }
}
