use forge_compiler::ast::{DeferBody, Expr, Literal, Module, Pat, PipelineStep, Stmt};

use crate::graph::{Diagnostic, NodeKind, NodeStatus, PipelineGraph, PipelineNode, SourceSpan};

pub fn extract_pipelines(stmts: &[Stmt]) -> Vec<PipelineGraph> {
    let mut graphs = Vec::new();
    let mut seen = Vec::new();
    for stmt in stmts {
        collect_stmt(stmt, &mut graphs, &mut seen, None);
    }
    graphs
}

fn collect_stmt(
    stmt: &Stmt,
    graphs: &mut Vec<PipelineGraph>,
    seen: &mut Vec<(usize, usize)>,
    current_fn: Option<&str>,
) {
    match stmt {
        Stmt::Let { pat, value, .. } => {
            let binding = if let Pat::Ident(name) = pat {
                Some(name.as_str())
            } else {
                None
            };
            collect_expr(value, binding, graphs, seen, current_fn);
        }
        Stmt::State { name, value, .. } | Stmt::Const { name, value, .. } => {
            collect_expr(value, Some(name.as_str()), graphs, seen, current_fn);
        }
        Stmt::Fn { name, body, .. } => collect_expr(body, None, graphs, seen, Some(name.as_str())),
        Stmt::Return(Some(expr), _) => collect_expr(expr, None, graphs, seen, current_fn),
        Stmt::Yield { value, .. } => collect_expr(value, None, graphs, seen, current_fn),
        Stmt::Expr(expr) => collect_expr(expr, None, graphs, seen, current_fn),
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for stmt in body {
                collect_stmt(stmt, graphs, seen, current_fn);
            }
        }
        Stmt::Defer { body, .. } => match body {
            DeferBody::Expr(expr) | DeferBody::Block(expr) => {
                collect_expr(expr, None, graphs, seen, current_fn)
            }
        },
        _ => {}
    }
}

fn collect_expr(
    expr: &Expr,
    binding: Option<&str>,
    graphs: &mut Vec<PipelineGraph>,
    seen: &mut Vec<(usize, usize)>,
    current_fn: Option<&str>,
) {
    if let Some(mut graph) = extract_method_chain(expr, binding) {
        graph.function_name = current_fn.map(str::to_string);
        push_graph_once(expr, graph, graphs, seen);
        collect_nested_from_method_chain(expr, graphs, seen, current_fn);
        return;
    }

    if let Some(mut graph) = extract_pipeline_block(expr, binding) {
        graph.function_name = current_fn.map(str::to_string);
        push_graph_once(expr, graph, graphs, seen);
        return;
    }

    walk_expr(expr, graphs, seen, current_fn);
}

fn collect_nested_from_method_chain(
    expr: &Expr,
    graphs: &mut Vec<PipelineGraph>,
    seen: &mut Vec<(usize, usize)>,
    current_fn: Option<&str>,
) {
    let mut current = expr;
    while let Expr::MethodCall { object, args, .. } = current {
        for arg in args {
            walk_expr(arg, graphs, seen, current_fn);
        }
        current = object;
    }
    walk_expr(current, graphs, seen, current_fn);
}

