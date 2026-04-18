use std::collections::HashMap;

use forge_compiler::ast::{Expr, Literal, Module, Pat, Stmt, TypeAnn};

use crate::graph::{
    DataShape, DataState, Diagnostic, NodeDataInfo, NodeStatus, PipelineGraph, TypeSummary,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinSig {
    pub type_name: String,
    pub method_name: String,
    pub input_type_param: String,
    pub output_type: String,
}

#[derive(Debug, Clone)]
pub struct TypeAnnotations {
    bindings: HashMap<String, ResolvedType>,
    structs: HashMap<String, Vec<(String, TypeAnn)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedType {
    display: String,
    shape: DataShape,
    nullable: bool,
    fallible: bool,
}

impl ResolvedType {
    fn summary(&self) -> TypeSummary {
        TypeSummary {
            display: self.display.clone(),
            nullable: self.nullable,
            fallible: self.fallible,
        }
    }

    fn scalar(name: &str) -> Self {
        Self {
            display: name.to_string(),
            shape: DataShape::Scalar(name.to_string()),
            nullable: false,
            fallible: false,
        }
    }

    fn list(inner: ResolvedType) -> Self {
        Self {
            display: format!("list<{}>", inner.display),
            shape: DataShape::List(Box::new(inner.shape)),
            nullable: false,
            fallible: false,
        }
    }

    fn option(inner: ResolvedType) -> Self {
        Self {
            display: format!("{}?", inner.display),
            shape: DataShape::Option(Box::new(inner.shape)),
            nullable: true,
            fallible: inner.fallible,
        }
    }

    fn result(inner: ResolvedType) -> Self {
        Self {
            display: format!("{}!", inner.display),
            shape: DataShape::Result(Box::new(inner.shape)),
            nullable: false,
            fallible: true,
        }
    }

    fn unknown() -> Self {
        Self {
            display: "unknown".to_string(),
            shape: DataShape::Unknown,
            nullable: false,
            fallible: false,
        }
    }
}

pub fn builtin_sigs() -> Vec<BuiltinSig> {
    vec![
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "map".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<U>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "filter".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "find".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "take".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "skip".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "fold".to_string(),
            input_type_param: "T".to_string(),
            output_type: "U".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "zip".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<(T,U)>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "partition".to_string(),
            input_type_param: "T".to_string(),
            output_type: "(list<T>, list<T>)".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "group_by".to_string(),
            input_type_param: "T".to_string(),
            output_type: "map<K, list<T>>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "len".to_string(),
            input_type_param: "T".to_string(),
            output_type: "number".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "first".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "last".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "find_index".to_string(),
            input_type_param: "T".to_string(),
            output_type: "number?".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "any".to_string(),
            input_type_param: "T".to_string(),
            output_type: "bool".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "all".to_string(),
            input_type_param: "T".to_string(),
            output_type: "bool".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "sort".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "dedup".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "reverse".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "count".to_string(),
            input_type_param: "T".to_string(),
            output_type: "number".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "sum".to_string(),
            input_type_param: "T".to_string(),
            output_type: "number".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "join".to_string(),
            input_type_param: "string".to_string(),
            output_type: "string".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "flatten".to_string(),
            input_type_param: "list<T>".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "flat_map".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<U>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "enumerate".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<(number,T)>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "each".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "list".to_string(),
            method_name: "order_by".to_string(),
            input_type_param: "T".to_string(),
            output_type: "list<T>".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "map".to_string(),
            input_type_param: "T".to_string(),
            output_type: "U?".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "and_then".to_string(),
            input_type_param: "T".to_string(),
            output_type: "U?".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "unwrap_or".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "is_some".to_string(),
            input_type_param: "T".to_string(),
            output_type: "bool".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "is_none".to_string(),
            input_type_param: "T".to_string(),
            output_type: "bool".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "unwrap".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "or".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "option".to_string(),
            method_name: "filter".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "result".to_string(),
            method_name: "map".to_string(),
            input_type_param: "T".to_string(),
            output_type: "U!".to_string(),
        },
        BuiltinSig {
            type_name: "result".to_string(),
            method_name: "and_then".to_string(),
            input_type_param: "T".to_string(),
            output_type: "U!".to_string(),
        },
        BuiltinSig {
            type_name: "result".to_string(),
            method_name: "unwrap_or".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T".to_string(),
        },
        BuiltinSig {
            type_name: "result".to_string(),
            method_name: "ok".to_string(),
            input_type_param: "T".to_string(),
            output_type: "T?".to_string(),
        },
        BuiltinSig {
            type_name: "string".to_string(),
            method_name: "trim".to_string(),
            input_type_param: "string".to_string(),
            output_type: "string".to_string(),
        },
        BuiltinSig {
            type_name: "string".to_string(),
            method_name: "capitalize".to_string(),
            input_type_param: "string".to_string(),
            output_type: "string".to_string(),
        },
        BuiltinSig {
            type_name: "string".to_string(),
            method_name: "len".to_string(),
            input_type_param: "string".to_string(),
            output_type: "number".to_string(),
        },
        BuiltinSig {
            type_name: "string".to_string(),
            method_name: "chars".to_string(),
            input_type_param: "string".to_string(),
            output_type: "list<string>".to_string(),
        },
    ]
}

impl TypeAnnotations {
    pub fn collect(module: &Module) -> Self {
        let mut structs = HashMap::new();
        for stmt in &module.stmts {
            collect_structs(stmt, &mut structs);
        }

        let mut annotations = Self {
            bindings: HashMap::new(),
            structs,
        };

        for stmt in &module.stmts {
            annotations.collect_stmt(stmt);
        }

        annotations
    }

    fn lookup_binding(&self, name: &str) -> Option<&ResolvedType> {
        self.bindings.get(name)
    }

    fn has_binding(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    fn bind(&mut self, name: impl Into<String>, ty: ResolvedType) {
        self.bindings.insert(name.into(), ty);
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                pat: Pat::Ident(name),
                type_ann,
                value,
                ..
            }
            | Stmt::State {
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
                let resolved = type_ann
                    .as_ref()
                    .and_then(|ann| self.resolve_type_ann(ann))
                    .or_else(|| self.infer_expr_type(value));
                if let Some(resolved) = resolved {
                    self.bindings.insert(name.clone(), resolved);
                }
            }
            Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            }
            Stmt::Fn {
                name,
                return_type,
                body,
                ..
            } => {
                if let Some(resolved) = return_type
                    .as_ref()
                    .and_then(|ann| self.resolve_type_ann(ann))
                {
                    self.bindings.insert(name.clone(), resolved);
                }
                self.collect_expr(body)
            }
            Stmt::Expr(expr) => self.collect_expr(expr),
            _ => {}
        }
    }

    fn collect_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts {
                    self.collect_stmt(stmt);
                }
                if let Some(tail) = tail {
                    self.collect_expr(tail);
                }
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.collect_expr(cond);
                self.collect_expr(then_block);
                if let Some(else_block) = else_block {
                    self.collect_expr(else_block);
                }
            }
            Expr::While { cond, body, .. } => {
                self.collect_expr(cond);
                self.collect_expr(body);
            }
            Expr::For { iter, body, .. } => {
                self.collect_expr(iter);
                self.collect_expr(body);
            }
            Expr::Call { callee, args, .. } => {
                self.collect_expr(callee);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            Expr::MethodCall { object, args, .. } => {
                self.collect_expr(object);
                for arg in args {
                    self.collect_expr(arg);
                }
            }
            _ => {}
        }
    }

    fn resolve_type_ann(&self, ann: &TypeAnn) -> Option<ResolvedType> {
        match ann {
            TypeAnn::Number => Some(ResolvedType::scalar("number")),
            TypeAnn::Float => Some(ResolvedType::scalar("float")),
            TypeAnn::String => Some(ResolvedType::scalar("string")),
            TypeAnn::Bool => Some(ResolvedType::scalar("bool")),
            TypeAnn::Option(inner) => self.resolve_type_ann(inner).map(ResolvedType::option),
            TypeAnn::Result(inner) | TypeAnn::ResultWith(inner, _) => {
                self.resolve_type_ann(inner).map(ResolvedType::result)
            }
            TypeAnn::List(inner) | TypeAnn::Generate(inner) => {
                self.resolve_type_ann(inner).map(ResolvedType::list)
            }
            TypeAnn::Named(name) => {
                if let Some(fields) = self.structs.get(name) {
                    let mut resolved_fields = Vec::new();
                    for (field, ann) in fields {
                        let resolved = self
                            .resolve_type_ann(ann)
                            .unwrap_or_else(ResolvedType::unknown);
                        resolved_fields.push((field.clone(), resolved.shape));
                    }
                    Some(ResolvedType {
                        display: name.clone(),
                        shape: DataShape::Struct {
                            name: name.clone(),
                            fields: resolved_fields,
                        },
                        nullable: false,
                        fallible: false,
                    })
                } else {
                    Some(ResolvedType::scalar(name))
                }
            }
            TypeAnn::AnonStruct(fields) => {
                let mut resolved_fields = Vec::new();
                for (field, ann) in fields {
                    let resolved = self
                        .resolve_type_ann(ann)
                        .unwrap_or_else(ResolvedType::unknown);
                    resolved_fields.push((field.clone(), resolved.shape));
                }
                Some(ResolvedType {
                    display: format!(
                        "{{ {} }}",
                        resolved_fields
                            .iter()
                            .map(|(name, shape)| format!("{name}: {}", shape_display(shape)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    shape: DataShape::AnonStruct(resolved_fields),
                    nullable: false,
                    fallible: false,
                })
            }
            _ => None,
        }
    }

    fn infer_expr_type(&self, expr: &Expr) -> Option<ResolvedType> {
        match expr {
            Expr::Literal(lit, _) => Some(match lit {
                Literal::Int(_) => ResolvedType::scalar("number"),
                Literal::Float(_) => ResolvedType::scalar("float"),
                Literal::String(_) => ResolvedType::scalar("string"),
                Literal::Bool(_) => ResolvedType::scalar("bool"),
            }),
            Expr::List(items, _) => {
                let inner = items.first().and_then(|item| self.infer_expr_type(item))?;
                Some(ResolvedType::list(inner))
            }
            Expr::StructInit { name, .. } => self.resolve_type_ann(&TypeAnn::Named(name.clone())),
            Expr::AnonStruct { fields, .. } => {
                let mut resolved_fields = Vec::new();
                for (name, value) in fields {
                    let resolved = value
                        .as_ref()
                        .and_then(|value| self.infer_expr_type(value))
                        .unwrap_or_else(ResolvedType::unknown);
                    resolved_fields.push((name.clone(), resolved.shape));
                }
                Some(ResolvedType {
                    display: format!(
                        "{{ {} }}",
                        resolved_fields
                            .iter()
                            .map(|(name, shape)| format!("{name}: {}", shape_display(shape)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    shape: DataShape::AnonStruct(resolved_fields),
                    nullable: false,
                    fallible: false,
                })
            }
            Expr::Question(inner, _) => self.infer_expr_type(inner).map(ResolvedType::option),
            Expr::Ident(name, _) => self.lookup_binding(name).cloned(),
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Ident(name, _) => self.lookup_binding(name).cloned(),
                _ => None,
            },
            _ => None,
        }
    }
}

pub fn type_propagate(graph: &mut PipelineGraph, annotations: &mut TypeAnnotations) {
    let mut current = None::<ResolvedType>;
    let mut current_state = DataState::Definite;

    for idx in 0..graph.nodes.len() {
        if idx == 0 && matches!(graph.nodes[idx].kind, crate::graph::NodeKind::Source) {
            let source_name = source_binding_name(&graph.nodes[idx])
                .unwrap_or_else(|| graph.nodes[idx].label.clone());
            if let Some(ty) = resolve_source_type(&source_name, annotations) {
                current_state = default_state(&ty.shape);
                graph.nodes[idx].output_type = Some(ty.summary());
                graph.nodes[idx].data_info = Some(NodeDataInfo {
                    param_name: None,
                    param_shape: None,
                    shape: ty.shape.clone(),
                    state: current_state.clone(),
                });
                append_shape_notes(&mut graph.nodes[idx], &ty.shape);
                current = Some(ty);
            } else {
                let base_name = source_name
                    .split_once('(')
                    .map(|(callee, _)| callee.trim())
                    .unwrap_or(source_name.as_str());
                let is_unknown_symbol = !annotations.has_binding(base_name)
                    && !base_name.contains('.')
                    && !base_name.starts_with('[');
                if is_unknown_symbol {
                    add_error(
                        graph,
                        idx,
                        "UnknownSymbol",
                        format!("undefined symbol `{base_name}`"),
                        None,
                        None,
                    );
                } else {
                    add_warning(
                        graph,
                        idx,
                        "InferenceFailed",
                        "source type inference failed",
                    );
                }
                current_state = DataState::Unknown;
                current = Some(ResolvedType::unknown());
            }
            continue;
        }

        let Some(input) = current.clone() else {
            add_warning(graph, idx, "InferenceFailed", "missing input type");
            continue;
        };

        graph.nodes[idx].input_type = Some(input.summary());
        let param_type = closure_param_type(&input);
        if let Some(param) = closure_param(&graph.nodes[idx]) {
            if let Some(param_type) = param_type.clone() {
                annotations.bind(param, param_type);
            }
        }
        let method = method_name(&graph.nodes[idx]);
        let (output, state) =
            propagate_method(graph, idx, &method, &input, &current_state, annotations);
        graph.nodes[idx].output_type = Some(output.summary());
        graph.nodes[idx].data_info = Some(NodeDataInfo {
            param_name: closure_param(&graph.nodes[idx]),
            param_shape: param_type.map(|ty| ty.shape),
            shape: output.shape.clone(),
            state: state.clone(),
        });
        append_shape_notes(&mut graph.nodes[idx], &output.shape);
        current = Some(output);
        current_state = state;
    }

    if let Some(binding) = output_binding_name(graph) {
        if let Some(output) = current {
            annotations.bind(binding, output);
        }
    }

    for edge in &mut graph.edges {
        if let Some(node) = graph.nodes.iter().find(|node| node.id == edge.to) {
            if let Some(output) = &node.output_type {
                edge.label = Some(output.display.clone());
            }
        }
    }
}

fn propagate_method(
    graph: &mut PipelineGraph,
    idx: usize,
    method: &str,
    input: &ResolvedType,
    input_state: &DataState,
    annotations: &TypeAnnotations,
) -> (ResolvedType, DataState) {
    match (&input.shape, method) {
        (DataShape::List(inner), "filter") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let closure_ret = infer_closure_return(&node, &item, graph, idx, annotations);
            if closure_ret.shape != DataShape::Unknown && closure_ret.display != "bool" {
                add_error(
                    graph,
                    idx,
                    "InvalidClosureReturn",
                    format!(
                        "filter closure must return `bool`, but returns `{}`",
                        closure_ret.display
                    ),
                    Some("bool".to_string()),
                    Some(closure_ret.display.clone()),
                );
            }
            (
                ResolvedType {
                    display: input.display.clone(),
                    shape: DataShape::List(Box::new((**inner).clone())),
                    nullable: false,
                    fallible: input.fallible,
                },
                DataState::MaybeEmpty,
            )
        }
        (DataShape::List(inner), "map") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            (ResolvedType::list(mapped), preserve_list_state(input_state))
        }
        (DataShape::List(inner), "flat_map") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            match mapped.shape {
                DataShape::List(inner_shape) => (
                    ResolvedType {
                        display: format!("list<{}>", shape_display(&inner_shape)),
                        shape: DataShape::List(inner_shape),
                        nullable: false,
                        fallible: false,
                    },
                    preserve_list_state(input_state),
                ),
                _ => (ResolvedType::list(mapped), preserve_list_state(input_state)),
            }
        }
        (DataShape::List(inner), "find") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            (ResolvedType::option(item), DataState::MaybeNone)
        }
        (DataShape::List(inner), "take" | "skip") => (
            ResolvedType {
                display: input.display.clone(),
                shape: DataShape::List(Box::new((**inner).clone())),
                nullable: false,
                fallible: input.fallible,
            },
            DataState::MaybeEmpty,
        ),
        (DataShape::List(inner), "sort" | "dedup" | "reverse" | "order_by") => (
            ResolvedType {
                display: input.display.clone(),
                shape: DataShape::List(Box::new((**inner).clone())),
                nullable: false,
                fallible: input.fallible,
            },
            preserve_list_state(input_state),
        ),
        (DataShape::List(_), "len") => (ResolvedType::scalar("number"), DataState::Definite),
        (DataShape::List(_), "count") => (ResolvedType::scalar("number"), DataState::Definite),
        (DataShape::List(_), "sum") => (ResolvedType::scalar("number"), DataState::Definite),
        (DataShape::List(_), "join") => (ResolvedType::scalar("string"), DataState::Definite),
        (DataShape::List(inner), "first") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            (ResolvedType::option(item), DataState::MaybeNone)
        }
        (DataShape::List(_), "any" | "all") => (ResolvedType::scalar("bool"), DataState::Definite),
        (DataShape::List(inner), "flatten") => match inner.as_ref() {
            DataShape::List(nested) => (
                ResolvedType {
                    display: format!("list<{}>", shape_display(nested)),
                    shape: DataShape::List(Box::new((**nested).clone())),
                    nullable: false,
                    fallible: false,
                },
                DataState::MaybeEmpty,
            ),
            _ => (ResolvedType::unknown(), DataState::Unknown),
        },
        (DataShape::List(inner), "enumerate") => (
            ResolvedType {
                display: format!("list<(number, {})>", item_display(inner)),
                shape: DataShape::List(Box::new(DataShape::Tuple(vec![
                    DataShape::Scalar("number".to_string()),
                    (**inner).clone(),
                ]))),
                nullable: false,
                fallible: false,
            },
            preserve_list_state(input_state),
        ),
        (DataShape::List(inner), "each") => (
            ResolvedType {
                display: format!("list<{}>", item_display(inner)),
                shape: DataShape::List(Box::new((**inner).clone())),
                nullable: false,
                fallible: false,
            },
            preserve_list_state(input_state),
        ),
        (DataShape::List(inner), "partition") => {
            let lhs = DataShape::List(Box::new((**inner).clone()));
            let rhs = DataShape::List(Box::new((**inner).clone()));
            (
                ResolvedType {
                    display: format!("({}, {})", input.display, input.display),
                    shape: DataShape::Tuple(vec![lhs, rhs]),
                    nullable: false,
                    fallible: false,
                },
                DataState::Definite,
            )
        }
        (DataShape::List(inner), "zip") => (
            zip_output_type(graph.nodes[idx].notes.as_slice(), inner, annotations),
            DataState::MaybeEmpty,
        ),
        (DataShape::List(inner), "group_by") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let key = infer_closure_return(&node, &item, graph, idx, annotations);
            (
                ResolvedType {
                    display: format!("list<Group<{}>>", item_display(inner)),
                    shape: DataShape::List(Box::new(DataShape::AnonStruct(vec![
                        ("key".to_string(), key.shape),
                        (
                            "values".to_string(),
                            DataShape::List(Box::new((**inner).clone())),
                        ),
                    ]))),
                    nullable: false,
                    fallible: false,
                },
                DataState::Definite,
            )
        }
        (DataShape::List(_), "fold") => (
            fold_output_type(graph.nodes[idx].notes.as_slice(), annotations),
            DataState::Definite,
        ),
        (DataShape::Option(inner), "unwrap_or") => (
            ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            },
            DataState::Definite,
        ),
        (DataShape::Option(inner), "map") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            (ResolvedType::option(mapped), DataState::MaybeNone)
        }
        (DataShape::Option(inner), "and_then") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            match mapped.shape {
                DataShape::Option(_) => (mapped, DataState::MaybeNone),
                DataShape::Unknown => (ResolvedType::option(mapped), DataState::MaybeNone),
                _ => {
                    add_error(
                        graph,
                        idx,
                        "TypeMismatch",
                        format!(
                            "and_then closure must return `Option<T>`, but returns `{}`",
                            mapped.display
                        ),
                        Some("Option<T>".to_string()),
                        Some(mapped.display.clone()),
                    );
                    (ResolvedType::option(mapped), DataState::MaybeNone)
                }
            }
        }
        (DataShape::Option(_), "is_some" | "is_none") => {
            (ResolvedType::scalar("bool"), DataState::Definite)
        }
        (DataShape::Result(inner), "unwrap_or") => (
            ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            },
            DataState::Definite,
        ),
        (DataShape::Result(inner), "ok") => (
            ResolvedType::option(ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            }),
            DataState::MaybeNone,
        ),
        (DataShape::Result(inner), "map") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            (ResolvedType::result(mapped), DataState::MaybeErr)
        }
        (DataShape::Result(inner), "and_then") => {
            let item = ResolvedType {
                display: item_display(inner),
                shape: (**inner).clone(),
                nullable: false,
                fallible: false,
            };
            let node = graph.nodes[idx].clone();
            let mapped = infer_closure_return(&node, &item, graph, idx, annotations);
            match mapped.shape {
                DataShape::Result(_) => (mapped, DataState::MaybeErr),
                DataShape::Unknown => (ResolvedType::result(mapped), DataState::MaybeErr),
                _ => {
                    add_error(
                        graph,
                        idx,
                        "TypeMismatch",
                        format!(
                            "and_then closure must return `Result<T>`, but returns `{}`",
                            mapped.display
                        ),
                        Some("Result<T>".to_string()),
                        Some(mapped.display.clone()),
                    );
                    (ResolvedType::result(mapped), DataState::MaybeErr)
                }
            }
        }
        (DataShape::Scalar(name), "trim" | "capitalize") if name == "string" => {
            (ResolvedType::scalar("string"), DataState::Definite)
        }
        (DataShape::Scalar(name), "len") if name == "string" => {
            (ResolvedType::scalar("number"), DataState::Definite)
        }
        (DataShape::Scalar(name), "chars") if name == "string" => (
            ResolvedType::list(ResolvedType::scalar("string")),
            DataState::MaybeEmpty,
        ),
        (_, method) if method.is_empty() => {
            add_warning(
                graph,
                idx,
                "InferenceFailed",
                "method name inference failed",
            );
            (ResolvedType::unknown(), DataState::Unknown)
        }
        (DataShape::Tuple(_) | DataShape::AnonStruct(_) | DataShape::Struct { .. }, method)
            if !method.is_empty() =>
        {
            add_error(
                graph,
                idx,
                "UnsupportedPipelineShape",
                format!(
                    "method `{method}` is not supported on shape `{}` in a pipeline",
                    input.display
                ),
                None,
                Some(input.display.clone()),
            );
            (ResolvedType::unknown(), DataState::Unknown)
        }
        _ => {
            add_error(
                graph,
                idx,
                "UnknownMethod",
                format!("unknown method `{method}` for `{}`", input.display),
                None,
                Some(input.display.clone()),
            );
            (ResolvedType::unknown(), DataState::Unknown)
        }
    }
}

