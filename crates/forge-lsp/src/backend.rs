use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_compiler::ast::{Expr, Module, Pat, Stmt, TypeAnn};
use forge_compiler::lexer::Span;
use forge_compiler::parser::{parse_source, ParseError};
use forge_compiler::typechecker::{type_check_source, TypeError};
use forge_goblet::{
    analyze_source as goblet_analyze_source, builtin_sigs, NodeId, NodeStatus, PipelineGraph,
};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    Location, MarkupContent, MarkupKind, MessageType, OneOf, Position, Range, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer};

#[derive(Debug, Clone, Default)]
pub struct DocumentState {
    pub text: String,
    pub version: i32,
    pub ast: Option<Module>,
    pub pipeline_graphs: Vec<PipelineGraph>,
    pub symbol_map: SymbolMap,
    pub symbol_table: SymbolTable,
    pub diagnostics: Vec<Diagnostic>,
}

pub type DocCache = Arc<Mutex<HashMap<Url, DocumentState>>>;

pub type SymbolMap = HashMap<String, SymbolInfo>;
pub type SymbolTable = HashMap<String, SymbolLocation>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Fn,
    Var,
    Struct,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolInfo {
    pub kind: SymbolKind,
    pub type_display: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolLocation {
    pub uri: Url,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverTarget {
    Ident(String),
    FnCall(String),
    FieldAccess { obj: String, field: String },
    PipeOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionCtx {
    AfterDot(String),
    AfterPipe,
    General,
}

#[derive(Debug, Clone, Default)]
pub struct Backend {
    client: Option<Client>,
    pub doc_cache: DocCache,
}

impl Backend {
    pub fn new() -> Self {
        Self {
            client: None,
            doc_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client: Some(client),
            doc_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn server_capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), "|".to_string(), ">".to_string()]),
                ..CompletionOptions::default()
            }),
            ..ServerCapabilities::default()
        }
    }

    pub async fn analyze_document(&self, url: Url, text: String, version: i32) {
        let text_for_analysis = text.clone();
        let analysis = tokio::task::spawn_blocking(move || {
            let parse_result = parse_source(&text_for_analysis);
            let type_errors = if parse_result.is_ok() {
                type_check_source(&text_for_analysis)
            } else {
                Vec::new()
            };
            let pipeline_graphs = goblet_analyze_source(&text_for_analysis);
            (parse_result, type_errors, pipeline_graphs)
        })
        .await;

        let (parse_result, type_errors, pipeline_graphs_result) = match analysis {
            Ok(result) => result,
            Err(join_err) => {
                let diagnostic = Diagnostic {
                    range: Range::default(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("analysis task failed: {join_err}"),
                    source: Some("forge-lsp".to_string()),
                    ..Diagnostic::default()
                };
                self.store_document(
                    url.clone(),
                    DocumentState {
                        text,
                        version,
                        ast: None,
                        pipeline_graphs: Vec::new(),
                        symbol_map: SymbolMap::new(),
                        symbol_table: SymbolTable::new(),
                        diagnostics: vec![diagnostic.clone()],
                    },
                );
                self.publish_diagnostics(url, vec![diagnostic], version)
                    .await;
                return;
            }
        };

        let mut diagnostics = Vec::new();
        let ast = match parse_result {
            Ok(module) => {
                diagnostics.extend(type_errors.iter().cloned().map(type_error_to_diagnostic));
                Some(module)
            }
            Err(err) => {
                diagnostics.push(parse_error_to_diagnostic(err));
                None
            }
        };

        let symbol_map = ast
            .as_ref()
            .map(|module| collect_symbols(&module.stmts))
            .unwrap_or_default();
        let symbol_table = ast
            .as_ref()
            .map(|module| build_symbol_table(url.clone(), &module.stmts))
            .unwrap_or_default();
        let pipeline_graphs = match pipeline_graphs_result {
            Ok(graphs) => graphs,
            Err(err) => {
                if let Some(client) = &self.client {
                    client
                        .log_message(
                            MessageType::WARNING,
                            format!("forge-goblet analysis failed: {err}"),
                        )
                        .await;
                }
                Vec::new()
            }
        };

        self.store_document(
            url.clone(),
            DocumentState {
                text,
                version,
                ast,
                pipeline_graphs,
                symbol_map,
                symbol_table,
                diagnostics: diagnostics.clone(),
            },
        );
        self.publish_diagnostics(url, diagnostics, version).await;
    }

    fn store_document(&self, url: Url, state: DocumentState) {
        self.doc_cache
            .lock()
            .expect("doc_cache poisoned")
            .insert(url, state);
    }

    async fn publish_diagnostics(&self, url: Url, diagnostics: Vec<Diagnostic>, version: i32) {
        if let Some(client) = &self.client {
            client
                .publish_diagnostics(url, diagnostics, Some(version))
                .await;
        }
    }

    async fn queue_analysis(&self, url: Url, text: String, version: i32) {
        if self.client.is_some() {
            let backend = self.clone();
            tokio::spawn(async move {
                backend.analyze_document(url, text, version).await;
            });
        } else {
            self.analyze_document(url, text, version).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: Self::server_capabilities(),
            server_info: Some(ServerInfo {
                name: "forge-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        if let Some(client) = &self.client {
            client
                .log_message(MessageType::INFO, "forge-lsp initialized")
                .await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let text_document_position = params.text_document_position_params;
        let cache = self.doc_cache.lock().expect("doc_cache poisoned");
        let Some(doc) = cache.get(&text_document_position.text_document.uri) else {
            return Ok(None);
        };
        let Some(ast) = &doc.ast else {
            return Ok(None);
        };

        let target = find_node_at(ast, text_document_position.position, &doc.text);
        let Some(target) = target else {
            return Ok(None);
        };

        let markdown = match target {
            HoverTarget::Ident(name) => doc
                .symbol_map
                .get(&name)
                .map(|symbol| symbol_hover_markdown(&name, symbol))
                .or_else(|| field_hover_markdown(&name, &doc.symbol_map)),
            HoverTarget::FnCall(name) => doc
                .symbol_map
                .get(&name)
                .map(|symbol| symbol_hover_markdown(&name, symbol)),
            HoverTarget::FieldAccess { field, .. } => field_hover_markdown(&field, &doc.symbol_map),
            HoverTarget::PipeOp => {
                find_pipeline_at(&doc.pipeline_graphs, text_document_position.position)
                    .map(|(graph, node_id)| format_pipeline_hover(graph, node_id))
            }
        };

        Ok(markdown.map(|value| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let text_document_position = params.text_document_position_params;
        let cache = self.doc_cache.lock().expect("doc_cache poisoned");
        let Some(doc) = cache.get(&text_document_position.text_document.uri) else {
            return Ok(None);
        };
        let Some(ast) = &doc.ast else {
            return Ok(None);
        };
        let Some(symbol) = find_ident_at(ast, text_document_position.position, &doc.text) else {
            return Ok(None);
        };
        let Some(location) = doc.symbol_table.get(&symbol) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: location.uri.clone(),
            range: span_to_range(&location.span),
        })))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let text_document_position = params.text_document_position;
        let cache = self.doc_cache.lock().expect("doc_cache poisoned");
        let Some(doc) = cache.get(&text_document_position.text_document.uri) else {
            return Ok(None);
        };
        let ctx = completion_context(&doc.text, text_document_position.position, &doc.symbol_map);
        let items = match ctx {
            CompletionCtx::AfterDot(type_display) => method_completions(&type_display),
            CompletionCtx::AfterPipe => pipeline_completions(),
            CompletionCtx::General => {
                let mut items = keyword_completions();
                items.extend(local_var_completions(&doc.symbol_map));
                items
            }
        };
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.queue_analysis(document.uri, document.text, document.version)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.last() else {
            return;
        };
        self.queue_analysis(
            params.text_document.uri,
            change.text.clone(),
            params.text_document.version,
        )
        .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.doc_cache
            .lock()
            .expect("doc_cache poisoned")
            .remove(&params.text_document.uri);
        self.publish_diagnostics(params.text_document.uri, Vec::new(), 0)
            .await;
    }
}

pub fn span_to_range(span: &Span) -> Range {
    let start_line = span.line.saturating_sub(1) as u32;
    let start_col = span.col.saturating_sub(1) as u32;
    let width = span.end.saturating_sub(span.start).max(1) as u32;
    Range {
        start: Position {
            line: start_line,
            character: start_col,
        },
        end: Position {
            line: start_line,
            character: start_col + width,
        },
    }
}

pub fn range_contains_position(range: &Range, pos: &Position) -> bool {
    (pos.line > range.start.line
        || (pos.line == range.start.line && pos.character >= range.start.character))
        && (pos.line < range.end.line
            || (pos.line == range.end.line && pos.character < range.end.character))
}

fn parse_error_to_diagnostic(err: ParseError) -> Diagnostic {
    Diagnostic {
        range: parse_error_span(&err)
            .as_ref()
            .map(span_to_range)
            .unwrap_or_default(),
        severity: Some(DiagnosticSeverity::ERROR),
        message: err.to_string(),
        source: Some("forge-lsp".to_string()),
        ..Diagnostic::default()
    }
}

fn parse_error_span(err: &ParseError) -> Option<Span> {
    match err {
        ParseError::UnexpectedToken { span, .. } => Some(span.clone()),
        ParseError::UnexpectedEof { .. } => None,
    }
}

fn type_error_to_diagnostic(err: TypeError) -> Diagnostic {
    Diagnostic {
        range: err.span.as_ref().map(span_to_range).unwrap_or_default(),
        severity: Some(DiagnosticSeverity::ERROR),
        message: err.message,
        source: Some("forge-lsp".to_string()),
        ..Diagnostic::default()
    }
}

pub fn collect_symbols(stmts: &[Stmt]) -> SymbolMap {
    let mut symbols = SymbolMap::new();
    for stmt in stmts {
        collect_stmt_symbols(stmt, &mut symbols);
    }
    symbols
}

pub fn build_symbol_table(uri: Url, stmts: &[Stmt]) -> SymbolTable {
    let mut table = SymbolTable::new();
    for stmt in stmts {
        collect_stmt_locations(&uri, stmt, &mut table);
    }
    table
}

fn collect_stmt_symbols(stmt: &Stmt, symbols: &mut SymbolMap) {
    match stmt {
        Stmt::Fn {
            name,
            params,
            return_type,
            span,
            ..
        } => {
            symbols.insert(
                name.clone(),
                SymbolInfo {
                    kind: SymbolKind::Fn,
                    type_display: format_fn_signature(name, params, return_type.as_ref()),
                    span: span.clone(),
                },
            );
        }
        Stmt::Let {
            pat: Pat::Ident(name),
            type_ann,
            value,
            span,
            ..
        } => {
            symbols.insert(
                name.clone(),
                SymbolInfo {
                    kind: SymbolKind::Var,
                    type_display: type_ann
                        .as_ref()
                        .map(type_ann_display)
                        .unwrap_or_else(|| infer_expr_type_display(value)),
                    span: span.clone(),
                },
            );
        }
        Stmt::StructDef {
            name, fields, span, ..
        } => {
            symbols.insert(
                name.clone(),
                SymbolInfo {
                    kind: SymbolKind::Struct,
                    type_display: format!(
                        "struct {} {{ {} }}",
                        name,
                        fields
                            .iter()
                            .map(|(field, ann)| format!("{field}: {}", type_ann_display(ann)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span: span.clone(),
                },
            );
            for (field, ann) in fields {
                symbols.entry(field.clone()).or_insert(SymbolInfo {
                    kind: SymbolKind::Var,
                    type_display: type_ann_display(ann),
                    span: span.clone(),
                });
            }
        }
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for stmt in body {
                collect_stmt_symbols(stmt, symbols);
            }
        }
        _ => {}
    }
}

fn collect_stmt_locations(uri: &Url, stmt: &Stmt, table: &mut SymbolTable) {
    match stmt {
        Stmt::Fn { name, span, .. }
        | Stmt::State { name, span, .. }
        | Stmt::Const { name, span, .. }
        | Stmt::StructDef { name, span, .. } => {
            table.insert(
                name.clone(),
                SymbolLocation {
                    uri: uri.clone(),
                    span: span.clone(),
                },
            );
        }
        Stmt::Let {
            pat: Pat::Ident(name),
            span,
            ..
        } => {
            table.insert(
                name.clone(),
                SymbolLocation {
                    uri: uri.clone(),
                    span: span.clone(),
                },
            );
        }
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for stmt in body {
                collect_stmt_locations(uri, stmt, table);
            }
        }
        _ => {}
    }
}

fn format_fn_signature(
    name: &str,
    params: &[forge_compiler::ast::Param],
    return_type: Option<&TypeAnn>,
) -> String {
    let params = params
        .iter()
        .map(|param| match &param.type_ann {
            Some(ann) => format!("{}: {}", param.name, type_ann_display(ann)),
            None => param.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = return_type
        .map(type_ann_display)
        .unwrap_or_else(|| "unit".to_string());
    format!("fn {name}({params}) -> {return_type}")
}

fn type_ann_display(ann: &TypeAnn) -> String {
    match ann {
        TypeAnn::Number => "number".to_string(),
        TypeAnn::Float => "float".to_string(),
        TypeAnn::String => "string".to_string(),
        TypeAnn::Bool => "bool".to_string(),
        TypeAnn::Option(inner) => format!("{}?", type_ann_display(inner)),
        TypeAnn::Result(inner) | TypeAnn::ResultWith(inner, _) => {
            format!("{}!", type_ann_display(inner))
        }
        TypeAnn::List(inner) | TypeAnn::Generate(inner) => {
            format!("list<{}>", type_ann_display(inner))
        }
        TypeAnn::Named(name) => name.clone(),
        TypeAnn::AnonStruct(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, ann)| format!("{name}: {}", type_ann_display(ann)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeAnn::Generic { name, args } => format!(
            "{}<{}>",
            name,
            args.iter()
                .map(type_ann_display)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeAnn::Map(key, value) | TypeAnn::OrderedMap(key, value) => {
            format!(
                "map<{}, {}>",
                type_ann_display(key),
                type_ann_display(value)
            )
        }
        TypeAnn::Set(inner) | TypeAnn::OrderedSet(inner) => {
            format!("set<{}>", type_ann_display(inner))
        }
        TypeAnn::Unit => "unit".to_string(),
        TypeAnn::Fn {
            params,
            return_type,
        } => format!(
            "fn({}) -> {}",
            params
                .iter()
                .map(type_ann_display)
                .collect::<Vec<_>>()
                .join(", "),
            type_ann_display(return_type)
        ),
        TypeAnn::StringLiteralUnion(values) => values.join(" | "),
    }
}

fn infer_expr_type_display(expr: &Expr) -> String {
    match expr {
        Expr::Literal(lit, _) => match lit {
            forge_compiler::ast::Literal::Int(_) => "number".to_string(),
            forge_compiler::ast::Literal::Float(_) => "float".to_string(),
            forge_compiler::ast::Literal::String(_) => "string".to_string(),
            forge_compiler::ast::Literal::Bool(_) => "bool".to_string(),
        },
        Expr::List(items, _) => items
            .first()
            .map(|item| format!("list<{}>", infer_expr_type_display(item)))
            .unwrap_or_else(|| "list<unknown>".to_string()),
        Expr::Question(inner, _) => format!("{}?", infer_expr_type_display(inner)),
        Expr::StructInit { name, .. } => name.clone(),
        Expr::AnonStruct { fields, .. } => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(name, value)| match value {
                    Some(value) => format!("{name}: {}", infer_expr_type_display(value)),
                    None => format!("{name}: unknown"),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "unknown".to_string(),
    }
}

pub fn find_node_at(module: &Module, pos: Position, text: &str) -> Option<HoverTarget> {
    let mut best: Option<(usize, HoverTarget)> = None;
    for stmt in &module.stmts {
        visit_stmt_for_hover(stmt, pos, text, &mut best);
    }
    best.map(|(_, target)| target)
}

pub fn find_ident_at(module: &Module, pos: Position, text: &str) -> Option<String> {
    match find_node_at(module, pos, text) {
        Some(HoverTarget::Ident(name)) | Some(HoverTarget::FnCall(name)) => Some(name),
        Some(HoverTarget::FieldAccess { field, .. }) => Some(field),
        Some(HoverTarget::PipeOp) | None => None,
    }
}

fn visit_stmt_for_hover(
    stmt: &Stmt,
    pos: Position,
    text: &str,
    best: &mut Option<(usize, HoverTarget)>,
) {
    match stmt {
        Stmt::Let {
            pat, value, span, ..
        } => {
            if let Pat::Ident(name) = pat {
                update_best(best, span, pos, HoverTarget::Ident(name.clone()));
            }
            visit_expr_for_hover(value, pos, text, best);
        }
        Stmt::State {
            name, value, span, ..
        }
        | Stmt::Const {
            name, value, span, ..
        } => {
            update_best(best, span, pos, HoverTarget::Ident(name.clone()));
            visit_expr_for_hover(value, pos, text, best);
        }
        Stmt::Fn {
            name, body, span, ..
        } => {
            update_best(best, span, pos, HoverTarget::Ident(name.clone()));
            visit_expr_for_hover(body, pos, text, best);
        }
        Stmt::StructDef { name, span, .. } => {
            update_best(best, span, pos, HoverTarget::Ident(name.clone()));
        }
        Stmt::Expr(expr) => visit_expr_for_hover(expr, pos, text, best),
        Stmt::Return(Some(expr), _) => visit_expr_for_hover(expr, pos, text, best),
        Stmt::Yield { value, .. } => visit_expr_for_hover(value, pos, text, best),
        Stmt::When { body, .. } | Stmt::TestBlock { body, .. } => {
            for stmt in body {
                visit_stmt_for_hover(stmt, pos, text, best);
            }
        }
        Stmt::Defer { body, .. } => match body {
            forge_compiler::ast::DeferBody::Expr(expr)
            | forge_compiler::ast::DeferBody::Block(expr) => {
                visit_expr_for_hover(expr, pos, text, best)
            }
        },
        _ => {}
    }
}

fn visit_expr_for_hover(
    expr: &Expr,
    pos: Position,
    text: &str,
    best: &mut Option<(usize, HoverTarget)>,
) {
    match expr {
        Expr::Ident(name, span) => update_best(best, span, pos, HoverTarget::Ident(name.clone())),
        Expr::Call { callee, args, span } => {
            if let Expr::Ident(name, _) = callee.as_ref() {
                update_best(best, span, pos, HoverTarget::FnCall(name.clone()));
            }
            visit_expr_for_hover(callee, pos, text, best);
            for arg in args {
                visit_expr_for_hover(arg, pos, text, best);
            }
        }
        Expr::MethodCall {
            object,
            method,
            args,
            span,
        } => {
            let target = if line_contains_pipe(text, pos.line) {
                HoverTarget::PipeOp
            } else {
                HoverTarget::FnCall(method.clone())
            };
            update_best(best, span, pos, target);
            visit_expr_for_hover(object, pos, text, best);
            for arg in args {
                visit_expr_for_hover(arg, pos, text, best);
            }
        }
        Expr::Field {
            object,
            field,
            span,
        } => {
            update_best(
                best,
                span,
                pos,
                HoverTarget::FieldAccess {
                    obj: expr_head_name(object).unwrap_or_default(),
                    field: field.clone(),
                },
            );
            visit_expr_for_hover(object, pos, text, best);
        }
        Expr::BinOp { left, right, .. } => {
            visit_expr_for_hover(left, pos, text, best);
            visit_expr_for_hover(right, pos, text, best);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Await { expr: operand, .. }
        | Expr::Question(operand, ..)
        | Expr::Spawn { body: operand, .. } => visit_expr_for_hover(operand, pos, text, best),
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            visit_expr_for_hover(cond, pos, text, best);
            visit_expr_for_hover(then_block, pos, text, best);
            if let Some(else_block) = else_block {
                visit_expr_for_hover(else_block, pos, text, best);
            }
        }
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                visit_stmt_for_hover(stmt, pos, text, best);
            }
            if let Some(tail) = tail {
                visit_expr_for_hover(tail, pos, text, best);
            }
        }
        Expr::Closure { body, .. } => visit_expr_for_hover(body, pos, text, best),
        Expr::List(items, _) => {
            for item in items {
                visit_expr_for_hover(item, pos, text, best);
            }
        }
        Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                visit_expr_for_hover(value, pos, text, best);
            }
        }
        Expr::AnonStruct { fields, .. } => {
            for (_, value) in fields {
                if let Some(value) = value {
                    visit_expr_for_hover(value, pos, text, best);
                }
            }
        }
        _ => {}
    }
}

fn update_best(
    best: &mut Option<(usize, HoverTarget)>,
    span: &Span,
    pos: Position,
    target: HoverTarget,
) {
    let range = span_to_range(span);
    if !range_contains_position(&range, &pos) {
        return;
    }
    let width = span.end.saturating_sub(span.start);
    match best {
        Some((best_width, _)) if *best_width <= width => {}
        _ => *best = Some((width, target)),
    }
}

fn expr_head_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => Some(name.clone()),
        Expr::Field { object, .. } => expr_head_name(object),
        _ => None,
    }
}

fn line_contains_pipe(text: &str, line: u32) -> bool {
    text.lines()
        .nth(line as usize)
        .map(|line| line.contains("|>"))
        .unwrap_or(false)
}

fn symbol_hover_markdown(name: &str, symbol: &SymbolInfo) -> String {
    match symbol.kind {
        SymbolKind::Fn => format!("`{}`", symbol.type_display),
        SymbolKind::Var => format!("`{}: {}`", name, symbol.type_display),
        SymbolKind::Struct => format!("`{}`", symbol.type_display),
    }
}

fn field_hover_markdown(field: &str, symbols: &SymbolMap) -> Option<String> {
    symbols
        .get(field)
        .map(|symbol| format!("`{}: {}`", field, symbol.type_display))
}

pub fn completion_context(text: &str, pos: Position, symbols: &SymbolMap) -> CompletionCtx {
    let Some(line) = text.lines().nth(pos.line as usize) else {
        return CompletionCtx::General;
    };
    let prefix = &line[..std::cmp::min(pos.character as usize, line.len())];

    if prefix.trim_end().ends_with("|>") {
        return CompletionCtx::AfterPipe;
    }

    if let Some(head) = prefix.strip_suffix('.') {
        let ident = trailing_ident(head);
        if let Some(symbol) = ident.and_then(|name| symbols.get(name)) {
            return CompletionCtx::AfterDot(symbol.type_display.clone());
        }
    }

    CompletionCtx::General
}

pub fn method_completions(type_display: &str) -> Vec<CompletionItem> {
    let type_name = classify_type_name(type_display);
    builtin_sigs()
        .into_iter()
        .filter(|sig| sig.type_name == type_name)
        .map(|sig| CompletionItem {
            label: sig.method_name,
            detail: Some(sig.output_type),
            kind: Some(CompletionItemKind::METHOD),
            ..CompletionItem::default()
        })
        .collect()
}

pub fn pipeline_completions() -> Vec<CompletionItem> {
    [
        "filter", "map", "find", "fold", "take", "skip", "group_by", "zip", "flat_map",
    ]
    .into_iter()
    .map(|label| CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        ..CompletionItem::default()
    })
    .collect()
}