fn walk_expr(
    expr: &Expr,
    graphs: &mut Vec<PipelineGraph>,
    seen: &mut Vec<(usize, usize)>,
    current_fn: Option<&str>,
) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_expr(left, None, graphs, seen, current_fn);
            collect_expr(right, None, graphs, seen, current_fn);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Await { expr: operand, .. }
        | Expr::Question(operand, ..)
        | Expr::Spawn { body: operand, .. } => {
            collect_expr(operand, None, graphs, seen, current_fn)
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            collect_expr(cond, None, graphs, seen, current_fn);
            collect_expr(then_block, None, graphs, seen, current_fn);
            if let Some(else_block) = else_block {
                collect_expr(else_block, None, graphs, seen, current_fn);
            }
        }
        Expr::While { cond, body, .. } => {
            collect_expr(cond, None, graphs, seen, current_fn);
            collect_expr(body, None, graphs, seen, current_fn);
        }
        Expr::Loop { body, .. } => collect_expr(body, None, graphs, seen, current_fn),
        Expr::For { iter, body, .. } => {
            collect_expr(iter, None, graphs, seen, current_fn);
            collect_expr(body, None, graphs, seen, current_fn);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_expr(scrutinee, None, graphs, seen, current_fn);
            for arm in arms {
                collect_expr(&arm.body, None, graphs, seen, current_fn);
            }
        }
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                collect_stmt(stmt, graphs, seen, current_fn);
            }
            if let Some(tail) = tail {
                collect_expr(tail, None, graphs, seen, current_fn);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_expr(callee, None, graphs, seen, current_fn);
            for arg in args {
                collect_expr(arg, None, graphs, seen, current_fn);
            }
        }
        Expr::MethodCall { .. } => {
            if let Some(mut graph) = extract_method_chain(expr, None) {
                graph.function_name = current_fn.map(str::to_string);
                push_graph_once(expr, graph, graphs, seen);
                collect_nested_from_method_chain(expr, graphs, seen, current_fn);
            }
        }
        Expr::Field { object, .. } => collect_expr(object, None, graphs, seen, current_fn),
        Expr::Index { object, index, .. } => {
            collect_expr(object, None, graphs, seen, current_fn);
            collect_expr(index, None, graphs, seen, current_fn);
        }
        Expr::Closure { body, .. } => collect_expr(body, None, graphs, seen, current_fn),
        Expr::Range { start, end, .. } => {
            collect_expr(start, None, graphs, seen, current_fn);
            collect_expr(end, None, graphs, seen, current_fn);
        }
        Expr::List(items, _) => {
            for item in items {
                collect_expr(item, None, graphs, seen, current_fn);
            }
        }
        Expr::MapLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_expr(key, None, graphs, seen, current_fn);
                collect_expr(value, None, graphs, seen, current_fn);
            }
        }
        Expr::SetLiteral { items, .. } => {
            for item in items {
                collect_expr(item, None, graphs, seen, current_fn);
            }
        }
        Expr::Assign { value, .. } => collect_expr(value, None, graphs, seen, current_fn),
        Expr::IndexAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_expr(object, None, graphs, seen, current_fn);
            collect_expr(index, None, graphs, seen, current_fn);
            collect_expr(value, None, graphs, seen, current_fn);
        }
        Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                collect_expr(value, None, graphs, seen, current_fn);
            }
        }
        Expr::AnonStruct { fields, .. } => {
            for (_, value) in fields {
                if let Some(value) = value {
                    collect_expr(value, None, graphs, seen, current_fn);
                }
            }
        }
        Expr::EnumInit { data, .. } => match data {
            forge_compiler::ast::EnumInitData::Tuple(items) => {
                for item in items {
                    collect_expr(item, None, graphs, seen, current_fn);
                }
            }
            forge_compiler::ast::EnumInitData::Struct(fields) => {
                for (_, value) in fields {
                    collect_expr(value, None, graphs, seen, current_fn);
                }
            }
            forge_compiler::ast::EnumInitData::None => {}
        },
        Expr::FieldAssign { object, value, .. } => {
            collect_expr(object, None, graphs, seen, current_fn);
            collect_expr(value, None, graphs, seen, current_fn);
        }
        Expr::OptionalChain { object, chain, .. } => {
            collect_expr(object, None, graphs, seen, current_fn);
            if let forge_compiler::ast::ChainKind::Method { args, .. } = chain {
                for arg in args {
                    collect_expr(arg, None, graphs, seen, current_fn);
                }
            }
        }
        Expr::NullCoalesce { value, default, .. } => {
            collect_expr(value, None, graphs, seen, current_fn);
            collect_expr(default, None, graphs, seen, current_fn);
        }
        Expr::Pipeline { .. } => {
            if let Some(mut graph) = extract_pipeline_block(expr, None) {
                graph.function_name = current_fn.map(str::to_string);
                push_graph_once(expr, graph, graphs, seen);
            }
        }
        Expr::Literal(..) | Expr::Ident(..) | Expr::Interpolation { .. } | Expr::Break { .. } => {}
    }
}