fn infer_closure_return(
    node: &crate::graph::PipelineNode,
    param_type: &ResolvedType,
    graph: &mut PipelineGraph,
    idx: usize,
    annotations: &TypeAnnotations,
) -> ResolvedType {
    let body = closure_tail(node).or_else(|| closure_body(node));
    let Some(body) = body else {
        add_warning(
            graph,
            idx,
            "InferenceFailed",
            "closure body inference failed",
        );
        return ResolvedType::unknown();
    };

    if body.trim().starts_with('{') {
        if let Some(fields) = infer_anon_struct_fields(&body, param_type, graph, idx) {
            let display = format!(
                "{{ {} }}",
                fields
                    .iter()
                    .map(|(name, shape)| format!("{name}: {}", shape_display(shape)))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return ResolvedType {
                display,
                shape: DataShape::AnonStruct(fields),
                nullable: false,
                fallible: false,
            };
        }
    }

    if let Some(struct_name) = named_struct_body_name(&body) {
        if let Some(named) = annotations.resolve_type_ann(&TypeAnn::Named(struct_name.to_string()))
        {
            return named;
        }
    }

    if let Some(wrapped) = infer_option_body(&body, param_type, annotations) {
        return wrapped;
    }

    if let Some(wrapped) = infer_result_body(&body, param_type, annotations) {
        return wrapped;
    }

    if let (Some(then_branch), Some(else_branch)) =
        (closure_branch_then(node), closure_branch_else(node))
    {
        if let Some(option_ty) =
            infer_option_branches(&then_branch, &else_branch, param_type, annotations)
        {
            return option_ty;
        }
        if let Some(result_ty) =
            infer_result_branches(&then_branch, &else_branch, param_type, annotations)
        {
            return result_ty;
        }
        let then_ty = infer_simple_body(&then_branch, param_type);
        let else_ty = infer_simple_body(&else_branch, param_type);
        if then_ty != ResolvedType::unknown() && then_ty == else_ty {
            return then_ty;
        }
    }

    if let Some(field) = closure_field_access(node) {
        if let Some(resolved) = resolve_field_path(param_type, &field) {
            return resolved;
        }
        add_error(
            graph,
            idx,
            "InvalidFieldAccess",
            format!(
                "field access `{field}` is invalid on `{}`",
                param_type.display
            ),
            None,
            Some(param_type.display.clone()),
        );
        return ResolvedType::unknown();
    }

    infer_simple_body(&body, param_type)
}

fn infer_anon_struct_fields(
    body: &str,
    param_type: &ResolvedType,
    graph: &mut PipelineGraph,
    idx: usize,
) -> Option<Vec<(String, DataShape)>> {
    let trimmed = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }

    let mut fields = Vec::new();
    for field in trimmed.split(',') {
        let (name, expr) = field.split_once(':')?;
        let expr = expr.trim();
        let resolved = if expr.starts_with("if ") {
            infer_simple_body(expr, param_type)
        } else {
            resolve_field_path(param_type, expr).or_else(|| {
                add_error(
                    graph,
                    idx,
                    "InvalidFieldAccess",
                    format!(
                        "field access `{expr}` is invalid on `{}`",
                        param_type.display
                    ),
                    None,
                    Some(param_type.display.clone()),
                );
                None
            })?
        };
        fields.push((name.trim().to_string(), resolved.shape));
    }
    Some(fields)
}

