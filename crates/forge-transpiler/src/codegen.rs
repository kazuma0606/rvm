use std::collections::{BTreeSet, HashMap, HashSet};

use forge_compiler::ast::{
    BinOp, ChainKind, Constraint, DeferBody, EnumInitData, EnumVariant, Expr, FnDef, InterpPart,
    Literal, MatchArm, Module, OperatorDef, OperatorKind, Param, Pat, Pattern, PipelineStep, Stmt,
    TraitMethod, TypeAnn, TypestateMarker, UnaryOp, UsePath, UseSymbols, ValidateRule,
    WhenCondition,
};

use crate::builtin::{try_builtin_call, try_constructor_call};
use crate::error::TranspileError;

pub struct CodeGenerator {
    indent: usize,
    rename_main: bool,
    scopes: Vec<HashMap<String, VarInfo>>,
    generator_stack: Vec<String>,
    generator_counter: usize,
    defer_counter: usize,
    async_fns: HashSet<String>,
    recursive_async_fns: HashSet<String>,
    synthetic_main_async: bool,
    needs_tokio: bool,
    async_context_depth: usize,
    suppress_auto_await_depth: usize,
    needs_phantom_data: bool,
    needs_hashmap_use: bool,
    needs_hashset_use: bool,
    needs_ordering_use: bool,
    generic_type_params: HashMap<String, Vec<String>>,
    typestate_initial_states: HashMap<String, String>,
    typestate_state_names: HashMap<String, HashSet<String>>,
    fn_cleanup: HashMap<String, String>,
    pipeline_counter: usize,
    named_struct_defs: HashMap<String, Vec<(String, TypeAnn)>>,
    anon_struct_defs: HashMap<String, Vec<(String, TypeAnn)>>,
    /// `use forge/http` が宣言されているかどうか
    http_imported: bool,
}