fn push_graph_once(
    expr: &Expr,
    graph: PipelineGraph,
    graphs: &mut Vec<PipelineGraph>,
    seen: &mut Vec<(usize, usize)>,
) {
    let Some(span) = expr_span(expr) else {
        graphs.push(graph);
        return;
    };

    let key = (span.start, span.end);
    if seen.contains(&key) {
        return;
    }

    seen.push(key);
    graphs.push(graph);
}

fn extract_method_chain(expr: &Expr, binding: Option<&str>) -> Option<PipelineGraph> {
    if !matches!(expr, Expr::MethodCall { .. }) {
        return None;
    }

    let mut steps = Vec::new();
    let source = flatten_method_chain(expr, &mut steps)?;

    let mut graph = PipelineGraph::new();
    let mut source_node =
        PipelineNode::new(binding.unwrap_or(&expr_label(source)), NodeKind::Source);
    source_node.span = expr_span(source).map(SourceSpan::from);
    source_node.status = NodeStatus::Ok;
    source_node
        .notes
        .push(format!("source expr: {}", expr_label(source)));
    let source_id = graph.add_node(source_node);
    graph.roots.push(source_id);

    let mut prev = source_id;
    for step in steps {
        let mut node = PipelineNode::new(step.label, step.kind);
        node.span = step.span;
        node.status = NodeStatus::Ok;
        node.notes = step.notes;
        let id = graph.add_node(node);
        graph.add_edge(prev, id, None);
        prev = id;
    }

    Some(graph)
}

struct StepSpec {
    label: String,
    kind: NodeKind,
    span: Option<SourceSpan>,
    notes: Vec<String>,
}

fn flatten_method_chain<'a>(expr: &'a Expr, steps: &mut Vec<StepSpec>) -> Option<&'a Expr> {
    match expr {
        Expr::MethodCall {
            object,
            method,
            args,
            span,
        } => {
            let source = flatten_method_chain(object, steps).unwrap_or(object.as_ref());
            steps.push(StepSpec {
                label: method_label(method, args),
                kind: if has_closure_arg(args) {
                    NodeKind::Closure
                } else {
                    NodeKind::MethodCall
                },
                span: Some(SourceSpan::from(span)),
                notes: {
                    let mut notes = vec![format!("method: {method}")];
                    notes.extend(notes_from_args(args));
                    notes
                },
            });
            Some(source)
        }
        _ => Some(expr),
    }
}

fn extract_pipeline_block(expr: &Expr, binding: Option<&str>) -> Option<PipelineGraph> {
    let Expr::Pipeline { steps, .. } = expr else {
        return None;
    };

    let mut graph = PipelineGraph::new();
    let mut step_iter = steps.iter();
    let source_expr = match step_iter.next() {
        Some(PipelineStep::Source(expr)) => expr.as_ref(),
        Some(step) => pipeline_step_expr(step),
        None => return Some(graph),
    };

    let mut source_node = PipelineNode::new(
        binding.unwrap_or(&expr_label(source_expr)),
        NodeKind::Source,
    );
    source_node.span = expr_span(source_expr).map(SourceSpan::from);
    source_node.status = NodeStatus::Ok;
    source_node
        .notes
        .push(format!("source expr: {}", expr_label(source_expr)));
    let source_id = graph.add_node(source_node);
    graph.roots.push(source_id);

    let mut prev = source_id;
    let remaining = if matches!(steps.first(), Some(PipelineStep::Source(_))) {
        step_iter.collect::<Vec<_>>()
    } else {
        steps.iter().collect::<Vec<_>>()
    };

    for step in remaining {
        let mut node = PipelineNode::new(pipeline_step_label(step), pipeline_step_kind(step));
        node.span = expr_span(pipeline_step_expr(step)).map(SourceSpan::from);
        node.status = NodeStatus::Ok;
        node.notes = pipeline_step_notes(step);
        let id = graph.add_node(node);
        graph.add_edge(prev, id, None);
        prev = id;
    }

    Some(graph)
}