fn infer_simple_body(body: &str, param_type: &ResolvedType) -> ResolvedType {
    match body.trim() {
        "trim()" | "capitalize()" if param_type.display == "string" => {
            return ResolvedType::scalar("string");
        }
        "len()" if param_type.display == "string" => return ResolvedType::scalar("number"),
        "chars()" if param_type.display == "string" => {
            return ResolvedType::list(ResolvedType::scalar("string"));
        }
        _ => {}
    }

    if body.starts_with("if ") && body.matches('"').count() >= 4 {
        return ResolvedType::scalar("string");
    }

    if [
        " Gt ", " Ge ", " Lt ", " Le ", " Eq ", " Ne ", " And ", " Or ",
    ]
    .iter()
    .any(|op| body.contains(op))
    {
        return ResolvedType::scalar("bool");
    }

    if [" Add ", " Sub ", " Mul ", " Div ", " Rem "]
        .iter()
        .any(|op| body.contains(op))
    {
        return ResolvedType::scalar("number");
    }

    if body.starts_with('"') && body.ends_with('"') {
        return ResolvedType::scalar("string");
    }

    ResolvedType::unknown()
}

fn infer_option_body(
    body: &str,
    param_type: &ResolvedType,
    annotations: &TypeAnnotations,
) -> Option<ResolvedType> {
    let trimmed = body.trim();
    if trimmed == "none" {
        return Some(ResolvedType::option(ResolvedType::unknown()));
    }

    let inner = trimmed
        .strip_prefix("some(")
        .and_then(|rest| rest.strip_suffix(')'))?;
    let inner = infer_wrapped_value(inner.trim(), param_type, annotations);
    Some(ResolvedType::option(inner))
}