pub fn keyword_completions() -> Vec<CompletionItem> {
    [
        "let", "fn", "if", "else", "match", "for", "in", "return", "struct", "enum",
    ]
    .into_iter()
    .map(|label| CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        ..CompletionItem::default()
    })
    .collect()
}

pub fn local_var_completions(symbols: &SymbolMap) -> Vec<CompletionItem> {
    let mut items = symbols
        .iter()
        .filter(|(_, symbol)| symbol.kind == SymbolKind::Var)
        .map(|(name, symbol)| CompletionItem {
            label: name.clone(),
            detail: Some(symbol.type_display.clone()),
            kind: Some(CompletionItemKind::VARIABLE),
            ..CompletionItem::default()
        })
        .collect::<Vec<_>>();
    items.sort_by(|lhs, rhs| lhs.label.cmp(&rhs.label));
    items
}

fn classify_type_name(type_display: &str) -> &'static str {
    if type_display.starts_with("list<") {
        "list"
    } else if type_display.ends_with('?') {
        "option"
    } else if type_display.ends_with('!') {
        "result"
    } else if type_display == "string" {
        "string"
    } else {
        ""
    }
}

fn trailing_ident(input: &str) -> Option<&str> {
    let end = input.trim_end();
    let start = end
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let ident = &end[start..];
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

pub fn find_pipeline_at(
    graphs: &[PipelineGraph],
    pos: Position,
) -> Option<(&PipelineGraph, NodeId)> {
    let mut best: Option<(&PipelineGraph, NodeId, usize)> = None;

    for graph in graphs {
        for node in &graph.nodes {
            let Some(span) = &node.span else {
                continue;
            };
            let range = source_span_to_range(span);
            if !range_contains_position(&range, &pos) && range.start.line != pos.line {
                continue;
            }
            let distance = span.col.saturating_sub(pos.character as usize + 1);
            match best {
                Some((_, _, best_distance)) if best_distance <= distance => {}
                _ => best = Some((graph, node.id, distance)),
            }
        }
    }

    best.map(|(graph, node_id, _)| (graph, node_id))
}

pub fn format_pipeline_hover(graph: &PipelineGraph, cursor_node: NodeId) -> String {
    let mut lines = Vec::new();
    let title = graph
        .function_name
        .clone()
        .or_else(|| pipeline_root_name(graph));
    if let Some(name) = title {
        lines.push(format!("**Pipeline: `{}`**", name));
        lines.push(String::new());
    }
    lines.push("```text".to_string());

    for node in &graph.nodes {
        let marker = if node.id == cursor_node {
            "▶"
        } else if node.status == NodeStatus::Error {
            "⚠"
        } else {
            " "
        };
        let ty = node
            .output_type
            .as_ref()
            .map(|summary| summary.display.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let state = node
            .data_info
            .as_ref()
            .map(|info| format!("{:?}", info.state))
            .unwrap_or_else(|| "Unknown".to_string());
        lines.push(format!(
            "{} [{}] {}  {}  {}",
            marker, node.id.0, node.label, ty, state
        ));
    }

    lines.push("```".to_string());

    if !graph.diagnostics.is_empty() {
        lines.push(String::new());
        for diagnostic in &graph.diagnostics {
            lines.push(format!("⚠ {}", diagnostic.message));
        }
    }

    lines.join("\n")
}

fn pipeline_root_name(graph: &PipelineGraph) -> Option<String> {
    graph
        .roots
        .first()
        .and_then(|root| graph.nodes.iter().find(|node| node.id == *root))
        .map(|node| node.label.clone())
}

fn source_span_to_range(span: &forge_goblet::SourceSpan) -> Range {
    let start_line = span.line.saturating_sub(1) as u32;
    let start_col = span.col.saturating_sub(1) as u32;
    let width = span.end.saturating_sub(span.start).max(1) as u32;
    Range {
        start: Position {
            line: start_line,
            character: start_col,
        },
        end: Position {
            line: start_line,
            character: start_col + width,
        },
    }
}

#[cfg(test)]
mod tests {
    use forge_goblet::{
        DataShape, DataState, Diagnostic as GobletDiagnostic, NodeDataInfo, NodeId, PipelineNode,
        TypeSummary,
    };
    use tower_lsp::lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    };

    use super::*;

    fn test_url(name: &str) -> Url {
        Url::parse(&format!("file:///C:/tmp/{name}.forge")).expect("valid url")
    }

    fn sample_pipeline_graph() -> PipelineGraph {
        let mut graph = PipelineGraph::new();
        graph.function_name = Some("names".to_string());

        let mut source = PipelineNode::new("students", forge_goblet::NodeKind::Source);
        source.id = NodeId(1);
        source.span = Some(forge_goblet::SourceSpan {
            start: 0,
            end: 8,
            line: 1,
            col: 1,
        });
        source.output_type = Some(TypeSummary::new("list<Student>"));
        source.data_info = Some(NodeDataInfo {
            param_name: None,
            param_shape: None,
            shape: DataShape::List(Box::new(DataShape::Struct {
                name: "Student".to_string(),
                fields: vec![("name".to_string(), DataShape::Scalar("string".to_string()))],
            })),
            state: DataState::Definite,
        });
        let source_id = graph.add_node(source);
        graph.roots.push(source_id);

        let mut filter =
            PipelineNode::new("filter(s => s.score >= 80)", forge_goblet::NodeKind::Filter);
        filter.span = Some(forge_goblet::SourceSpan {
            start: 12,
            end: 30,
            line: 1,
            col: 13,
        });
        filter.output_type = Some(TypeSummary::new("list<Student>"));
        filter.data_info = Some(NodeDataInfo {
            param_name: Some("s".to_string()),
            param_shape: Some(DataShape::Struct {
                name: "Student".to_string(),
                fields: vec![("score".to_string(), DataShape::Scalar("number".to_string()))],
            }),
            shape: DataShape::List(Box::new(DataShape::Struct {
                name: "Student".to_string(),
                fields: vec![("score".to_string(), DataShape::Scalar("number".to_string()))],
            })),
            state: DataState::MaybeEmpty,
        });
        filter.status = NodeStatus::Error;
        let filter_id = graph.add_node(filter);
        graph.add_edge(source_id, filter_id, None);
        graph.add_diagnostic(GobletDiagnostic {
            node_id: Some(filter_id),
            code: "InvalidFieldAccess".to_string(),
            message: "field `score` not found".to_string(),
            span: None,
            expected: None,
            actual: None,
        });

        graph
    }

    #[test]
    fn test_backend_new() {
        let backend = Backend::new();
        assert!(backend.client.is_none());
        assert!(backend.doc_cache.lock().expect("cache").is_empty());
    }

    #[tokio::test]
    async fn test_initialize_capabilities() {
        let backend = Backend::new();
        let result = backend
            .initialize(InitializeParams::default())
            .await
            .expect("initialize succeeds");
        assert_eq!(
            result.capabilities.text_document_sync,
            Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL))
        );
        assert_eq!(
            result.server_info.expect("server info").name,
            "forge-lsp".to_string()
        );
    }

    #[test]
    fn test_span_to_range() {
        let range = span_to_range(&Span {
            start: 10,
            end: 14,
            line: 2,
            col: 5,
        });
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 4);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 8);
    }

    #[test]
    fn test_range_contains_position() {
        let range = Range {
            start: Position {
                line: 0,
                character: 2,
            },
            end: Position {
                line: 0,
                character: 5,
            },
        };
        assert!(range_contains_position(
            &range,
            &Position {
                line: 0,
                character: 2
            }
        ));
        assert!(range_contains_position(
            &range,
            &Position {
                line: 0,
                character: 4
            }
        ));
        assert!(!range_contains_position(
            &range,
            &Position {
                line: 0,
                character: 5
            }
        ));
    }

    #[test]
    fn test_parse_error_to_diagnostic() {
        let diagnostic = parse_error_to_diagnostic(ParseError::UnexpectedToken {
            expected: "Ident".to_string(),
            found: forge_compiler::lexer::tokens::TokenKind::Let,
            span: Span {
                start: 0,
                end: 3,
                line: 1,
                col: 1,
            },
        });
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn test_type_error_to_diagnostic() {
        let diagnostic = type_error_to_diagnostic(TypeError {
            message: "型不一致".to_string(),
            span: Some(Span {
                start: 4,
                end: 5,
                line: 1,
                col: 5,
            }),
        });
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.range.start.character, 4);
    }

    #[tokio::test]
    async fn test_did_open_updates_cache() {
        let backend = Backend::new();
        let url = test_url("did_open");
        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: url.clone(),
                    language_id: "forge".to_string(),
                    version: 1,
                    text: "let x = 1".to_string(),
                },
            })
            .await;
        let cache = backend.doc_cache.lock().expect("cache");
        let doc = cache.get(&url).expect("document cached");
        assert_eq!(doc.text, "let x = 1");
        assert_eq!(doc.version, 1);
    }

    #[tokio::test]
    async fn test_no_diagnostics_on_valid_source() {
        let backend = Backend::new();
        let url = test_url("valid");
        backend
            .analyze_document(url.clone(), "let x = 1".to_string(), 1)
            .await;
        let cache = backend.doc_cache.lock().expect("cache");
        let doc = cache.get(&url).expect("document cached");
        assert!(doc.diagnostics.is_empty());
        assert!(doc.ast.is_some());
        assert!(doc.symbol_map.contains_key("x"));
    }

    #[tokio::test]
    async fn test_did_change_updates_cache() {
        let backend = Backend::new();
        let url = test_url("did_change");
        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: url.clone(),
                    language_id: "forge".to_string(),
                    version: 1,
                    text: "let x = 1".to_string(),
                },
            })
            .await;
        backend
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: url.clone(),
                    version: 2,
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "let x = 2".to_string(),
                }],
            })
            .await;
        let cache = backend.doc_cache.lock().expect("cache");
        let doc = cache.get(&url).expect("document cached");
        assert_eq!(doc.text, "let x = 2");
        assert_eq!(doc.version, 2);
    }

    #[tokio::test]
    async fn test_did_close_removes_cache() {
        let backend = Backend::new();
        let url = test_url("did_close");
        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: url.clone(),
                    language_id: "forge".to_string(),
                    version: 1,
                    text: "let x = 1".to_string(),
                },
            })
            .await;
        backend
            .did_close(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: url.clone() },
            })
            .await;
        assert!(!backend.doc_cache.lock().expect("cache").contains_key(&url));
    }

    #[test]
    fn test_collect_fn_symbols() {
        let module = parse_source("fn foo(a: string) -> bool { true }").expect("parse");
        let symbols = collect_symbols(&module.stmts);
        assert_eq!(
            symbols.get("foo").expect("fn symbol").type_display,
            "fn foo(a: string) -> bool"
        );
    }

    #[test]
    fn test_collect_let_symbols() {
        let module = parse_source("let x: number = 5").expect("parse");
        let symbols = collect_symbols(&module.stmts);
        assert_eq!(symbols.get("x").expect("let symbol").type_display, "number");
    }

    #[test]
    fn test_completion_ctx_after_dot() {
        let mut symbols = SymbolMap::new();
        symbols.insert(
            "xs".to_string(),
            SymbolInfo {
                kind: SymbolKind::Var,
                type_display: "list<number>".to_string(),
                span: Span {
                    start: 0,
                    end: 2,
                    line: 1,
                    col: 1,
                },
            },
        );
        let ctx = completion_context(
            "let xs: list<number> = [1]\nxs.",
            Position {
                line: 1,
                character: 3,
            },
            &symbols,
        );
        assert_eq!(ctx, CompletionCtx::AfterDot("list<number>".to_string()));
    }

    #[test]
    fn test_completion_ctx_after_pipe() {
        let ctx = completion_context(
            "let ys = xs |> ",
            Position {
                line: 0,
                character: 15,
            },
            &SymbolMap::new(),
        );
        assert_eq!(ctx, CompletionCtx::AfterPipe);
    }

    #[test]
    fn test_completion_ctx_after_pipe_without_space() {
        let ctx = completion_context(
            "let ys = xs |>",
            Position {
                line: 0,
                character: 14,
            },
            &SymbolMap::new(),
        );
        assert_eq!(ctx, CompletionCtx::AfterPipe);
    }

    #[test]
    fn test_method_completions_list() {
        let labels = method_completions("list<number>")
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();
        assert!(labels.contains(&"map".to_string()));
        assert!(labels.contains(&"filter".to_string()));
        assert!(labels.contains(&"find".to_string()));
    }

    #[test]
    fn test_pipeline_completions_include_filter() {
        let labels = pipeline_completions()
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();
        assert!(labels.contains(&"filter".to_string()));
    }

    #[test]
    fn test_keyword_completions_include_let() {
        let labels = keyword_completions()
            .into_iter()
            .map(|item| item.label)
            .collect::<Vec<_>>();
        assert!(labels.contains(&"let".to_string()));
    }

    #[test]
    fn test_build_symbol_table_fn() {
        let module = parse_source("fn foo() -> unit { }").expect("parse");
        let table = build_symbol_table(test_url("defs_fn"), &module.stmts);
        assert!(table.contains_key("foo"));
    }

    #[test]
    fn test_build_symbol_table_let() {
        let module = parse_source("let x = 1").expect("parse");
        let table = build_symbol_table(test_url("defs_let"), &module.stmts);
        assert!(table.contains_key("x"));
    }

    #[test]
    fn test_find_node_at_ident() {
        let module = parse_source("let x: number = 5\nx").expect("parse");
        let target = find_node_at(
            &module,
            Position {
                line: 1,
                character: 0,
            },
            "let x: number = 5\nx",
        );
        assert_eq!(target, Some(HoverTarget::Ident("x".to_string())));
    }

    #[tokio::test]
    async fn test_hover_returns_fn_signature() {
        let backend = Backend::new();
        let url = test_url("hover_fn");
        let text = "fn foo(a: string) -> bool { true }\nfoo(\"x\")";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let hover = backend
            .hover(HoverParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url },
                    position: Position {
                        line: 1,
                        character: 1,
                    },
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover result")
            .expect("hover payload");
        let HoverContents::Markup(content) = hover.contents else {
            panic!("markup hover expected");
        };
        assert!(content.value.contains("fn foo(a: string) -> bool"));
    }

    #[tokio::test]
    async fn test_hover_returns_var_type() {
        let backend = Backend::new();
        let url = test_url("hover_var");
        let text = "let x: number = 5\nx";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let hover = backend
            .hover(HoverParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url },
                    position: Position {
                        line: 1,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover result")
            .expect("hover payload");
        let HoverContents::Markup(content) = hover.contents else {
            panic!("markup hover expected");
        };
        assert!(content.value.contains("`x: number`"));
    }

    #[tokio::test]
    async fn test_goto_definition_fn_call() {
        let backend = Backend::new();
        let url = test_url("goto_fn");
        let text = "fn foo() -> bool { true }\nfoo()";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let response = backend
            .goto_definition(GotoDefinitionParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position: Position {
                        line: 1,
                        character: 1,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("goto result")
            .expect("location");
        let GotoDefinitionResponse::Scalar(location) = response else {
            panic!("scalar location expected");
        };
        assert_eq!(location.uri, url);
        assert_eq!(location.range.start.line, 0);
    }

    #[tokio::test]
    async fn test_goto_definition_not_found() {
        let backend = Backend::new();
        let url = test_url("goto_none");
        let text = "let x = 1\nmissing";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let response = backend
            .goto_definition(GotoDefinitionParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url },
                    position: Position {
                        line: 1,
                        character: 1,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .expect("goto result");
        assert!(response.is_none());
    }

    #[test]
    fn test_format_pipeline_hover_marks_cursor_node() {
        let graph = sample_pipeline_graph();
        let output = format_pipeline_hover(&graph, NodeId(2));
        assert!(output.contains("▶ [2] filter"));
    }

    #[test]
    fn test_format_pipeline_hover_marks_error_node() {
        let graph = sample_pipeline_graph();
        let output = format_pipeline_hover(&graph, NodeId(1));
        assert!(output.contains("⚠ field `score` not found"));
    }

    #[test]
    fn test_find_pipeline_at_correct_graph() {
        let mut other = sample_pipeline_graph();
        other.function_name = Some("other".to_string());
        other.nodes[0].span = Some(forge_goblet::SourceSpan {
            start: 0,
            end: 4,
            line: 3,
            col: 1,
        });
        let graphs = vec![sample_pipeline_graph(), other];
        let (graph, _) = find_pipeline_at(
            &graphs,
            Position {
                line: 0,
                character: 15,
            },
        )
        .expect("pipeline found");
        assert_eq!(graph.function_name.as_deref(), Some("names"));
    }

    #[tokio::test]
    async fn test_pipe_hover_fallback_when_no_pipeline() {
        let backend = Backend::new();
        let url = test_url("pipe_none");
        let text = "let x = 1\nx";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let hover = backend
            .hover(HoverParams {
                text_document_position_params: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url },
                    position: Position {
                        line: 0,
                        character: 7,
                    },
                },
                work_done_progress_params: Default::default(),
            })
            .await
            .expect("hover result");
        assert!(hover.is_none());
    }

    #[tokio::test]
    async fn test_analyze_document_updates_pipeline_graphs() {
        let backend = Backend::new();
        let url = test_url("graphs");
        let text = "let names = students |> filter(s => s) |> map(s => s)";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let cache = backend.doc_cache.lock().expect("cache");
        let doc = cache.get(&url).expect("document cached");
        assert!(!doc.pipeline_graphs.is_empty());
    }

    #[tokio::test]
    async fn test_completion_general_includes_locals() {
        let backend = Backend::new();
        let url = test_url("completion_general");
        let text = "let x: number = 5\n";
        backend
            .analyze_document(url.clone(), text.to_string(), 1)
            .await;
        let response = backend
            .completion(CompletionParams {
                text_document_position: tower_lsp::lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url },
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            })
            .await
            .expect("completion result")
            .expect("completion payload");
        let CompletionResponse::Array(items) = response else {
            panic!("completion array expected");
        };
        assert!(items.iter().any(|item| item.label == "x"));
        assert!(items.iter().any(|item| item.label == "let"));
    }
}