fn pipeline_step_expr(step: &PipelineStep) -> &Expr {
    match step {
        PipelineStep::Source(expr)
        | PipelineStep::Filter(expr)
        | PipelineStep::Map(expr)
        | PipelineStep::FlatMap(expr)
        | PipelineStep::Group(expr)
        | PipelineStep::Take(expr)
        | PipelineStep::Skip(expr)
        | PipelineStep::Each(expr)
        | PipelineStep::Sink(expr)
        | PipelineStep::Parallel(expr) => expr,
        PipelineStep::Sort { key, .. } => key,
    }
}

fn pipeline_step_kind(step: &PipelineStep) -> NodeKind {
    let expr = pipeline_step_expr(step);
    if matches!(expr, Expr::Closure { .. }) || has_closure_arg(std::slice::from_ref(expr)) {
        NodeKind::Closure
    } else {
        NodeKind::MethodCall
    }
}

fn pipeline_step_label(step: &PipelineStep) -> String {
    match step {
        PipelineStep::Source(expr) => format!("source({})", expr_label(expr)),
        PipelineStep::Filter(expr) => format!("filter({})", expr_label(expr)),
        PipelineStep::Map(expr) => format!("map({})", expr_label(expr)),
        PipelineStep::FlatMap(expr) => format!("flat_map({})", expr_label(expr)),
        PipelineStep::Group(expr) => format!("group({})", expr_label(expr)),
        PipelineStep::Sort { key, descending } => {
            if *descending {
                format!("sort({}, desc)", expr_label(key))
            } else {
                format!("sort({})", expr_label(key))
            }
        }
        PipelineStep::Take(expr) => format!("take({})", expr_label(expr)),
        PipelineStep::Skip(expr) => format!("skip({})", expr_label(expr)),
        PipelineStep::Each(expr) => format!("each({})", expr_label(expr)),
        PipelineStep::Sink(expr) => format!("sink({})", expr_label(expr)),
        PipelineStep::Parallel(expr) => format!("parallel({})", expr_label(expr)),
    }
}

fn pipeline_step_notes(step: &PipelineStep) -> Vec<String> {
    match step {
        PipelineStep::Filter(expr)
        | PipelineStep::Map(expr)
        | PipelineStep::FlatMap(expr)
        | PipelineStep::Group(expr)
        | PipelineStep::Each(expr)
        | PipelineStep::Sink(expr)
        | PipelineStep::Parallel(expr)
        | PipelineStep::Source(expr)
        | PipelineStep::Take(expr)
        | PipelineStep::Skip(expr) => notes_from_expr(expr),
        PipelineStep::Sort { key, descending } => {
            let mut notes = notes_from_expr(key);
            if *descending {
                notes.push("sort order: desc".to_string());
            }
            notes
        }
    }
}

fn has_closure_arg(args: &[Expr]) -> bool {
    args.iter().any(|a| matches!(a, Expr::Closure { .. }))
}

fn method_label(method: &str, args: &[Expr]) -> String {
    if args.is_empty() {
        format!("{method}()")
    } else {
        let args = args.iter().map(expr_label).collect::<Vec<_>>().join(", ");
        format!("{method}({args})")
    }
}

fn notes_from_args(args: &[Expr]) -> Vec<String> {
    let mut notes = Vec::new();
    for (idx, arg) in args.iter().enumerate() {
        notes.push(format!("arg[{idx}]: {}", expr_label(arg)));
        notes.extend(notes_from_expr(arg));
    }
    notes
}

fn notes_from_expr(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Closure { params, body, .. } => {
            let mut notes = Vec::new();
            notes.push(format!("closure params: {}", params.join(", ")));
            notes.push(format!("closure body: {}", expr_label(body)));
            append_if_branch_notes(&mut notes, body);
            if let Some(tail) = closure_tail_expr(body) {
                notes.push(format!("closure tail: {}", expr_label(tail)));
                append_if_branch_notes(&mut notes, tail);
            }
            for field in collect_field_accesses(body) {
                notes.push(format!("field access: {field}"));
            }
            notes
        }
        _ => Vec::new(),
    }
}