fn infer_option_branches(
    then_branch: &str,
    else_branch: &str,
    param_type: &ResolvedType,
    annotations: &TypeAnnotations,
) -> Option<ResolvedType> {
    let then_ty = infer_option_body(then_branch, param_type, annotations)?;
    let else_ty = infer_option_body(else_branch, param_type, annotations)?;
    match (&then_ty.shape, &else_ty.shape) {
        (DataShape::Option(inner), DataShape::Option(other)) => {
            if !matches!(**inner, DataShape::Unknown) {
                Some(ResolvedType::option(ResolvedType {
                    display: shape_display(inner),
                    shape: (**inner).clone(),
                    nullable: false,
                    fallible: false,
                }))
            } else if !matches!(**other, DataShape::Unknown) {
                Some(ResolvedType::option(ResolvedType {
                    display: shape_display(other),
                    shape: (**other).clone(),
                    nullable: false,
                    fallible: false,
                }))
            } else {
                Some(then_ty)
            }
        }
        _ => None,
    }
}

fn infer_result_body(
    body: &str,
    param_type: &ResolvedType,
    annotations: &TypeAnnotations,
) -> Option<ResolvedType> {
    let trimmed = body.trim();
    if let Some(inner) = trimmed
        .strip_prefix("ok(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return Some(ResolvedType::result(infer_wrapped_value(
            inner.trim(),
            param_type,
            annotations,
        )));
    }
    if trimmed.starts_with("err(") && trimmed.ends_with(')') {
        return Some(ResolvedType::result(ResolvedType::unknown()));
    }
    None
}

