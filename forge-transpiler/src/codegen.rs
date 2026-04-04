use std::collections::BTreeSet;

use forge_compiler::ast::{
    BinOp, Constraint, EnumInitData, EnumVariant, Expr, FnDef, InterpPart, Literal, MatchArm,
    Module, Param, Pattern, Stmt, TraitMethod, TypeAnn, UnaryOp, UsePath, UseSymbols,
    ValidateRule, WhenCondition,
};

use crate::builtin::{try_builtin_call, try_constructor_call};

pub struct CodeGenerator {
    indent: usize,
    rename_main: bool,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            indent: 0,
            rename_main: false,
        }
    }

    pub fn generate_module(&mut self, module: &Module) -> String {
        self.rename_main = module
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Fn { name, .. } if name == "main"));

        let mut out = String::new();

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
            out.push_str("fn main() -> Result<(), anyhow::Error> {\n");
            self.indent += 1;

            if self.rename_main && main_body.is_empty() {
                out.push_str(&self.indent_str());
                out.push_str("forge_main();\n");
            }

            for stmt in main_body {
                out.push_str(&self.gen_stmt(stmt));
            }

            out.push_str(&self.indent_str());
            out.push_str("Ok(())\n");
            self.indent -= 1;
            out.push_str("}\n");
        }

        out
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
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
                format!("{}let {}{} = {};\n", self.indent_str(), name, ty, val)
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
            } => self.gen_fn(name, params, return_type, body, *is_pub),
            Stmt::Return(Some(expr), _) => {
                format!("{}return {};\n", self.indent_str(), self.gen_expr(expr, false))
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
            Stmt::TypestateDef { name, .. } => {
                format!("{}// typestate {} (transpile pending)\n", self.indent_str(), name)
            }
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

        let mut out = format!(
            "{}{}fn {}({}){} {{\n",
            self.indent_str(),
            Self::vis(is_pub),
            fn_name,
            params_str,
            ret_str
        );
        out.push_str(&self.gen_block_body(body, false));
        out.push_str(&self.indent_str());
        out.push_str("}\n");
        out
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
        out.push_str(&self.gen_block_body(&method.body, false));
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
                let receiver = if *has_state_self { "&mut self" } else { "&self" };
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
                out.push_str(&self.gen_block_body(body, false));
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
        let mut out = format!(
            "{}#[cfg(test)]\n{}mod {} {{\n",
            self.indent_str(),
            self.indent_str(),
            module_name
        );
        self.indent += 1;
        out.push_str(&format!("{}use super::*;\n", self.indent_str()));
        out.push_str(&format!("{}#[test]\n", self.indent_str()));
        out.push_str(&format!("{}fn {}() {{\n", self.indent_str(), fn_name));
        self.indent += 1;
        for stmt in body {
            out.push_str(&self.gen_stmt(stmt));
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

        match expr {
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    out.push_str(&self.gen_stmt(stmt));
                }
                if let Some(tail) = tail {
                    if is_main {
                        out.push_str(&self.gen_stmt(&Stmt::Expr((**tail).clone())));
                    } else {
                        out.push_str(&format!("{}{}\n", self.indent_str(), self.gen_expr(tail, false)));
                    }
                }
            }
            other => {
                if is_main {
                    out.push_str(&self.gen_stmt(&Stmt::Expr(other.clone())));
                } else {
                    out.push_str(&format!("{}{}\n", self.indent_str(), self.gen_expr(other, false)));
                }
            }
        }

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
                op,
                left,
                right,
                ..
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

    fn gen_if(
        &mut self,
        cond: &Expr,
        then_block: &Expr,
        else_block: &Option<Box<Expr>>,
    ) -> String {
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
                expr => format!("if {} {} else {}", cond_str, then_str, self.gen_inline_block(expr)),
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
                    inner.push_str(&format!("{}{}\n", self.indent_str(), self.gen_expr(tail, false)));
                }
            }
            other => {
                inner.push_str(&format!("{}{}\n", self.indent_str(), self.gen_expr(other, false)));
            }
        }
        self.indent -= 1;
        inner.push_str(&format!("{}}}", self.indent_str()));
        inner
    }

    fn gen_while(&mut self, cond: &Expr, body: &Expr) -> String {
        format!("while {} {}", self.gen_expr(cond, false), self.gen_inline_block(body))
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
            out.push_str(&format!("{}{}\n", self.indent_str(), self.gen_expr(tail, false)));
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

        format!("{}({})", self.gen_expr(callee, false), arg_strs.join(", "))
    }

    fn gen_method_call(&mut self, object: &Expr, method: &str, args: &[Expr]) -> String {
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
                format!("{}.iter().filter(|x| ({})(x)).collect::<Vec<_>>()", object, f)
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

    fn gen_closure(&mut self, params: &[String], body: &Expr) -> String {
        format!("|{}| {}", params.join(", "), self.gen_expr(body, false))
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
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '_' })
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
            format!("Result<{}, {}>", type_ann_to_rust(inner), type_ann_to_rust(err))
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
        let module = parse_source(src).unwrap_or_else(|e| panic!("parse failed for input {:?}: {}", src, e));
        let mut gen = CodeGenerator::new();
        gen.generate_module(&module)
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
        assert!(out.contains("fn add(a: i64, b: i64) -> i64"), "got: {}", out);
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
        assert!(out.contains("pub fn instance() -> &'static Self"), "got: {}", out);
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
        assert!(out.contains("fn validate(&self) -> Result<(), String>"), "got: {}", out);
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
        assert!(out.contains("use crate::utils::helper::add;"), "got: {}", out);
        assert!(out.contains("#[cfg(target_os = \"windows\")]"), "got: {}", out);
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
        assert!(out.contains("pub fn validate(&self) -> Result<(), String>"), "got: {}", out);
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
        assert!(out.contains("use crate::utils::helper::{add, subtract as sub};"), "got: {}", out);
    }

    #[test]
    fn use_external_snapshot() {
        let src = r#"
use serde.{Serialize}
"#;
        let out = transpile(src);
        assert!(out.contains("use serde::{Serialize};") || out.contains("use serde::Serialize;"), "got: {}", out);
    }

    #[test]
    fn when_platform_snapshot() {
        let src = r#"
when platform.linux {
    fn os_name() -> string { "linux" }
}
"#;
        let out = transpile(src);
        assert!(out.contains("#[cfg(target_os = \"linux\")]"), "got: {}", out);
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
}