fn closure_tail_expr(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Block { stmts, tail, .. } => tail.as_deref().or_else(|| {
            stmts.iter().rev().find_map(|stmt| match stmt {
                Stmt::Expr(expr) => Some(expr),
                _ => None,
            })
        }),
        _ => None,
    }
}

fn append_if_branch_notes(notes: &mut Vec<String>, expr: &Expr) {
    if let Expr::If {
        cond,
        then_block,
        else_block,
        ..
    } = expr
    {
        notes.push(format!("closure condition: {}", expr_label(cond)));
        if let Some(then_tail) = closure_tail_expr(then_block).or(Some(then_block.as_ref())) {
            notes.push(format!("closure branch then: {}", expr_label(then_tail)));
        }
        if let Some(else_block) = else_block {
            if let Some(else_tail) = closure_tail_expr(else_block).or(Some(else_block.as_ref())) {
                notes.push(format!("closure branch else: {}", expr_label(else_tail)));
            }
        }
    }
}

fn collect_field_accesses(expr: &Expr) -> Vec<String> {
    let mut fields = Vec::new();
    collect_field_accesses_inner(expr, &mut fields);
    fields
}

fn collect_field_accesses_inner(expr: &Expr, fields: &mut Vec<String>) {
    match expr {
        Expr::Field { object, field, .. } => {
            fields.push(format!("{}.{}", expr_label(object), field));
            collect_field_accesses_inner(object, fields);
        }
        Expr::BinOp { left, right, .. } => {
            collect_field_accesses_inner(left, fields);
            collect_field_accesses_inner(right, fields);
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            collect_field_accesses_inner(cond, fields);
            collect_field_accesses_inner(then_block, fields);
            if let Some(else_block) = else_block {
                collect_field_accesses_inner(else_block, fields);
            }
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Await { expr: operand, .. }
        | Expr::Question(operand, ..)
        | Expr::Spawn { body: operand, .. } => collect_field_accesses_inner(operand, fields),
        Expr::Call { callee, args, .. } => {
            collect_field_accesses_inner(callee, fields);
            for arg in args {
                collect_field_accesses_inner(arg, fields);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_field_accesses_inner(object, fields);
            for arg in args {
                collect_field_accesses_inner(arg, fields);
            }
        }
        Expr::Index { object, index, .. } => {
            collect_field_accesses_inner(object, fields);
            collect_field_accesses_inner(index, fields);
        }
        Expr::Closure { body, .. } => collect_field_accesses_inner(body, fields),
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                match stmt {
                    Stmt::Expr(expr) => collect_field_accesses_inner(expr, fields),
                    Stmt::Return(Some(expr), _) => collect_field_accesses_inner(expr, fields),
                    Stmt::Yield { value, .. } => collect_field_accesses_inner(value, fields),
                    Stmt::Let { value, .. }
                    | Stmt::State { value, .. }
                    | Stmt::Const { value, .. } => collect_field_accesses_inner(value, fields),
                    _ => {}
                }
            }
            if let Some(tail) = tail {
                collect_field_accesses_inner(tail, fields);
            }
        }
        Expr::List(items, _) => {
            for item in items {
                collect_field_accesses_inner(item, fields);
            }
        }
        Expr::Pipeline { steps, .. } => {
            for step in steps {
                collect_field_accesses_inner(pipeline_step_expr(step), fields);
            }
        }
        _ => {}
    }
}