fn infer_result_branches(
    then_branch: &str,
    else_branch: &str,
    param_type: &ResolvedType,
    annotations: &TypeAnnotations,
) -> Option<ResolvedType> {
    let then_ty = infer_result_body(then_branch, param_type, annotations)?;
    let else_ty = infer_result_body(else_branch, param_type, annotations)?;
    match (&then_ty.shape, &else_ty.shape) {
        (DataShape::Result(inner), DataShape::Result(other)) => {
            if !matches!(**inner, DataShape::Unknown) {
                Some(ResolvedType::result(ResolvedType {
                    display: shape_display(inner),
                    shape: (**inner).clone(),
                    nullable: false,
                    fallible: false,
                }))
            } else if !matches!(**other, DataShape::Unknown) {
                Some(ResolvedType::result(ResolvedType {
                    display: shape_display(other),
                    shape: (**other).clone(),
                    nullable: false,
                    fallible: false,
                }))
            } else {
                Some(then_ty)
            }
        }
        _ => None,
    }
}

fn infer_wrapped_value(
    body: &str,
    param_type: &ResolvedType,
    annotations: &TypeAnnotations,
) -> ResolvedType {
    if let Some(struct_name) = named_struct_body_name(body) {
        if let Some(named) = annotations.resolve_type_ann(&TypeAnn::Named(struct_name.to_string()))
        {
            return named;
        }
    }

    if body.contains('.') {
        if let Some(resolved) = resolve_field_path(param_type, body.trim()) {
            return resolved;
        }
    }

    if let Some(binding) = resolve_source_type(body, annotations) {
        return binding;
    }

    infer_simple_body(body, param_type)
}

