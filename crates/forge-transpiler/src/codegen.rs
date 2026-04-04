use std::collections::{BTreeSet, HashMap, HashSet};

use forge_compiler::ast::{
    BinOp, Constraint, EnumInitData, EnumVariant, Expr, FnDef, InterpPart, Literal, MatchArm,
    Module, Param, Pattern, Stmt, TraitMethod, TypeAnn, TypestateMarker, UnaryOp, UsePath,
    UseSymbols, ValidateRule, WhenCondition,
};

use crate::builtin::{try_builtin_call, try_constructor_call};
use crate::error::TranspileError;

pub struct CodeGenerator {
    indent: usize,
    rename_main: bool,
    scopes: Vec<HashMap<String, VarInfo>>,
    async_fns: HashSet<String>,
    recursive_async_fns: HashSet<String>,
    synthetic_main_async: bool,
    needs_tokio: bool,
    async_context_depth: usize,
    suppress_auto_await_depth: usize,
    needs_phantom_data: bool,
    typestate_initial_states: HashMap<String, String>,
    typestate_state_names: HashMap<String, HashSet<String>>,
}

#[derive(Clone, Copy, Default)]
struct VarInfo {
    is_state: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClosureKind {
    Fn,
    FnMut,
    FnOnce,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            indent: 0,
            rename_main: false,
            scopes: vec![HashMap::new()],
            async_fns: HashSet::new(),
            recursive_async_fns: HashSet::new(),
            synthetic_main_async: false,
            needs_tokio: false,
            async_context_depth: 0,
            suppress_auto_await_depth: 0,
            needs_phantom_data: false,
            typestate_initial_states: HashMap::new(),
            typestate_state_names: HashMap::new(),
        }
    }

    pub fn generate_module(&mut self, module: &Module) -> Result<String, TranspileError> {
        self.analyze_async(module)?;
        self.analyze_typestates(module)?;
        self.rename_main = module
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Fn { name, .. } if name == "main"));

        let mut out = String::new();
        if self.needs_phantom_data {
            out.push_str("use std::marker::PhantomData;\n\n");
        }

        let mut top_level: Vec<&Stmt> = Vec::new();
        let mut main_body: Vec<&Stmt> = Vec::new();

        for stmt in &module.stmts {
            match stmt {
                Stmt::Let { .. } | Stmt::State { .. } | Stmt::Expr(..) | Stmt::Return(..) => {
                    main_body.push(stmt)
                }
                _ => top_level.push(stmt),
            }
        }

        for stmt in top_level {
            out.push_str(&self.gen_stmt(stmt));
            out.push('\n');
        }

        if !main_body.is_empty() || self.rename_main {
            if self.synthetic_main_async {
                out.push_str("#[tokio::main]\nasync fn main() -> Result<(), anyhow::Error> {\n");
            } else {
                out.push_str("fn main() -> Result<(), anyhow::Error> {\n");
            }
            self.indent += 1;
            if self.synthetic_main_async {
                self.async_context_depth += 1;
            }

            if self.rename_main && main_body.is_empty() {
                out.push_str(&self.indent_str());
                if self.async_fns.contains("main") {
                    out.push_str("forge_main().await?;\n");
                } else {
                    out.push_str("forge_main();\n");
                }
            }

            for stmt in main_body {
                out.push_str(&self.gen_stmt(stmt));
            }

            out.push_str(&self.indent_str());
            out.push_str("Ok(())\n");
            if self.synthetic_main_async {
                self.async_context_depth -= 1;
            }
            self.indent -= 1;
            out.push_str("}\n");
        }

        Ok(out)
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn analyze_async(&mut self, module: &Module) -> Result<(), TranspileError> {
        self.async_fns.clear();
        self.recursive_async_fns.clear();
        self.synthetic_main_async = false;
        self.needs_tokio = false;

        let mut fn_bodies = HashMap::new();
        let mut called_fns: HashMap<String, HashSet<String>> = HashMap::new();

        for stmt in &module.stmts {
            if let Stmt::Fn { name, body, .. } = stmt {
                self.ensure_no_await_in_closure(body)?;
                if self.expr_contains_await(body) {
                    self.async_fns.insert(name.clone());
                }
                fn_bodies.insert(name.clone(), body.as_ref().clone());
                called_fns.insert(name.clone(), self.collect_called_fns(body));
            } else if let Stmt::TestBlock { body, .. } = stmt {
                for inner in body {
                    self.ensure_no_await_in_stmt_closure(inner)?;
                }
            } else {
                self.ensure_no_await_in_stmt_closure(stmt)?;
            }
        }

        let synthetic_body = Expr::Block {
            stmts: module
                .stmts
                .iter()
                .filter_map(|stmt| match stmt {
                    Stmt::Let { .. } | Stmt::State { .. } | Stmt::Expr(..) | Stmt::Return(..) => {
                        Some(stmt.clone())
                    }
                    _ => None,
                })
                .collect(),
            tail: None,
            span: forge_compiler::lexer::Span {
                start: 0,
                end: 0,
                line: 1,
                col: 1,
            },
        };
        self.synthetic_main_async = self.expr_contains_await(&synthetic_body);

        let mut changed = true;
        while changed {
            changed = false;
            for (name, callees) in &called_fns {
                if self.async_fns.contains(name) {
                    continue;
                }
                if callees.iter().any(|callee| self.async_fns.contains(callee)) {
                    self.async_fns.insert(name.clone());
                    changed = true;
                }
            }
        }

        for (name, body) in &fn_bodies {
            if self.async_fns.contains(name) && self.expr_calls_fn(body, name) {
                self.recursive_async_fns.insert(name.clone());
            }
        }

        if self.async_fns.contains("main") {
            self.synthetic_main_async = true;
        }

        self.needs_tokio = self.synthetic_main_async
            || module.stmts.iter().any(|stmt| matches!(stmt, Stmt::TestBlock { body, .. } if self.test_body_contains_await(body)))
            || !self.async_fns.is_empty();

        Ok(())
    }

    fn analyze_typestates(&mut self, module: &Module) -> Result<(), TranspileError> {
        self.needs_phantom_data = false;
        self.typestate_initial_states.clear();
        self.typestate_state_names.clear();

        for stmt in &module.stmts {
            let Stmt::TypestateDef {
                name,
                states,
                any_block_count,
                derives,
                generic_params,
                ..
            } = stmt
            else {
                continue;
            };

            self.needs_phantom_data = true;

            if !generic_params.is_empty() {
                return Err(TranspileError::UnsupportedFeature(
                    "ジェネリクス付き typestate は未サポートです".to_string(),
                ));
            }

            if !derives.is_empty() {
                return Err(TranspileError::UnsupportedFeature(
                    "typestate への @derive は未サポートです".to_string(),
                ));
            }

            let mut state_names = HashSet::new();
            let mut initial_state = None::<String>;
            for state in states {
                match state {
                    TypestateMarker::Unit(state_name) => {
                        if initial_state.is_none() {
                            initial_state = Some(state_name.clone());
                        }
                        state_names.insert(state_name.clone());
                    }
                    TypestateMarker::Tuple(_, _) | TypestateMarker::Struct(_, _) => {
                        return Err(TranspileError::UnsupportedFeature(
                            "typestate の状態は Unit 型のみサポートされます".to_string(),
                        ));
                    }
                }
            }

            if initial_state.is_none() {
                return Err(TranspileError::UnsupportedFeature(
                    "typestate には少なくとも1つの状態が必要です".to_string(),
                ));
            }

            if *any_block_count > 1 {
                return Err(TranspileError::UnsupportedFeature(
                    "any ブロックは1つのみ定義できます".to_string(),
                ));
            }

            self.typestate_initial_states.insert(
                name.clone(),
                initial_state.unwrap_or_else(|| {
                    unreachable!("initial_state must be Some: states non-empty check passed above")
                }),
            );
            self.typestate_state_names.insert(name.clone(), state_names);
        }

        Ok(())
    }

    fn ensure_no_await_in_stmt_closure(&self, stmt: &Stmt) -> Result<(), TranspileError> {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::State { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Expr(value) => self.ensure_no_await_in_closure(value),
            Stmt::Fn { body, .. } => self.ensure_no_await_in_closure(body),
            Stmt::Return(Some(expr), _) => self.ensure_no_await_in_closure(expr),
            Stmt::Return(None, _) => Ok(()),
            Stmt::ImplBlock { methods, .. }
            | Stmt::MixinDef { methods, .. }
            | Stmt::ImplTrait { methods, .. } => {
                for method in methods {
                    self.ensure_no_await_in_closure(&method.body)?;
                }
                Ok(())
            }
            Stmt::TraitDef { methods, .. } => {
                for method in methods {
                    if let TraitMethod::Default { body, .. } = method {
                        self.ensure_no_await_in_closure(body)?;
                    }
                }
                Ok(())
            }
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for inner in body {
                    self.ensure_no_await_in_stmt_closure(inner)?;
                }
                Ok(())
            }
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::DataDef { .. }
            | Stmt::TypestateDef { .. }
            | Stmt::UseDecl { .. }
            | Stmt::UseRaw { .. } => Ok(()),
        }
    }

    fn ensure_no_await_in_closure(&self, expr: &Expr) -> Result<(), TranspileError> {
        match expr {
            Expr::Closure { body, .. } => {
                if self.expr_contains_await(body) {
                    Err(TranspileError::UnsupportedFeature(
                        "クロージャ内での .await はサポートされていません".to_string(),
                    ))
                } else {
                    self.ensure_no_await_in_closure(body)
                }
            }
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    self.ensure_no_await_in_stmt_closure(stmt)?;
                }
                if let Some(tail) = tail {
                    self.ensure_no_await_in_closure(tail)?;
                }
                Ok(())
            }
            Expr::BinOp { left, right, .. } => {
                self.ensure_no_await_in_closure(left)?;
                self.ensure_no_await_in_closure(right)
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Question(operand, _)
            | Expr::Await { expr: operand, .. }
            | Expr::Field {
                object: operand, ..
            } => self.ensure_no_await_in_closure(operand),
            Expr::Index { object, index, .. } => {
                self.ensure_no_await_in_closure(object)?;
                self.ensure_no_await_in_closure(index)
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.ensure_no_await_in_closure(cond)?;
                self.ensure_no_await_in_closure(then_block)?;
                if let Some(other) = else_block {
                    self.ensure_no_await_in_closure(other)?;
                }
                Ok(())
            }
            Expr::While { cond, body, .. } => {
                self.ensure_no_await_in_closure(cond)?;
                self.ensure_no_await_in_closure(body)
            }
            Expr::For { iter, body, .. } => {
                self.ensure_no_await_in_closure(iter)?;
                self.ensure_no_await_in_closure(body)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.ensure_no_await_in_closure(scrutinee)?;
                for arm in arms {
                    self.ensure_no_await_in_closure(&arm.body)?;
                }
                Ok(())
            }
            Expr::Call { callee, args, .. } => {
                self.ensure_no_await_in_closure(callee)?;
                for arg in args {
                    self.ensure_no_await_in_closure(arg)?;
                }
                Ok(())
            }
            Expr::MethodCall { object, args, .. } => {
                self.ensure_no_await_in_closure(object)?;
                for arg in args {
                    self.ensure_no_await_in_closure(arg)?;
                }
                Ok(())
            }
            Expr::Assign { value, .. } => self.ensure_no_await_in_closure(value),
            Expr::StructInit { fields, .. } => {
                for (_, expr) in fields {
                    self.ensure_no_await_in_closure(expr)?;
                }
                Ok(())
            }
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => Ok(()),
                EnumInitData::Tuple(items) => {
                    for item in items {
                        self.ensure_no_await_in_closure(item)?;
                    }
                    Ok(())
                }
                EnumInitData::Struct(fields) => {
                    for (_, expr) in fields {
                        self.ensure_no_await_in_closure(expr)?;
                    }
                    Ok(())
                }
            },
            Expr::FieldAssign { object, value, .. } => {
                self.ensure_no_await_in_closure(object)?;
                self.ensure_no_await_in_closure(value)
            }
            Expr::Interpolation { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(expr) = part {
                        self.ensure_no_await_in_closure(expr)?;
                    }
                }
                Ok(())
            }
            Expr::Range { start, end, .. } => {
                self.ensure_no_await_in_closure(start)?;
                self.ensure_no_await_in_closure(end)
            }
            Expr::List(items, _) => {
                for item in items {
                    self.ensure_no_await_in_closure(item)?;
                }
                Ok(())
            }
            Expr::Literal(_, _) | Expr::Ident(_, _) => Ok(()),
        }
    }

    fn expr_contains_await(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Await { .. } => true,
            Expr::Block { stmts, tail, .. } => {
                stmts.iter().any(|stmt| self.stmt_contains_await(stmt))
                    || tail
                        .as_ref()
                        .is_some_and(|tail| self.expr_contains_await(tail))
            }
            Expr::BinOp { left, right, .. } => {
                self.expr_contains_await(left) || self.expr_contains_await(right)
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Question(operand, _)
            | Expr::Field {
                object: operand, ..
            } => self.expr_contains_await(operand),
            Expr::Index { object, index, .. } => {
                self.expr_contains_await(object) || self.expr_contains_await(index)
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr_contains_await(cond)
                    || self.expr_contains_await(then_block)
                    || else_block
                        .as_ref()
                        .is_some_and(|other| self.expr_contains_await(other))
            }
            Expr::While { cond, body, .. } => {
                self.expr_contains_await(cond) || self.expr_contains_await(body)
            }
            Expr::For { iter, body, .. } => {
                self.expr_contains_await(iter) || self.expr_contains_await(body)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr_contains_await(scrutinee)
                    || arms.iter().any(|arm| self.expr_contains_await(&arm.body))
            }
            Expr::Call { callee, args, .. } => {
                self.expr_contains_await(callee)
                    || args.iter().any(|arg| self.expr_contains_await(arg))
            }
            Expr::MethodCall { object, args, .. } => {
                self.expr_contains_await(object)
                    || args.iter().any(|arg| self.expr_contains_await(arg))
            }
            Expr::Closure { body, .. } => self.expr_contains_await(body),
            Expr::Assign { value, .. } => self.expr_contains_await(value),
            Expr::StructInit { fields, .. } => fields
                .iter()
                .any(|(_, expr)| self.expr_contains_await(expr)),
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => false,
                EnumInitData::Tuple(items) => {
                    items.iter().any(|item| self.expr_contains_await(item))
                }
                EnumInitData::Struct(fields) => fields
                    .iter()
                    .any(|(_, expr)| self.expr_contains_await(expr)),
            },
            Expr::FieldAssign { object, value, .. } => {
                self.expr_contains_await(object) || self.expr_contains_await(value)
            }
            Expr::Interpolation { parts, .. } => parts.iter().any(|part| match part {
                InterpPart::Literal(_) => false,
                InterpPart::Expr(expr) => self.expr_contains_await(expr),
            }),
            Expr::Range { start, end, .. } => {
                self.expr_contains_await(start) || self.expr_contains_await(end)
            }
            Expr::List(items, _) => items.iter().any(|item| self.expr_contains_await(item)),
            Expr::Literal(_, _) | Expr::Ident(_, _) => false,
        }
    }

    fn stmt_contains_await(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::State { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Expr(value) => self.expr_contains_await(value),
            Stmt::Fn { body, .. } => self.expr_contains_await(body),
            Stmt::Return(Some(expr), _) => self.expr_contains_await(expr),
            Stmt::Return(None, _) => false,
            Stmt::ImplBlock { methods, .. }
            | Stmt::MixinDef { methods, .. }
            | Stmt::ImplTrait { methods, .. } => methods
                .iter()
                .any(|method| self.expr_contains_await(&method.body)),
            Stmt::TraitDef { methods, .. } => methods.iter().any(|method| match method {
                TraitMethod::Abstract { .. } => false,
                TraitMethod::Default { body, .. } => self.expr_contains_await(body),
            }),
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                body.iter().any(|stmt| self.stmt_contains_await(stmt))
            }
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::DataDef { .. }
            | Stmt::TypestateDef { .. }
            | Stmt::UseDecl { .. }
            | Stmt::UseRaw { .. } => false,
        }
    }

    fn collect_called_fns(&self, expr: &Expr) -> HashSet<String> {
        let mut names = HashSet::new();
        self.collect_called_fns_expr(expr, &mut names);
        names
    }

    fn collect_called_fns_expr(&self, expr: &Expr, names: &mut HashSet<String>) {
        match expr {
            Expr::Await { expr, .. } => {
                if let Expr::Call { callee, .. } = expr.as_ref() {
                    if let Expr::Ident(name, _) = callee.as_ref() {
                        names.insert(name.clone());
                    }
                }
                self.collect_called_fns_expr(expr, names);
            }
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    self.collect_called_fns_stmt(stmt, names);
                }
                if let Some(tail) = tail {
                    self.collect_called_fns_expr(tail, names);
                }
            }
            Expr::BinOp { left, right, .. } => {
                self.collect_called_fns_expr(left, names);
                self.collect_called_fns_expr(right, names);
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Question(operand, _)
            | Expr::Field {
                object: operand, ..
            } => self.collect_called_fns_expr(operand, names),
            Expr::Index { object, index, .. } => {
                self.collect_called_fns_expr(object, names);
                self.collect_called_fns_expr(index, names);
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.collect_called_fns_expr(cond, names);
                self.collect_called_fns_expr(then_block, names);
                if let Some(other) = else_block {
                    self.collect_called_fns_expr(other, names);
                }
            }
            Expr::While { cond, body, .. } => {
                self.collect_called_fns_expr(cond, names);
                self.collect_called_fns_expr(body, names);
            }
            Expr::For { iter, body, .. } => {
                self.collect_called_fns_expr(iter, names);
                self.collect_called_fns_expr(body, names);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_called_fns_expr(scrutinee, names);
                for arm in arms {
                    self.collect_called_fns_expr(&arm.body, names);
                }
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident(name, _) = callee.as_ref() {
                    names.insert(name.clone());
                }
                self.collect_called_fns_expr(callee, names);
                for arg in args {
                    self.collect_called_fns_expr(arg, names);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.collect_called_fns_expr(object, names);
                for arg in args {
                    self.collect_called_fns_expr(arg, names);
                }
            }
            Expr::Closure { .. } | Expr::Literal(_, _) | Expr::Ident(_, _) => {}
            Expr::Assign { value, .. } => self.collect_called_fns_expr(value, names),
            Expr::StructInit { fields, .. } => {
                for (_, expr) in fields {
                    self.collect_called_fns_expr(expr, names);
                }
            }
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => {}
                EnumInitData::Tuple(items) => {
                    for item in items {
                        self.collect_called_fns_expr(item, names);
                    }
                }
                EnumInitData::Struct(fields) => {
                    for (_, expr) in fields {
                        self.collect_called_fns_expr(expr, names);
                    }
                }
            },
            Expr::FieldAssign { object, value, .. } => {
                self.collect_called_fns_expr(object, names);
                self.collect_called_fns_expr(value, names);
            }
            Expr::Interpolation { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(expr) = part {
                        self.collect_called_fns_expr(expr, names);
                    }
                }
            }
            Expr::Range { start, end, .. } => {
                self.collect_called_fns_expr(start, names);
                self.collect_called_fns_expr(end, names);
            }
            Expr::List(items, _) => {
                for item in items {
                    self.collect_called_fns_expr(item, names);
                }
            }
        }
    }

    fn collect_called_fns_stmt(&self, stmt: &Stmt, names: &mut HashSet<String>) {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::State { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Expr(value) => self.collect_called_fns_expr(value, names),
            Stmt::Fn { body, .. } => self.collect_called_fns_expr(body, names),
            Stmt::Return(Some(expr), _) => self.collect_called_fns_expr(expr, names),
            Stmt::Return(None, _) => {}
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for inner in body {
                    self.collect_called_fns_stmt(inner, names);
                }
            }
            Stmt::ImplBlock { methods, .. }
            | Stmt::MixinDef { methods, .. }
            | Stmt::ImplTrait { methods, .. } => {
                for method in methods {
                    self.collect_called_fns_expr(&method.body, names);
                }
            }
            Stmt::TraitDef { methods, .. } => {
                for method in methods {
                    if let TraitMethod::Default { body, .. } = method {
                        self.collect_called_fns_expr(body, names);
                    }
                }
            }
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::DataDef { .. }
            | Stmt::TypestateDef { .. }
            | Stmt::UseDecl { .. }
            | Stmt::UseRaw { .. } => {}
        }
    }

    fn expr_calls_fn(&self, expr: &Expr, fn_name: &str) -> bool {
        match expr {
            Expr::Call { callee, args, .. } => {
                matches!(callee.as_ref(), Expr::Ident(name, _) if name == fn_name)
                    || args.iter().any(|arg| self.expr_calls_fn(arg, fn_name))
            }
            Expr::Await { expr, .. } => self.expr_calls_fn(expr, fn_name),
            Expr::Block { stmts, tail, .. } => {
                stmts.iter().any(|stmt| self.stmt_calls_fn(stmt, fn_name))
                    || tail
                        .as_ref()
                        .is_some_and(|tail| self.expr_calls_fn(tail, fn_name))
            }
            Expr::BinOp { left, right, .. } => {
                self.expr_calls_fn(left, fn_name) || self.expr_calls_fn(right, fn_name)
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Question(operand, _)
            | Expr::Field {
                object: operand, ..
            } => self.expr_calls_fn(operand, fn_name),
            Expr::Index { object, index, .. } => {
                self.expr_calls_fn(object, fn_name) || self.expr_calls_fn(index, fn_name)
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr_calls_fn(cond, fn_name)
                    || self.expr_calls_fn(then_block, fn_name)
                    || else_block
                        .as_ref()
                        .is_some_and(|other| self.expr_calls_fn(other, fn_name))
            }
            Expr::While { cond, body, .. } => {
                self.expr_calls_fn(cond, fn_name) || self.expr_calls_fn(body, fn_name)
            }
            Expr::For { iter, body, .. } => {
                self.expr_calls_fn(iter, fn_name) || self.expr_calls_fn(body, fn_name)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr_calls_fn(scrutinee, fn_name)
                    || arms
                        .iter()
                        .any(|arm| self.expr_calls_fn(&arm.body, fn_name))
            }
            Expr::MethodCall { object, args, .. } => {
                self.expr_calls_fn(object, fn_name)
                    || args.iter().any(|arg| self.expr_calls_fn(arg, fn_name))
            }
            Expr::Closure { .. } | Expr::Literal(_, _) | Expr::Ident(_, _) => false,
            Expr::Assign { value, .. } => self.expr_calls_fn(value, fn_name),
            Expr::StructInit { fields, .. } => fields
                .iter()
                .any(|(_, expr)| self.expr_calls_fn(expr, fn_name)),
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => false,
                EnumInitData::Tuple(items) => {
                    items.iter().any(|item| self.expr_calls_fn(item, fn_name))
                }
                EnumInitData::Struct(fields) => fields
                    .iter()
                    .any(|(_, expr)| self.expr_calls_fn(expr, fn_name)),
            },
            Expr::FieldAssign { object, value, .. } => {
                self.expr_calls_fn(object, fn_name) || self.expr_calls_fn(value, fn_name)
            }
            Expr::Interpolation { parts, .. } => parts.iter().any(|part| match part {
                InterpPart::Literal(_) => false,
                InterpPart::Expr(expr) => self.expr_calls_fn(expr, fn_name),
            }),
            Expr::Range { start, end, .. } => {
                self.expr_calls_fn(start, fn_name) || self.expr_calls_fn(end, fn_name)
            }
            Expr::List(items, _) => items.iter().any(|item| self.expr_calls_fn(item, fn_name)),
        }
    }

    fn stmt_calls_fn(&self, stmt: &Stmt, fn_name: &str) -> bool {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::State { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Expr(value) => self.expr_calls_fn(value, fn_name),
            Stmt::Fn { body, .. } => self.expr_calls_fn(body, fn_name),
            Stmt::Return(Some(expr), _) => self.expr_calls_fn(expr, fn_name),
            Stmt::Return(None, _) => false,
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                body.iter().any(|stmt| self.stmt_calls_fn(stmt, fn_name))
            }
            Stmt::ImplBlock { methods, .. }
            | Stmt::MixinDef { methods, .. }
            | Stmt::ImplTrait { methods, .. } => methods
                .iter()
                .any(|method| self.expr_calls_fn(&method.body, fn_name)),
            Stmt::TraitDef { methods, .. } => methods.iter().any(|method| match method {
                TraitMethod::Abstract { .. } => false,
                TraitMethod::Default { body, .. } => self.expr_calls_fn(body, fn_name),
            }),
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::DataDef { .. }
            | Stmt::TypestateDef { .. }
            | Stmt::UseDecl { .. }
            | Stmt::UseRaw { .. } => false,
        }
    }

    fn test_body_contains_await(&self, body: &[Stmt]) -> bool {
        body.iter().any(|stmt| self.stmt_contains_await(stmt))
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn declare_var(&mut self, name: &str, is_state: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), VarInfo { is_state });
        }
    }

    fn current_scope_names(&self) -> HashSet<String> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.keys().cloned())
            .collect()
    }

    fn is_state_var(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .map(|info| info.is_state)
            .unwrap_or(false)
    }

    fn rust_fn_name<'a>(&self, name: &'a str) -> &'a str {
        if self.rename_main && name == "main" {
            "forge_main"
        } else {
            name
        }
    }

    fn vis(is_pub: bool) -> &'static str {
        if is_pub {
            "pub "
        } else {
            ""
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let {
                name,
                type_ann,
                value,
                ..
            } => {
                let ty = type_ann
                    .as_ref()
                    .map(|t| format!(": {}", type_ann_to_rust(t)))
                    .unwrap_or_default();
                let val = self.gen_expr(value, false);
                let binding_kw = match value {
                    Expr::Closure { params, body, .. }
                        if self.closure_kind(params, body) == ClosureKind::FnMut =>
                    {
                        "let mut"
                    }
                    _ => "let",
                };
                self.declare_var(name, false);
                format!(
                    "{}{} {}{} = {};\n",
                    self.indent_str(),
                    binding_kw,
                    name,
                    ty,
                    val
                )
            }
            Stmt::State {
                name,
                type_ann,
                value,
                ..
            } => {
                let ty = type_ann
                    .as_ref()
                    .map(|t| format!(": {}", type_ann_to_rust(t)))
                    .unwrap_or_default();
                let val = self.gen_expr(value, false);
                self.declare_var(name, true);
                format!("{}let mut {}{} = {};\n", self.indent_str(), name, ty, val)
            }
            Stmt::Const {
                name,
                type_ann,
                value,
                is_pub,
                ..
            } => {
                let ty = type_ann
                    .as_ref()
                    .map(|t| format!(": {}", type_ann_to_rust(t)))
                    .unwrap_or_default();
                let val = self.gen_expr(value, false);
                self.declare_var(name, false);
                format!(
                    "{}{}const {}{} = {};\n",
                    self.indent_str(),
                    Self::vis(*is_pub),
                    name,
                    ty,
                    val
                )
            }
            Stmt::Fn {
                name,
                params,
                return_type,
                body,
                is_pub,
                ..
            } => {
                self.declare_var(name, false);
                self.gen_fn(name, params, return_type, body, *is_pub)
            }
            Stmt::Return(Some(expr), _) => {
                format!(
                    "{}return {};\n",
                    self.indent_str(),
                    self.gen_expr(expr, false)
                )
            }
            Stmt::Return(None, _) => format!("{}return;\n", self.indent_str()),
            Stmt::Expr(expr) => format!("{}{};\n", self.indent_str(), self.gen_expr(expr, false)),
            Stmt::StructDef {
                name,
                fields,
                derives,
                is_pub,
                ..
            } => self.gen_struct_def(name, fields, derives, *is_pub),
            Stmt::ImplBlock {
                target,
                trait_name,
                methods,
                ..
            } => self.gen_impl_block(target, trait_name.as_deref(), methods),
            Stmt::EnumDef {
                name,
                variants,
                derives,
                is_pub,
                ..
            } => self.gen_enum_def(name, variants, derives, *is_pub),
            Stmt::TraitDef {
                name,
                methods,
                is_pub,
                ..
            } => self.gen_trait_def(name, methods, *is_pub),
            Stmt::MixinDef {
                name,
                methods,
                is_pub,
                ..
            } => self.gen_mixin_def(name, methods, *is_pub),
            Stmt::ImplTrait {
                trait_name,
                target,
                methods,
                ..
            } => self.gen_impl_trait(trait_name, target, methods),
            Stmt::DataDef {
                name,
                fields,
                validate_rules,
                is_pub,
                ..
            } => self.gen_data_def(name, fields, validate_rules, *is_pub),
            Stmt::TypestateDef {
                name,
                fields,
                states,
                state_methods,
                any_methods,
                ..
            } => self.gen_typestate_def(name, fields, states, state_methods, any_methods),
            Stmt::UseDecl {
                path,
                symbols,
                is_pub,
                ..
            } => self.gen_use_decl(path, symbols, *is_pub),
            Stmt::UseRaw { rust_code, .. } => self.gen_use_raw(rust_code),
            Stmt::When {
                condition, body, ..
            } => self.gen_when(condition, body),
            Stmt::TestBlock { name, body, .. } => self.gen_test_block(name, body),
        }
    }

    fn gen_fn(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: &Option<TypeAnn>,
        body: &Expr,
        is_pub: bool,
    ) -> String {
        let fn_name = self.rust_fn_name(name);
        let params_str = params
            .iter()
            .map(|p| {
                let ty = p
                    .type_ann
                    .as_ref()
                    .map(type_ann_to_rust)
                    .unwrap_or_else(|| "_".to_string());
                format!("{}: {}", p.name, ty)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let ret_str = match return_type {
            Some(ty) => format!(" -> {}", type_ann_to_rust(ty)),
            None => String::new(),
        };
        let is_async = self.async_fns.contains(name);
        let is_recursive_async = self.recursive_async_fns.contains(name);

        self.push_scope();
        for p in params {
            self.declare_var(&p.name, false);
        }
        if is_async || is_recursive_async {
            self.async_context_depth += 1;
        }
        let body_str = self.gen_block_body(body, false);
        if is_async || is_recursive_async {
            self.async_context_depth -= 1;
        }
        self.pop_scope();

        if is_recursive_async {
            let output_ty = match return_type {
                Some(ty) => type_ann_to_rust(ty),
                None => "()".to_string(),
            };
            let mut out = format!(
                "{}{}fn {}({}) -> std::pin::Pin<Box<dyn std::future::Future<Output = {}>>> {{\n",
                self.indent_str(),
                Self::vis(is_pub),
                fn_name,
                params_str,
                output_ty
            );
            self.indent += 1;
            out.push_str(&format!("{}Box::pin(async move {{\n", self.indent_str()));
            out.push_str(&body_str);
            out.push_str(&format!("{}}})\n", self.indent_str()));
            self.indent -= 1;
            out.push_str(&format!("{}}}\n", self.indent_str()));
            out
        } else {
            let async_kw = if is_async { "async " } else { "" };
            let mut out = format!(
                "{}{}{}fn {}({}){} {{\n",
                self.indent_str(),
                Self::vis(is_pub),
                async_kw,
                fn_name,
                params_str,
                ret_str
            );
            out.push_str(&body_str);
            out.push_str(&self.indent_str());
            out.push_str("}\n");
            out
        }
    }

    fn gen_method_def(&mut self, method: &FnDef) -> String {
        self.gen_method_def_with_vis(method, false)
    }

    fn gen_pub_method_def(&mut self, method: &FnDef) -> String {
        self.gen_method_def_with_vis(method, true)
    }

    fn gen_method_def_with_vis(&mut self, method: &FnDef, is_pub: bool) -> String {
        let receiver = if method.has_state_self {
            "&mut self"
        } else {
            "&self"
        };
        let mut params = vec![receiver.to_string()];
        for p in &method.params {
            let ty = p
                .type_ann
                .as_ref()
                .map(type_ann_to_rust)
                .unwrap_or_else(|| "_".to_string());
            params.push(format!("{}: {}", p.name, ty));
        }
        let ret = method
            .return_type
            .as_ref()
            .map(|t| format!(" -> {}", type_ann_to_rust(t)))
            .unwrap_or_default();

        let mut out = format!(
            "{}{}fn {}({}){} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            method.name,
            params.join(", "),
            ret
        );
        self.push_scope();
        self.declare_var("self", method.has_state_self);
        for p in &method.params {
            self.declare_var(&p.name, false);
        }
        out.push_str(&self.gen_block_body(&method.body, false));
        self.pop_scope();
        out.push_str(&self.indent_str());
        out.push_str("}\n");
        out
    }

    fn gen_trait_method(&mut self, method: &TraitMethod) -> String {
        match method {
            TraitMethod::Abstract {
                name,
                params,
                return_type,
                ..
            } => {
                let mut all_params = vec!["&self".to_string()];
                for p in params {
                    let ty = p
                        .type_ann
                        .as_ref()
                        .map(type_ann_to_rust)
                        .unwrap_or_else(|| "_".to_string());
                    all_params.push(format!("{}: {}", p.name, ty));
                }
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", type_ann_to_rust(t)))
                    .unwrap_or_default();
                format!(
                    "{}fn {}({}){};\n",
                    self.indent_str(),
                    name,
                    all_params.join(", "),
                    ret
                )
            }
            TraitMethod::Default {
                name,
                params,
                return_type,
                body,
                has_state_self,
                ..
            } => {
                let receiver = if *has_state_self {
                    "&mut self"
                } else {
                    "&self"
                };
                let mut all_params = vec![receiver.to_string()];
                for p in params {
                    let ty = p
                        .type_ann
                        .as_ref()
                        .map(type_ann_to_rust)
                        .unwrap_or_else(|| "_".to_string());
                    all_params.push(format!("{}: {}", p.name, ty));
                }
                let ret = return_type
                    .as_ref()
                    .map(|t| format!(" -> {}", type_ann_to_rust(t)))
                    .unwrap_or_default();

                let mut out = format!(
                    "{}fn {}({}){} {{\n",
                    self.indent_str(),
                    name,
                    all_params.join(", "),
                    ret
                );
                self.push_scope();
                self.declare_var("self", *has_state_self);
                for p in params {
                    self.declare_var(&p.name, false);
                }
                out.push_str(&self.gen_block_body(body, false));
                self.pop_scope();
                out.push_str(&self.indent_str());
                out.push_str("}\n");
                out
            }
        }
    }

    fn gen_struct_def(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        derives: &[String],
        is_pub: bool,
    ) -> String {
        let mut out = String::new();
        let derive_attr = self.derive_attr(derives);
        if !derive_attr.is_empty() {
            out.push_str(&format!("{}{}\n", self.indent_str(), derive_attr));
        }
        out.push_str(&format!(
            "{}{}struct {} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            name
        ));
        self.indent += 1;
        for (field, ty) in fields {
            out.push_str(&format!(
                "{}{}{}: {},\n",
                self.indent_str(),
                Self::vis(is_pub),
                field,
                type_ann_to_rust(ty)
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));

        let accessor_impl = self.gen_accessor_impl(name, fields, derives);
        if !accessor_impl.is_empty() {
            out.push('\n');
            out.push_str(&accessor_impl);
        }

        let singleton_impl = self.gen_singleton_impl(name, derives);
        if !singleton_impl.is_empty() {
            out.push('\n');
            out.push_str(&singleton_impl);
        }

        out
    }

    fn gen_impl_block(
        &mut self,
        target: &str,
        trait_name: Option<&str>,
        methods: &[FnDef],
    ) -> String {
        let header = match trait_name {
            Some(name) => format!("{}impl {} for {} {{\n", self.indent_str(), name, target),
            None => format!("{}impl {} {{\n", self.indent_str(), target),
        };

        let mut out = header;
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_pub_method_def(method));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_enum_def(
        &mut self,
        name: &str,
        variants: &[EnumVariant],
        derives: &[String],
        is_pub: bool,
    ) -> String {
        let mut out = String::new();
        let derive_attr = self.derive_attr(derives);
        if !derive_attr.is_empty() {
            out.push_str(&format!("{}{}\n", self.indent_str(), derive_attr));
        }
        out.push_str(&format!(
            "{}{}enum {} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            name
        ));
        self.indent += 1;
        for variant in variants {
            let line = match variant {
                EnumVariant::Unit(name) => name.clone(),
                EnumVariant::Tuple(name, tys) => format!(
                    "{}({})",
                    name,
                    tys.iter()
                        .map(type_ann_to_rust)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                EnumVariant::Struct(name, fields) => format!(
                    "{} {{ {} }}",
                    name,
                    fields
                        .iter()
                        .map(|(field, ty)| format!("{}: {}", field, type_ann_to_rust(ty)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            out.push_str(&format!("{}{},\n", self.indent_str(), line));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_trait_def(&mut self, name: &str, methods: &[TraitMethod], is_pub: bool) -> String {
        let mut out = format!(
            "{}{}trait {} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            name
        );
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_trait_method(method));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_mixin_def(&mut self, name: &str, methods: &[FnDef], is_pub: bool) -> String {
        let mut out = format!(
            "{}{}trait {} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            name
        );
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_method_def(method));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_impl_trait(&mut self, trait_name: &str, target: &str, methods: &[FnDef]) -> String {
        let mut out = format!(
            "{}impl {} for {} {{\n",
            self.indent_str(),
            trait_name,
            target
        );
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_method_def(method));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_data_def(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        validate_rules: &[ValidateRule],
        is_pub: bool,
    ) -> String {
        let derives = vec![
            "Debug".to_string(),
            "Clone".to_string(),
            "Eq".to_string(),
            "Hash".to_string(),
            "Serialize".to_string(),
            "Deserialize".to_string(),
            "Accessor".to_string(),
        ];
        let mut out = self.gen_struct_def(name, fields, &derives, is_pub);

        let validate_impl = self.gen_validate_impl(name, validate_rules);
        if !validate_impl.is_empty() {
            out.push('\n');
            out.push_str(&validate_impl);
        }

        out
    }

    fn gen_typestate_def(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        states: &[TypestateMarker],
        state_methods: &[forge_compiler::ast::TypestateState],
        any_methods: &[FnDef],
    ) -> String {
        let state_names = states
            .iter()
            .map(|state| state.name().to_string())
            .collect::<Vec<_>>();
        let initial_state = state_names.first().cloned().unwrap_or_else(|| {
            unreachable!("initial_state must exist: states validated by analyze_typestates")
        });

        let mut out = String::new();
        for state_name in &state_names {
            out.push_str(&format!("{}struct {};\n", self.indent_str(), state_name));
        }
        if !state_names.is_empty() {
            out.push('\n');
        }

        out.push_str(&format!("{}struct {}<S> {{\n", self.indent_str(), name));
        self.indent += 1;
        for (field, ty) in fields {
            out.push_str(&format!(
                "{}{}: {},\n",
                self.indent_str(),
                field,
                type_ann_to_rust(ty)
            ));
        }
        out.push_str(&format!("{}_state: PhantomData<S>,\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));

        if !initial_state.is_empty() {
            out.push('\n');
            out.push_str(&self.gen_typestate_constructor(name, fields, &initial_state));
        }

        for state in state_methods {
            out.push('\n');
            out.push_str(&self.gen_typestate_state_impl(
                name,
                fields,
                &state.name,
                &state_names,
                &state.methods,
            ));
        }

        for state_name in &state_names {
            if any_methods.is_empty() {
                continue;
            }
            out.push('\n');
            out.push_str(&self.gen_typestate_state_impl(
                name,
                fields,
                state_name,
                &state_names,
                any_methods,
            ));
        }

        out
    }

    fn gen_typestate_constructor(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        initial_state: &str,
    ) -> String {
        let params = fields
            .iter()
            .map(|(field, ty)| format!("{}: {}", field, type_ann_to_rust(ty)))
            .collect::<Vec<_>>()
            .join(", ");

        let mut out = format!("{}impl {}<{}> {{\n", self.indent_str(), name, initial_state);
        self.indent += 1;
        out.push_str(&format!(
            "{}pub fn new({}) -> Self {{\n",
            self.indent_str(),
            params
        ));
        self.indent += 1;
        out.push_str(&format!(
            "{}{} {{ {}_state: PhantomData",
            self.indent_str(),
            name,
            if fields.is_empty() {
                String::new()
            } else {
                fields
                    .iter()
                    .map(|(field, _)| format!("{}: {}, ", field, field))
                    .collect::<String>()
            }
        ));
        out.push_str(" }\n");
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_typestate_state_impl(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        current_state: &str,
        state_names: &[String],
        methods: &[FnDef],
    ) -> String {
        let mut out = format!("{}impl {}<{}> {{\n", self.indent_str(), name, current_state);
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_typestate_method(
                name,
                fields,
                current_state,
                state_names,
                method,
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_typestate_method(
        &mut self,
        type_name: &str,
        fields: &[(String, TypeAnn)],
        current_state: &str,
        state_names: &[String],
        method: &FnDef,
    ) -> String {
        let transition_target =
            self.typestate_transition_target(current_state, state_names, &method.return_type);
        let receiver = if transition_target.is_some() {
            "self"
        } else if method.has_state_self {
            "&mut self"
        } else {
            "&self"
        };

        let mut params = vec![receiver.to_string()];
        for p in &method.params {
            let ty = p
                .type_ann
                .as_ref()
                .map(type_ann_to_rust)
                .unwrap_or_else(|| "_".to_string());
            params.push(format!("{}: {}", p.name, ty));
        }

        let ret = self
            .typestate_return_type(type_name, current_state, state_names, &method.return_type)
            .map(|ret| format!(" -> {}", ret))
            .unwrap_or_default();

        let mut out = format!(
            "{}pub fn {}({}){} {{\n",
            self.indent_str(),
            method.name,
            params.join(", "),
            ret
        );
        self.indent += 1;
        self.push_scope();
        self.declare_var("self", method.has_state_self);
        for p in &method.params {
            self.declare_var(&p.name, false);
        }

        if self.typestate_method_has_body(method) {
            out.push_str(&self.gen_block_body(&method.body, false));
        } else {
            out.push_str(&self.gen_typestate_synthesized_body(
                type_name,
                fields,
                current_state,
                state_names,
                method,
                transition_target.as_deref(),
            ));
        }

        self.pop_scope();
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_typestate_synthesized_body(
        &mut self,
        type_name: &str,
        fields: &[(String, TypeAnn)],
        current_state: &str,
        state_names: &[String],
        method: &FnDef,
        transition_target: Option<&str>,
    ) -> String {
        let expr = if let Some(_next_state) = transition_target {
            let field_values = fields
                .iter()
                .map(|(field, _)| {
                    if method.params.iter().any(|param| param.name == *field) {
                        format!("{}: {}", field, field)
                    } else {
                        format!("{}: self.{}", field, field)
                    }
                })
                .collect::<Vec<_>>();
            let mut parts = field_values;
            parts.push("_state: PhantomData".to_string());
            format!("{} {{ {} }}", type_name, parts.join(", "))
        } else {
            match &method.return_type {
                Some(TypeAnn::Result(inner)) => {
                    let inner_expr = self.typestate_non_transition_value_expr(
                        current_state,
                        state_names,
                        method,
                        inner,
                    );
                    format!("Ok({})", inner_expr)
                }
                Some(inner) => self.typestate_non_transition_value_expr(
                    current_state,
                    state_names,
                    method,
                    inner,
                ),
                None => "()".to_string(),
            }
        };

        format!("{}{}\n", self.indent_str(), expr)
    }

    fn typestate_non_transition_value_expr(
        &self,
        current_state: &str,
        state_names: &[String],
        method: &FnDef,
        return_type: &TypeAnn,
    ) -> String {
        match return_type {
            TypeAnn::Named(state_name)
                if state_names.iter().any(|name| name == state_name)
                    && state_name == current_state =>
            {
                "self".to_string()
            }
            _ => method
                .params
                .first()
                .map(|param| param.name.clone())
                .unwrap_or_else(|| "()".to_string()),
        }
    }

    fn typestate_method_has_body(&self, method: &FnDef) -> bool {
        match method.body.as_ref() {
            Expr::Block { stmts, tail, .. } => !stmts.is_empty() || tail.is_some(),
            _ => true,
        }
    }

    fn typestate_transition_target(
        &self,
        current_state: &str,
        state_names: &[String],
        return_type: &Option<TypeAnn>,
    ) -> Option<String> {
        match return_type {
            Some(TypeAnn::Named(state_name))
                if state_names.iter().any(|name| name == state_name)
                    && state_name != current_state =>
            {
                Some(state_name.clone())
            }
            Some(TypeAnn::Result(inner)) => match inner.as_ref() {
                TypeAnn::Named(state_name)
                    if state_names.iter().any(|name| name == state_name)
                        && state_name != current_state =>
                {
                    Some(state_name.clone())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn typestate_return_type(
        &self,
        type_name: &str,
        _current_state: &str,
        state_names: &[String],
        return_type: &Option<TypeAnn>,
    ) -> Option<String> {
        match return_type {
            None => None,
            Some(TypeAnn::Named(state_name))
                if state_names.iter().any(|name| name == state_name) =>
            {
                Some(format!("{}<{}>", type_name, state_name))
            }
            Some(TypeAnn::Result(inner)) => match inner.as_ref() {
                TypeAnn::Named(state_name) if state_names.iter().any(|name| name == state_name) => {
                    Some(format!(
                        "Result<{}<{}>, anyhow::Error>",
                        type_name, state_name
                    ))
                }
                _ => Some(type_ann_to_rust(&TypeAnn::Result(inner.clone()))),
            },
            Some(other) => Some(type_ann_to_rust(other)),
        }
    }

    fn gen_accessor_impl(
        &mut self,
        name: &str,
        fields: &[(String, TypeAnn)],
        derives: &[String],
    ) -> String {
        if !derives.iter().any(|d| d == "Accessor") {
            return String::new();
        }

        let mut out = format!("{}impl {} {{\n", self.indent_str(), name);
        self.indent += 1;
        for (field, ty) in fields {
            let rust_ty = type_ann_to_rust(ty);
            out.push_str(&format!(
                "{}pub fn get_{}(&self) -> {} {{\n{}self.{}.clone()\n{}}}\n",
                self.indent_str(),
                field,
                rust_ty,
                "    ".repeat(self.indent + 1),
                field,
                self.indent_str()
            ));
            out.push_str(&format!(
                "{}pub fn set_{}(&mut self, value: {}) {{\n{}self.{} = value;\n{}}}\n",
                self.indent_str(),
                field,
                rust_ty,
                "    ".repeat(self.indent + 1),
                field,
                self.indent_str()
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_singleton_impl(&mut self, name: &str, derives: &[String]) -> String {
        if !derives.iter().any(|d| d == "Singleton") {
            return String::new();
        }

        format!(
            "{}impl {} {{\n    pub fn instance() -> &'static Self {{\n        static INSTANCE: once_cell::sync::Lazy<{}> = once_cell::sync::Lazy::new({}::default);\n        &INSTANCE\n    }}\n}}\n",
            self.indent_str(),
            name,
            name,
            name
        )
    }

    fn gen_validate_impl(&mut self, name: &str, rules: &[ValidateRule]) -> String {
        if rules.is_empty() {
            return String::new();
        }

        let mut out = format!("{}impl {} {{\n", self.indent_str(), name);
        self.indent += 1;
        out.push_str(&format!(
            "{}pub fn validate(&self) -> Result<(), String> {{\n",
            self.indent_str()
        ));
        self.indent += 1;
        for rule in rules {
            for constraint in &rule.constraints {
                let field = &rule.field;
                match constraint {
                    Constraint::Length { min, max } => {
                        if let Some(min) = min {
                            out.push_str(&format!(
                                "{}if self.{}.len() < {} {{ return Err(\"{}: length\".to_string()); }}\n",
                                self.indent_str(),
                                field,
                                min,
                                field
                            ));
                        }
                        if let Some(max) = max {
                            out.push_str(&format!(
                                "{}if self.{}.len() > {} {{ return Err(\"{}: length\".to_string()); }}\n",
                                self.indent_str(),
                                field,
                                max,
                                field
                            ));
                        }
                    }
                    Constraint::Alphanumeric => {
                        out.push_str(&format!(
                            "{}if !self.{}.chars().all(|c| c.is_ascii_alphanumeric()) {{ return Err(\"{}: alphanumeric\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field
                        ));
                    }
                    Constraint::EmailFormat => {
                        out.push_str(&format!(
                            "{}if !(self.{}.contains('@') && self.{}.contains('.')) {{ return Err(\"{}: email_format\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field,
                            field
                        ));
                    }
                    Constraint::UrlFormat => {
                        out.push_str(&format!(
                            "{}if !(self.{}.starts_with(\"http://\") || self.{}.starts_with(\"https://\")) {{ return Err(\"{}: url_format\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field,
                            field
                        ));
                    }
                    Constraint::Range { min, max } => {
                        if let Some(min) = min {
                            out.push_str(&format!(
                                "{}if (self.{} as f64) < {} {{ return Err(\"{}: range\".to_string()); }}\n",
                                self.indent_str(),
                                field,
                                min,
                                field
                            ));
                        }
                        if let Some(max) = max {
                            out.push_str(&format!(
                                "{}if (self.{} as f64) > {} {{ return Err(\"{}: range\".to_string()); }}\n",
                                self.indent_str(),
                                field,
                                max,
                                field
                            ));
                        }
                    }
                    Constraint::ContainsDigit => {
                        out.push_str(&format!(
                            "{}if !self.{}.chars().any(|c| c.is_ascii_digit()) {{ return Err(\"{}: contains_digit\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field
                        ));
                    }
                    Constraint::ContainsUppercase => {
                        out.push_str(&format!(
                            "{}if !self.{}.chars().any(|c| c.is_ascii_uppercase()) {{ return Err(\"{}: contains_uppercase\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field
                        ));
                    }
                    Constraint::ContainsLowercase => {
                        out.push_str(&format!(
                            "{}if !self.{}.chars().any(|c| c.is_ascii_lowercase()) {{ return Err(\"{}: contains_lowercase\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field
                        ));
                    }
                    Constraint::NotEmpty => {
                        out.push_str(&format!(
                            "{}if self.{}.is_empty() {{ return Err(\"{}: not_empty\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            field
                        ));
                    }
                    Constraint::Matches(pattern) => {
                        out.push_str(&format!(
                            "{}if !self.{}.contains({:?}) {{ return Err(\"{}: matches\".to_string()); }}\n",
                            self.indent_str(),
                            field,
                            pattern,
                            field
                        ));
                    }
                }
            }
        }
        out.push_str(&format!("{}Ok(())\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn derive_attr(&self, derives: &[String]) -> String {
        let mut rust_derives = BTreeSet::new();
        for derive in derives {
            match derive.as_str() {
                "Debug" => {
                    rust_derives.insert("Debug");
                }
                "Clone" => {
                    rust_derives.insert("Clone");
                }
                "Eq" => {
                    rust_derives.insert("PartialEq");
                    rust_derives.insert("Eq");
                }
                "Hash" => {
                    rust_derives.insert("Hash");
                }
                "Ord" => {
                    rust_derives.insert("PartialOrd");
                    rust_derives.insert("Ord");
                }
                "Default" => {
                    rust_derives.insert("Default");
                }
                "Serialize" => {
                    rust_derives.insert("serde::Serialize");
                }
                "Deserialize" => {
                    rust_derives.insert("serde::Deserialize");
                }
                _ => {}
            }
        }

        if rust_derives.is_empty() {
            String::new()
        } else {
            format!(
                "#[derive({})]",
                rust_derives.into_iter().collect::<Vec<_>>().join(", ")
            )
        }
    }

    fn gen_use_decl(&mut self, path: &UsePath, symbols: &UseSymbols, is_pub: bool) -> String {
        let prefix = Self::vis(is_pub);
        let base = match path {
            UsePath::Local(path) => format!("crate::{}", path.replace('/', "::")),
            UsePath::External(path) => path.replace('/', "::"),
            UsePath::Stdlib(path) => format!("forge_std::{}", path.replace('/', "::")),
        };

        let rendered = match symbols {
            UseSymbols::Single(name, alias) => match alias {
                Some(alias) => format!("{}::{} as {}", base, name, alias),
                None => format!("{}::{}", base, name),
            },
            UseSymbols::Multiple(items) => format!(
                "{}::{{{}}}",
                base,
                items
                    .iter()
                    .map(|(name, alias)| match alias {
                        Some(alias) => format!("{} as {}", name, alias),
                        None => name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            UseSymbols::All => format!("{}::*", base),
        };

        format!("{}{}use {};\n", self.indent_str(), prefix, rendered)
    }

    fn gen_use_raw(&self, rust_code: &str) -> String {
        rust_code
            .lines()
            .map(|line| format!("{}{}\n", self.indent_str(), line))
            .collect::<String>()
    }

    fn gen_when(&mut self, condition: &WhenCondition, body: &[Stmt]) -> String {
        let cfg = when_condition_to_cfg(condition);
        let mut out = String::new();
        for stmt in body {
            out.push_str(&format!("{}#[cfg({})]\n", self.indent_str(), cfg));
            out.push_str(&self.gen_stmt(stmt));
        }
        out
    }

    fn gen_test_block(&mut self, name: &str, body: &[Stmt]) -> String {
        let module_name = format!("forge_test_{}", sanitize_test_name(name));
        let fn_name = sanitize_test_name(name);
        let is_async = self.test_body_contains_await(body);
        let mut out = format!(
            "{}#[cfg(test)]\n{}mod {} {{\n",
            self.indent_str(),
            self.indent_str(),
            module_name
        );
        self.indent += 1;
        out.push_str(&format!("{}use super::*;\n", self.indent_str()));
        if is_async {
            out.push_str(&format!("{}#[tokio::test]\n", self.indent_str()));
            out.push_str(&format!(
                "{}async fn {}() -> Result<(), anyhow::Error> {{\n",
                self.indent_str(),
                fn_name
            ));
        } else {
            out.push_str(&format!("{}#[test]\n", self.indent_str()));
            out.push_str(&format!("{}fn {}() {{\n", self.indent_str(), fn_name));
        }
        self.indent += 1;
        if is_async {
            self.async_context_depth += 1;
        }
        for stmt in body {
            out.push_str(&self.gen_stmt(stmt));
        }
        if is_async {
            out.push_str(&format!("{}Ok(())\n", self.indent_str()));
            self.async_context_depth -= 1;
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_block_body(&mut self, expr: &Expr, is_main: bool) -> String {
        self.indent += 1;
        let mut out = String::new();
        self.push_scope();

        match expr {
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    out.push_str(&self.gen_stmt(stmt));
                }
                if let Some(tail) = tail {
                    if is_main {
                        out.push_str(&self.gen_stmt(&Stmt::Expr((**tail).clone())));
                    } else {
                        out.push_str(&format!(
                            "{}{}\n",
                            self.indent_str(),
                            self.gen_expr(tail, false)
                        ));
                    }
                }
            }
            other => {
                if is_main {
                    out.push_str(&self.gen_stmt(&Stmt::Expr(other.clone())));
                } else {
                    out.push_str(&format!(
                        "{}{}\n",
                        self.indent_str(),
                        self.gen_expr(other, false)
                    ));
                }
            }
        }

        self.pop_scope();
        self.indent -= 1;
        out
    }

    fn gen_expr(&mut self, expr: &Expr, needs_parens: bool) -> String {
        let inner = self.gen_expr_inner(expr);
        if needs_parens {
            format!("({})", inner)
        } else {
            inner
        }
    }

    fn gen_expr_inner(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Literal(lit, _) => gen_literal(lit),
            Expr::Ident(name, _) => self.rust_fn_name(name).to_string(),
            Expr::BinOp {
                op, left, right, ..
            } => {
                let lhs = self.gen_expr(left, binop_needs_parens(left));
                let rhs = self.gen_expr(right, binop_needs_parens(right));
                format!("{} {} {}", lhs, binop_to_rust(op), rhs)
            }
            Expr::UnaryOp { op, operand, .. } => {
                let inner = self.gen_expr(operand, true);
                match op {
                    UnaryOp::Neg => format!("-{}", inner),
                    UnaryOp::Not => format!("!{}", inner),
                }
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => self.gen_if(cond, then_block, else_block),
            Expr::While { cond, body, .. } => self.gen_while(cond, body),
            Expr::For {
                var, iter, body, ..
            } => self.gen_for(var, iter, body),
            Expr::Match {
                scrutinee, arms, ..
            } => self.gen_match(scrutinee, arms),
            Expr::Block { stmts, tail, .. } => self.gen_block(stmts, tail),
            Expr::Call { callee, args, .. } => self.gen_call(callee, args),
            Expr::MethodCall {
                object,
                method,
                args,
                ..
            } => self.gen_method_call(object, method, args),
            Expr::Field { object, field, .. } => {
                format!("{}.{}", self.gen_expr(object, false), field)
            }
            Expr::Index { object, index, .. } => {
                format!(
                    "{}[{}]",
                    self.gen_expr(object, false),
                    self.gen_expr(index, false)
                )
            }
            Expr::Closure { params, body, .. } => self.gen_closure(params, body),
            Expr::Interpolation { parts, .. } => self.gen_interpolation(parts),
            Expr::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let lhs = self.gen_expr(start, false);
                let rhs = self.gen_expr(end, false);
                if *inclusive {
                    format!("{}..={}", lhs, rhs)
                } else {
                    format!("{}..{}", lhs, rhs)
                }
            }
            Expr::List(items, _) => self.gen_list(items),
            Expr::Question(inner, _) => format!("{}?", self.gen_expr(inner, false)),
            Expr::Await { expr, .. } => {
                self.suppress_auto_await_depth += 1;
                let inner = self.gen_expr(expr, false);
                self.suppress_auto_await_depth -= 1;
                format!("{}.await", inner)
            }
            Expr::Assign { name, value, .. } => {
                format!("{} = {}", name, self.gen_expr(value, false))
            }
            Expr::StructInit { name, fields, .. } => format!(
                "{} {{ {} }}",
                name,
                fields
                    .iter()
                    .map(|(field, expr)| format!("{}: {}", field, self.gen_expr(expr, false)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::EnumInit {
                enum_name,
                variant,
                data,
                ..
            } => match data {
                EnumInitData::None => format!("{}::{}", enum_name, variant),
                EnumInitData::Tuple(items) => format!(
                    "{}::{}({})",
                    enum_name,
                    variant,
                    items
                        .iter()
                        .map(|expr| self.gen_expr(expr, false))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                EnumInitData::Struct(fields) => format!(
                    "{}::{} {{ {} }}",
                    enum_name,
                    variant,
                    fields
                        .iter()
                        .map(|(field, expr)| format!("{}: {}", field, self.gen_expr(expr, false)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            },
            Expr::FieldAssign {
                object,
                field,
                value,
                ..
            } => format!(
                "{}.{} = {}",
                self.gen_expr(object, false),
                field,
                self.gen_expr(value, false)
            ),
        }
    }

    fn gen_if(&mut self, cond: &Expr, then_block: &Expr, else_block: &Option<Box<Expr>>) -> String {
        let cond_str = self.gen_expr(cond, false);
        let then_str = self.gen_inline_block(then_block);
        match else_block {
            None => format!("if {} {}", cond_str, then_str),
            Some(other) => match other.as_ref() {
                Expr::If {
                    cond,
                    then_block,
                    else_block,
                    ..
                } => format!(
                    "if {} {} else {}",
                    cond_str,
                    then_str,
                    self.gen_if(cond, then_block, else_block)
                ),
                expr => format!(
                    "if {} {} else {}",
                    cond_str,
                    then_str,
                    self.gen_inline_block(expr)
                ),
            },
        }
    }

    fn gen_inline_block(&mut self, expr: &Expr) -> String {
        self.indent += 1;
        let mut inner = String::new();
        inner.push_str("{\n");
        match expr {
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    inner.push_str(&self.gen_stmt(stmt));
                }
                if let Some(tail) = tail {
                    inner.push_str(&format!(
                        "{}{}\n",
                        self.indent_str(),
                        self.gen_expr(tail, false)
                    ));
                }
            }
            other => {
                inner.push_str(&format!(
                    "{}{}\n",
                    self.indent_str(),
                    self.gen_expr(other, false)
                ));
            }
        }
        self.indent -= 1;
        inner.push_str(&format!("{}}}", self.indent_str()));
        inner
    }

    fn gen_while(&mut self, cond: &Expr, body: &Expr) -> String {
        format!(
            "while {} {}",
            self.gen_expr(cond, false),
            self.gen_inline_block(body)
        )
    }

    fn gen_for(&mut self, var: &str, iter: &Expr, body: &Expr) -> String {
        format!(
            "for {} in &{} {}",
            var,
            self.gen_expr(iter, false),
            self.gen_inline_block(body)
        )
    }

    fn gen_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> String {
        let mut out = format!("match {} {{\n", self.gen_expr(scrutinee, false));
        self.indent += 1;
        for arm in arms {
            out.push_str(&format!(
                "{}{} => {},\n",
                self.indent_str(),
                gen_pattern(&arm.pattern),
                self.gen_expr(&arm.body, false)
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}", self.indent_str()));
        out
    }

    fn gen_block(&mut self, stmts: &[Stmt], tail: &Option<Box<Expr>>) -> String {
        self.indent += 1;
        let mut out = String::from("{\n");
        for stmt in stmts {
            out.push_str(&self.gen_stmt(stmt));
        }
        if let Some(tail) = tail {
            out.push_str(&format!(
                "{}{}\n",
                self.indent_str(),
                self.gen_expr(tail, false)
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}", self.indent_str()));
        out
    }

    fn gen_call(&mut self, callee: &Expr, args: &[Expr]) -> String {
        let arg_strs: Vec<String> = args.iter().map(|arg| self.gen_expr(arg, false)).collect();

        if let Expr::Ident(name, _) = callee {
            if let Some(rendered) = try_builtin_call(name, &arg_strs) {
                return rendered;
            }
            if let Some(rendered) = try_constructor_call(name, &arg_strs) {
                return rendered;
            }
        }

        let call = format!("{}({})", self.gen_expr(callee, false), arg_strs.join(", "));
        if self.should_auto_await(callee) {
            format!("{}.await", call)
        } else {
            call
        }
    }

    fn gen_method_call(&mut self, object: &Expr, method: &str, args: &[Expr]) -> String {
        if let Expr::Ident(type_name, _) = object {
            if method == "new" && self.typestate_initial_states.contains_key(type_name) {
                let args: Vec<String> = args
                    .iter()
                    .skip(1)
                    .map(|arg| self.gen_expr(arg, false))
                    .collect();
                return format!("{}::new({})", type_name, args.join(", "));
            }
        }

        let object = self.gen_expr(object, false);
        let args: Vec<String> = args.iter().map(|arg| self.gen_expr(arg, false)).collect();

        match method {
            "is_some" | "is_none" | "is_ok" | "is_err" | "unwrap_or" => {
                format!("{}.{}({})", object, method, args.join(", "))
            }
            "map" => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().map({}).collect::<Vec<_>>()", object, f)
            }
            "filter" => {
                let f = args.first().cloned().unwrap_or_default();
                format!(
                    "{}.iter().filter(|x| ({})(x)).collect::<Vec<_>>()",
                    object, f
                )
            }
            "flat_map" => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().flat_map({}).collect::<Vec<_>>()", object, f)
            }
            "fold" => {
                let init = args.first().cloned().unwrap_or_else(|| "0".to_string());
                let f = args.get(1).cloned().unwrap_or_default();
                format!("{}.iter().fold({}, {})", object, init, f)
            }
            "sum" => format!("{}.iter().sum::<i64>()", object),
            "count" | "len" => format!("{}.len()", object),
            "any" => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().any(|x| ({})(x))", object, f)
            }
            "all" => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().all(|x| ({})(x))", object, f)
            }
            "first" => format!("{}.first().copied()", object),
            "last" => format!("{}.last().copied()", object),
            "take" => {
                let n = args.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().take({}).cloned().collect::<Vec<_>>()", object, n)
            }
            "skip" => {
                let n = args.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().skip({}).cloned().collect::<Vec<_>>()", object, n)
            }
            "reverse" => format!("{{ let mut v = {}.clone(); v.reverse(); v }}", object),
            "distinct" => format!("{{ let mut v = {}.clone(); v.dedup(); v }}", object),
            "enumerate" => format!("{}.iter().enumerate()", object),
            "zip" => {
                let other = args.first().cloned().unwrap_or_default();
                format!("{}.iter().zip({}.iter())", object, other)
            }
            "to_string" => format!("{}.to_string()", object),
            _ => format!("{}.{}({})", object, method, args.join(", ")),
        }
    }

    fn should_auto_await(&self, callee: &Expr) -> bool {
        if self.async_context_depth == 0 || self.suppress_auto_await_depth > 0 {
            return false;
        }

        match callee {
            Expr::Ident(name, _) => self.async_fns.contains(name),
            _ => false,
        }
    }

    fn gen_closure(&mut self, params: &[String], body: &Expr) -> String {
        let kind = self.closure_kind(params, body);
        let prefix = match kind {
            ClosureKind::Fn => "",
            ClosureKind::FnMut | ClosureKind::FnOnce => "move ",
        };

        format!(
            "{}|{}| {}",
            prefix,
            params.join(", "),
            self.gen_expr(body, false)
        )
    }

    fn closure_kind(&self, params: &[String], body: &Expr) -> ClosureKind {
        let outer_names = self.current_scope_names();
        let mut local_scopes = vec![params.iter().cloned().collect::<HashSet<_>>()];
        let mut captured = HashSet::new();
        let mut mutated = HashSet::new();
        let mut consumed = HashSet::new();

        self.analyze_closure_expr(
            body,
            &outer_names,
            &mut local_scopes,
            &mut captured,
            &mut mutated,
            &mut consumed,
            true,
        );

        if consumed.iter().any(|name| captured.contains(name)) {
            ClosureKind::FnOnce
        } else if mutated
            .iter()
            .any(|name| captured.contains(name) && self.is_state_var(name))
        {
            ClosureKind::FnMut
        } else {
            ClosureKind::Fn
        }
    }

    fn analyze_closure_stmt(
        &self,
        stmt: &Stmt,
        outer_names: &HashSet<String>,
        local_scopes: &mut Vec<HashSet<String>>,
        captured: &mut HashSet<String>,
        mutated: &mut HashSet<String>,
        consumed: &mut HashSet<String>,
    ) {
        match stmt {
            Stmt::Let { name, value, .. } => {
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                if let Some(scope) = local_scopes.last_mut() {
                    scope.insert(name.clone());
                }
            }
            Stmt::State { name, value, .. } => {
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                if let Some(scope) = local_scopes.last_mut() {
                    scope.insert(name.clone());
                }
            }
            Stmt::Const { name, value, .. } => {
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                if let Some(scope) = local_scopes.last_mut() {
                    scope.insert(name.clone());
                }
            }
            Stmt::Fn {
                name, params, body, ..
            } => {
                if let Some(scope) = local_scopes.last_mut() {
                    scope.insert(name.clone());
                }
                local_scopes.push(
                    params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect::<HashSet<_>>(),
                );
                self.analyze_closure_expr(
                    body,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                local_scopes.pop();
            }
            Stmt::Return(Some(expr), _) | Stmt::Expr(expr) => self.analyze_closure_expr(
                expr,
                outer_names,
                local_scopes,
                captured,
                mutated,
                consumed,
                false,
            ),
            Stmt::Return(None, _) => {}
            Stmt::ImplBlock { methods, .. } => {
                for method in methods {
                    local_scopes.push(
                        method
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .chain(std::iter::once("self".to_string()))
                            .collect::<HashSet<_>>(),
                    );
                    self.analyze_closure_expr(
                        &method.body,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                    local_scopes.pop();
                }
            }
            Stmt::TraitDef { methods, .. } => {
                for method in methods {
                    if let TraitMethod::Default { params, body, .. } = method {
                        local_scopes.push(
                            params
                                .iter()
                                .map(|param| param.name.clone())
                                .chain(std::iter::once("self".to_string()))
                                .collect::<HashSet<_>>(),
                        );
                        self.analyze_closure_expr(
                            body,
                            outer_names,
                            local_scopes,
                            captured,
                            mutated,
                            consumed,
                            false,
                        );
                        local_scopes.pop();
                    }
                }
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
                for method in methods {
                    local_scopes.push(
                        method
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .chain(std::iter::once("self".to_string()))
                            .collect::<HashSet<_>>(),
                    );
                    self.analyze_closure_expr(
                        &method.body,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                    local_scopes.pop();
                }
            }
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for inner in body {
                    self.analyze_closure_stmt(
                        inner,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                    );
                }
            }
            Stmt::StructDef { .. }
            | Stmt::EnumDef { .. }
            | Stmt::DataDef { .. }
            | Stmt::TypestateDef { .. }
            | Stmt::UseDecl { .. }
            | Stmt::UseRaw { .. } => {}
        }
    }

    fn analyze_closure_expr(
        &self,
        expr: &Expr,
        outer_names: &HashSet<String>,
        local_scopes: &mut Vec<HashSet<String>>,
        captured: &mut HashSet<String>,
        mutated: &mut HashSet<String>,
        consumed: &mut HashSet<String>,
        tail_position: bool,
    ) {
        match expr {
            Expr::Ident(name, _) => {
                if outer_names.contains(name)
                    && !local_scopes.iter().rev().any(|scope| scope.contains(name))
                {
                    captured.insert(name.clone());
                    if tail_position && !self.is_state_var(name) {
                        consumed.insert(name.clone());
                    }
                }
            }
            Expr::Assign { name, value, .. } => {
                if outer_names.contains(name)
                    && !local_scopes.iter().rev().any(|scope| scope.contains(name))
                {
                    captured.insert(name.clone());
                    mutated.insert(name.clone());
                }
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::FieldAssign { object, value, .. } => {
                if let Expr::Ident(name, _) = object.as_ref() {
                    if outer_names.contains(name)
                        && !local_scopes.iter().rev().any(|scope| scope.contains(name))
                    {
                        captured.insert(name.clone());
                        mutated.insert(name.clone());
                    }
                }
                self.analyze_closure_expr(
                    object,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::Await { expr, .. } => {
                self.analyze_closure_expr(
                    expr,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::Block { stmts, tail, .. } => {
                local_scopes.push(HashSet::new());
                for stmt in stmts {
                    self.analyze_closure_stmt(
                        stmt,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                    );
                }
                if let Some(tail) = tail {
                    self.analyze_closure_expr(
                        tail,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        true,
                    );
                }
                local_scopes.pop();
            }
            Expr::Call { callee, args, .. } => {
                let drop_consume =
                    matches!(callee.as_ref(), Expr::Ident(name, _) if name == "drop");
                self.analyze_closure_expr(
                    callee,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                for arg in args {
                    self.analyze_closure_expr(
                        arg,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                    if drop_consume {
                        self.mark_consumed_idents(
                            arg,
                            outer_names,
                            local_scopes,
                            captured,
                            consumed,
                        );
                    }
                }
            }
            Expr::MethodCall {
                object,
                method,
                args,
                ..
            } => {
                self.analyze_closure_expr(
                    object,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                if method.starts_with("into_") {
                    self.mark_consumed_idents(
                        object,
                        outer_names,
                        local_scopes,
                        captured,
                        consumed,
                    );
                }
                for arg in args {
                    self.analyze_closure_expr(
                        arg,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                }
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.analyze_closure_expr(
                    cond,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    then_block,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    tail_position,
                );
                if let Some(other) = else_block {
                    self.analyze_closure_expr(
                        other,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        tail_position,
                    );
                }
            }
            Expr::While { cond, body, .. } => {
                self.analyze_closure_expr(
                    cond,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    body,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::For {
                iter, body, var, ..
            } => {
                self.analyze_closure_expr(
                    iter,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                local_scopes.push(HashSet::from([var.clone()]));
                self.analyze_closure_expr(
                    body,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                local_scopes.pop();
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.analyze_closure_expr(
                    scrutinee,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                for arm in arms {
                    self.analyze_closure_expr(
                        &arm.body,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        tail_position,
                    );
                }
            }
            Expr::Closure { params, body, .. } => {
                local_scopes.push(params.iter().cloned().collect());
                self.analyze_closure_expr(
                    body,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                local_scopes.pop();
            }
            Expr::BinOp { left, right, .. } => {
                self.analyze_closure_expr(
                    left,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    right,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::UnaryOp { operand, .. } | Expr::Question(operand, _) => self
                .analyze_closure_expr(
                    operand,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                ),
            Expr::Field { object, .. } => self.analyze_closure_expr(
                object,
                outer_names,
                local_scopes,
                captured,
                mutated,
                consumed,
                false,
            ),
            Expr::Index { object, index, .. } => {
                self.analyze_closure_expr(
                    object,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    index,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::StructInit { fields, .. } => {
                for (_, field_expr) in fields {
                    self.analyze_closure_expr(
                        field_expr,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                }
            }
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => {}
                EnumInitData::Tuple(items) => {
                    for item in items {
                        self.analyze_closure_expr(
                            item,
                            outer_names,
                            local_scopes,
                            captured,
                            mutated,
                            consumed,
                            false,
                        );
                    }
                }
                EnumInitData::Struct(fields) => {
                    for (_, field_expr) in fields {
                        self.analyze_closure_expr(
                            field_expr,
                            outer_names,
                            local_scopes,
                            captured,
                            mutated,
                            consumed,
                            false,
                        );
                    }
                }
            },
            Expr::Interpolation { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(inner) = part {
                        self.analyze_closure_expr(
                            inner,
                            outer_names,
                            local_scopes,
                            captured,
                            mutated,
                            consumed,
                            false,
                        );
                    }
                }
            }
            Expr::Range { start, end, .. } => {
                self.analyze_closure_expr(
                    start,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    end,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::List(items, _) => {
                for item in items {
                    self.analyze_closure_expr(
                        item,
                        outer_names,
                        local_scopes,
                        captured,
                        mutated,
                        consumed,
                        false,
                    );
                }
            }
            Expr::Literal(_, _) => {}
        }
    }

    fn mark_consumed_idents(
        &self,
        expr: &Expr,
        outer_names: &HashSet<String>,
        local_scopes: &[HashSet<String>],
        captured: &mut HashSet<String>,
        consumed: &mut HashSet<String>,
    ) {
        match expr {
            Expr::Ident(name, _) => {
                if outer_names.contains(name)
                    && !local_scopes.iter().rev().any(|scope| scope.contains(name))
                {
                    captured.insert(name.clone());
                    if !self.is_state_var(name) {
                        consumed.insert(name.clone());
                    }
                }
            }
            Expr::Block { tail, .. } => {
                if let Some(tail) = tail {
                    self.mark_consumed_idents(tail, outer_names, local_scopes, captured, consumed);
                }
            }
            _ => {}
        }
    }

    fn gen_interpolation(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt = String::new();
        let mut args = Vec::new();
        for part in parts {
            match part {
                InterpPart::Literal(s) => fmt.push_str(&s.replace('{', "{{").replace('}', "}}")),
                InterpPart::Expr(expr) => {
                    fmt.push_str("{}");
                    args.push(self.gen_expr(expr, false));
                }
            }
        }

        if args.is_empty() {
            format!("\"{}\".to_string()", fmt)
        } else {
            format!("format!(\"{}\", {})", fmt, args.join(", "))
        }
    }

    fn gen_list(&mut self, items: &[Expr]) -> String {
        if items.len() == 1 {
            if let Expr::Range {
                start,
                end,
                inclusive,
                ..
            } = &items[0]
            {
                let start = self.gen_expr(start, false);
                let end = self.gen_expr(end, false);
                let start = if start.parse::<i64>().is_ok() {
                    format!("{}_i64", start)
                } else {
                    start
                };
                return if *inclusive {
                    format!("({}..={}).collect::<Vec<_>>()", start, end)
                } else {
                    format!("({}..{}).collect::<Vec<_>>()", start, end)
                };
            }
        }

        if items.is_empty() {
            return "vec![]".to_string();
        }

        let values = items
            .iter()
            .enumerate()
            .map(|(idx, expr)| match (idx, expr) {
                (0, Expr::Literal(Literal::Int(n), _)) => format!("{}_i64", n),
                _ => self.gen_expr(expr, false),
            })
            .collect::<Vec<_>>();

        format!("vec![{}]", values.join(", "))
    }
}

fn sanitize_test_name(name: &str) -> String {
    let mut out = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if out.is_empty() {
        out.push_str("test");
    }
    if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        out.insert(0, '_');
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn when_condition_to_cfg(condition: &WhenCondition) -> String {
    match condition {
        WhenCondition::Platform(name) => format!("target_os = {:?}", name),
        WhenCondition::Feature(name) => format!("feature = {:?}", name),
        WhenCondition::Env(name) => match name.as_str() {
            "dev" => "debug_assertions".to_string(),
            "prod" => "not(debug_assertions)".to_string(),
            "test" => "test".to_string(),
            other => format!("feature = {:?}", other),
        },
        WhenCondition::Test => "test".to_string(),
        WhenCondition::Not(inner) => format!("not({})", when_condition_to_cfg(inner)),
    }
}

fn gen_literal(lit: &Literal) -> String {
    match lit {
        Literal::Int(n) => n.to_string(),
        Literal::Float(f) if f.fract() == 0.0 => format!("{:.1}", f),
        Literal::Float(f) => f.to_string(),
        Literal::String(s) => format!(
            "\"{}\".to_string()",
            s.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        Literal::Bool(b) => b.to_string(),
    }
}

fn gen_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Literal(lit) => gen_literal(lit),
        Pattern::Wildcard => "_".to_string(),
        Pattern::Ident(name) => name.clone(),
        Pattern::Some(inner) => format!("Some({})", gen_pattern(inner)),
        Pattern::None => "None".to_string(),
        Pattern::Ok(inner) => format!("Ok({})", gen_pattern(inner)),
        Pattern::Err(inner) => format!("Err({})", gen_pattern(inner)),
        Pattern::Range {
            start,
            end,
            inclusive,
        } => {
            if *inclusive {
                format!("{}..={}", gen_literal(start), gen_literal(end))
            } else {
                format!("{}..{}", gen_literal(start), gen_literal(end))
            }
        }
        Pattern::EnumUnit { enum_name, variant } => match enum_name {
            Some(enum_name) => format!("{}::{}", enum_name, variant),
            None => variant.clone(),
        },
        Pattern::EnumTuple {
            enum_name,
            variant,
            bindings,
        } => match enum_name {
            Some(enum_name) => format!("{}::{}({})", enum_name, variant, bindings.join(", ")),
            None => format!("{}({})", variant, bindings.join(", ")),
        },
        Pattern::EnumStruct {
            enum_name,
            variant,
            fields,
        } => match enum_name {
            Some(enum_name) => format!("{}::{} {{ {} }}", enum_name, variant, fields.join(", ")),
            None => format!("{} {{ {} }}", variant, fields.join(", ")),
        },
    }
}

pub fn type_ann_to_rust(ann: &TypeAnn) -> String {
    match ann {
        TypeAnn::Number => "i64".to_string(),
        TypeAnn::Float => "f64".to_string(),
        TypeAnn::String => "String".to_string(),
        TypeAnn::Bool => "bool".to_string(),
        TypeAnn::Option(inner) => format!("Option<{}>", type_ann_to_rust(inner)),
        TypeAnn::Result(inner) => format!("Result<{}, anyhow::Error>", type_ann_to_rust(inner)),
        TypeAnn::ResultWith(inner, err) => {
            format!(
                "Result<{}, {}>",
                type_ann_to_rust(inner),
                type_ann_to_rust(err)
            )
        }
        TypeAnn::List(inner) => format!("Vec<{}>", type_ann_to_rust(inner)),
        TypeAnn::Named(name) => name.clone(),
    }
}

fn binop_to_rust(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn binop_needs_parens(expr: &Expr) -> bool {
    matches!(expr, Expr::BinOp { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_compiler::parser::parse_source;

    fn transpile(src: &str) -> String {
        let module =
            parse_source(src).unwrap_or_else(|e| panic!("parse failed for input {:?}: {}", src, e));
        let mut gen = CodeGenerator::new();
        gen.generate_module(&module)
            .unwrap_or_else(|e| panic!("transpile failed for input {:?}: {}", src, e))
    }

    fn transpile_err(src: &str) -> String {
        let module =
            parse_source(src).unwrap_or_else(|e| panic!("parse failed for input {:?}: {}", src, e));
        let mut gen = CodeGenerator::new();
        gen.generate_module(&module)
            .err()
            .unwrap_or_else(|| panic!("transpile unexpectedly succeeded for input {:?}", src))
            .to_string()
    }

    #[test]
    fn let_binding() {
        let out = transpile("let x = 42");
        assert!(out.contains("let x = 42;"), "got: {}", out);
    }

    #[test]
    fn fn_definition() {
        let src = r#"
fn add(a: number, b: number) -> number {
    a + b
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("fn add(a: i64, b: i64) -> i64"),
            "got: {}",
            out
        );
        assert!(out.contains("a + b"), "got: {}", out);
    }

    #[test]
    fn option_result() {
        let src = r#"
fn safe_div(a: number, b: number) -> number! {
    if b == 0 { return err("division by zero") }
    ok(a / b)
}
"#;
        let out = transpile(src);
        assert!(out.contains("Result<i64, anyhow::Error>"), "got: {}", out);
        assert!(out.contains("Ok("), "got: {}", out);
        assert!(out.contains("Err("), "got: {}", out);
    }

    #[test]
    fn struct_and_impl() {
        let src = r#"
@derive(Debug, Clone, Accessor)
struct User {
    name: string
    age: number
}

impl User {
    fn label() -> string { self.name }
}
"#;
        let out = transpile(src);
        assert!(out.contains("struct User"), "got: {}", out);
        assert!(out.contains("impl User"), "got: {}", out);
        assert!(out.contains("pub fn label(&self)"), "got: {}", out);
        assert!(out.contains("pub fn get_name(&self)"), "got: {}", out);
    }

    #[test]
    fn derive_default_and_singleton() {
        let src = r#"
@derive(Default, Singleton)
struct AppConfig {
    port: number
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[derive(Default)]"), "got: {}", out);
        assert!(out.contains("once_cell::sync::Lazy"), "got: {}", out);
        assert!(
            out.contains("pub fn instance() -> &'static Self"),
            "got: {}",
            out
        );
    }

    #[test]
    fn enum_codegen() {
        let src = r#"
enum Shape {
    Circle(number)
    Move { x: number, y: number }
}

let s = Shape::Circle(3)
"#;
        let out = transpile(src);
        assert!(out.contains("enum Shape"), "got: {}", out);
        assert!(out.contains("Circle(i64)"), "got: {}", out);
        assert!(out.contains("Move { x: i64, y: i64 }"), "got: {}", out);
        assert!(out.contains("let s = Shape::Circle(3);"), "got: {}", out);
    }

    #[test]
    fn trait_and_mixin_codegen() {
        let src = r#"
trait Printable {
    fn display() -> string
}

mixin Walker {
    fn walk() -> string { self.name }
}

impl Printable for User {
    fn display() -> string { self.name }
}
"#;
        let out = transpile(src);
        assert!(out.contains("trait Printable"), "got: {}", out);
        assert!(out.contains("fn display(&self) -> String;"), "got: {}", out);
        assert!(out.contains("trait Walker"), "got: {}", out);
        assert!(out.contains("impl Printable for User"), "got: {}", out);
    }

    #[test]
    fn data_validate_codegen() {
        let src = r#"
data UserRegistration {
    username: string
} validate {
    username: length(3..20), alphanumeric
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("fn validate(&self) -> Result<(), String>"),
            "got: {}",
            out
        );
        assert!(out.contains("username: length"), "got: {}", out);
        assert!(out.contains("username: alphanumeric"), "got: {}", out);
    }

    #[test]
    fn use_when_and_test_codegen() {
        let src = r#"
use ./utils/helper.add
when platform.windows {
    fn os_name() -> string { "windows" }
}
test "add works" {
    assert_eq(1 + 1, 2)
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("use crate::utils::helper::add;"),
            "got: {}",
            out
        );
        assert!(
            out.contains("#[cfg(target_os = \"windows\")]"),
            "got: {}",
            out
        );
        assert!(out.contains("#[test]"), "got: {}", out);
        assert!(out.contains("assert_eq!("), "got: {}", out);
    }

    #[test]
    fn use_raw_codegen() {
        let src = r#"
use raw {
    let map = ::std::collections::HashMap::<String, i64>::new();
}
"#;
        let out = transpile(src);
        assert!(out.contains("HashMap"), "got: {}", out);
        assert!(out.contains("new()"), "got: {}", out);
    }

    #[test]
    fn struct_basic_snapshot() {
        let src = r#"
struct Point {
    x: number
    y: number
}
"#;
        let out = transpile(src);
        assert!(out.contains("struct Point"), "got: {}", out);
        assert!(out.contains("x: i64"), "got: {}", out);
        assert!(out.contains("y: i64"), "got: {}", out);
    }

    #[test]
    fn struct_impl_snapshot() {
        let src = r#"
struct Point {
    x: number
}

impl Point {
    fn x_value() -> number { self.x }
}
"#;
        let out = transpile(src);
        assert!(out.contains("impl Point"), "got: {}", out);
        assert!(out.contains("pub fn x_value(&self) -> i64"), "got: {}", out);
    }

    #[test]
    fn struct_derive_snapshot() {
        let src = r#"
@derive(Debug, Clone, Eq, Hash, Ord, Default, Accessor)
struct User {
    name: string
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[derive("), "got: {}", out);
        assert!(out.contains("Debug"), "got: {}", out);
        assert!(out.contains("Clone"), "got: {}", out);
        assert!(out.contains("PartialEq"), "got: {}", out);
        assert!(out.contains("Eq"), "got: {}", out);
        assert!(out.contains("Hash"), "got: {}", out);
        assert!(out.contains("PartialOrd"), "got: {}", out);
        assert!(out.contains("Ord"), "got: {}", out);
        assert!(out.contains("Default"), "got: {}", out);
        assert!(out.contains("pub fn get_name(&self)"), "got: {}", out);
    }

    #[test]
    fn data_basic_snapshot() {
        let src = r#"
data User {
    name: string
}
"#;
        let out = transpile(src);
        assert!(out.contains("struct User"), "got: {}", out);
        assert!(out.contains("Debug"), "got: {}", out);
        assert!(out.contains("Clone"), "got: {}", out);
        assert!(out.contains("serde::Serialize"), "got: {}", out);
        assert!(out.contains("serde::Deserialize"), "got: {}", out);
        assert!(out.contains("Hash"), "got: {}", out);
    }

    #[test]
    fn data_validate_snapshot() {
        let src = r#"
data UserRegistration {
    username: string
    email: string
    password: string
} validate {
    username: length(3..20), alphanumeric
    email: email_format
    password: not_empty, contains_digit, contains_uppercase
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("pub fn validate(&self) -> Result<(), String>"),
            "got: {}",
            out
        );
        assert!(out.contains("username: length"), "got: {}", out);
        assert!(out.contains("email: email_format"), "got: {}", out);
        assert!(out.contains("password: not_empty"), "got: {}", out);
        assert!(out.contains("password: contains_digit"), "got: {}", out);
        assert!(out.contains("password: contains_uppercase"), "got: {}", out);
    }

    #[test]
    fn enum_basic_snapshot() {
        let src = r#"
enum Direction {
    North
    South
}
"#;
        let out = transpile(src);
        assert!(out.contains("enum Direction"), "got: {}", out);
        assert!(out.contains("North"), "got: {}", out);
        assert!(out.contains("South"), "got: {}", out);
    }

    #[test]
    fn enum_match_snapshot() {
        let src = r#"
enum Direction {
    North
    South
}

fn label(d: Direction) -> string {
    match d {
        Direction::North => "n"
        Direction::South => "s"
    }
}
"#;
        let out = transpile(src);
        assert!(out.contains("match d"), "got: {}", out);
        assert!(out.contains("Direction::North =>"), "got: {}", out);
        assert!(out.contains("Direction::South =>"), "got: {}", out);
    }

    #[test]
    fn trait_impl_snapshot() {
        let src = r#"
trait Greeter {
    fn greet() -> string
}

impl Greeter for User {
    fn greet() -> string { "hi" }
}
"#;
        let out = transpile(src);
        assert!(out.contains("trait Greeter"), "got: {}", out);
        assert!(out.contains("impl Greeter for User"), "got: {}", out);
    }

    #[test]
    fn mixin_impl_snapshot() {
        let src = r#"
mixin Greeter {
    fn greet() -> string { "hi" }
}

impl Greeter for User {}
"#;
        let out = transpile(src);
        assert!(out.contains("trait Greeter"), "got: {}", out);
        assert!(out.contains("impl Greeter for User"), "got: {}", out);
    }

    #[test]
    fn use_local_snapshot() {
        let src = r#"
use ./utils/helper.{add, subtract as sub}
"#;
        let out = transpile(src);
        assert!(
            out.contains("use crate::utils::helper::{add, subtract as sub};"),
            "got: {}",
            out
        );
    }

    #[test]
    fn use_external_snapshot() {
        let src = r#"
use serde.{Serialize}
"#;
        let out = transpile(src);
        assert!(
            out.contains("use serde::{Serialize};") || out.contains("use serde::Serialize;"),
            "got: {}",
            out
        );
    }

    #[test]
    fn when_platform_snapshot() {
        let src = r#"
when platform.linux {
    fn os_name() -> string { "linux" }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("#[cfg(target_os = \"linux\")]"),
            "got: {}",
            out
        );
    }

    #[test]
    fn when_test_snapshot() {
        let src = r#"
when test {
    fn only_for_test() -> string { "ok" }
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[cfg(test)]"), "got: {}", out);
    }

    #[test]
    fn test_block_snapshot() {
        let src = r#"
test "math works" {
    assert_eq(1 + 1, 2)
    assert(true)
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[cfg(test)]"), "got: {}", out);
        assert!(out.contains("#[test]"), "got: {}", out);
        assert!(out.contains("assert_eq!("), "got: {}", out);
        assert!(out.contains("assert!("), "got: {}", out);
    }

    #[test]
    fn closure_fn() {
        let src = r#"
let factor = 2
let double = x => x * factor
"#;
        let out = transpile(src);
        assert!(out.contains("|x| x * factor"), "got: {}", out);
        assert!(!out.contains("move |x|"), "got: {}", out);
    }

    #[test]
    fn closure_fnmut() {
        let src = r#"
state total = 0
let add = x => {
    total = total + x
    total
}
"#;
        let out = transpile(src);
        assert!(out.contains("move |x|"), "got: {}", out);
        assert!(out.contains("total = total + x"), "got: {}", out);
    }

    #[test]
    fn closure_fnonce() {
        let src = r#"
let name = "Forge"
let into_owner = () => name
"#;
        let out = transpile(src);
        assert!(out.contains("move || name"), "got: {}", out);
    }

    #[test]
    fn async_basic() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(41) }
}

fn load() -> number! {
    fetch_num().await
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("async fn load() -> Result<i64, anyhow::Error>"),
            "got: {}",
            out
        );
        assert!(out.contains("fetch_num().await"), "got: {}", out);
    }

    #[test]
    fn async_propagation() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(41) }
}

fn load() -> number! {
    fetch_num().await
}

fn render() -> number! {
    load()
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("async fn render() -> Result<i64, anyhow::Error>"),
            "got: {}",
            out
        );
        assert!(out.contains("load().await"), "got: {}", out);
    }

    #[test]
    fn async_tokio_main() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(41) }
}

fn load() -> number! {
    fetch_num().await
}

print(load().await?)
"#;
        let out = transpile(src);
        assert!(out.contains("#[tokio::main]"), "got: {}", out);
        assert!(
            out.contains("async fn main() -> Result<(), anyhow::Error>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("print!(\"{}\", load().await?)")
                || out.contains("println!(\"{}\", load().await?)"),
            "got: {}",
            out
        );
    }

    #[test]
    fn async_recursive() {
        let src = r#"
use raw {
    async fn tick(n: i64) -> Result<i64, anyhow::Error> { Ok(n) }
}

fn countdown(n: number) -> number! {
    if n == 0 {
        ok(0)
    } else {
        let next = tick(n - 1).await?
        countdown(next)
    }
}
"#;
        let out = transpile(src);
        assert!(out.contains("fn countdown(n: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64, anyhow::Error>>>>"), "got: {}", out);
        assert!(out.contains("Box::pin(async move {"), "got: {}", out);
        assert!(out.contains("countdown(next).await"), "got: {}", out);
    }

    #[test]
    fn async_test_block() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(2) }
}

test "async works" {
    let value = fetch_num().await?
    assert_eq(value, 2)
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[tokio::test]"), "got: {}", out);
        assert!(
            out.contains("async fn async_works() -> Result<(), anyhow::Error>"),
            "got: {}",
            out
        );
        assert!(out.contains("fetch_num().await?"), "got: {}", out);
    }

    #[test]
    fn closure_await_error() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(1) }
}

let f = () => fetch_num().await
"#;
        let err = transpile_err(src);
        assert!(
            err.contains("クロージャ内での .await はサポートされていません"),
            "got: {}",
            err
        );
    }

    #[test]
    fn typestate_basic() {
        let src = r#"
typestate Connection {
    host: string
    states: [Disconnected, Connected]

    Disconnected {
        fn connect() -> Connected
    }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("use std::marker::PhantomData;"),
            "got: {}",
            out
        );
        assert!(out.contains("struct Disconnected;"), "got: {}", out);
        assert!(out.contains("struct Connected;"), "got: {}", out);
        assert!(out.contains("struct Connection<S>"), "got: {}", out);
        assert!(out.contains("_state: PhantomData<S>"), "got: {}", out);
        assert!(
            out.contains("impl Connection<Disconnected>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("pub fn new(host: String) -> Self"),
            "got: {}",
            out
        );
    }

    #[test]
    fn typestate_transitions() {
        let src = r#"
typestate Door {
    states: [Closed, Open, Locked]

    Closed {
        fn open() -> Open
        fn lock() -> Locked
    }

    Open {
        fn close() -> Closed
    }
}
"#;
        let out = transpile(src);
        assert!(out.contains("impl Door<Closed>"), "got: {}", out);
        assert!(
            out.contains("pub fn open(self) -> Door<Open>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("pub fn lock(self) -> Door<Locked>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("pub fn close(self) -> Door<Closed>"),
            "got: {}",
            out
        );
    }

    #[test]
    fn typestate_any_block() {
        let src = r#"
typestate Connection {
    host: string
    states: [Disconnected, Connected]

    any {
        fn host() -> string { self.host }
    }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("impl Connection<Disconnected>"),
            "got: {}",
            out
        );
        assert!(out.contains("impl Connection<Connected>"), "got: {}", out);
        assert!(out.contains("pub fn host(&self) -> String"), "got: {}", out);
        assert!(out.contains("self.host"), "got: {}", out);
    }

    #[test]
    fn typestate_constraint_unit_state_error() {
        let src = r#"
typestate Door {
    states: [Closed, Open(number)]
}
"#;
        let err = transpile_err(src);
        assert!(
            err.contains("typestate の状態は Unit 型のみサポートされます"),
            "got: {}",
            err
        );
    }

    #[test]
    fn typestate_constraint_generic_error() {
        let src = r#"
typestate Query<T> {
    states: [Init]
}
"#;
        let err = transpile_err(src);
        assert!(
            err.contains("ジェネリクス付き typestate は未サポートです"),
            "got: {}",
            err
        );
    }

    #[test]
    fn typestate_constraint_derive_error() {
        let src = r#"
@derive(Debug)
typestate Query {
    states: [Init]
}
"#;
        let err = transpile_err(src);
        assert!(
            err.contains("typestate への @derive は未サポートです"),
            "got: {}",
            err
        );
    }

    #[test]
    fn typestate_constraint_any_block_error() {
        let src = r#"
typestate Query {
    states: [Init]

    any {
        fn a() -> string { "a" }
    }

    any {
        fn b() -> string { "b" }
    }
}
"#;
        let err = transpile_err(src);
        assert!(
            err.contains("any ブロックは1つのみ定義できます"),
            "got: {}",
            err
        );
    }
}