#[derive(Clone, Default)]
struct VarInfo {
    is_state: bool,
    type_ann: Option<TypeAnn>,
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
            generator_stack: Vec::new(),
            generator_counter: 0,
            defer_counter: 0,
            async_fns: HashSet::new(),
            recursive_async_fns: HashSet::new(),
            synthetic_main_async: false,
            needs_tokio: false,
            async_context_depth: 0,
            suppress_auto_await_depth: 0,
            needs_phantom_data: false,
            needs_hashmap_use: false,
            needs_hashset_use: false,
            needs_ordering_use: false,
            generic_type_params: HashMap::new(),
            typestate_initial_states: HashMap::new(),
            typestate_state_names: HashMap::new(),
            fn_cleanup: HashMap::new(),
            pipeline_counter: 0,
            named_struct_defs: HashMap::new(),
            anon_struct_defs: HashMap::new(),
            http_imported: false,
        }
    }

    fn current_generator_buf(&self) -> Option<&str> {
        self.generator_stack.last().map(|name| name.as_str())
    }

    fn next_generator_buf(&mut self) -> String {
        let name = format!("__forge_generator_vals_{}", self.generator_counter);
        self.generator_counter += 1;
        name
    }

    fn next_defer_guard(&mut self) -> String {
        let name = format!("__forge_defer_guard_{}", self.defer_counter);
        self.defer_counter += 1;
        name
    }

    fn next_pipeline_name(&mut self, suffix: &str) -> String {
        let name = format!("__pipeline_{}_{}", suffix, self.pipeline_counter);
        self.pipeline_counter += 1;
        name
    }

    fn called_function_name<'a>(&self, expr: &'a Expr) -> Option<&'a str> {
        match expr {
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Ident(name, _) => Some(name.as_str()),
                _ => None,
            },
            Expr::Question(inner, _) => self.called_function_name(inner),
            Expr::Await { expr, .. } => self.called_function_name(expr),
            _ => None,
        }
    }

    fn maybe_generate_defer_guard(&mut self, var: &str, value: &Expr) -> Option<String> {
        if let Some(fn_name) = self.called_function_name(value) {
            let method = self.fn_cleanup.get(fn_name).cloned();
            if let Some(method) = method {
                let guard_name = self.next_defer_guard();
                return Some(format!(
                    "{}let {guard} = scopeguard::defer(|| {{ {var}.{method}(); }});\n",
                    self.indent_str(),
                    guard = guard_name,
                    var = var,
                    method = method,
                ));
            }
        }
        None
    }

    fn operator_helper_name(op: &OperatorKind) -> &'static str {
        match op {
            OperatorKind::Add => "__forge_operator_add",
            OperatorKind::Sub => "__forge_operator_sub",
            OperatorKind::Mul => "__forge_operator_mul",
            OperatorKind::Div => "__forge_operator_div",
            OperatorKind::Rem => "__forge_operator_rem",
            OperatorKind::Eq => "__forge_operator_eq",
            OperatorKind::Lt => "__forge_operator_lt",
            OperatorKind::Index => "__forge_operator_index",
            OperatorKind::Neg => "__forge_operator_neg",
        }
    }

    fn operator_param_type_string(param: &Param) -> String {
        param
            .type_ann
            .as_ref()
            .map(type_ann_to_rust)
            .unwrap_or_else(|| "_".to_string())
    }

    fn operator_return_type_string(&self, operator: &OperatorDef, target_base: &str) -> String {
        operator
            .return_type
            .as_ref()
            .map(type_ann_to_rust)
            .unwrap_or_else(|| target_base.to_string())
    }

    fn gen_operator_helper(
        &mut self,
        operator: &OperatorDef,
        _target_impl: &str,
        target_base: &str,
    ) -> Option<String> {
        let helper_name = Self::operator_helper_name(&operator.op);
        match operator.op {
            OperatorKind::Add
            | OperatorKind::Sub
            | OperatorKind::Mul
            | OperatorKind::Div
            | OperatorKind::Rem => {
                self.gen_binary_operator_helper(operator, helper_name, target_base)
            }
            OperatorKind::Neg => {
                Some(self.gen_unary_operator_helper(operator, helper_name, target_base))
            }
            OperatorKind::Eq | OperatorKind::Lt => {
                self.gen_reference_operator_helper(operator, helper_name, target_base)
            }
            OperatorKind::Index => {
                self.gen_index_operator_helper(operator, helper_name, target_base)
            }
        }
    }

    fn gen_binary_operator_helper(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let return_type = Self::operator_return_type_string(self, operator, target_base);
        let self_binding = if operator.has_state_self {
            "mut self"
        } else {
            "self"
        };
        let params = vec![(param, format!("{}: {}", param.name, param_type))];
        Some(self.gen_operator_helper_method(
            helper_name,
            operator,
            self_binding,
            &params,
            Some(return_type),
        ))
    }

    fn gen_unary_operator_helper(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        target_base: &str,
    ) -> String {
        let return_type = Self::operator_return_type_string(self, operator, target_base);
        let self_binding = if operator.has_state_self {
            "mut self"
        } else {
            "self"
        };
        self.gen_operator_helper_method(helper_name, operator, self_binding, &[], Some(return_type))
    }

    fn gen_reference_operator_helper(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        _target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let return_type = "bool".to_string();
        let params = vec![(param, format!("{}: &{}", param.name, param_type))];
        Some(self.gen_operator_helper_method(
            helper_name,
            operator,
            "&self",
            &params,
            Some(return_type),
        ))
    }

    fn gen_index_operator_helper(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let return_type = Self::operator_return_type_string(self, operator, target_base);
        let params = vec![(param, format!("{}: {}", param.name, param_type))];
        Some(self.gen_operator_helper_method(
            helper_name,
            operator,
            "&self",
            &params,
            Some(format!("&{}", return_type)),
        ))
    }

    fn gen_operator_helper_method(
        &mut self,
        helper_name: &str,
        operator: &OperatorDef,
        self_binding: &str,
        params: &[(&Param, String)],
        return_type: Option<String>,
    ) -> String {
        let mut param_names = Vec::new();
        param_names.push(self_binding.to_string());
        param_names.extend(params.iter().map(|(_, decl)| decl.clone()));
        let params_str = param_names.join(", ");
        let ret_decl = return_type
            .map(|ret| format!(" -> {}", ret))
            .unwrap_or_default();
        let mut out = format!(
            "{}fn {}({}){} {{\n",
            self.indent_str(),
            helper_name,
            params_str,
            ret_decl
        );
        self.push_scope();
        self.declare_var("self", operator.has_state_self, None);
        for (param, _) in params {
            self.declare_var(&param.name, false, param.type_ann.clone());
        }
        out.push_str(&self.gen_block_body(&operator.body, false));
        self.pop_scope();
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_operator_trait_impl(
        &mut self,
        operator: &OperatorDef,
        impl_generics: &str,
        target_impl: &str,
        target_base: &str,
    ) -> Option<String> {
        let helper_name = Self::operator_helper_name(&operator.op);
        match operator.op {
            OperatorKind::Add => self.gen_binary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Add",
                "add",
            ),
            OperatorKind::Sub => self.gen_binary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Sub",
                "sub",
            ),
            OperatorKind::Mul => self.gen_binary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Mul",
                "mul",
            ),
            OperatorKind::Div => self.gen_binary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Div",
                "div",
            ),
            OperatorKind::Rem => self.gen_binary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Rem",
                "rem",
            ),
            OperatorKind::Neg => self.gen_unary_operator_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
                "std::ops::Neg",
                "neg",
            ),
            OperatorKind::Eq => self.gen_partial_eq_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
            ),
            OperatorKind::Lt => self.gen_partial_ord_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
            ),
            OperatorKind::Index => self.gen_index_trait_impl(
                operator,
                helper_name,
                impl_generics,
                target_impl,
                target_base,
            ),
        }
    }

    fn gen_binary_operator_trait_impl(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        impl_generics: &str,
        target_impl: &str,
        target_base: &str,
        trait_path: &str,
        method_name: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let output_type = Self::operator_return_type_string(self, operator, target_base);
        let base_indent = self.indent;
        let mut out = format!(
            "{}impl{} {} for {} {{\n",
            self.indent_str(),
            impl_generics,
            trait_path,
            target_impl
        );
        self.indent += 1;
        out.push_str(&format!(
            "{}type Output = {};\n",
            self.indent_str(),
            output_type
        ));
        out.push_str(&self.gen_operator_trait_method_call(
            method_name,
            helper_name,
            operator,
            if operator.has_state_self {
                "mut self"
            } else {
                "self"
            },
            &[format!("{}: {}", param.name, param_type)],
            &format!(" -> {}", output_type),
        ));
        self.indent = base_indent;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        Some(out)
    }

    fn gen_unary_operator_trait_impl(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        impl_generics: &str,
        target_impl: &str,
        target_base: &str,
        trait_path: &str,
        method_name: &str,
    ) -> Option<String> {
        let output_type = Self::operator_return_type_string(self, operator, target_base);
        let base_indent = self.indent;
        let mut out = format!(
            "{}impl{} {} for {} {{\n",
            self.indent_str(),
            impl_generics,
            trait_path,
            target_impl
        );
        self.indent += 1;
        out.push_str(&format!(
            "{}type Output = {};\n",
            self.indent_str(),
            output_type
        ));
        out.push_str(&self.gen_operator_trait_method_call(
            method_name,
            helper_name,
            operator,
            if operator.has_state_self {
                "mut self"
            } else {
                "self"
            },
            &[],
            &format!(" -> {}", output_type),
        ));
        self.indent = base_indent;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        Some(out)
    }

    fn gen_partial_eq_trait_impl(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        impl_generics: &str,
        target_impl: &str,
        _target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let base_indent = self.indent;
        let mut out = format!(
            "{}impl{} PartialEq for {} {{\n",
            self.indent_str(),
            impl_generics,
            target_impl
        );
        self.indent += 1;
        out.push_str(&self.gen_operator_trait_method_call(
            "eq",
            helper_name,
            operator,
            "&self",
            &[format!("{}: &{}", param.name, param_type)],
            " -> bool",
        ));
        self.indent = base_indent;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        Some(out)
    }

    fn gen_partial_ord_trait_impl(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        impl_generics: &str,
        target_impl: &str,
        _target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        self.needs_ordering_use = true;
        let base_indent = self.indent;
        let mut out = format!(
            "{}impl{} PartialOrd for {} {{\n",
            self.indent_str(),
            impl_generics,
            target_impl
        );
        self.indent += 1;
        out.push_str(&self.gen_partial_ord_method(
            helper_name,
            operator,
            &format!("{}: &{}", param.name, param_type),
        ));
        self.indent = base_indent;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        Some(out)
    }

    fn gen_index_trait_impl(
        &mut self,
        operator: &OperatorDef,
        helper_name: &str,
        impl_generics: &str,
        target_impl: &str,
        target_base: &str,
    ) -> Option<String> {
        let param = operator.params.get(0)?;
        let param_type = Self::operator_param_type_string(param);
        let return_type = Self::operator_return_type_string(self, operator, target_base);
        let trait_path = format!("std::ops::Index<{}>", param_type);
        let base_indent = self.indent;
        let mut out = format!(
            "{}impl{} {} for {} {{\n",
            self.indent_str(),
            impl_generics,
            trait_path,
            target_impl
        );
        self.indent += 1;
        out.push_str(&format!(
            "{}type Output = {};\n",
            self.indent_str(),
            return_type
        ));
        out.push_str(&self.gen_operator_trait_method_call(
            "index",
            helper_name,
            operator,
            "&self",
            &[format!("{}: {}", param.name, param_type)],
            " -> &Self::Output",
        ));
        self.indent = base_indent;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        Some(out)
    }

    fn gen_partial_ord_method(
        &mut self,
        helper_name: &str,
        operator: &OperatorDef,
        param_decl: &str,
    ) -> String {
        let param_name = operator
            .params
            .get(0)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "other".to_string());
        let mut out = format!(
            "{}fn partial_cmp(&self, {}) -> Option<Ordering> {{\n",
            self.indent_str(),
            param_decl
        );
        self.indent += 1;
        out.push_str(&format!(
            "{}if self.{}({}) {{\n",
            self.indent_str(),
            helper_name,
            param_name
        ));
        self.indent += 1;
        out.push_str(&format!("{}Some(Ordering::Less)\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!(
            "{}}} else if {}.{}(self) {{\n",
            self.indent_str(),
            param_name,
            helper_name
        ));
        self.indent += 1;
        out.push_str(&format!("{}Some(Ordering::Greater)\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}} else {{\n", self.indent_str()));
        self.indent += 1;
        out.push_str(&format!("{}Some(Ordering::Equal)\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    fn gen_operator_trait_method_call(
        &mut self,
        method_name: &str,
        helper_name: &str,
        operator: &OperatorDef,
        self_binding: &str,
        param_decls: &[String],
        ret_decl: &str,
    ) -> String {
        let mut param_names = Vec::new();
        param_names.push(self_binding.to_string());
        param_names.extend(param_decls.iter().cloned());
        let params_str = param_names.join(", ");
        let mut out = format!(
            "{}fn {}({}){} {{\n",
            self.indent_str(),
            method_name,
            params_str,
            ret_decl
        );
        self.indent += 1;
        let args = operator
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "{}self.{}({});\n",
            self.indent_str(),
            helper_name,
            args
        ));
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));
        out
    }

    pub fn generate_module(&mut self, module: &Module) -> Result<String, TranspileError> {
        self.fn_cleanup.clear();
        self.defer_counter = 0;
        self.pipeline_counter = 0;
        self.analyze_http(module);
        self.analyze_async(module)?;
        self.analyze_typestates(module)?;
        self.analyze_generic_types(module);
        self.analyze_collections_and_utility_types(module);
        self.named_struct_defs = collect_struct_defs(module);
        self.anon_struct_defs = collect_anon_structs(module, &self.named_struct_defs);
        self.rename_main = module
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Fn { name, .. } if name == "main"));

        let mut out = String::new();
        if self.needs_hashmap_use || self.needs_hashset_use {
            let mut imports = Vec::new();
            if self.needs_hashmap_use {
                imports.push("HashMap");
            }
            if self.needs_hashset_use {
                imports.push("HashSet");
            }
            out.push_str(&format!(
                "use std::collections::{{{}}};\n\n",
                imports.join(", ")
            ));
        }
        if self.needs_phantom_data {
            out.push_str("use std::marker::PhantomData;\n\n");
        }
        if self.needs_ordering_use {
            out.push_str("use std::cmp::Ordering;\n\n");
        }

        out.push_str(&self.gen_anon_structs());
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&self.gen_utility_structs(module));
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }

        let mut top_level: Vec<&Stmt> = Vec::new();
        let mut main_body: Vec<&Stmt> = Vec::new();

        for stmt in &module.stmts {
            match stmt {
                Stmt::Let { .. }
                | Stmt::State { .. }
                | Stmt::Expr(..)
                | Stmt::Return(..)
                | Stmt::Defer { .. } => main_body.push(stmt),
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

    fn analyze_collections_and_utility_types(&mut self, module: &Module) {
        self.needs_hashmap_use = false;
        self.needs_hashset_use = false;
        for stmt in &module.stmts {
            self.scan_stmt_codegen_requirements(stmt);
        }
    }

    fn analyze_generic_types(&mut self, module: &Module) {
        self.generic_type_params.clear();
        for stmt in &module.stmts {
            match stmt {
                Stmt::StructDef {
                    name,
                    generic_params,
                    ..
                }
                | Stmt::DataDef {
                    name,
                    generic_params,
                    ..
                } if !generic_params.is_empty() => {
                    self.generic_type_params
                        .insert(name.clone(), generic_params.clone());
                    self.needs_phantom_data = true;
                }
                _ => {}
            }
        }
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    /// `use forge/http` が宣言されているかどうかを検出する
    fn analyze_http(&mut self, module: &Module) {
        self.http_imported = module.stmts.iter().any(|stmt| {
            matches!(
                stmt,
                Stmt::UseDecl {
                    path: UsePath::External(p),
                    ..
                } if p == "forge/http"
            )
        });
    }

    fn analyze_async(&mut self, module: &Module) -> Result<(), TranspileError> {
        self.async_fns.clear();
        self.recursive_async_fns.clear();
        self.synthetic_main_async = false;
        self.needs_tokio = false;

        let mut fn_bodies = HashMap::new();
        let mut called_fns: HashMap<String, HashSet<String>> = HashMap::new();

        for stmt in &module.stmts {
            if let Stmt::Fn {
                name,
                body,
                defer_cleanup,
                ..
            } = stmt
            {
                self.ensure_no_await_in_closure(body)?;
                if self.expr_contains_await(body) {
                    self.async_fns.insert(name.clone());
                }
                fn_bodies.insert(name.clone(), body.as_ref().clone());
                called_fns.insert(name.clone(), self.collect_called_fns(body));
                if let Some(cleanup) = defer_cleanup {
                    self.fn_cleanup.insert(name.clone(), cleanup.clone());
                }
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
                    Stmt::Let { .. }
                    | Stmt::State { .. }
                    | Stmt::Expr(..)
                    | Stmt::Return(..)
                    | Stmt::Defer { .. } => Some(stmt.clone()),
                    _ => None,
                })
                .collect(),
            tail: None,
            span: forge_compiler::lexer::Span {
                file: "<transpiler>".to_string(),
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

    fn scan_stmt_codegen_requirements(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                type_ann, value, ..
            }
            | Stmt::State {
                type_ann, value, ..
            }
            | Stmt::Const {
                type_ann, value, ..
            } => {
                if let Some(ann) = type_ann {
                    self.scan_type_ann_codegen_requirements(ann);
                }
                self.scan_expr_codegen_requirements(value);
            }
            Stmt::Fn {
                params,
                return_type,
                body,
                ..
            } => {
                for p in params {
                    if let Some(ann) = &p.type_ann {
                        self.scan_type_ann_codegen_requirements(ann);
                    }
                }
                if let Some(ann) = return_type {
                    self.scan_type_ann_codegen_requirements(ann);
                }
                self.scan_expr_codegen_requirements(body);
            }
            Stmt::Yield { value, .. } => {
                self.scan_expr_codegen_requirements(value);
            }
            Stmt::Defer { body, .. } => match body {
                DeferBody::Expr(expr) | DeferBody::Block(expr) => {
                    self.scan_expr_codegen_requirements(expr)
                }
            },
            Stmt::Return(Some(expr), _) | Stmt::Expr(expr) => {
                self.scan_expr_codegen_requirements(expr)
            }
            Stmt::Return(None, _) => {}
            Stmt::StructDef { fields, .. } | Stmt::DataDef { fields, .. } => {
                for (_, ann) in fields {
                    self.scan_type_ann_codegen_requirements(ann);
                }
            }
            Stmt::EnumDef { variants, .. } => {
                for variant in variants {
                    match variant {
                        EnumVariant::Unit(_) => {}
                        EnumVariant::Tuple(_, tys) => {
                            for ann in tys {
                                self.scan_type_ann_codegen_requirements(ann);
                            }
                        }
                        EnumVariant::Struct(_, fields) => {
                            for (_, ann) in fields {
                                self.scan_type_ann_codegen_requirements(ann);
                            }
                        }
                    }
                }
            }
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
                for method in methods {
                    for p in &method.params {
                        if let Some(ann) = &p.type_ann {
                            self.scan_type_ann_codegen_requirements(ann);
                        }
                    }
                    if let Some(ann) = &method.return_type {
                        self.scan_type_ann_codegen_requirements(ann);
                    }
                    self.scan_expr_codegen_requirements(&method.body);
                }
                for operator in operators {
                    for param in &operator.params {
                        if let Some(ann) = &param.type_ann {
                            self.scan_type_ann_codegen_requirements(ann);
                        }
                    }
                    if let Some(ann) = &operator.return_type {
                        self.scan_type_ann_codegen_requirements(ann);
                    }
                    self.scan_expr_codegen_requirements(&operator.body);
                }
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
                for method in methods {
                    for p in &method.params {
                        if let Some(ann) = &p.type_ann {
                            self.scan_type_ann_codegen_requirements(ann);
                        }
                    }
                    if let Some(ann) = &method.return_type {
                        self.scan_type_ann_codegen_requirements(ann);
                    }
                    self.scan_expr_codegen_requirements(&method.body);
                }
            }
            Stmt::TraitDef { methods, .. } => {
                for method in methods {
                    match method {
                        TraitMethod::Abstract {
                            params,
                            return_type,
                            ..
                        } => {
                            for p in params {
                                if let Some(ann) = &p.type_ann {
                                    self.scan_type_ann_codegen_requirements(ann);
                                }
                            }
                            if let Some(ann) = return_type {
                                self.scan_type_ann_codegen_requirements(ann);
                            }
                        }
                        TraitMethod::Default {
                            params,
                            return_type,
                            body,
                            ..
                        } => {
                            for p in params {
                                if let Some(ann) = &p.type_ann {
                                    self.scan_type_ann_codegen_requirements(ann);
                                }
                            }
                            if let Some(ann) = return_type {
                                self.scan_type_ann_codegen_requirements(ann);
                            }
                            self.scan_expr_codegen_requirements(body);
                        }
                    }
                }
            }
            Stmt::TypestateDef {
                fields,
                any_methods,
                state_methods,
                ..
            } => {
                for (_, ann) in fields {
                    self.scan_type_ann_codegen_requirements(ann);
                }
                for method in any_methods {
                    self.scan_expr_codegen_requirements(&method.body);
                }
                for state in state_methods {
                    for method in &state.methods {
                        self.scan_expr_codegen_requirements(&method.body);
                    }
                }
            }
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for inner in body {
                    self.scan_stmt_codegen_requirements(inner);
                }
            }
            Stmt::UseDecl { .. } | Stmt::UseRaw { .. } => {}
        }
    }

    fn scan_expr_codegen_requirements(&mut self, expr: &Expr) {
        match expr {
            Expr::MapLiteral { pairs, .. } => {
                self.needs_hashmap_use = true;
                for (k, v) in pairs {
                    self.scan_expr_codegen_requirements(k);
                    self.scan_expr_codegen_requirements(v);
                }
            }
            Expr::SetLiteral { items, .. } => {
                self.needs_hashset_use = true;
                for item in items {
                    self.scan_expr_codegen_requirements(item);
                }
            }
            Expr::AnonStruct { fields, .. } => {
                for (_, expr) in fields {
                    if let Some(expr) = expr {
                        self.scan_expr_codegen_requirements(expr);
                    }
                }
            }
            Expr::BinOp { left, right, .. } => {
                self.scan_expr_codegen_requirements(left);
                self.scan_expr_codegen_requirements(right);
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Question(operand, _)
            | Expr::Await { expr: operand, .. }
            | Expr::Field {
                object: operand, ..
            } => self.scan_expr_codegen_requirements(operand),
            Expr::OptionalChain { object, chain, .. } => {
                self.scan_expr_codegen_requirements(object);
                if let ChainKind::Method { args, .. } = chain {
                    for arg in args {
                        self.scan_expr_codegen_requirements(arg);
                    }
                }
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.scan_expr_codegen_requirements(value);
                self.scan_expr_codegen_requirements(default);
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.scan_expr_codegen_requirements(cond);
                self.scan_expr_codegen_requirements(then_block);
                if let Some(other) = else_block {
                    self.scan_expr_codegen_requirements(other);
                }
            }
            Expr::While { cond, body, .. }
            | Expr::For {
                iter: cond, body, ..
            } => {
                self.scan_expr_codegen_requirements(cond);
                self.scan_expr_codegen_requirements(body);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.scan_expr_codegen_requirements(scrutinee);
                for arm in arms {
                    self.scan_expr_codegen_requirements(&arm.body);
                }
            }
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    self.scan_stmt_codegen_requirements(stmt);
                }
                if let Some(tail) = tail {
                    self.scan_expr_codegen_requirements(tail);
                }
            }
            Expr::Spawn { body, .. } => {
                self.scan_expr_codegen_requirements(body);
            }
            Expr::Call { callee, args, .. } => {
                self.scan_expr_codegen_requirements(callee);
                for arg in args {
                    self.scan_expr_codegen_requirements(arg);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.scan_expr_codegen_requirements(object);
                for arg in args {
                    self.scan_expr_codegen_requirements(arg);
                }
            }
            Expr::Index { object, index, .. } => {
                self.scan_expr_codegen_requirements(object);
                self.scan_expr_codegen_requirements(index);
            }
            Expr::Assign { value, .. } => self.scan_expr_codegen_requirements(value),
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.scan_expr_codegen_requirements(object);
                self.scan_expr_codegen_requirements(index);
                self.scan_expr_codegen_requirements(value);
            }
            Expr::StructInit { fields, .. } => {
                for (_, expr) in fields {
                    self.scan_expr_codegen_requirements(expr);
                }
            }
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => {}
                EnumInitData::Tuple(items) => {
                    for item in items {
                        self.scan_expr_codegen_requirements(item);
                    }
                }
                EnumInitData::Struct(fields) => {
                    for (_, expr) in fields {
                        self.scan_expr_codegen_requirements(expr);
                    }
                }
            },
            Expr::FieldAssign { object, value, .. } => {
                self.scan_expr_codegen_requirements(object);
                self.scan_expr_codegen_requirements(value);
            }
            Expr::Interpolation { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(expr) = part {
                        self.scan_expr_codegen_requirements(expr);
                    }
                }
            }
            Expr::Range { start, end, .. } => {
                self.scan_expr_codegen_requirements(start);
                self.scan_expr_codegen_requirements(end);
            }
            Expr::List(items, _) => {
                for item in items {
                    self.scan_expr_codegen_requirements(item);
                }
            }
            Expr::Literal(_, _) | Expr::Ident(_, _) | Expr::Closure { .. } => {}
            Expr::Pipeline { .. } => {}
            Expr::Loop { body, .. } => self.scan_expr_codegen_requirements(body),
            Expr::Break { .. } => {}
        }
    }

    fn scan_type_ann_codegen_requirements(&mut self, ann: &TypeAnn) {
        match ann {
            TypeAnn::Option(inner)
            | TypeAnn::Result(inner)
            | TypeAnn::List(inner)
            | TypeAnn::Generate(inner) => self.scan_type_ann_codegen_requirements(inner),
            TypeAnn::Set(inner) => {
                self.needs_hashset_use = true;
                self.scan_type_ann_codegen_requirements(inner);
            }
            TypeAnn::OrderedSet(inner) => self.scan_type_ann_codegen_requirements(inner),
            TypeAnn::ResultWith(ok, err) | TypeAnn::OrderedMap(ok, err) => {
                self.scan_type_ann_codegen_requirements(ok);
                self.scan_type_ann_codegen_requirements(err);
            }
            TypeAnn::Map(ok, err) => {
                self.needs_hashmap_use = true;
                self.scan_type_ann_codegen_requirements(ok);
                self.scan_type_ann_codegen_requirements(err);
            }
            TypeAnn::Generic { name, args } => {
                if name == "Record" {
                    self.needs_hashmap_use = true;
                }
                for arg in args {
                    self.scan_type_ann_codegen_requirements(arg);
                }
            }
            TypeAnn::Fn {
                params,
                return_type,
            } => {
                for param in params {
                    self.scan_type_ann_codegen_requirements(param);
                }
                self.scan_type_ann_codegen_requirements(return_type);
            }
            TypeAnn::AnonStruct(fields) => {
                for (_, ann) in fields {
                    self.scan_type_ann_codegen_requirements(ann);
                }
            }
            TypeAnn::Number
            | TypeAnn::Float
            | TypeAnn::String
            | TypeAnn::Bool
            | TypeAnn::Named(_)
            | TypeAnn::Unit
            | TypeAnn::StringLiteralUnion(_) => {}
        }
    }

    fn gen_utility_structs(&self, module: &Module) -> String {
        let struct_map = collect_struct_defs(module);
        let used = collect_utility_types(module);
        let mut seen: HashSet<UtilitySpec> = HashSet::new();
        let mut out = String::new();

        for util in used {
            if !seen.insert(util.clone()) {
                continue;
            }
            if let Some(rendered) = render_utility_struct(&util, &struct_map) {
                out.push_str(&rendered);
                out.push('\n');
            }
        }

        out
    }

    fn gen_anon_structs(&self) -> String {
        let mut defs = self
            .anon_struct_defs
            .iter()
            .map(|(name, fields)| (name.clone(), fields.clone()))
            .collect::<Vec<_>>();
        defs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = String::new();
        for (name, fields) in defs {
            out.push_str(&render_plain_struct(&name, &fields));
            out.push('\n');
        }
        out
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
            Stmt::Yield { value, .. } => self.ensure_no_await_in_closure(value),
            Stmt::Defer { body, .. } => match body {
                DeferBody::Expr(expr) | DeferBody::Block(expr) => {
                    self.ensure_no_await_in_closure(expr)
                }
            },
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
                for method in methods {
                    self.ensure_no_await_in_closure(&method.body)?;
                }
                for operator in operators {
                    self.ensure_no_await_in_closure(&operator.body)?;
                }
                Ok(())
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
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
            Expr::Closure { body, .. } => self.ensure_no_await_in_closure(body),
            Expr::AnonStruct { fields, .. } => {
                for (_, expr) in fields {
                    if let Some(expr) = expr {
                        self.ensure_no_await_in_closure(expr)?;
                    }
                }
                Ok(())
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
            Expr::Spawn { body, .. } => self.ensure_no_await_in_closure(body),
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
            Expr::OptionalChain { object, chain, .. } => {
                self.ensure_no_await_in_closure(object)?;
                if let ChainKind::Method { args, .. } = chain {
                    for arg in args {
                        self.ensure_no_await_in_closure(arg)?;
                    }
                }
                Ok(())
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.ensure_no_await_in_closure(value)?;
                self.ensure_no_await_in_closure(default)
            }
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
            Expr::MapLiteral { pairs, .. } => {
                for (key, value) in pairs {
                    self.ensure_no_await_in_closure(key)?;
                    self.ensure_no_await_in_closure(value)?;
                }
                Ok(())
            }
            Expr::SetLiteral { items, .. } => {
                for item in items {
                    self.ensure_no_await_in_closure(item)?;
                }
                Ok(())
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.ensure_no_await_in_closure(object)?;
                self.ensure_no_await_in_closure(index)?;
                self.ensure_no_await_in_closure(value)
            }
            Expr::Literal(_, _) | Expr::Ident(_, _) => Ok(()),
            Expr::Pipeline { .. } => Ok(()),
            Expr::Loop { .. } | Expr::Break { .. } => Ok(()),
        }
    }

    fn expr_contains_await(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Await { .. } => true,
            Expr::Spawn { .. } => false,
            Expr::AnonStruct { fields, .. } => fields
                .iter()
                .filter_map(|(_, expr)| expr.as_ref())
                .any(|expr| self.expr_contains_await(expr)),
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
            Expr::MethodCall {
                object,
                method,
                args,
                ..
            } => {
                // forge/http の .send() / .text() / .json() / .bytes() は async
                if self.http_imported
                    && matches!(method.as_str(), "send" | "text" | "bytes")
                    && args.is_empty()
                {
                    return true;
                }
                // .json() 引数なし = レスポンスから JSON を取得 → async
                if self.http_imported && method == "json" && args.is_empty() {
                    return true;
                }
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
            Expr::MapLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
                self.expr_contains_await(key) || self.expr_contains_await(value)
            }),
            Expr::SetLiteral { items, .. } => {
                items.iter().any(|item| self.expr_contains_await(item))
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.expr_contains_await(object)
                    || self.expr_contains_await(index)
                    || self.expr_contains_await(value)
            }
            Expr::OptionalChain { object, chain, .. } => {
                self.expr_contains_await(object)
                    || matches!(
                        chain,
                        ChainKind::Method { args, .. }
                            if args.iter().any(|arg| self.expr_contains_await(arg))
                    )
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.expr_contains_await(value) || self.expr_contains_await(default)
            }
            Expr::Literal(_, _) | Expr::Ident(_, _) => false,
            Expr::Pipeline { .. } => false,
            Expr::Loop { .. } | Expr::Break { .. } => false,
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
            Stmt::Yield { value, .. } => self.expr_contains_await(value),
            Stmt::Defer { body, .. } => match body {
                DeferBody::Expr(expr) | DeferBody::Block(expr) => self.expr_contains_await(expr),
            },
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
                methods
                    .iter()
                    .any(|method| self.expr_contains_await(&method.body))
                    || operators
                        .iter()
                        .any(|operator| self.expr_contains_await(&operator.body))
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => methods
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
            Expr::MapLiteral { pairs, .. } => {
                for (key, value) in pairs {
                    self.collect_called_fns_expr(key, names);
                    self.collect_called_fns_expr(value, names);
                }
            }
            Expr::SetLiteral { items, .. } => {
                for item in items {
                    self.collect_called_fns_expr(item, names);
                }
            }
            Expr::OptionalChain { object, chain, .. } => {
                self.collect_called_fns_expr(object, names);
                if let ChainKind::Method { args, .. } = chain {
                    for arg in args {
                        self.collect_called_fns_expr(arg, names);
                    }
                }
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.collect_called_fns_expr(value, names);
                self.collect_called_fns_expr(default, names);
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.collect_called_fns_expr(object, names);
                self.collect_called_fns_expr(index, names);
                self.collect_called_fns_expr(value, names);
            }
            _ => {}
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
            Stmt::Yield { value, .. } => self.collect_called_fns_expr(value, names),
            Stmt::Defer { body, .. } => match body {
                DeferBody::Expr(expr) | DeferBody::Block(expr) => {
                    self.collect_called_fns_expr(expr, names)
                }
            },
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for inner in body {
                    self.collect_called_fns_stmt(inner, names);
                }
            }
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
                for method in methods {
                    self.collect_called_fns_expr(&method.body, names);
                }
                for operator in operators {
                    self.collect_called_fns_expr(&operator.body, names);
                }
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
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
            Expr::Spawn { .. } => false,
            Expr::AnonStruct { fields, .. } => fields
                .iter()
                .filter_map(|(_, expr)| expr.as_ref())
                .any(|expr| self.expr_calls_fn(expr, fn_name)),
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
            Expr::MapLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
                self.expr_calls_fn(key, fn_name) || self.expr_calls_fn(value, fn_name)
            }),
            Expr::SetLiteral { items, .. } => {
                items.iter().any(|item| self.expr_calls_fn(item, fn_name))
            }
            Expr::OptionalChain { object, chain, .. } => {
                self.expr_calls_fn(object, fn_name)
                    || matches!(
                        chain,
                        ChainKind::Method { args, .. }
                            if args.iter().any(|arg| self.expr_calls_fn(arg, fn_name))
                    )
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.expr_calls_fn(value, fn_name) || self.expr_calls_fn(default, fn_name)
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.expr_calls_fn(object, fn_name)
                    || self.expr_calls_fn(index, fn_name)
                    || self.expr_calls_fn(value, fn_name)
            }
            Expr::Pipeline { .. } => false,
            Expr::Loop { .. } | Expr::Break { .. } => false,
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
            Stmt::Yield { value, .. } => self.expr_calls_fn(value, fn_name),
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                body.iter().any(|stmt| self.stmt_calls_fn(stmt, fn_name))
            }
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
                methods
                    .iter()
                    .any(|method| self.expr_calls_fn(&method.body, fn_name))
                    || operators
                        .iter()
                        .any(|operator| self.expr_calls_fn(&operator.body, fn_name))
            }
            Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => methods
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
            | Stmt::UseRaw { .. }
            | Stmt::Defer { .. } => false,
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

    fn declare_var(&mut self, name: &str, is_state: bool, type_ann: Option<TypeAnn>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), VarInfo { is_state, type_ann });
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
            .find_map(|scope| scope.get(name))
            .map(|info| info.is_state)
            .unwrap_or(false)
    }

    fn lookup_var_type(&self, name: &str) -> Option<&TypeAnn> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .and_then(|info| info.type_ann.as_ref())
    }

    fn expr_type_ann(&self, expr: &Expr) -> Option<&TypeAnn> {
        match expr {
            Expr::Ident(name, _) => self.lookup_var_type(name),
            _ => None,
        }
    }

    fn infer_expr_type_ann(&self, expr: &Expr) -> Option<TypeAnn> {
        match expr {
            Expr::Literal(Literal::Int(_), _) => Some(TypeAnn::Number),
            Expr::Literal(Literal::Float(_), _) => Some(TypeAnn::Float),
            Expr::Literal(Literal::String(_), _) => Some(TypeAnn::String),
            Expr::Literal(Literal::Bool(_), _) => Some(TypeAnn::Bool),
            Expr::Ident(name, _) => self.lookup_var_type(name).cloned(),
            Expr::StructInit { name, .. } => Some(TypeAnn::Named(name.clone())),
            Expr::AnonStruct { fields, .. } => Some(TypeAnn::AnonStruct(
                fields
                    .iter()
                    .map(|(field, expr)| {
                        let ann = expr
                            .as_ref()
                            .and_then(|expr| self.infer_expr_type_ann(expr))
                            .or_else(|| self.lookup_var_type(field).cloned())
                            .unwrap_or(TypeAnn::Named("unknown".to_string()));
                        (field.clone(), ann)
                    })
                    .collect(),
            )),
            Expr::Field { object, field, .. } => match self.infer_expr_type_ann(object) {
                Some(TypeAnn::Named(name)) => self
                    .named_struct_defs
                    .get(&name)
                    .and_then(|fields| fields.iter().find(|(f, _)| f == field))
                    .map(|(_, ann)| ann.clone()),
                Some(TypeAnn::AnonStruct(fields)) => fields
                    .into_iter()
                    .find(|(f, _)| f == field)
                    .map(|(_, ann)| ann),
                _ => None,
            },
            Expr::BinOp {
                op, left, right, ..
            } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => Some(TypeAnn::Bool),
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                    let left_ty = self.infer_expr_type_ann(left);
                    let right_ty = self.infer_expr_type_ann(right);
                    if matches!(left_ty, Some(TypeAnn::Float))
                        || matches!(right_ty, Some(TypeAnn::Float))
                    {
                        Some(TypeAnn::Float)
                    } else {
                        Some(TypeAnn::Number)
                    }
                }
            },
            Expr::If {
                then_block,
                else_block,
                ..
            } => {
                let then_ty = self.infer_expr_type_ann(then_block);
                let else_ty = else_block
                    .as_ref()
                    .and_then(|expr| self.infer_expr_type_ann(expr));
                if then_ty == else_ty {
                    then_ty
                } else {
                    then_ty.or(else_ty)
                }
            }
            Expr::Block { tail, .. } => tail
                .as_ref()
                .and_then(|tail| self.infer_expr_type_ann(tail)),
            _ => None,
        }
    }

    fn rust_fn_name<'a>(&self, name: &'a str) -> &'a str {
        if name == "use" {
            "r#use"
        } else if self.rename_main && name == "main" {
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
                pat,
                type_ann,
                value,
                ..
            } => {
                match pat {
                    Pat::Ident(name) => {
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
                        self.declare_var(name, false, type_ann.clone());
                        let mut result = format!(
                            "{}{} {}{} = {};\n",
                            self.indent_str(),
                            binding_kw,
                            name,
                            ty,
                            val
                        );
                        if let Some(guard) = self.maybe_generate_defer_guard(name, value) {
                            result.push_str(&guard);
                        }
                        result
                    }
                    Pat::Wildcard => {
                        // ワイルドカード: _ = expr; → let _tmp = expr; のみ
                        let val = self.gen_expr(value, false);
                        format!("{}let _ = {};\n", self.indent_str(), val)
                    }
                    Pat::Tuple(pats) | Pat::List(pats) => {
                        // let (a, b) = expr; → let _destructure = expr; let a = _destructure[0].clone(); ...
                        let val = self.gen_expr(value, false);
                        let tmp = "_destructure";
                        let mut result = format!("{}let {} = {};\n", self.indent_str(), tmp, val);
                        let mut idx = 0usize;
                        for sub_pat in pats {
                            match sub_pat {
                                Pat::Ident(n) => {
                                    result.push_str(&format!(
                                        "{}let {} = {}[{}].clone();\n",
                                        self.indent_str(),
                                        n,
                                        tmp,
                                        idx
                                    ));
                                    self.declare_var(n, false, None);
                                    idx += 1;
                                }
                                Pat::Wildcard => {
                                    idx += 1;
                                }
                                Pat::Rest(rest_name) => {
                                    result.push_str(&format!(
                                        "{}let {} = {}[{}..].to_vec();\n",
                                        self.indent_str(),
                                        rest_name,
                                        tmp,
                                        idx
                                    ));
                                    self.declare_var(rest_name, false, None);
                                    // rest は最後なので終了
                                    break;
                                }
                                _ => {
                                    idx += 1;
                                }
                            }
                        }
                        result
                    }
                    Pat::Rest(name) => {
                        let val = self.gen_expr(value, false);
                        self.declare_var(name, false, type_ann.clone());
                        format!("{}let {} = {};\n", self.indent_str(), name, val)
                    }
                }
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
                self.declare_var(name, true, type_ann.clone());
                let mut stmt = format!("{}let mut {}{} = {};\n", self.indent_str(), name, ty, val);
                if let Some(guard) = self.maybe_generate_defer_guard(name, value) {
                    stmt.push_str(&guard);
                }
                stmt
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
                self.declare_var(name, false, type_ann.clone());
                let mut stmt = format!(
                    "{}{}const {}{} = {};\n",
                    self.indent_str(),
                    Self::vis(*is_pub),
                    name,
                    ty,
                    val
                );
                if let Some(guard) = self.maybe_generate_defer_guard(name, value) {
                    stmt.push_str(&guard);
                }
                stmt
            }
            Stmt::Fn {
                name,
                type_params,
                params,
                return_type,
                body,
                is_pub,
                is_const,
                annotations,
                ..
            } => {
                self.declare_var(name, false, None);
                let mut out = String::new();
                for ann in annotations {
                    out.push_str(&format!("{}// {}\n", self.indent_str(), ann));
                }
                out.push_str(&self.gen_fn(
                    name,
                    type_params,
                    params,
                    return_type,
                    body,
                    *is_pub,
                    *is_const,
                ));
                out
            }
            Stmt::Return(Some(expr), _) => {
                format!(
                    "{}return {};\n",
                    self.indent_str(),
                    self.gen_expr(expr, false)
                )
            }
            Stmt::Return(None, _) => format!("{}return;\n", self.indent_str()),
            Stmt::Yield { value, .. } => {
                let buf = self
                    .current_generator_buf()
                    .expect("yield must only appear inside generator functions")
                    .to_string();
                let value_str = self.gen_expr(value, false);
                format!("{}{}.push({});\n", self.indent_str(), buf, value_str)
            }
            Stmt::Defer { body, .. } => {
                let guard_name = self.next_defer_guard();
                let stmt = match body {
                    DeferBody::Expr(expr) => {
                        let expr_str = self.gen_expr(expr, false);
                        format!(
                            "{}let {guard} = scopeguard::defer(|| {{ {expr}; }});\n",
                            self.indent_str(),
                            guard = guard_name,
                            expr = expr_str
                        )
                    }
                    DeferBody::Block(block) => {
                        let block_str = self.gen_expr(block, false);
                        format!(
                            "{}let {guard} = scopeguard::defer(|| {block});\n",
                            self.indent_str(),
                            guard = guard_name,
                            block = block_str
                        )
                    }
                };
                stmt
            }
            Stmt::Expr(expr) => format!("{}{};\n", self.indent_str(), self.gen_expr(expr, false)),
            Stmt::StructDef {
                name,
                generic_params,
                fields,
                derives,
                is_pub,
                ..
            } => self.gen_struct_def(name, generic_params, fields, derives, *is_pub),
            Stmt::ImplBlock {
                target,
                type_params,
                target_type_args,
                trait_name,
                methods,
                operators,
                ..
            } => self.gen_impl_block(
                target,
                type_params,
                target_type_args,
                trait_name.as_deref(),
                methods,
                operators,
            ),
            Stmt::EnumDef {
                name,
                generic_params,
                variants,
                derives,
                is_pub,
                ..
            } => self.gen_enum_def(name, generic_params, variants, derives, *is_pub),
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
                generic_params,
                fields,
                validate_rules,
                is_pub,
                ..
            } => self.gen_data_def(name, generic_params, fields, validate_rules, *is_pub),
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
        type_params: &[String],
        params: &[Param],
        return_type: &Option<TypeAnn>,
        body: &Expr,
        is_pub: bool,
        is_const: bool,
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

        let generator_inner_ann = return_type.as_ref().and_then(|ty| {
            if let TypeAnn::Generate(inner) = ty {
                Some(inner.clone())
            } else {
                None
            }
        });
        let is_generator = generator_inner_ann.is_some();
        let ret_str = if let Some(inner_ann) = generator_inner_ann.as_ref() {
            format!(" -> impl Iterator<Item = {}>", type_ann_to_rust(inner_ann))
        } else if let Some(ty) = return_type {
            format!(" -> {}", type_ann_to_rust(ty))
        } else {
            String::new()
        };
        let generic_str = format_generic_params(type_params);
        let is_async = self.async_fns.contains(name);
        let is_recursive_async = self.recursive_async_fns.contains(name);

        self.push_scope();
        for p in params {
            self.declare_var(&p.name, false, p.type_ann.clone());
        }
        if is_async || is_recursive_async {
            self.async_context_depth += 1;
        }
        let body_str = if is_generator {
            let buf_name = self.next_generator_buf();
            let mut block = String::new();
            block.push_str(&format!(
                "{}let mut {} = Vec::new();\n",
                self.indent_str(),
                buf_name
            ));
            self.generator_stack.push(buf_name.clone());
            block.push_str(&self.gen_block_body(body, false));
            self.generator_stack.pop();
            block.push_str(&format!(
                "{}let mut __forge_iter = {}.into_iter();\n",
                self.indent_str(),
                buf_name
            ));
            block.push_str(&format!(
                "{}std::iter::from_fn(move || __forge_iter.next())\n",
                self.indent_str()
            ));
            block
        } else {
            self.gen_block_body(body, false)
        };
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
                format!("{}{}", fn_name, generic_str),
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
            let const_kw = if is_const && !is_async && !is_recursive_async {
                "const "
            } else {
                ""
            };
            let mut out = format!(
                "{}{}{}{}fn {}({}){} {{\n",
                self.indent_str(),
                Self::vis(is_pub),
                const_kw,
                async_kw,
                format!("{}{}", fn_name, generic_str),
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
        let implicit_self = !method.has_self && self.expr_references_self(&method.body);
        let generic_str = format_generic_params(&method.type_params);
        let mut params = Vec::new();
        if method.has_state_self {
            params.push("mut self".to_string());
        } else if method.has_self || implicit_self {
            params.push("&self".to_string());
        }
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

        let const_kw = if method.is_const { "const " } else { "" };
        let mut out = format!(
            "{}{}{}fn {}({}){} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            const_kw,
            format!("{}{}", self.rust_fn_name(&method.name), generic_str),
            params.join(", "),
            ret
        );
        self.push_scope();
        if method.has_self || implicit_self {
            self.declare_var("self", method.has_state_self, None);
        }
        for p in &method.params {
            self.declare_var(&p.name, false, p.type_ann.clone());
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
                self.declare_var("self", *has_state_self, None);
                for p in params {
                    self.declare_var(&p.name, false, p.type_ann.clone());
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
        generic_params: &[String],
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
            format!("{}{}", name, format_generic_params(generic_params))
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
        if !generic_params.is_empty() {
            out.push_str(&format!(
                "{}_marker: PhantomData<{}>,\n",
                self.indent_str(),
                generic_marker_type(generic_params)
            ));
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));

        let accessor_impl = self.gen_accessor_impl(name, generic_params, fields, derives);
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
        type_params: &[String],
        target_type_args: &[TypeAnn],
        trait_name: Option<&str>,
        methods: &[FnDef],
        operators: &[OperatorDef],
    ) -> String {
        let impl_generics = format_generic_params(type_params);
        let target_base = target.to_string();
        let target_impl = if target_type_args.is_empty() {
            target_base.clone()
        } else {
            format!(
                "{}<{}>",
                target_base,
                target_type_args
                    .iter()
                    .map(type_ann_to_rust)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let header = match trait_name {
            Some(name) => format!(
                "{}impl{} {} for {} {{\n",
                self.indent_str(),
                impl_generics,
                name,
                target_impl
            ),
            None => format!(
                "{}impl{} {} {{\n",
                self.indent_str(),
                impl_generics,
                target_impl
            ),
        };

        let mut out = header;
        self.indent += 1;
        for method in methods {
            out.push_str(&self.gen_pub_method_def(method));
        }
        for operator in operators {
            if let Some(helper_str) = self.gen_operator_helper(operator, &target_impl, &target_base)
            {
                out.push_str(&helper_str);
            }
        }
        self.indent -= 1;
        out.push_str(&format!("{}}}\n", self.indent_str()));

        for operator in operators {
            if let Some(trait_str) =
                self.gen_operator_trait_impl(operator, &impl_generics, &target_impl, &target_base)
            {
                out.push('\n');
                out.push_str(&trait_str);
            }
        }

        out
    }

    fn gen_enum_def(
        &mut self,
        name: &str,
        generic_params: &[String],
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
            format!("{}{}", name, format_generic_params(generic_params))
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
        generic_params: &[String],
        fields: &[(String, TypeAnn)],
        validate_rules: &[ValidateRule],
        is_pub: bool,
    ) -> String {
        let mut derives = vec!["Debug".to_string(), "Clone".to_string()];
        if fields.iter().all(|(_, ty)| type_supports_eq(ty)) {
            derives.push("Eq".to_string());
        }
        if fields.iter().all(|(_, ty)| type_supports_serde(ty)) {
            derives.push("Serialize".to_string());
            derives.push("Deserialize".to_string());
        }
        if generic_params.is_empty() && fields.iter().all(|(_, ty)| type_supports_hash(ty)) {
            derives.push("Hash".to_string());
        }
        if generic_params.is_empty() {
            derives.push("Accessor".to_string());
        }
        let mut out = self.gen_struct_def(name, generic_params, fields, &derives, is_pub);

        let validate_impl = self.gen_validate_impl(name, generic_params, validate_rules);
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
        let implicit_self = !method.has_self && self.expr_references_self(&method.body);
        let mut params = Vec::new();
        if transition_target.is_some() {
            params.push("self".to_string());
        } else if method.has_state_self {
            params.push("&mut self".to_string());
        } else if method.has_self || implicit_self {
            params.push("&self".to_string());
        }
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
        if method.has_self || implicit_self || transition_target.is_some() {
            self.declare_var("self", method.has_state_self, None);
        }
        for p in &method.params {
            self.declare_var(&p.name, false, p.type_ann.clone());
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
            let transition_expr = format!("{} {{ {} }}", type_name, parts.join(", "));
            match &method.return_type {
                Some(TypeAnn::Result(_)) => format!("Ok({})", transition_expr),
                _ => transition_expr,
            }
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
        generic_params: &[String],
        fields: &[(String, TypeAnn)],
        derives: &[String],
    ) -> String {
        if !derives.iter().any(|d| d == "Accessor") {
            return String::new();
        }

        let generics = format_generic_params(generic_params);
        let mut out = format!(
            "{}impl{} {}{} {{\n",
            self.indent_str(),
            generics,
            name,
            generics
        );
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

    fn gen_validate_impl(
        &mut self,
        name: &str,
        generic_params: &[String],
        rules: &[ValidateRule],
    ) -> String {
        if rules.is_empty() {
            return String::new();
        }

        let generics = if generic_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", generic_params.join(", "))
        };
        let mut out = format!(
            "{}impl{} {}{} {{\n",
            self.indent_str(),
            generics,
            name,
            generics
        );
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
        // forge/http → reqwest に変換
        if let UsePath::External(p) = path {
            if p == "forge/http" {
                return format!("{}use reqwest;\n", Self::vis(is_pub));
            }
        }

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
                pat, iter, body, ..
            } => self.gen_for(pat, iter, body),
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
            Expr::Pipeline { steps, .. } => self.gen_pipeline(steps),
            Expr::Loop { body, .. } => {
                format!("loop {{\n{}\n}}", self.gen_expr(body, true))
            }
            Expr::Break { .. } => "break".to_string(),
            Expr::Field { object, field, .. } => {
                format!("{}.{}", self.gen_expr(object, false), field)
            }
            Expr::OptionalChain { object, chain, .. } => {
                let object_expr = self.gen_expr(object, false);
                match chain {
                    ChainKind::Field(field) => {
                        format!("{}.and_then(|v| Some(v.{}))", object_expr, field)
                    }
                    ChainKind::Method { name, args } => {
                        let args_code = args
                            .iter()
                            .map(|expr| self.gen_expr(expr, false))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "{}.and_then(|v| Some(v.{}({})))",
                            object_expr, name, args_code
                        )
                    }
                }
            }
            Expr::NullCoalesce { value, default, .. } => format!(
                "{}.unwrap_or({})",
                self.gen_expr(value, false),
                self.gen_expr(default, false)
            ),
            Expr::Index { object, index, .. } => {
                let object_expr = self.gen_expr(object, false);
                let index_expr = self.gen_expr(index, false);
                if is_map_like_ann(self.expr_type_ann(object)) {
                    format!("{}[&{}]", object_expr, index_expr)
                } else {
                    format!("{}[{} as usize]", object_expr, index_expr)
                }
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
            Expr::MapLiteral { pairs, .. } => format!(
                "::std::collections::HashMap::from([{}])",
                pairs
                    .iter()
                    .map(|(key, value)| format!(
                        "({}, {})",
                        self.gen_expr(key, false),
                        self.gen_expr(value, false)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::SetLiteral { items, .. } => format!(
                "::std::collections::HashSet::from([{}])",
                items
                    .iter()
                    .map(|item| self.gen_expr(item, false))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::Question(inner, _) => format!("{}?", self.gen_expr(inner, false)),
            Expr::Await { expr, .. } => {
                self.suppress_auto_await_depth += 1;
                let inner = self.gen_expr(expr, false);
                self.suppress_auto_await_depth -= 1;
                format!("{}.await", inner)
            }
            Expr::Spawn { body, .. } => {
                self.async_context_depth += 1;
                let body_str = self.gen_block_body(body, false);
                self.async_context_depth -= 1;
                let mut out = String::from("tokio::spawn(async move {\n");
                out.push_str(&body_str);
                out.push_str(&format!("{}}})", self.indent_str()));
                out
            }
            Expr::Assign { name, value, .. } => {
                format!("{} = {}", name, self.gen_expr(value, false))
            }
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                let object_expr = self.gen_expr(object, false);
                let index_expr = self.gen_expr(index, false);
                let value_expr = self.gen_expr(value, false);
                if is_map_like_ann(self.expr_type_ann(object)) {
                    format!(
                        "{{ let _ = {}.insert({}, {}); }}",
                        object_expr, index_expr, value_expr
                    )
                } else {
                    format!("{}[{}] = {}", object_expr, index_expr, value_expr)
                }
            }
            Expr::StructInit { name, fields, .. } => {
                let mut rendered_fields = fields
                    .iter()
                    .map(|(field, expr)| format!("{}: {}", field, self.gen_expr(expr, false)))
                    .collect::<Vec<_>>();
                if self.generic_type_params.contains_key(name) {
                    rendered_fields.push("_marker: PhantomData".to_string());
                }
                format!("{} {{ {} }}", name, rendered_fields.join(", "))
            }
            Expr::AnonStruct { fields, .. } => {
                let field_types = fields
                    .iter()
                    .map(|(field, expr)| {
                        let ann = expr
                            .as_ref()
                            .and_then(|expr| self.infer_expr_type_ann(expr))
                            .or_else(|| self.lookup_var_type(field).cloned())
                            .unwrap_or(TypeAnn::Named("unknown".to_string()));
                        (field.clone(), ann)
                    })
                    .collect::<Vec<_>>();
                let type_name = anon_struct_name(&field_types);
                let rendered_fields = fields
                    .iter()
                    .map(|(field, expr)| {
                        let value = match expr {
                            Some(expr) => self.gen_expr(expr, false),
                            None => field.clone(),
                        };
                        format!("{}: {}", field, value)
                    })
                    .collect::<Vec<_>>();
                format!("{} {{ {} }}", type_name, rendered_fields.join(", "))
            }
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

    fn gen_pipeline(&mut self, steps: &[PipelineStep]) -> String {
        let source_expr = steps
            .iter()
            .find_map(|step| match step {
                PipelineStep::Source(expr) => Some(expr),
                _ => None,
            })
            .expect("pipeline block missing source");
        let sink_expr = steps
            .iter()
            .rev()
            .find_map(|step| match step {
                PipelineStep::Sink(expr) => Some(expr),
                _ => None,
            })
            .expect("pipeline block missing sink");

        let dataset_var = self.next_pipeline_name("dataset");
        let sink_var = self.next_pipeline_name("sink");

        let mut out = String::new();
        out.push_str("{\n");
        self.indent += 1;
        out.push_str(&format!(
            "{}let mut {} = ({}).collect()?;\n",
            self.indent_str(),
            dataset_var,
            self.gen_expr(source_expr, false)
        ));

        for step in steps {
            match step {
                PipelineStep::Source(_) | PipelineStep::Sink(_) => continue,
                PipelineStep::Filter(expr) => {
                    let (closure_name, closure_code) = self.gen_pipeline_closure(expr, "filter");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}{} = {}.into_iter().filter({}).collect::<Vec<_>>();\n",
                        self.indent_str(),
                        dataset_var,
                        dataset_var,
                        closure_name
                    ));
                }
                PipelineStep::Map(expr) => {
                    let (closure_name, closure_code) = self.gen_pipeline_closure(expr, "map");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}{} = {}.into_iter().map({}).collect::<Vec<_>>();\n",
                        self.indent_str(),
                        dataset_var,
                        dataset_var,
                        closure_name
                    ));
                }
                PipelineStep::FlatMap(expr) => {
                    let (closure_name, closure_code) = self.gen_pipeline_closure(expr, "flat_map");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}let mut __pipeline_next = Vec::new();\n",
                        self.indent_str()
                    ));
                    out.push_str(&format!(
                        "{}for item in {} {{\n",
                        self.indent_str(),
                        dataset_var
                    ));
                    self.indent += 1;
                    out.push_str(&format!(
                        "{}let mapped = {}(item);\n",
                        self.indent_str(),
                        closure_name
                    ));
                    out.push_str(&format!(
                        "{}__pipeline_next.extend(mapped.into_iter());\n",
                        self.indent_str()
                    ));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}\n", self.indent_str()));
                    out.push_str(&format!(
                        "{}{} = __pipeline_next;\n",
                        self.indent_str(),
                        dataset_var
                    ));
                }
                PipelineStep::Group(expr) => {
                    self.needs_hashmap_use = true;
                    let (closure_name, closure_code) = self.gen_pipeline_closure(expr, "group");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}let mut __pipeline_groups: HashMap<_, Vec<_>> = HashMap::new();\n",
                        self.indent_str()
                    ));
                    out.push_str(&format!(
                        "{}for item in {} {{\n",
                        self.indent_str(),
                        dataset_var
                    ));
                    self.indent += 1;
                    out.push_str(&format!(
                        "{}let key = {}(&item);\n",
                        self.indent_str(),
                        closure_name
                    ));
                    out.push_str(&format!(
                        "{}__pipeline_groups.entry(key).or_insert_with(Vec::new).push(item);\n",
                        self.indent_str()
                    ));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}\n", self.indent_str()));
                    out.push_str(&format!(
                        "{}let mut __pipeline_next = Vec::with_capacity(__pipeline_groups.len());\n",
                        self.indent_str()
                    ));
                    out.push_str(&format!(
                        "{}for (key, values) in __pipeline_groups {{\n",
                        self.indent_str()
                    ));
                    self.indent += 1;
                    out.push_str(&format!(
                        "{}__pipeline_next.push(forge_std::pipeline::Group {{ key, values }});\n",
                        self.indent_str()
                    ));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}\n", self.indent_str()));
                    out.push_str(&format!(
                        "{}{} = __pipeline_next;\n",
                        self.indent_str(),
                        dataset_var
                    ));
                }
                PipelineStep::Sort { key, descending } => {
                    self.needs_ordering_use = true;
                    let (closure_name, closure_code) = self.gen_pipeline_closure(key, "sort_key");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}{}.sort_by(|a, b| {{\n",
                        self.indent_str(),
                        dataset_var
                    ));
                    self.indent += 1;
                    out.push_str(&format!(
                        "{}let key_a = {}(a);\n",
                        self.indent_str(),
                        closure_name
                    ));
                    out.push_str(&format!(
                        "{}let key_b = {}(b);\n",
                        self.indent_str(),
                        closure_name
                    ));
                    out.push_str(&format!(
                        "{}let ordering = key_a.cmp(&key_b);\n",
                        self.indent_str()
                    ));
                    if *descending {
                        out.push_str(&format!("{}ordering.reverse()\n", self.indent_str()));
                    } else {
                        out.push_str(&format!("{}ordering\n", self.indent_str()));
                    }
                    self.indent -= 1;
                    out.push_str(&format!("{}}});\n", self.indent_str()));
                }
                PipelineStep::Take(expr) => {
                    let expr_code = self.gen_expr(expr, false);
                    out.push_str(&format!(
                        "{}{} = {}.into_iter().take(({}) as usize).collect::<Vec<_>>();\n",
                        self.indent_str(),
                        dataset_var,
                        dataset_var,
                        expr_code
                    ));
                }
                PipelineStep::Skip(expr) => {
                    let expr_code = self.gen_expr(expr, false);
                    out.push_str(&format!(
                        "{}{} = {}.into_iter().skip(({}) as usize).collect::<Vec<_>>();\n",
                        self.indent_str(),
                        dataset_var,
                        dataset_var,
                        expr_code
                    ));
                }
                PipelineStep::Each(expr) => {
                    let (closure_name, closure_code) = self.gen_pipeline_closure(expr, "each");
                    out.push_str(&closure_code);
                    out.push_str(&format!(
                        "{}for item in &{} {{\n",
                        self.indent_str(),
                        dataset_var
                    ));
                    self.indent += 1;
                    out.push_str(&format!(
                        "{}{}(item.clone());\n",
                        self.indent_str(),
                        closure_name
                    ));
                    self.indent -= 1;
                    out.push_str(&format!("{}}}\n", self.indent_str()));
                }
                PipelineStep::Parallel(expr) => {
                    let expr_code = self.gen_expr(expr, false);
                    out.push_str(&format!(
                        "{}let _parallel_degree = {};\n",
                        self.indent_str(),
                        expr_code
                    ));
                }
            }
        }

        out.push_str(&format!(
            "{}let {} = {};\n",
            self.indent_str(),
            sink_var,
            self.gen_expr(sink_expr, false)
        ));
        out.push_str(&format!(
            "{}{}.run({})\n",
            self.indent_str(),
            sink_var,
            dataset_var
        ));

        self.indent -= 1;
        out.push_str(&format!("{}}}", self.indent_str()));
        out
    }

    fn gen_pipeline_closure(&mut self, expr: &Expr, label: &str) -> (String, String) {
        let name = self.next_pipeline_name(label);
        let body = self.gen_expr(expr, false);
        let code = format!("{}let {} = {};\n", self.indent_str(), name, body);
        (name, code)
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

    fn gen_for(&mut self, pat: &Pat, iter: &Expr, body: &Expr) -> String {
        match pat {
            Pat::Ident(var) => format!(
                "for {} in &{} {}",
                var,
                self.gen_expr(iter, false),
                self.gen_inline_block(body)
            ),
            _ => {
                let iter_expr = self.gen_expr(iter, false);
                let tmp = "__item";
                let mut out = format!("for {} in &{} {{\n", tmp, iter_expr);
                self.indent += 1;
                out.push_str(&format!(
                    "{}let {} = {}.clone();\n",
                    self.indent_str(),
                    tmp,
                    tmp
                ));
                out.push_str(&self.gen_pat_bindings(pat, tmp));
                match body {
                    Expr::Block { stmts, tail, .. } => {
                        for stmt in stmts {
                            out.push_str(&self.gen_stmt(stmt));
                        }
                        if let Some(tail) = tail {
                            out.push_str(&format!(
                                "{}{};\n",
                                self.indent_str(),
                                self.gen_expr(tail, false)
                            ));
                        }
                    }
                    other => {
                        out.push_str(&format!(
                            "{}{};\n",
                            self.indent_str(),
                            self.gen_expr(other, false)
                        ));
                    }
                }
                self.indent -= 1;
                out.push_str(&format!("{}}}", self.indent_str()));
                out
            }
        }
    }

    fn gen_pat_bindings(&mut self, pat: &Pat, source: &str) -> String {
        match pat {
            Pat::Ident(name) => {
                self.declare_var(name, false, None);
                format!("{}let {} = {};\n", self.indent_str(), name, source)
            }
            Pat::Wildcard => String::new(),
            Pat::Tuple(pats) | Pat::List(pats) => {
                let mut out = String::new();
                let mut idx = 0usize;
                for sub_pat in pats {
                    match sub_pat {
                        Pat::Ident(name) => {
                            self.declare_var(name, false, None);
                            out.push_str(&format!(
                                "{}let {} = {}[{}].clone();\n",
                                self.indent_str(),
                                name,
                                source,
                                idx
                            ));
                            idx += 1;
                        }
                        Pat::Wildcard => idx += 1,
                        Pat::Rest(name) => {
                            self.declare_var(name, false, None);
                            out.push_str(&format!(
                                "{}let {} = {}[{}..].to_vec();\n",
                                self.indent_str(),
                                name,
                                source,
                                idx
                            ));
                            break;
                        }
                        nested => {
                            let nested_tmp = format!("{}_{}", source.replace('.', "_"), idx);
                            out.push_str(&format!(
                                "{}let {} = {}[{}].clone();\n",
                                self.indent_str(),
                                nested_tmp,
                                source,
                                idx
                            ));
                            out.push_str(&self.gen_pat_bindings(nested, &nested_tmp));
                            idx += 1;
                        }
                    }
                }
                out
            }
            Pat::Rest(name) => {
                self.declare_var(name, false, None);
                format!("{}let {} = {};\n", self.indent_str(), name, source)
            }
        }
    }

    fn pat_bindings(pat: &Pat, names: &mut Vec<String>) {
        match pat {
            Pat::Ident(name) | Pat::Rest(name) => names.push(name.clone()),
            Pat::Wildcard => {}
            Pat::Tuple(items) | Pat::List(items) => {
                for item in items {
                    Self::pat_bindings(item, names);
                }
            }
        }
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
            // forge/http コンストラクタ → reqwest::Client::new().METHOD(url)
            if self.http_imported {
                let method = match name.as_str() {
                    "get" => Some("get"),
                    "post" => Some("post"),
                    "put" => Some("put"),
                    "patch" => Some("patch"),
                    "delete" => Some("delete"),
                    _ => None,
                };
                if let Some(m) = method {
                    let url = arg_strs.first().cloned().unwrap_or_default();
                    return format!("reqwest::Client::new().{}({})", m, url);
                }
            }

            if let Some(rendered) = try_builtin_call(name, &arg_strs) {
                return rendered;
            }
            if let Some(rendered) = try_constructor_call(name, &arg_strs) {
                return rendered;
            }
        }

        let callee_expr = self.gen_expr(callee, false);
        let rendered_callee = match callee {
            Expr::Ident(_, _) => callee_expr,
            _ => format!("({})", callee_expr),
        };
        let call = format!("{}({})", rendered_callee, arg_strs.join(", "));
        if self.should_auto_await(callee) {
            format!("{}.await", call)
        } else {
            call
        }
    }

    fn gen_method_call(&mut self, object: &Expr, method: &str, args: &[Expr]) -> String {
        let rust_method = self.rust_fn_name(method);
        if let Expr::Ident(type_name, _) = object {
            if method == "new" && self.typestate_initial_states.contains_key(type_name) {
                let args: Vec<String> = args
                    .iter()
                    .skip(1)
                    .map(|arg| self.gen_expr(arg, false))
                    .collect();
                return format!("{}::new({})", type_name, args.join(", "));
            }
            if type_name
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
            {
                let args: Vec<String> = args.iter().map(|arg| self.gen_expr(arg, false)).collect();
                if type_name == "Response" && method == "text" {
                    return format!("Response::<String>::text({})", args.join(", "));
                }
                if type_name == "Response" && method == "empty" {
                    return format!("Response::<()>::empty({})", args.join(", "));
                }
                return format!("{}::{}({})", type_name, rust_method, args.join(", "));
            }
        }

        // ── forge/http メソッド変換 ──
        if self.http_imported {
            let obj_str = self.gen_expr(object, false);
            let args: Vec<String> = args.iter().map(|a| self.gen_expr(a, false)).collect();
            match method {
                // .send() → .send().await
                "send" if args.is_empty() => {
                    return format!("{}.send().await", obj_str);
                }
                // .query(map) → .query(&map)
                "query" if args.len() == 1 => {
                    return format!("{}.query(&{})", obj_str, args[0]);
                }
                // .json(v) (RequestBuilder) → .json(&v)
                "json" if args.len() == 1 => {
                    return format!("{}.json(&{})", obj_str, args[0]);
                }
                // .json() (Response) → .json::<serde_json::Value>().await
                "json" if args.is_empty() => {
                    return format!("{}.json::<serde_json::Value>().await", obj_str);
                }
                // .form(map) → .form(&map)
                "form" if args.len() == 1 => {
                    return format!("{}.form(&{})", obj_str, args[0]);
                }
                // .text() → .text().await
                "text" if args.is_empty() => {
                    return format!("{}.text().await", obj_str);
                }
                // .bytes() → .bytes().await
                "bytes" if args.is_empty() => {
                    return format!("{}.bytes().await", obj_str);
                }
                // .header(k, v) → .header(k, v)  (同じ形式)
                "header" if args.len() == 2 => {
                    return format!("{}.header({}, {})", obj_str, args[0], args[1]);
                }
                // .timeout(ms) → .timeout(std::time::Duration::from_millis(ms))
                "timeout" if args.len() == 1 => {
                    return format!(
                        "{}.timeout(std::time::Duration::from_millis({}))",
                        obj_str, args[0]
                    );
                }
                // .retry(n) → (reqwest に組み込みリトライなし: コメントとして残す)
                "retry" => {
                    return format!("{} /* .retry({}) */", obj_str, args.join(", "));
                }
                _ => {}
            }
        }

        let object_is_map_like = is_map_like_ann(self.expr_type_ann(object));
        let object_is_set_like = is_set_like_ann(self.expr_type_ann(object));
        let object_is_string = matches!(self.expr_type_ann(object), Some(TypeAnn::String));
        // 型が List と判明している、または型が不明（map/set/string でない）の場合はリストとして扱う
        let object_is_list = matches!(self.expr_type_ann(object), Some(TypeAnn::List(_)))
            || (!object_is_map_like
                && !object_is_set_like
                && !object_is_string
                && self.expr_type_ann(object).is_none());
        let object = self.gen_expr(object, false);
        let args: Vec<String> = args.iter().map(|arg| self.gen_expr(arg, false)).collect();

        match method {
            "split" if object_is_string || method == "split" => {
                let sep = args.first().cloned().unwrap_or_else(|| "\"\"".to_string());
                format!(
                    "{}.split(&{}).filter(|part| !part.is_empty()).map(|part| part.to_string()).collect::<Vec<String>>()",
                    object, sep
                )
            }
            "starts_with" if object_is_string || method == "starts_with" => {
                let prefix = args.first().cloned().unwrap_or_else(|| "\"\"".to_string());
                format!("{}.starts_with(&{})", object, prefix)
            }
            "strip_prefix" if object_is_string || method == "strip_prefix" => {
                let prefix = args.first().cloned().unwrap_or_else(|| "\"\"".to_string());
                format!(
                    "{}.strip_prefix(&{}).map(|part| part.to_string())",
                    object, prefix
                )
            }
            "contains" if object_is_string || (!object_is_set_like && method == "contains") => {
                let pattern = args.first().cloned().unwrap_or_else(|| "\"\"".to_string());
                format!("{}.contains(&{})", object, pattern)
            }
            // ── map / ordered_map メソッド ─────────────────────────────────
            "get" if object_is_map_like => {
                let key = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!("{}.get(&{}).cloned()", object, key)
            }
            "insert" if object_is_map_like => {
                format!("{{ let _ = {}.insert({}); }}", object, args.join(", "))
            }
            "keys" if object_is_map_like => {
                format!("{}.keys().cloned().collect::<Vec<_>>()", object)
            }
            "values" if object_is_map_like => {
                format!("{}.values().cloned().collect::<Vec<_>>()", object)
            }
            "entries" if object_is_map_like => {
                format!(
                    "{}.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect::<Vec<_>>()",
                    object
                )
            }
            "contains_key" if object_is_map_like => {
                let key = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!("{}.contains_key(&{})", object, key)
            }
            "remove" if object_is_map_like => {
                let key = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!("{}.remove(&{})", object, key)
            }
            "len" if object_is_map_like => format!("{}.len() as i64", object),
            // ── set / ordered_set メソッド ────────────────────────────────
            "contains" if object_is_set_like => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!("{}.contains(&{})", object, val)
            }
            "insert" if object_is_set_like => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!(
                    "{{ let mut _s = {}.clone(); _s.insert({}); _s }}",
                    object, val
                )
            }
            "union" if object_is_set_like => {
                let other = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!(
                    "{}.union(&{}).cloned().collect::<std::collections::HashSet<_>>()",
                    object, other
                )
            }
            "intersect" if object_is_set_like => {
                let other = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!(
                    "{}.intersection(&{}).cloned().collect::<std::collections::HashSet<_>>()",
                    object, other
                )
            }
            "difference" if object_is_set_like => {
                let other = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/* missing arg */".to_string());
                format!(
                    "{}.difference(&{}).cloned().collect::<std::collections::HashSet<_>>()",
                    object, other
                )
            }
            "len" if object_is_set_like => format!("{}.len() as i64", object),
            "to_list" if object_is_set_like => {
                format!("{}.iter().cloned().collect::<Vec<_>>()", object)
            }
            "push" => {
                let value = args.first().cloned().unwrap_or_default();
                format!("{{ {}.push({}); }}", object, value)
            }
            "is_some" | "is_none" | "is_ok" | "is_err" | "unwrap_or" => {
                format!("{}.{}({})", object, rust_method, args.join(", "))
            }
            "map" if object_is_list => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().map({}).collect::<Vec<_>>()", object, f)
            }
            "filter" if object_is_list => {
                let f = args.first().cloned().unwrap_or_default();
                format!(
                    "{}.iter().filter(|x| ({})(x)).collect::<Vec<_>>()",
                    object, f
                )
            }
            "flat_map" if object_is_list => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().flat_map({}).collect::<Vec<_>>()", object, f)
            }
            "fold" if object_is_list => {
                let init = args.first().cloned().unwrap_or_else(|| "0".to_string());
                let f = args.get(1).cloned().unwrap_or_default();
                format!("{}.iter().fold({}, {})", object, init, f)
            }
            "sum" if object_is_list => format!("{}.iter().sum::<i64>()", object),
            "count" | "len" if object_is_list => format!("{}.len() as i64", object),
            "any" if object_is_list => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().any(|x| ({})(x))", object, f)
            }
            "all" if object_is_list => {
                let f = args.first().cloned().unwrap_or_default();
                format!("{}.iter().all(|x| ({})(x))", object, f)
            }
            "first" if object_is_list => format!("{}.first().cloned()", object),
            "last" if object_is_list => format!("{}.last().cloned()", object),
            "take" if object_is_list => {
                let n = args.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().take({}).cloned().collect::<Vec<_>>()", object, n)
            }
            "skip" if object_is_list => {
                let n = args.first().cloned().unwrap_or_else(|| "0".to_string());
                format!("{}.iter().skip({}).cloned().collect::<Vec<_>>()", object, n)
            }
            "reverse" if object_is_list => {
                format!("{{ let mut v = {}.clone(); v.reverse(); v }}", object)
            }
            "distinct" if object_is_list => {
                format!("{{ let mut v = {}.clone(); v.dedup(); v }}", object)
            }
            "enumerate" if object_is_list => format!("{}.iter().enumerate()", object),
            "zip" if object_is_list => {
                let other = args.first().cloned().unwrap_or_default();
                format!("{}.iter().zip({}.iter())", object, other)
            }
            "to_string" => format!("{}.to_string()", object),
            _ => format!("{}.{}({})", object, rust_method, args.join(", ")),
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

        if self.expr_contains_await(body) {
            self.async_context_depth += 1;
            let body_str = self.gen_block_body(body, false);
            self.async_context_depth -= 1;
            let mut out = format!("{}|{}| async move {{\n", prefix, params.join(", "));
            out.push_str(&body_str);
            out.push_str(&format!("{}}}", self.indent_str()));
            out
        } else {
            format!(
                "{}|{}| {}",
                prefix,
                params.join(", "),
                self.gen_expr(body, false)
            )
        }
    }

    fn expr_references_self(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Ident(name, _) => name == "self",
            Expr::Literal(_, _) => false,
            Expr::BinOp { left, right, .. } => {
                self.expr_references_self(left) || self.expr_references_self(right)
            }
            Expr::UnaryOp { operand, .. } => self.expr_references_self(operand),
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.expr_references_self(cond)
                    || self.expr_references_self(then_block)
                    || else_block
                        .as_ref()
                        .map(|expr| self.expr_references_self(expr))
                        .unwrap_or(false)
            }
            Expr::While { cond, body, .. } => {
                self.expr_references_self(cond) || self.expr_references_self(body)
            }
            Expr::For { iter, body, .. } => {
                self.expr_references_self(iter) || self.expr_references_self(body)
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr_references_self(scrutinee)
                    || arms.iter().any(|arm| self.expr_references_self(&arm.body))
            }
            Expr::Block { stmts, tail, .. } => {
                stmts.iter().any(|stmt| self.stmt_references_self(stmt))
                    || tail
                        .as_ref()
                        .map(|expr| self.expr_references_self(expr))
                        .unwrap_or(false)
            }
            Expr::Call { callee, args, .. } => {
                self.expr_references_self(callee)
                    || args.iter().any(|arg| self.expr_references_self(arg))
            }
            Expr::MethodCall { object, args, .. } => {
                self.expr_references_self(object)
                    || args.iter().any(|arg| self.expr_references_self(arg))
            }
            Expr::Field { object, .. } => self.expr_references_self(object),
            Expr::Index { object, index, .. } => {
                self.expr_references_self(object) || self.expr_references_self(index)
            }
            Expr::Closure { body, .. } => self.expr_references_self(body),
            Expr::Interpolation { parts, .. } => parts.iter().any(|part| match part {
                InterpPart::Literal(_) => false,
                InterpPart::Expr(expr) => self.expr_references_self(expr),
            }),
            Expr::Range { start, end, .. } => {
                self.expr_references_self(start) || self.expr_references_self(end)
            }
            Expr::List(items, _) => items.iter().any(|item| self.expr_references_self(item)),
            Expr::MapLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
                self.expr_references_self(key) || self.expr_references_self(value)
            }),
            Expr::SetLiteral { items, .. } => {
                items.iter().any(|item| self.expr_references_self(item))
            }
            Expr::Question(expr, _) => self.expr_references_self(expr),
            Expr::Await { expr, .. } => self.expr_references_self(expr),
            Expr::Assign { value, .. } => self.expr_references_self(value),
            Expr::IndexAssign {
                object,
                index,
                value,
                ..
            } => {
                self.expr_references_self(object)
                    || self.expr_references_self(index)
                    || self.expr_references_self(value)
            }
            Expr::StructInit { fields, .. } => fields
                .iter()
                .any(|(_, value)| self.expr_references_self(value)),
            Expr::EnumInit { data, .. } => match data {
                EnumInitData::None => false,
                EnumInitData::Tuple(items) => {
                    items.iter().any(|item| self.expr_references_self(item))
                }
                EnumInitData::Struct(fields) => fields
                    .iter()
                    .any(|(_, value)| self.expr_references_self(value)),
            },
            Expr::FieldAssign { object, value, .. } => {
                self.expr_references_self(object) || self.expr_references_self(value)
            }
            Expr::OptionalChain { object, chain, .. } => {
                self.expr_references_self(object)
                    || matches!(
                        chain,
                        ChainKind::Method { args, .. }
                            if args.iter().any(|arg| self.expr_references_self(arg))
                    )
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.expr_references_self(value) || self.expr_references_self(default)
            }
            _ => false,
        }
    }

    fn stmt_references_self(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::State { value, .. }
            | Stmt::Const { value, .. }
            | Stmt::Expr(value)
            | Stmt::Return(Some(value), ..) => self.expr_references_self(value),
            Stmt::Return(None, ..) => false,
            Stmt::Fn { body, .. } => self.expr_references_self(body),
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                body.iter().any(|stmt| self.stmt_references_self(stmt))
            }
            _ => false,
        }
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

        if captured.contains("self") {
            return ClosureKind::FnMut;
        }
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
            Stmt::Let { pat, value, .. } => {
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
                    let mut names = Vec::new();
                    Self::pat_bindings(pat, &mut names);
                    scope.extend(names);
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
            Stmt::Yield { value, .. } => {
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
            Stmt::Return(None, _) => {}
            Stmt::ImplBlock {
                methods, operators, ..
            } => {
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
                for operator in operators {
                    local_scopes.push(
                        operator
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .chain(std::iter::once("self".to_string()))
                            .collect::<HashSet<_>>(),
                    );
                    self.analyze_closure_expr(
                        &operator.body,
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
            | Stmt::UseRaw { .. }
            | Stmt::Defer { .. } => {}
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
                iter, body, pat, ..
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
                let mut names = Vec::new();
                Self::pat_bindings(pat, &mut names);
                local_scopes.push(names.into_iter().collect());
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
            Expr::MapLiteral { pairs, .. } => {
                for (key, value) in pairs {
                    self.analyze_closure_expr(
                        key,
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
            }
            Expr::SetLiteral { items, .. } => {
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
            Expr::IndexAssign {
                object,
                index,
                value,
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
                self.analyze_closure_expr(
                    index,
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
            Expr::OptionalChain { object, chain, .. } => {
                self.analyze_closure_expr(
                    object,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                if let ChainKind::Method { name, args } = chain {
                    if name.starts_with("into_") {
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
            }
            Expr::NullCoalesce { value, default, .. } => {
                self.analyze_closure_expr(
                    value,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
                self.analyze_closure_expr(
                    default,
                    outer_names,
                    local_scopes,
                    captured,
                    mutated,
                    consumed,
                    false,
                );
            }
            Expr::Literal(_, _) => {}
            _ => {}
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
            Expr::OptionalChain { object, chain, .. } => {
                self.mark_consumed_idents(object, outer_names, local_scopes, captured, consumed);
                if let ChainKind::Method { args, .. } = chain {
                    for arg in args {
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
            Expr::NullCoalesce { value, default, .. } => {
                self.mark_consumed_idents(value, outer_names, local_scopes, captured, consumed);
                self.mark_consumed_idents(default, outer_names, local_scopes, captured, consumed);
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

fn type_supports_hash(ann: &TypeAnn) -> bool {
    match ann {
        TypeAnn::Number
        | TypeAnn::Float
        | TypeAnn::String
        | TypeAnn::Bool
        | TypeAnn::Unit
        | TypeAnn::Named(_)
        | TypeAnn::StringLiteralUnion(_) => true,
        TypeAnn::Option(inner) | TypeAnn::Result(inner) => type_supports_hash(inner),
        TypeAnn::ResultWith(ok, err) => type_supports_hash(ok) && type_supports_hash(err),
        TypeAnn::AnonStruct(fields) => fields.iter().all(|(_, ann)| type_supports_hash(ann)),
        TypeAnn::Generic { .. } => false,
        TypeAnn::List(_)
        | TypeAnn::Map(_, _)
        | TypeAnn::Set(_)
        | TypeAnn::OrderedMap(_, _)
        | TypeAnn::OrderedSet(_)
        | TypeAnn::Fn { .. } => false,
        TypeAnn::Generate(inner) => type_supports_hash(inner),
    }
}

fn type_supports_eq(ann: &TypeAnn) -> bool {
    match ann {
        TypeAnn::Number
        | TypeAnn::Float
        | TypeAnn::String
        | TypeAnn::Bool
        | TypeAnn::Unit
        | TypeAnn::Named(_)
        | TypeAnn::StringLiteralUnion(_) => true,
        TypeAnn::Option(inner) | TypeAnn::Result(inner) => type_supports_eq(inner),
        TypeAnn::ResultWith(ok, err) => type_supports_eq(ok) && type_supports_eq(err),
        TypeAnn::List(inner) | TypeAnn::Set(inner) | TypeAnn::OrderedSet(inner) => {
            type_supports_eq(inner)
        }
        TypeAnn::Map(key, value) | TypeAnn::OrderedMap(key, value) => {
            type_supports_eq(key) && type_supports_eq(value)
        }
        TypeAnn::AnonStruct(fields) => fields.iter().all(|(_, ann)| type_supports_eq(ann)),
        TypeAnn::Generic { .. } | TypeAnn::Fn { .. } => false,
        TypeAnn::Generate(inner) => type_supports_eq(inner),
    }
}

fn type_supports_serde(ann: &TypeAnn) -> bool {
    match ann {
        TypeAnn::Number
        | TypeAnn::Float
        | TypeAnn::String
        | TypeAnn::Bool
        | TypeAnn::Unit
        | TypeAnn::Named(_)
        | TypeAnn::StringLiteralUnion(_) => true,
        TypeAnn::Option(inner)
        | TypeAnn::Result(inner)
        | TypeAnn::List(inner)
        | TypeAnn::Set(inner)
        | TypeAnn::OrderedSet(inner) => type_supports_serde(inner),
        TypeAnn::ResultWith(ok, err) | TypeAnn::Map(ok, err) | TypeAnn::OrderedMap(ok, err) => {
            type_supports_serde(ok) && type_supports_serde(err)
        }
        TypeAnn::AnonStruct(fields) => fields.iter().all(|(_, ann)| type_supports_serde(ann)),
        TypeAnn::Generic { .. } | TypeAnn::Fn { .. } => false,
        TypeAnn::Generate(inner) => type_supports_serde(inner),
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
        TypeAnn::Generate(inner) => format!("Vec<{}>", type_ann_to_rust(inner)),
        TypeAnn::Named(name) => name.clone(),
        TypeAnn::AnonStruct(fields) => anon_struct_name(fields),
        TypeAnn::Generic { name, args } => {
            utility_type_ann_to_rust(name, args).unwrap_or_else(|| {
                format!(
                    "{}<{}>",
                    name,
                    args.iter()
                        .map(type_ann_to_rust)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
        }
        TypeAnn::Map(key, value) => {
            format!(
                "std::collections::HashMap<{}, {}>",
                type_ann_to_rust(key),
                type_ann_to_rust(value)
            )
        }
        TypeAnn::Set(inner) => {
            format!("std::collections::HashSet<{}>", type_ann_to_rust(inner))
        }
        TypeAnn::OrderedMap(key, value) => {
            format!(
                "std::collections::BTreeMap<{}, {}>",
                type_ann_to_rust(key),
                type_ann_to_rust(value)
            )
        }
        TypeAnn::OrderedSet(inner) => {
            format!("std::collections::BTreeSet<{}>", type_ann_to_rust(inner))
        }
        TypeAnn::Unit => "()".to_string(),
        TypeAnn::StringLiteralUnion(keys) => keys.join("_"),
        TypeAnn::Fn {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .map(type_ann_to_rust)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params, type_ann_to_rust(return_type))
        }
    }
}

fn utility_type_ann_to_rust(name: &str, args: &[TypeAnn]) -> Option<String> {
    match (name, args) {
        ("Record", [key, value]) => Some(format!(
            "std::collections::HashMap<{}, {}>",
            type_ann_to_rust(key),
            type_ann_to_rust(value)
        )),
        ("Readonly", [inner]) => Some(format!("&{}", type_ann_to_rust(inner))),
        ("Partial", [TypeAnn::Named(inner)]) => Some(format!("Partial{}", inner)),
        ("Required", [TypeAnn::Named(inner)]) => Some(format!("Required{}", inner)),
        ("Pick", [TypeAnn::Named(inner), TypeAnn::Named(keys)]) => {
            Some(format!("{}Pick_{}", inner, sanitize_type_name(keys)))
        }
        ("Pick", [TypeAnn::Named(inner), TypeAnn::StringLiteralUnion(keys)]) => {
            Some(format!("{}Pick_{}", inner, keys.join("_")))
        }
        ("Omit", [TypeAnn::Named(inner), TypeAnn::Named(keys)]) => {
            Some(format!("{}Omit_{}", inner, sanitize_type_name(keys)))
        }
        ("Omit", [TypeAnn::Named(inner), TypeAnn::StringLiteralUnion(keys)]) => {
            Some(format!("{}Omit_{}", inner, keys.join("_")))
        }
        _ => None,
    }
}

fn sanitize_type_name(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn format_generic_params(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn generic_marker_type(params: &[String]) -> String {
    if params.len() == 1 {
        params[0].clone()
    } else {
        format!("({})", params.join(", "))
    }
}

fn is_map_like_ann(ann: Option<&TypeAnn>) -> bool {
    matches!(
        ann,
        Some(TypeAnn::Map(_, _)) | Some(TypeAnn::OrderedMap(_, _))
    ) || matches!(
        ann,
        Some(TypeAnn::Generic { name, .. }) if name == "Record"
    )
}

fn is_set_like_ann(ann: Option<&TypeAnn>) -> bool {
    matches!(ann, Some(TypeAnn::Set(_)) | Some(TypeAnn::OrderedSet(_)))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UtilitySpec {
    Partial(String),
    Required(String),
    Pick(String, String),
    Omit(String, String),
}

fn collect_struct_defs(module: &Module) -> HashMap<String, Vec<(String, TypeAnn)>> {
    let mut defs = HashMap::new();
    for stmt in &module.stmts {
        match stmt {
            Stmt::StructDef { name, fields, .. } | Stmt::DataDef { name, fields, .. } => {
                defs.insert(name.clone(), fields.clone());
            }
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                let nested = collect_struct_defs(&Module {
                    stmts: body.clone(),
                });
                defs.extend(nested);
            }
            _ => {}
        }
    }
    defs
}

fn anon_struct_name(fields: &[(String, TypeAnn)]) -> String {
    let mut out = String::from("AnonStruct");
    for (field, ty) in fields {
        out.push('_');
        out.push_str(&sanitize_type_name(field));
        out.push('_');
        out.push_str(&sanitize_type_name(&anon_type_fragment(ty)));
    }
    out
}

fn anon_type_fragment(ann: &TypeAnn) -> String {
    match ann {
        TypeAnn::Number => "number".to_string(),
        TypeAnn::Float => "float".to_string(),
        TypeAnn::String => "string".to_string(),
        TypeAnn::Bool => "bool".to_string(),
        TypeAnn::Unit => "unit".to_string(),
        TypeAnn::Named(name) => name.clone(),
        TypeAnn::Option(inner) => format!("option_{}", anon_type_fragment(inner)),
        TypeAnn::Result(inner) => format!("result_{}", anon_type_fragment(inner)),
        TypeAnn::ResultWith(ok, err) => {
            format!(
                "result_{}_{}",
                anon_type_fragment(ok),
                anon_type_fragment(err)
            )
        }
        TypeAnn::List(inner) | TypeAnn::Generate(inner) => {
            format!("list_{}", anon_type_fragment(inner))
        }
        TypeAnn::Map(key, value) | TypeAnn::OrderedMap(key, value) => {
            format!(
                "map_{}_{}",
                anon_type_fragment(key),
                anon_type_fragment(value)
            )
        }
        TypeAnn::Set(inner) | TypeAnn::OrderedSet(inner) => {
            format!("set_{}", anon_type_fragment(inner))
        }
        TypeAnn::Generic { name, args } => format!(
            "{}_{}",
            name,
            args.iter()
                .map(anon_type_fragment)
                .collect::<Vec<_>>()
                .join("_")
        ),
        TypeAnn::Fn {
            params,
            return_type,
        } => format!(
            "fn_{}_to_{}",
            params
                .iter()
                .map(anon_type_fragment)
                .collect::<Vec<_>>()
                .join("_"),
            anon_type_fragment(return_type)
        ),
        TypeAnn::AnonStruct(fields) => anon_struct_name(fields),
        TypeAnn::StringLiteralUnion(keys) => keys.join("_"),
    }
}

fn collect_anon_structs(
    module: &Module,
    named_struct_defs: &HashMap<String, Vec<(String, TypeAnn)>>,
) -> HashMap<String, Vec<(String, TypeAnn)>> {
    let mut out = HashMap::new();
    let mut scopes = vec![HashMap::new()];
    for stmt in &module.stmts {
        collect_anon_structs_stmt(stmt, named_struct_defs, &mut scopes, &mut out);
    }
    out
}

fn collect_anon_structs_stmt(
    stmt: &Stmt,
    named_struct_defs: &HashMap<String, Vec<(String, TypeAnn)>>,
    scopes: &mut Vec<HashMap<String, TypeAnn>>,
    out: &mut HashMap<String, Vec<(String, TypeAnn)>>,
) {
    match stmt {
        Stmt::Let {
            pat,
            type_ann,
            value,
            ..
        } => {
            collect_anon_structs_expr(value, named_struct_defs, scopes, out);
            let inferred = type_ann
                .clone()
                .or_else(|| infer_expr_type_ann_for_scan(value, named_struct_defs, scopes, out));
            if let Some(ann) = inferred {
                register_pat_type(pat, ann, scopes, out);
            }
        }
        Stmt::State {
            name,
            type_ann,
            value,
            ..
        }
        | Stmt::Const {
            name,
            type_ann,
            value,
            ..
        } => {
            collect_anon_structs_expr(value, named_struct_defs, scopes, out);
            if let Some(ann) = type_ann
                .clone()
                .or_else(|| infer_expr_type_ann_for_scan(value, named_struct_defs, scopes, out))
            {
                register_anon_ann(&ann, out);
                if let Some(scope) = scopes.last_mut() {
                    scope.insert(name.clone(), ann);
                }
            }
        }
        Stmt::Fn {
            params,
            return_type,
            body,
            ..
        } => {
            if let Some(ann) = return_type {
                register_anon_ann(ann, out);
            }
            scopes.push(HashMap::new());
            if let Some(scope) = scopes.last_mut() {
                for param in params {
                    if let Some(ann) = &param.type_ann {
                        register_anon_ann(ann, out);
                        scope.insert(param.name.clone(), ann.clone());
                    }
                }
            }
            collect_anon_structs_expr(body, named_struct_defs, scopes, out);
            scopes.pop();
        }
        Stmt::Expr(expr) => collect_anon_structs_expr(expr, named_struct_defs, scopes, out),
        Stmt::Return(Some(expr), _) => {
            collect_anon_structs_expr(expr, named_struct_defs, scopes, out)
        }
        Stmt::Return(None, _) => {}
        Stmt::StructDef { fields, .. } | Stmt::DataDef { fields, .. } => {
            for (_, ann) in fields {
                register_anon_ann(ann, out);
            }
        }
        Stmt::EnumDef { variants, .. } => {
            for variant in variants {
                match variant {
                    EnumVariant::Unit(_) => {}
                    EnumVariant::Tuple(_, tys) => {
                        for ann in tys {
                            register_anon_ann(ann, out);
                        }
                    }
                    EnumVariant::Struct(_, fields) => {
                        for (_, ann) in fields {
                            register_anon_ann(ann, out);
                        }
                    }
                }
            }
        }
        Stmt::ImplBlock {
            methods, operators, ..
        } => {
            for method in methods {
                if let Some(ann) = &method.return_type {
                    register_anon_ann(ann, out);
                }
                collect_anon_structs_expr(&method.body, named_struct_defs, scopes, out);
            }
            for operator in operators {
                if let Some(ann) = &operator.return_type {
                    register_anon_ann(ann, out);
                }
                collect_anon_structs_expr(&operator.body, named_struct_defs, scopes, out);
            }
        }
        Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
            for method in methods {
                if let Some(ann) = &method.return_type {
                    register_anon_ann(ann, out);
                }
                collect_anon_structs_expr(&method.body, named_struct_defs, scopes, out);
            }
        }
        Stmt::TraitDef { methods, .. } => {
            for method in methods {
                match method {
                    TraitMethod::Abstract {
                        params,
                        return_type,
                        ..
                    }
                    | TraitMethod::Default {
                        params,
                        return_type,
                        ..
                    } => {
                        for param in params {
                            if let Some(ann) = &param.type_ann {
                                register_anon_ann(ann, out);
                            }
                        }
                        if let Some(ann) = return_type {
                            register_anon_ann(ann, out);
                        }
                    }
                }
            }
        }
        Stmt::TypestateDef { fields, .. } => {
            for (_, ann) in fields {
                register_anon_ann(ann, out);
            }
        }
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            scopes.push(HashMap::new());
            for stmt in body {
                collect_anon_structs_stmt(stmt, named_struct_defs, scopes, out);
            }
            scopes.pop();
        }
        Stmt::UseDecl { .. } | Stmt::UseRaw { .. } | Stmt::Yield { .. } | Stmt::Defer { .. } => {}
    }
}

fn collect_anon_structs_expr(
    expr: &Expr,
    named_struct_defs: &HashMap<String, Vec<(String, TypeAnn)>>,
    scopes: &mut Vec<HashMap<String, TypeAnn>>,
    out: &mut HashMap<String, Vec<(String, TypeAnn)>>,
) {
    match expr {
        Expr::AnonStruct { fields, .. } => {
            let anns = fields
                .iter()
                .map(|(field, expr)| {
                    let ann = expr
                        .as_ref()
                        .and_then(|expr| {
                            infer_expr_type_ann_for_scan(expr, named_struct_defs, scopes, out)
                        })
                        .or_else(|| lookup_scan_type(scopes, field).cloned())
                        .unwrap_or(TypeAnn::Named("unknown".to_string()));
                    (field.clone(), ann)
                })
                .collect::<Vec<_>>();
            register_anon_ann(&TypeAnn::AnonStruct(anns), out);
            for (_, expr) in fields {
                if let Some(expr) = expr {
                    collect_anon_structs_expr(expr, named_struct_defs, scopes, out);
                }
            }
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields {
                collect_anon_structs_expr(expr, named_struct_defs, scopes, out);
            }
        }
        Expr::Field { object, .. }
        | Expr::Await { expr: object, .. }
        | Expr::Question(object, _) => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
        }
        Expr::OptionalChain { object, chain, .. } => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
            if let ChainKind::Method { args, .. } = chain {
                for arg in args {
                    collect_anon_structs_expr(arg, named_struct_defs, scopes, out);
                }
            }
        }
        Expr::NullCoalesce { value, default, .. } => {
            collect_anon_structs_expr(value, named_struct_defs, scopes, out);
            collect_anon_structs_expr(default, named_struct_defs, scopes, out);
        }
        Expr::BinOp { left, right, .. } => {
            collect_anon_structs_expr(left, named_struct_defs, scopes, out);
            collect_anon_structs_expr(right, named_struct_defs, scopes, out);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_anon_structs_expr(operand, named_struct_defs, scopes, out);
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            collect_anon_structs_expr(cond, named_struct_defs, scopes, out);
            collect_anon_structs_expr(then_block, named_struct_defs, scopes, out);
            if let Some(expr) = else_block {
                collect_anon_structs_expr(expr, named_struct_defs, scopes, out);
            }
        }
        Expr::For { iter, body, .. }
        | Expr::While {
            cond: iter, body, ..
        } => {
            collect_anon_structs_expr(iter, named_struct_defs, scopes, out);
            collect_anon_structs_expr(body, named_struct_defs, scopes, out);
        }
        Expr::Loop { body, .. } => collect_anon_structs_expr(body, named_struct_defs, scopes, out),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_anon_structs_expr(scrutinee, named_struct_defs, scopes, out);
            for arm in arms {
                collect_anon_structs_expr(&arm.body, named_struct_defs, scopes, out);
            }
        }
        Expr::Block { stmts, tail, .. } => {
            scopes.push(HashMap::new());
            for stmt in stmts {
                collect_anon_structs_stmt(stmt, named_struct_defs, scopes, out);
            }
            if let Some(tail) = tail {
                collect_anon_structs_expr(tail, named_struct_defs, scopes, out);
            }
            scopes.pop();
        }
        Expr::Call { callee, args, .. } => {
            collect_anon_structs_expr(callee, named_struct_defs, scopes, out);
            for arg in args {
                collect_anon_structs_expr(arg, named_struct_defs, scopes, out);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
            for arg in args {
                collect_anon_structs_expr(arg, named_struct_defs, scopes, out);
            }
        }
        Expr::List(items, _) | Expr::SetLiteral { items, .. } => {
            for item in items {
                collect_anon_structs_expr(item, named_struct_defs, scopes, out);
            }
        }
        Expr::MapLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_anon_structs_expr(key, named_struct_defs, scopes, out);
                collect_anon_structs_expr(value, named_struct_defs, scopes, out);
            }
        }
        Expr::Assign { value, .. } => {
            collect_anon_structs_expr(value, named_struct_defs, scopes, out)
        }
        Expr::IndexAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
            collect_anon_structs_expr(index, named_struct_defs, scopes, out);
            collect_anon_structs_expr(value, named_struct_defs, scopes, out);
        }
        Expr::Index { object, index, .. } => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
            collect_anon_structs_expr(index, named_struct_defs, scopes, out);
        }
        Expr::FieldAssign { object, value, .. } => {
            collect_anon_structs_expr(object, named_struct_defs, scopes, out);
            collect_anon_structs_expr(value, named_struct_defs, scopes, out);
        }
        Expr::EnumInit { data, .. } => match data {
            EnumInitData::None => {}
            EnumInitData::Tuple(items) => {
                for item in items {
                    collect_anon_structs_expr(item, named_struct_defs, scopes, out);
                }
            }
            EnumInitData::Struct(fields) => {
                for (_, expr) in fields {
                    collect_anon_structs_expr(expr, named_struct_defs, scopes, out);
                }
            }
        },
        Expr::Closure { body, .. } | Expr::Spawn { body, .. } => {
            collect_anon_structs_expr(body, named_struct_defs, scopes, out);
        }
        Expr::Interpolation { .. }
        | Expr::Range { .. }
        | Expr::Pipeline { .. }
        | Expr::Break { .. }
        | Expr::Ident(_, _)
        | Expr::Literal(_, _) => {}
    }
}

fn infer_expr_type_ann_for_scan(
    expr: &Expr,
    named_struct_defs: &HashMap<String, Vec<(String, TypeAnn)>>,
    scopes: &[HashMap<String, TypeAnn>],
    out: &mut HashMap<String, Vec<(String, TypeAnn)>>,
) -> Option<TypeAnn> {
    match expr {
        Expr::Literal(Literal::Int(_), _) => Some(TypeAnn::Number),
        Expr::Literal(Literal::Float(_), _) => Some(TypeAnn::Float),
        Expr::Literal(Literal::String(_), _) => Some(TypeAnn::String),
        Expr::Literal(Literal::Bool(_), _) => Some(TypeAnn::Bool),
        Expr::Ident(name, _) => lookup_scan_type(scopes, name).cloned(),
        Expr::StructInit { name, .. } => Some(TypeAnn::Named(name.clone())),
        Expr::AnonStruct { fields, .. } => {
            let anns = fields
                .iter()
                .map(|(field, expr)| {
                    let ann = expr
                        .as_ref()
                        .and_then(|expr| {
                            infer_expr_type_ann_for_scan(expr, named_struct_defs, scopes, out)
                        })
                        .or_else(|| lookup_scan_type(scopes, field).cloned())
                        .unwrap_or(TypeAnn::Named("unknown".to_string()));
                    (field.clone(), ann)
                })
                .collect::<Vec<_>>();
            let ann = TypeAnn::AnonStruct(anns);
            register_anon_ann(&ann, out);
            Some(ann)
        }
        Expr::Field { object, field, .. } => {
            match infer_expr_type_ann_for_scan(object, named_struct_defs, scopes, out) {
                Some(TypeAnn::Named(name)) => named_struct_defs
                    .get(&name)
                    .and_then(|fields| fields.iter().find(|(f, _)| f == field))
                    .map(|(_, ann)| ann.clone()),
                Some(TypeAnn::AnonStruct(fields)) => fields
                    .into_iter()
                    .find(|(f, _)| f == field)
                    .map(|(_, ann)| ann),
                _ => None,
            }
        }
        Expr::BinOp {
            op, left, right, ..
        } => match op {
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Le
            | BinOp::Ge
            | BinOp::And
            | BinOp::Or => Some(TypeAnn::Bool),
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                let left_ty = infer_expr_type_ann_for_scan(left, named_struct_defs, scopes, out);
                let right_ty = infer_expr_type_ann_for_scan(right, named_struct_defs, scopes, out);
                if matches!(left_ty, Some(TypeAnn::Float))
                    || matches!(right_ty, Some(TypeAnn::Float))
                {
                    Some(TypeAnn::Float)
                } else {
                    Some(TypeAnn::Number)
                }
            }
        },
        Expr::If {
            then_block,
            else_block,
            ..
        } => {
            let then_ty = infer_expr_type_ann_for_scan(then_block, named_struct_defs, scopes, out);
            let else_ty = else_block.as_ref().and_then(|expr| {
                infer_expr_type_ann_for_scan(expr, named_struct_defs, scopes, out)
            });
            if then_ty == else_ty {
                then_ty
            } else {
                then_ty.or(else_ty)
            }
        }
        Expr::Block { tail, .. } => tail
            .as_ref()
            .and_then(|tail| infer_expr_type_ann_for_scan(tail, named_struct_defs, scopes, out)),
        _ => None,
    }
}

fn register_pat_type(
    pat: &Pat,
    ann: TypeAnn,
    scopes: &mut [HashMap<String, TypeAnn>],
    out: &mut HashMap<String, Vec<(String, TypeAnn)>>,
) {
    register_anon_ann(&ann, out);
    if let Some(scope) = scopes.last_mut() {
        match pat {
            Pat::Ident(name) | Pat::Rest(name) => {
                scope.insert(name.clone(), ann);
            }
            Pat::Wildcard => {}
            Pat::Tuple(items) | Pat::List(items) => {
                if let TypeAnn::List(inner) = ann {
                    for item in items {
                        register_pat_type(item, inner.as_ref().clone(), scopes, out);
                    }
                }
            }
        }
    }
}

fn register_anon_ann(ann: &TypeAnn, out: &mut HashMap<String, Vec<(String, TypeAnn)>>) {
    match ann {
        TypeAnn::AnonStruct(fields) => {
            for (_, field_ann) in fields {
                register_anon_ann(field_ann, out);
            }
            out.entry(anon_struct_name(fields))
                .or_insert_with(|| fields.clone());
        }
        TypeAnn::Option(inner)
        | TypeAnn::Result(inner)
        | TypeAnn::List(inner)
        | TypeAnn::Set(inner)
        | TypeAnn::OrderedSet(inner)
        | TypeAnn::Generate(inner) => register_anon_ann(inner, out),
        TypeAnn::ResultWith(a, b) | TypeAnn::Map(a, b) | TypeAnn::OrderedMap(a, b) => {
            register_anon_ann(a, out);
            register_anon_ann(b, out);
        }
        TypeAnn::Generic { args, .. } => {
            for arg in args {
                register_anon_ann(arg, out);
            }
        }
        TypeAnn::Fn {
            params,
            return_type,
        } => {
            for param in params {
                register_anon_ann(param, out);
            }
            register_anon_ann(return_type, out);
        }
        TypeAnn::Number
        | TypeAnn::Float
        | TypeAnn::String
        | TypeAnn::Bool
        | TypeAnn::Named(_)
        | TypeAnn::Unit
        | TypeAnn::StringLiteralUnion(_) => {}
    }
}

fn lookup_scan_type<'a>(scopes: &'a [HashMap<String, TypeAnn>], name: &str) -> Option<&'a TypeAnn> {
    scopes.iter().rev().find_map(|scope| scope.get(name))
}

fn collect_utility_types(module: &Module) -> Vec<UtilitySpec> {
    let mut out = Vec::new();
    for stmt in &module.stmts {
        collect_utility_types_stmt(stmt, &mut out);
    }
    out
}

fn collect_utility_types_stmt(stmt: &Stmt, out: &mut Vec<UtilitySpec>) {
    match stmt {
        Stmt::Let { type_ann, .. }
        | Stmt::State { type_ann, .. }
        | Stmt::Const { type_ann, .. } => {
            if let Some(ann) = type_ann {
                collect_utility_types_ann(ann, out);
            }
        }
        Stmt::Fn {
            params,
            return_type,
            ..
        } => {
            for p in params {
                if let Some(ann) = &p.type_ann {
                    collect_utility_types_ann(ann, out);
                }
            }
            if let Some(ann) = return_type {
                collect_utility_types_ann(ann, out);
            }
        }
        Stmt::StructDef { fields, .. } | Stmt::DataDef { fields, .. } => {
            for (_, ann) in fields {
                collect_utility_types_ann(ann, out);
            }
        }
        Stmt::EnumDef { variants, .. } => {
            for variant in variants {
                match variant {
                    EnumVariant::Unit(_) => {}
                    EnumVariant::Tuple(_, tys) => {
                        for ann in tys {
                            collect_utility_types_ann(ann, out);
                        }
                    }
                    EnumVariant::Struct(_, fields) => {
                        for (_, ann) in fields {
                            collect_utility_types_ann(ann, out);
                        }
                    }
                }
            }
        }
        Stmt::ImplBlock {
            methods, operators, ..
        } => {
            for method in methods {
                for p in &method.params {
                    if let Some(ann) = &p.type_ann {
                        collect_utility_types_ann(ann, out);
                    }
                }
                if let Some(ann) = &method.return_type {
                    collect_utility_types_ann(ann, out);
                }
            }
            for operator in operators {
                for p in &operator.params {
                    if let Some(ann) = &p.type_ann {
                        collect_utility_types_ann(ann, out);
                    }
                }
                if let Some(ann) = &operator.return_type {
                    collect_utility_types_ann(ann, out);
                }
            }
        }
        Stmt::MixinDef { methods, .. } | Stmt::ImplTrait { methods, .. } => {
            for method in methods {
                for p in &method.params {
                    if let Some(ann) = &p.type_ann {
                        collect_utility_types_ann(ann, out);
                    }
                }
                if let Some(ann) = &method.return_type {
                    collect_utility_types_ann(ann, out);
                }
            }
        }
        Stmt::TraitDef { methods, .. } => {
            for method in methods {
                match method {
                    TraitMethod::Abstract {
                        params,
                        return_type,
                        ..
                    }
                    | TraitMethod::Default {
                        params,
                        return_type,
                        ..
                    } => {
                        for p in params {
                            if let Some(ann) = &p.type_ann {
                                collect_utility_types_ann(ann, out);
                            }
                        }
                        if let Some(ann) = return_type {
                            collect_utility_types_ann(ann, out);
                        }
                    }
                }
            }
        }
        Stmt::TypestateDef { fields, .. } => {
            for (_, ann) in fields {
                collect_utility_types_ann(ann, out);
            }
        }
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for inner in body {
                collect_utility_types_stmt(inner, out);
            }
        }
        Stmt::Yield { .. } => {}
        Stmt::Return(_, _) | Stmt::Expr(_) | Stmt::UseDecl { .. } | Stmt::UseRaw { .. } => {}
        Stmt::Defer { .. } => {}
    }
}

fn collect_utility_types_ann(ann: &TypeAnn, out: &mut Vec<UtilitySpec>) {
    match ann {
        TypeAnn::Generic { name, args } => {
            match (name.as_str(), args.as_slice()) {
                ("Partial", [TypeAnn::Named(inner)]) => {
                    out.push(UtilitySpec::Partial(inner.clone()))
                }
                ("Required", [TypeAnn::Named(inner)]) => {
                    out.push(UtilitySpec::Required(inner.clone()))
                }
                ("Pick", [TypeAnn::Named(inner), TypeAnn::Named(keys)]) => {
                    out.push(UtilitySpec::Pick(inner.clone(), keys.clone()))
                }
                ("Pick", [TypeAnn::Named(inner), TypeAnn::StringLiteralUnion(keys)]) => {
                    out.push(UtilitySpec::Pick(inner.clone(), keys.join("_")))
                }
                ("Omit", [TypeAnn::Named(inner), TypeAnn::Named(keys)]) => {
                    out.push(UtilitySpec::Omit(inner.clone(), keys.clone()))
                }
                ("Omit", [TypeAnn::Named(inner), TypeAnn::StringLiteralUnion(keys)]) => {
                    out.push(UtilitySpec::Omit(inner.clone(), keys.join("_")))
                }
                _ => {}
            }
            for arg in args {
                collect_utility_types_ann(arg, out);
            }
        }
        TypeAnn::Option(inner)
        | TypeAnn::Result(inner)
        | TypeAnn::List(inner)
        | TypeAnn::Set(inner)
        | TypeAnn::OrderedSet(inner) => collect_utility_types_ann(inner, out),
        TypeAnn::ResultWith(a, b) | TypeAnn::Map(a, b) | TypeAnn::OrderedMap(a, b) => {
            collect_utility_types_ann(a, out);
            collect_utility_types_ann(b, out);
        }
        TypeAnn::Fn {
            params,
            return_type,
        } => {
            for p in params {
                collect_utility_types_ann(p, out);
            }
            collect_utility_types_ann(return_type, out);
        }
        TypeAnn::AnonStruct(fields) => {
            for (_, ann) in fields {
                collect_utility_types_ann(ann, out);
            }
        }
        TypeAnn::Generate(inner) => collect_utility_types_ann(inner, out),
        TypeAnn::Number
        | TypeAnn::Float
        | TypeAnn::String
        | TypeAnn::Bool
        | TypeAnn::Named(_)
        | TypeAnn::Unit
        | TypeAnn::StringLiteralUnion(_) => {}
    }
}

fn render_utility_struct(
    spec: &UtilitySpec,
    struct_map: &HashMap<String, Vec<(String, TypeAnn)>>,
) -> Option<String> {
    match spec {
        UtilitySpec::Partial(name) => {
            let fields = struct_map.get(name)?;
            Some(render_plain_struct(
                &format!("Partial{}", name),
                &fields
                    .iter()
                    .map(|(field, ty)| (field.clone(), TypeAnn::Option(Box::new(ty.clone()))))
                    .collect::<Vec<_>>(),
            ))
        }
        UtilitySpec::Required(name) => {
            let fields = struct_map.get(name)?;
            Some(render_plain_struct(
                &format!("Required{}", name),
                &fields
                    .iter()
                    .map(|(field, ty)| (field.clone(), strip_option_ann(ty)))
                    .collect::<Vec<_>>(),
            ))
        }
        UtilitySpec::Pick(name, keys) => {
            let fields = struct_map.get(name)?;
            let wanted = split_key_names(keys);
            Some(render_plain_struct(
                &format!("{}Pick_{}", name, sanitize_type_name(keys)),
                &fields
                    .iter()
                    .filter(|(field, _)| wanted.contains(field))
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
        }
        UtilitySpec::Omit(name, keys) => {
            let fields = struct_map.get(name)?;
            let omitted = split_key_names(keys);
            Some(render_plain_struct(
                &format!("{}Omit_{}", name, sanitize_type_name(keys)),
                &fields
                    .iter()
                    .filter(|(field, _)| !omitted.contains(field))
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
        }
    }
}

fn render_plain_struct(name: &str, fields: &[(String, TypeAnn)]) -> String {
    let mut out = format!("struct {} {{\n", name);
    for (field, ty) in fields {
        out.push_str(&format!("    {}: {},\n", field, type_ann_to_rust(ty)));
    }
    out.push_str("}\n");
    out
}

fn strip_option_ann(ann: &TypeAnn) -> TypeAnn {
    match ann {
        TypeAnn::Option(inner) => inner.as_ref().clone(),
        other => other.clone(),
    }
}

fn split_key_names(keys: &str) -> HashSet<String> {
    keys.split('_')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
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
    fn test_transpile_pipe_arrow() {
        let src = r#"
let nums = [1, 2, 3]
let total = nums
    |> filter(x => x > 1)
    |> sum()
"#;
        let out = transpile(src);
        assert!(out.contains("iter().filter("), "filter not found: {}", out);
        assert!(out.contains(".sum::<"), "sum not found: {}", out);
    }

    #[test]
    fn test_transpile_optional_chain_field() {
        let src = "let city = user?.name";
        let out = transpile(src);
        assert!(out.contains(".and_then(|v| Some(v.name))"));
    }

    #[test]
    fn test_transpile_operator_overload_vector2() {
        let src = r#"
struct Vec2 { x: number, y: number }
impl Vec2 {
    operator +(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
    operator unary-(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("impl std::ops::Add for Vec2"),
            "Add impl missing: {}",
            out
        );
        assert!(
            out.contains("impl std::ops::Neg for Vec2"),
            "Neg impl missing: {}",
            out
        );
    }

    #[test]
    fn test_transpile_operator_eq_index() {
        let src = r#"
struct Pair { left: number, right: number }
impl Pair {
    operator ==(self, other: Pair) -> bool {
        self.left == other.left && self.right == other.right
    }
    operator [](self, index: number) -> number {
        if index == 0 { self.left } else { self.right }
    }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("impl PartialEq for Pair"),
            "PartialEq impl missing: {}",
            out
        );
        assert!(
            out.contains("impl std::ops::Index<i64> for Pair"),
            "Index impl missing: {}",
            out
        );
    }

    #[test]
    fn test_transpile_optional_chain_method() {
        let src = "let len = name?.len()";
        let out = transpile(src);
        assert!(out.contains(".and_then(|v| Some(v.len()))"));
    }

    #[test]
    fn test_transpile_null_coalesce() {
        let src = r#"let city = user ?? "unknown""#;
        let out = transpile(src);
        assert!(out.contains(".unwrap_or(\"unknown\".to_string())"));
    }

    #[test]
    fn test_transpile_optional_chain_nested() {
        let src = "let city = user?.address?.city";
        let out = transpile(src);
        assert!(out.contains(".and_then(|v| Some(v.address))"));
        assert!(out.contains(".and_then(|v| Some(v.city))"));
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
    fn test_transpile_const_fn() {
        let src = r#"
const fn clamp(value: number) -> number {
    if value < 0 { 0 } else { value }
}
"#;
        let out = transpile(src);
        assert!(out.contains("const fn clamp"));
    }

    #[test]
    fn test_transpile_const_var_with_const_fn() {
        let src = r#"
const fn clamp(value: number) -> number {
    if value < 0 { 0 } else if value > 100 { 100 } else { value }
}
const MAX = clamp(150)
"#;
        let out = transpile(src);
        assert!(out.contains("const fn clamp"));
        assert!(out.contains("const MAX"));
        assert!(out.contains("clamp(150)"));
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
    fn snapshot_generic_struct() {
        let src = r#"
struct Response<T> {
    body: T
}
"#;
        let out = transpile(src);
        assert!(out.contains("struct Response<T>"), "got: {}", out);
        assert!(out.contains("body: T"), "got: {}", out);
    }

    #[test]
    fn snapshot_generic_fn() {
        let src = r#"
fn wrap<T>(value: T) -> Response<T> {
    value
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("fn wrap<T>(value: T) -> Response<T>"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_generic_impl() {
        let src = r#"
impl<T> Response<T> {
    fn is_ok() -> bool { true }
}
"#;
        let out = transpile(src);
        assert!(out.contains("impl<T> Response<T>"), "got: {}", out);
        assert!(out.contains("pub fn is_ok() -> bool"), "got: {}", out);
    }

    #[test]
    fn snapshot_generic_enum() {
        let src = r#"
enum Either<L, R> {
    Left(L)
    Right(R)
}
"#;
        let out = transpile(src);
        assert!(out.contains("enum Either<L, R>"), "got: {}", out);
        assert!(out.contains("Left(L)"), "got: {}", out);
        assert!(out.contains("Right(R)"), "got: {}", out);
    }

    #[test]
    fn snapshot_map_type() {
        let src = r#"
let m: map<string, number> = {"a": 1}
"#;
        let out = transpile(src);
        assert!(
            out.contains("std::collections::HashMap<String, i64>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("HashMap::from([(\"a\".to_string(), 1)])"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_set_type() {
        let src = r#"
let s: set<string> = {"a", "b"}
"#;
        let out = transpile(src);
        assert!(
            out.contains("std::collections::HashSet<String>"),
            "got: {}",
            out
        );
        assert!(
            out.contains("HashSet::from([\"a\".to_string(), \"b\".to_string()])"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_map_methods_and_index() {
        let src = r#"
state m: map<string, number> = {"a": 1}
let a = m.get("a")
m.insert("b", 2)
let c = m["a"]
m["d"] = 4
"#;
        let out = transpile(src);
        assert!(
            out.contains(".get(&\"a\".to_string()).cloned()"),
            "got: {}",
            out
        );
        assert!(
            out.contains(".insert(\"b\".to_string(), 2)"),
            "got: {}",
            out
        );
        assert!(out.contains("m[&\"a\".to_string()]"), "got: {}", out);
        assert!(
            out.contains("m.insert(\"d\".to_string(), 4)"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_partial_type() {
        let src = r#"
let u: Partial<User> = Partial::from(user)
"#;
        let out = transpile(src);
        assert!(out.contains("PartialUser"), "got: {}", out);
    }

    #[test]
    fn snapshot_pick_type() {
        assert_eq!(
            type_ann_to_rust(&TypeAnn::Generic {
                name: "Pick".to_string(),
                args: vec![
                    TypeAnn::Named("User".to_string()),
                    TypeAnn::Named("id_name".to_string())
                ]
            }),
            "UserPick_id_name"
        );
    }

    #[test]
    fn snapshot_omit_type() {
        assert_eq!(
            type_ann_to_rust(&TypeAnn::Generic {
                name: "Omit".to_string(),
                args: vec![
                    TypeAnn::Named("User".to_string()),
                    TypeAnn::Named("password".to_string())
                ]
            }),
            "UserOmit_password"
        );
    }

    #[test]
    fn snapshot_pick_type_string_literal_union() {
        // spec 準拠: Pick<User, "id" | "name"> 形式
        assert_eq!(
            type_ann_to_rust(&TypeAnn::Generic {
                name: "Pick".to_string(),
                args: vec![
                    TypeAnn::Named("User".to_string()),
                    TypeAnn::StringLiteralUnion(vec!["id".to_string(), "name".to_string()])
                ]
            }),
            "UserPick_id_name"
        );
    }

    #[test]
    fn snapshot_omit_type_string_literal_union() {
        // spec 準拠: Omit<User, "password"> 形式
        assert_eq!(
            type_ann_to_rust(&TypeAnn::Generic {
                name: "Omit".to_string(),
                args: vec![
                    TypeAnn::Named("User".to_string()),
                    TypeAnn::StringLiteralUnion(vec!["password".to_string()])
                ]
            }),
            "UserOmit_password"
        );
    }

    #[test]
    fn snapshot_pick_struct_generation_string_literal_union() {
        // spec 準拠構文 Pick<User, "id" | "name"> でパーサーを通して struct が生成されること
        let src = r#"
struct User {
    id: number
    name: string
    password: string
}

let a: Pick<User, "id" | "name"> = none
let b: Omit<User, "password"> = none
"#;
        let out = transpile(src);
        assert!(out.contains("struct UserPick_id_name"), "got: {}", out);
        assert!(out.contains("id: i64"), "got: {}", out);
        assert!(out.contains("name: String"), "got: {}", out);
        assert!(out.contains("struct UserOmit_password"), "got: {}", out);
    }

    #[test]
    fn snapshot_collection_use_insertion() {
        let src = r#"
let m: map<string, number> = {"a": 1}
let s: set<string> = {"x", "y"}
"#;
        let out = transpile(src);
        assert!(
            out.contains("use std::collections::{HashMap, HashSet};"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_partial_required_generated_structs() {
        let src = r#"
struct User {
    id: number
    name: string
}

struct Config {
    host: string?
}

let a: Partial<User> = none
let b: Required<Config> = none
"#;
        let out = transpile(src);
        assert!(out.contains("struct PartialUser"), "got: {}", out);
        assert!(out.contains("id: Option<i64>"), "got: {}", out);
        assert!(out.contains("name: Option<String>"), "got: {}", out);
        assert!(out.contains("struct RequiredConfig"), "got: {}", out);
        assert!(out.contains("host: String"), "got: {}", out);
    }

    #[test]
    fn snapshot_pick_omit_generated_structs() {
        let src = r#"
struct User {
    id: number
    name: string
    password: string
}

let a: Pick<User, id_name> = none
let b: Omit<User, password> = none
"#;
        let out = transpile(src);
        assert!(out.contains("struct UserPick_id_name"), "got: {}", out);
        assert!(out.contains("id: i64"), "got: {}", out);
        assert!(out.contains("name: String"), "got: {}", out);
        assert!(out.contains("struct UserOmit_password"), "got: {}", out);
        assert!(
            !out.contains("password: String,\n}\n\nstruct UserOmit_password"),
            "got: {}",
            out
        );
    }

    #[test]
    fn snapshot_utility_struct_dedup() {
        let src = r#"
struct User {
    id: number
}

let a: Partial<User> = none
let b: Partial<User> = none
"#;
        let out = transpile(src);
        assert_eq!(out.matches("struct PartialUser").count(), 1, "got: {}", out);
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
    fn use_stdlib_crypto_snapshot() {
        let src = r#"
use forge/std/crypto.{ hash, hmac_verify, HashAlgo }

let digest = hash("password", HashAlgo::Sha256)
let verified = hmac_verify("payload", "mac", "secret", HashAlgo::Sha256)
"#;
        let out = transpile(src);
        assert!(
            out.contains("use forge_std::forge::std::crypto::{hash, hmac_verify, HashAlgo};"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let digest = hash(\"password\".to_string(), HashAlgo::Sha256);"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let verified = hmac_verify(\"payload\".to_string(), \"mac\".to_string(), \"secret\".to_string(), HashAlgo::Sha256);"),
            "got: {}",
            out
        );
    }

    #[test]
    fn use_stdlib_compress_snapshot() {
        let src = r#"
use forge/std/compress.{ compress_str, CompressAlgo }

let compressed = compress_str("payload", CompressAlgo::Gzip)
"#;
        let out = transpile(src);
        assert!(
            out.contains("use forge_std::forge::std::compress::{compress_str, CompressAlgo};"),
            "got: {}",
            out
        );
        assert!(
            out.contains(
                "let compressed = compress_str(\"payload\".to_string(), CompressAlgo::Gzip);"
            ),
            "got: {}",
            out
        );
    }

    #[test]
    fn use_stdlib_wasm_snapshot() {
        let src = r#"
use forge/std/wasm.Wasm

let app = Wasm::load("dist/app.wasm")
let html = app.call("render", "json")
"#;
        let out = transpile(src);
        assert!(
            out.contains("use forge_std::forge::std::wasm::Wasm;"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let app = Wasm::load(\"dist/app.wasm\".to_string());"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let html = app.call(\"render\".to_string(), \"json\".to_string());"),
            "got: {}",
            out
        );
    }

    #[test]
    fn use_stdlib_wasm_options_snapshot() {
        let src = r#"
use forge/std/wasm.{ Wasm, WasmOptions }

let opts = WasmOptions::trusted()
let app = Wasm::load_with("dist/app.wasm", opts)
"#;
        let out = transpile(src);
        assert!(
            out.contains("use forge_std::forge::std::wasm::{Wasm, WasmOptions};"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let opts = WasmOptions::trusted();"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let app = Wasm::load_with(\"dist/app.wasm\".to_string(), opts);"),
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
    fn async_spawn_block() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(3) }
}

let handle = spawn {
    fetch_num().await
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("tokio::spawn(async move {"),
            "spawn not found: {}",
            out
        );
        assert!(
            out.contains("fetch_num().await"),
            "await not found: {}",
            out
        );
    }

    #[test]
    fn test_transpile_spawn() {
        let src = r#"
let handle = spawn {
    let offset = 1
    offset + 2
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("tokio::spawn(async move {"),
            "spawn not found: {}",
            out
        );
        assert!(out.contains("offset + 2"), "body not found: {}", out);
    }

    #[test]
    fn test_transpile_spawn_handle_await() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(3) }
}

let handle = spawn {
    fetch_num().await
}
let value = handle.await?
"#;
        let out = transpile(src);
        assert!(
            out.contains("tokio::spawn(async move {"),
            "spawn not found: {}",
            out
        );
        assert!(
            out.contains("handle.await?"),
            "await handle not found: {}",
            out
        );
    }

    #[test]
    fn test_transpile_async_closure() {
        let src = r#"
use raw {
    async fn fetch_num() -> Result<i64, anyhow::Error> { Ok(1) }
}

let f = () => fetch_num().await
"#;
        let out = transpile(src);
        assert!(
            out.contains("|| async move {"),
            "async closure not generated: {}",
            out
        );
        assert!(out.contains("fetch_num().await"), "await missing: {}", out);
    }

    #[test]
    fn test_transpile_generator_fibonacci() {
        let src = r#"
fn fibonacci() -> generate<number> {
    state a = 0
    state b = 1
    loop {
        if a > 10 {
            return
        }
        yield a
        let next = a + b
        a = b
        b = next
    }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains(" -> impl Iterator<Item = i64>"),
            "expected iterator return: {}",
            out
        );
        assert!(out.contains("Vec::new()"), "buffer not found: {}", out);
        assert!(
            out.contains("std::iter::from_fn"),
            "from_fn missing: {}",
            out
        );
    }

    #[test]
    fn test_transpile_generator_with_take() {
        let src = r#"
fn ones() -> generate<number> {
    yield 1
}
let result = ones().take(3)
"#;
        let out = transpile(src);
        assert!(
            out.contains("std::iter::from_fn"),
            "from_fn missing: {}",
            out
        );
        assert!(out.contains(".take(3)"), "take not found: {}", out);
    }

    #[test]
    fn test_transpile_destructure_tuple() {
        let src = r#"
let (a, b) = [1, 2]
a + b
"#;
        let out = transpile(src);
        assert!(
            out.contains("let _destructure = vec![1_i64, 2];"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let a = _destructure[0].clone();"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let b = _destructure[1].clone();"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_transpile_destructure_rest() {
        let src = r#"
let (head, ..tail) = [1, 2, 3]
head
"#;
        let out = transpile(src);
        assert!(
            out.contains("let head = _destructure[0].clone();"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let tail = _destructure[1..].to_vec();"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_transpile_anon_struct_return_type() {
        let src = r#"
fn summarize(name: string, score: number) -> { name: string, score: number } {
    { name, score }
}
"#;
        let out = transpile(src);
        assert!(
            out.contains("struct AnonStruct_name_string_score_number"),
            "got: {}",
            out
        );
        assert!(
            out.contains(
                "fn summarize(name: String, score: i64) -> AnonStruct_name_string_score_number"
            ),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_transpile_anon_struct_literal() {
        let src = r#"
let user = { name: "Alice", score: 92 }
user
"#;
        let out = transpile(src);
        assert!(
            out.contains("struct AnonStruct_name_string_score_number"),
            "got: {}",
            out
        );
        assert!(
            out.contains("let user = AnonStruct_name_string_score_number { name: \"Alice\".to_string(), score: 92"),
            "got: {}",
            out
        );
    }

    #[test]
    fn test_transpile_anon_struct_dedup() {
        let src = r#"
fn a() -> { name: string, score: number } { { name: "Alice", score: 92 } }
fn b() -> { name: string, score: number } { { name: "Bob", score: 78 } }
"#;
        let out = transpile(src);
        assert_eq!(
            out.matches("struct AnonStruct_name_string_score_number")
                .count(),
            1,
            "got: {}",
            out
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