fn resolve_source_type(source: &str, annotations: &TypeAnnotations) -> Option<ResolvedType> {
    annotations
        .lookup_binding(source)
        .cloned()
        .or_else(|| {
            source
                .split_once('(')
                .and_then(|(callee, _)| annotations.lookup_binding(callee.trim()).cloned())
        })
        .or_else(|| resolve_path_type(source, annotations))
}

fn resolve_path_type(path: &str, annotations: &TypeAnnotations) -> Option<ResolvedType> {
    let mut segments = path.split('.');
    let head = segments.next()?.trim();
    let mut current = annotations.lookup_binding(head)?.clone();
    for field in segments {
        current = field_type_for(&current, field.trim())?;
    }
    Some(current)
}

fn closure_param_type(input: &ResolvedType) -> Option<ResolvedType> {
    match &input.shape {
        DataShape::List(inner) | DataShape::Option(inner) => Some(ResolvedType {
            display: item_display(inner),
            shape: (**inner).clone(),
            nullable: false,
            fallible: false,
        }),
        _ => None,
    }
}

fn output_binding_name(graph: &PipelineGraph) -> Option<String> {
    let source = graph.nodes.first()?;
    let source_expr = source_binding_name(source).unwrap_or_else(|| source.label.clone());
    if source.label != source_expr {
        Some(source.label.clone())
    } else {
        None
    }
}