fn expr_label(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit, _) => literal_label(lit),
        Expr::Ident(name, _) => name.clone(),
        Expr::Call { callee, args, .. } => {
            format!("{}({})", expr_label(callee), join_exprs(args))
        }
        Expr::MethodCall { method, args, .. } => method_label(method, args),
        Expr::Field { object, field, .. } => format!("{}.{}", expr_label(object), field),
        Expr::Index { object, index, .. } => {
            format!("{}[{}]", expr_label(object), expr_label(index))
        }
        Expr::Closure { params, body, .. } => {
            format!("{} => {}", params.join(", "), expr_label(body))
        }
        Expr::BinOp {
            left, op, right, ..
        } => {
            format!("{} {:?} {}", expr_label(left), op, expr_label(right))
        }
        Expr::UnaryOp { op, operand, .. } => format!("{:?} {}", op, expr_label(operand)),
        Expr::List(items, _) => format!("[{}]", join_exprs(items)),
        Expr::AnonStruct { fields, .. } => {
            let fields = fields
                .iter()
                .map(|(name, value)| match value {
                    Some(value) => format!("{name}: {}", expr_label(value)),
                    None => name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {fields} }}")
        }
        Expr::StructInit { name, .. } => format!("{name} {{ ... }}"),
        Expr::Pipeline { .. } => "pipeline { ... }".to_string(),
        Expr::Question(inner, _) => format!("{}?", expr_label(inner)),
        Expr::Await { expr, .. } => format!("{}.await", expr_label(expr)),
        Expr::Assign { name, .. } => format!("{name} = ..."),
        Expr::IndexAssign { .. } => "index_assign".to_string(),
        Expr::FieldAssign { .. } => "field_assign".to_string(),
        Expr::If { .. } => "if ...".to_string(),
        Expr::While { .. } => "while ...".to_string(),
        Expr::Loop { .. } => "loop".to_string(),
        Expr::Break { .. } => "break".to_string(),
        Expr::For { .. } => "for ...".to_string(),
        Expr::Match { .. } => "match ...".to_string(),
        Expr::Block { .. } => "{ ... }".to_string(),
        Expr::Interpolation { .. } => "\"...\"".to_string(),
        Expr::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            let sep = if *inclusive { "..=" } else { ".." };
            format!("{}{}{}", expr_label(start), sep, expr_label(end))
        }
        Expr::MapLiteral { .. } => "{ ... }".to_string(),
        Expr::SetLiteral { .. } => "#{ ... }".to_string(),
        Expr::EnumInit {
            enum_name, variant, ..
        } => format!("{enum_name}::{variant}"),
        Expr::OptionalChain { .. } => "optional_chain".to_string(),
        Expr::NullCoalesce { .. } => "??".to_string(),
        Expr::Spawn { .. } => "spawn { ... }".to_string(),
    }
}

fn literal_label(lit: &Literal) -> String {
    match lit {
        Literal::Int(value) => value.to_string(),
        Literal::Float(value) => value.to_string(),
        Literal::String(value) => format!("\"{value}\""),
        Literal::Bool(value) => value.to_string(),
    }
}

fn join_exprs(args: &[Expr]) -> String {
    args.iter().map(expr_label).collect::<Vec<_>>().join(", ")
}

fn expr_span(expr: &Expr) -> Option<&forge_compiler::lexer::Span> {
    match expr {
        Expr::Literal(_, span)
        | Expr::Ident(_, span)
        | Expr::Question(_, span)
        | Expr::List(_, span)
        | Expr::Break { span } => Some(span),
        Expr::BinOp { span, .. }
        | Expr::UnaryOp { span, .. }
        | Expr::If { span, .. }
        | Expr::While { span, .. }
        | Expr::Loop { span, .. }
        | Expr::For { span, .. }
        | Expr::Match { span, .. }
        | Expr::Block { span, .. }
        | Expr::Call { span, .. }
        | Expr::MethodCall { span, .. }
        | Expr::Field { span, .. }
        | Expr::Index { span, .. }
        | Expr::Closure { span, .. }
        | Expr::Interpolation { span, .. }
        | Expr::Range { span, .. }
        | Expr::MapLiteral { span, .. }
        | Expr::SetLiteral { span, .. }
        | Expr::Await { span, .. }
        | Expr::Assign { span, .. }
        | Expr::IndexAssign { span, .. }
        | Expr::StructInit { span, .. }
        | Expr::AnonStruct { span, .. }
        | Expr::EnumInit { span, .. }
        | Expr::FieldAssign { span, .. }
        | Expr::OptionalChain { span, .. }
        | Expr::NullCoalesce { span, .. }
        | Expr::Spawn { span, .. }
        | Expr::Pipeline { span, .. } => Some(span),
    }
}

pub fn analyze_module(module: &Module) -> Result<Vec<PipelineGraph>, Diagnostic> {
    Ok(extract_pipelines(&module.stmts))
}