fn zip_output_type(
    notes: &[String],
    lhs_inner: &DataShape,
    annotations: &TypeAnnotations,
) -> ResolvedType {
    let rhs = note_value_from(notes, "arg[0]: ")
        .and_then(|expr| resolve_source_type(&expr, annotations))
        .map(|ty| match ty.shape {
            DataShape::List(inner) => *inner,
            shape => shape,
        })
        .unwrap_or(DataShape::Unknown);

    ResolvedType {
        display: format!(
            "list<({}, {})>",
            item_display(lhs_inner),
            shape_display(&rhs)
        ),
        shape: DataShape::List(Box::new(DataShape::Tuple(vec![lhs_inner.clone(), rhs]))),
        nullable: false,
        fallible: false,
    }
}

fn fold_output_type(notes: &[String], annotations: &TypeAnnotations) -> ResolvedType {
    note_value_from(notes, "arg[0]: ")
        .and_then(|expr| infer_value_expr_type(&expr, annotations))
        .unwrap_or_else(ResolvedType::unknown)
}

fn infer_value_expr_type(expr: &str, annotations: &TypeAnnotations) -> Option<ResolvedType> {
    if expr.starts_with('"') && expr.ends_with('"') {
        return Some(ResolvedType::scalar("string"));
    }
    if expr == "true" || expr == "false" {
        return Some(ResolvedType::scalar("bool"));
    }
    if expr.parse::<i64>().is_ok() {
        return Some(ResolvedType::scalar("number"));
    }
    if expr.parse::<f64>().is_ok() {
        return Some(ResolvedType::scalar("float"));
    }
    resolve_source_type(expr, annotations)
}

fn note_value_from(notes: &[String], prefix: &str) -> Option<String> {
    notes
        .iter()
        .find_map(|note| note.strip_prefix(prefix).map(|value| value.to_string()))
}

fn named_struct_body_name(body: &str) -> Option<&str> {
    let body = body.trim();
    let name = body.split_whitespace().next()?;
    if body.contains('{') && name.chars().next()?.is_uppercase() {
        Some(name)
    } else {
        None
    }
}

fn field_type_for(param_type: &ResolvedType, field: &str) -> Option<ResolvedType> {
    match &param_type.shape {
        DataShape::Struct { fields, .. } | DataShape::AnonStruct(fields) => {
            let shape = fields.iter().find(|(name, _)| name == field)?.1.clone();
            Some(ResolvedType {
                display: shape_display(&shape),
                shape,
                nullable: false,
                fallible: false,
            })
        }
        _ => None,
    }
}

fn resolve_field_path(param_type: &ResolvedType, expr: &str) -> Option<ResolvedType> {
    let mut segments = expr.split('.').map(str::trim);
    let first = segments.next()?;
    let mut current = if expr.contains('.') {
        if first == param_type.display || first.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            param_type.clone()
        } else {
            field_type_for(param_type, first)?
        }
    } else {
        return field_type_for(param_type, first);
    };

    for segment in segments {
        current = field_type_for(&current, segment)?;
    }

    Some(current)
}

fn collect_structs(stmt: &Stmt, structs: &mut HashMap<String, Vec<(String, TypeAnn)>>) {
    match stmt {
        Stmt::StructDef { name, fields, .. } => {
            structs.insert(name.clone(), fields.clone());
        }
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for stmt in body {
                collect_structs(stmt, structs);
            }
        }
        _ => {}
    }
}

fn shape_display(shape: &DataShape) -> String {
    match shape {
        DataShape::Scalar(name) => name.clone(),
        DataShape::List(inner) => format!("list<{}>", shape_display(inner)),
        DataShape::Option(inner) => format!("{}?", shape_display(inner)),
        DataShape::Result(inner) => format!("{}!", shape_display(inner)),
        DataShape::Struct { name, .. } => name.clone(),
        DataShape::AnonStruct(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, shape)| format!("{name}: {}", shape_display(shape)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataShape::Tuple(items) => format!(
            "({})",
            items
                .iter()
                .map(shape_display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataShape::Unknown => "unknown".to_string(),
    }
}

fn append_shape_notes(node: &mut crate::graph::PipelineNode, shape: &DataShape) {
    if let Some(note) = anon_struct_note(shape) {
        if !node.notes.iter().any(|existing| existing == &note) {
            node.notes.push(note);
        }
    }
}

fn anon_struct_note(shape: &DataShape) -> Option<String> {
    match shape {
        DataShape::AnonStruct(_) => Some(format!("anon struct fields: {}", shape_display(shape))),
        DataShape::List(inner) | DataShape::Option(inner) | DataShape::Result(inner) => {
            anon_struct_note(inner)
        }
        _ => None,
    }
}

fn item_display(shape: &DataShape) -> String {
    shape_display(shape)
}

fn default_state(shape: &DataShape) -> DataState {
    match shape {
        DataShape::Option(_) => DataState::MaybeNone,
        DataShape::Result(_) => DataState::MaybeErr,
        _ => DataState::Definite,
    }
}

fn preserve_list_state(input_state: &DataState) -> DataState {
    match input_state {
        DataState::MaybeEmpty => DataState::MaybeEmpty,
        _ => DataState::MaybeEmpty,
    }
}

fn source_binding_name(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "source expr: ")
}

fn method_name(node: &crate::graph::PipelineNode) -> String {
    if let Some(name) = note_value(node, "method: ") {
        return name;
    }

    node.label
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn closure_param(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "closure params: ")
}

fn closure_body(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "closure body: ")
}

fn closure_tail(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "closure tail: ")
}

fn closure_branch_then(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "closure branch then: ")
}

fn closure_branch_else(node: &crate::graph::PipelineNode) -> Option<String> {
    note_value(node, "closure branch else: ")
}

fn closure_field_access(node: &crate::graph::PipelineNode) -> Option<String> {
    node.notes
        .iter()
        .find_map(|note| note.strip_prefix("field access: "))
        .map(str::to_string)
}

fn note_value(node: &crate::graph::PipelineNode, prefix: &str) -> Option<String> {
    note_value_from(&node.notes, prefix)
}

fn add_warning(graph: &mut PipelineGraph, idx: usize, code: &str, message: &str) {
    graph.nodes[idx].status = NodeStatus::Warning;
    graph.add_diagnostic(Diagnostic {
        node_id: Some(graph.nodes[idx].id),
        code: code.to_string(),
        message: message.to_string(),
        span: graph.nodes[idx].span.clone(),
        expected: None,
        actual: None,
    });
}

fn add_error(
    graph: &mut PipelineGraph,
    idx: usize,
    code: &str,
    message: String,
    expected: Option<String>,
    actual: Option<String>,
) {
    graph.nodes[idx].status = NodeStatus::Error;
    graph.add_diagnostic(Diagnostic {
        node_id: Some(graph.nodes[idx].id),
        code: code.to_string(),
        message,
        span: graph.nodes[idx].span.clone(),
        expected,
        actual,
    });
}
