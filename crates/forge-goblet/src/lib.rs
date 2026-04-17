pub mod extractor;
pub mod graph;
pub mod render;
pub mod typing;

use std::fmt;
use std::fs;
use std::path::Path;

use forge_compiler::parser::parse_source;

pub use extractor::{analyze_module, extract_pipelines};
pub use graph::{
    DataShape, DataState, Diagnostic, NodeDataInfo, NodeId, NodeKind, NodeStatus, PipelineEdge,
    PipelineGraph, PipelineNode, SourceSpan, TypeSummary,
};
pub use render::json::render_json;
pub use render::mermaid::render_mermaid;
pub use render::text::render_text;
pub use typing::{builtin_sigs, type_propagate, BuiltinSig, TypeAnnotations};

#[derive(Debug)]
pub enum GobletError {
    ParseError(String),
    ExtractionError(String),
    InternalError(String),
}

impl fmt::Display for GobletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GobletError::ParseError(message)
            | GobletError::ExtractionError(message)
            | GobletError::InternalError(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GobletError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Mermaid,
}

pub fn analyze_source(src: &str) -> Result<Vec<PipelineGraph>, GobletError> {
    let module = parse_source(src).map_err(|err| GobletError::ParseError(err.to_string()))?;
    let mut graphs =
        analyze_module(&module).map_err(|err| GobletError::ExtractionError(err.message))?;
    let mut annotations = TypeAnnotations::collect(&module);
    for graph in &mut graphs {
        type_propagate(graph, &mut annotations);
    }
    Ok(graphs)
}

pub fn save_pipeline(
    graph: &PipelineGraph,
    path: &Path,
    format: OutputFormat,
) -> Result<(), GobletError> {
    let content = match format {
        OutputFormat::Text => render_text(graph),
        OutputFormat::Json => render_json(graph),
        OutputFormat::Mermaid => format!("```mermaid\n{}```\n", render_mermaid(graph)),
    };

    fs::write(path, content).map_err(|err| GobletError::InternalError(err.to_string()))
}

pub fn expand_closure_details(graph: &PipelineGraph) -> PipelineGraph {
    let mut expanded = PipelineGraph::new();
    expanded.source_file = graph.source_file.clone();
    expanded.function_name = graph.function_name.clone();
    expanded.diagnostics = graph.diagnostics.clone();

    for node in &graph.nodes {
        expanded.add_node(node.clone());
    }
    expanded.roots = graph.roots.clone();

    for edge in &graph.edges {
        expanded.add_edge(edge.from, edge.to, edge.label.clone());
    }

    for node in &graph.nodes {
        if node.kind != NodeKind::Closure {
            continue;
        }

        for (edge_label, detail_label) in closure_detail_specs(node) {
            let mut detail = PipelineNode::new(detail_label, NodeKind::ClosureDetail);
            detail.span = node.span.clone();
            detail.status = NodeStatus::Ok;
            detail.notes.push(format!("closure parent: {}", node.label));
            populate_closure_detail_types(&mut detail, node, &edge_label);
            let detail_id = expanded.add_node(detail);
            expanded.add_edge(node.id, detail_id, Some(edge_label));
        }
    }

    expanded
}

fn closure_detail_specs(node: &PipelineNode) -> Vec<(String, String)> {
    let mut details = Vec::new();

    for (prefix, edge_label, label_prefix) in [
        ("closure condition: ", "condition", "condition"),
        ("closure body: ", "body", "body"),
        ("closure tail: ", "tail", "tail"),
        ("closure branch then: ", "then", "then"),
        ("closure branch else: ", "else", "else"),
    ] {
        if let Some(value) = node
            .notes
            .iter()
            .find_map(|note| note.strip_prefix(prefix).map(str::to_string))
        {
            details.push((edge_label.to_string(), format!("{label_prefix}: {value}")));
        }
    }

    for value in node
        .notes
        .iter()
        .filter_map(|note| note.strip_prefix("field access: "))
    {
        details.push(("field".to_string(), format!("field: {value}")));
    }

    details
}

fn populate_closure_detail_types(
    detail: &mut PipelineNode,
    parent: &PipelineNode,
    edge_label: &str,
) {
    match edge_label {
        "condition" => {
            detail.output_type = Some(TypeSummary::new("bool"));
            detail.data_info = Some(NodeDataInfo {
                param_name: None,
                param_shape: None,
                shape: DataShape::Scalar("bool".to_string()),
                state: DataState::Definite,
            });
        }
        "body" | "tail" | "then" | "else" => {
            detail.output_type = parent.output_type.clone();
            if let Some(info) = &parent.data_info {
                detail.data_info = Some(NodeDataInfo {
                    param_name: None,
                    param_shape: None,
                    shape: info.shape.clone(),
                    state: info.state.clone(),
                });
            }
        }
        "field" => {
            if let (Some(label), Some(info)) = (
                detail.label.strip_prefix("field: "),
                parent.data_info.as_ref(),
            ) {
                if let Some(param_shape) = &info.param_shape {
                    if let Some(shape) = resolve_shape_path(param_shape, label) {
                        detail.output_type = Some(TypeSummary::new(shape_display(&shape)));
                        detail.data_info = Some(NodeDataInfo {
                            param_name: None,
                            param_shape: None,
                            shape,
                            state: DataState::Definite,
                        });
                    }
                }
            }
        }
        _ => {}
    }
}

fn resolve_shape_path(param_shape: &DataShape, expr: &str) -> Option<DataShape> {
    let mut segments = expr.split('.').map(str::trim);
    segments.next()?;
    let mut current = param_shape.clone();
    for segment in segments {
        current = match current {
            DataShape::Struct { fields, .. } | DataShape::AnonStruct(fields) => {
                fields.into_iter().find(|(name, _)| name == segment)?.1
            }
            _ => return None,
        };
    }
    Some(current)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::NamedTempFile;

    use super::*;

    fn sample_graph() -> PipelineGraph {
        let mut graph = PipelineGraph::new();

        let mut source = PipelineNode::new("students", NodeKind::Source);
        source.output_type = Some(TypeSummary::new("list<Student>"));
        source.data_info = Some(NodeDataInfo {
            param_name: None,
            param_shape: None,
            shape: DataShape::Struct {
                name: "Student".to_string(),
                fields: vec![
                    ("id".to_string(), DataShape::Scalar("number".to_string())),
                    ("name".to_string(), DataShape::Scalar("string".to_string())),
                ],
            },
            state: DataState::Definite,
        });
        source.status = NodeStatus::Ok;
        let source_id = graph.add_node(source);
        graph.roots.push(source_id);

        let mut filter = PipelineNode::new("filter(score >= 80)", NodeKind::Filter);
        filter.input_type = Some(TypeSummary::new("list<Student>"));
        filter.output_type = Some(TypeSummary::new("list<Student>"));
        filter.data_info = Some(NodeDataInfo {
            param_name: Some("s".to_string()),
            param_shape: Some(DataShape::Scalar("Student".to_string())),
            shape: DataShape::List(Box::new(DataShape::Scalar("Student".to_string()))),
            state: DataState::MaybeEmpty,
        });
        filter.status = NodeStatus::Ok;
        let filter_id = graph.add_node(filter);
        graph.add_edge(source_id, filter_id, Some("list<Student>".to_string()));

        let mut map = PipelineNode::new("map(s => s.name)", NodeKind::Map);
        map.input_type = Some(TypeSummary::new("list<Student>"));
        map.output_type = Some(TypeSummary::new("list<string>"));
        map.data_info = Some(NodeDataInfo {
            param_name: Some("s".to_string()),
            param_shape: Some(DataShape::Scalar("Student".to_string())),
            shape: DataShape::AnonStruct(vec![(
                "field".to_string(),
                DataShape::Scalar("string".to_string()),
            )]),
            state: DataState::MaybeEmpty,
        });
        map.status = NodeStatus::Error;
        let map_id = graph.add_node(map);
        graph.add_edge(filter_id, map_id, Some("list<string>".to_string()));
        graph.add_diagnostic(Diagnostic {
            node_id: Some(map_id),
            code: "InvalidFieldAccess".to_string(),
            message: "field access `name` is invalid on `number`".to_string(),
            span: None,
            expected: Some("Student".to_string()),
            actual: Some("number".to_string()),
        });

        graph
    }

    #[test]
    fn test_render_text_matches_expected_shape() {
        let output = render_text(&sample_graph());
        assert!(output.contains(
            "[1] students    list<Student>    { id: number, name: string }    (Definite)"
        ));
        assert!(output
            .contains("[2] filter(score >= 80)    list<Student>    list<Student>    (MaybeEmpty)"));
        assert!(output.contains("error: field access `name` is invalid on `number`"));
    }

    #[test]
    fn test_render_json_is_valid() {
        let output = render_json(&sample_graph());
        let value: serde_json::Value = serde_json::from_str(&output).expect("valid json");
        assert!(value.get("nodes").is_some());
    }

    #[test]
    fn test_render_mermaid_expectations() {
        let output = render_mermaid(&sample_graph());
        assert!(output.starts_with("flowchart LR"));
        assert!(output.contains("list&lt;Student&gt;"));
        assert!(output.contains("{ id: number, name: string }"));
        assert!(output.contains("N1[\""));
        assert!(output.contains("N2[\""));
        assert!(output.contains("N3[\""));
        assert!(output.contains(":::error"));
        assert!(output.contains("classDef error fill:#f88"));
    }

    #[test]
    fn test_save_pipeline_writes_non_empty_file() {
        let file = NamedTempFile::new().expect("tempfile");
        save_pipeline(&sample_graph(), file.path(), OutputFormat::Mermaid).expect("save succeeds");
        let content = fs::read_to_string(file.path()).expect("read output");
        assert!(!content.is_empty());
        assert!(content.contains("```mermaid"));
    }

    #[test]
    fn test_empty_graph_rendering_does_not_panic() {
        let graph = PipelineGraph::new();
        assert_eq!(render_text(&graph), "");
        let json = render_json(&graph);
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(value["nodes"].as_array().expect("nodes").len(), 0);
        assert!(render_mermaid(&graph).starts_with("flowchart LR"));
    }

    fn node_labels(graph: &PipelineGraph) -> Vec<String> {
        graph.nodes.iter().map(|node| node.label.clone()).collect()
    }

    fn longest_graph(graphs: &[PipelineGraph]) -> &PipelineGraph {
        graphs
            .iter()
            .max_by_key(|graph| graph.nodes.len())
            .expect("at least one graph")
    }

    #[test]
    fn test_extract_pipe_3_steps() {
        let graphs = analyze_source("let names = a |> f() |> g() |> h()").expect("extract");
        assert_eq!(graphs.len(), 1);
        assert_eq!(node_labels(&graphs[0]), vec!["names", "f()", "g()", "h()"]);
    }

    #[test]
    fn test_extract_method_chain() {
        let graphs = analyze_source("let ys = xs.filter(n => n).map(n => n)").expect("extract");
        let graph = longest_graph(&graphs);
        assert_eq!(
            node_labels(graph),
            vec!["ys", "filter(n => n)", "map(n => n)"]
        );
    }

    #[test]
    fn test_extract_pipe_equals_method_chain() {
        let pipe = analyze_source("let names = xs |> filter(n => n) |> map(n => n)").expect("pipe");
        let method = analyze_source("let names = xs.filter(n => n).map(n => n)").expect("method");
        assert_eq!(
            node_labels(longest_graph(&pipe)),
            node_labels(longest_graph(&method))
        );
    }

    #[test]
    fn test_extract_pipeline_block() {
        let graphs = analyze_source(
            r#"
pipeline {
    source xs
    filter n => n > 0
    map n => n * 2
    take 3
}
"#,
        )
        .expect("extract");
        let graph = longest_graph(&graphs);
        assert_eq!(
            node_labels(graph),
            vec!["xs", "filter(n => n Gt 0)", "map(n => n Mul 2)", "take(3)"]
        );
    }

    #[test]
    fn test_extract_let_binding_label() {
        let graphs = analyze_source("let names = students.map(s => s.name)").expect("extract");
        assert_eq!(graphs[0].nodes[0].label, "names");
        assert_eq!(graphs[0].roots.len(), 1);
    }

    #[test]
    fn test_extract_closure_notes() {
        let graphs =
            analyze_source("let names = students.filter(s => s.score >= 80)").expect("extract");
        let filter = &graphs[0].nodes[1];
        assert!(filter
            .notes
            .iter()
            .any(|note| note.contains("closure params: s")));
        assert!(filter
            .notes
            .iter()
            .any(|note| note.contains("field access: s.score")));
    }

    #[test]
    fn test_extract_tracks_function_name() {
        let graphs = analyze_source(
            r#"
fn names(xs: list<number>) -> list<number> {
    xs |> map(n => n * 2)
}
"#,
        )
        .expect("extract");
        assert_eq!(graphs[0].function_name.as_deref(), Some("names"));
    }

    #[test]
    fn test_expand_closure_details_adds_child_nodes() {
        let graphs = analyze_source(
            r#"
struct Student { name: string, score: number }
let xs: list<Student> = [Student { name: "A", score: 90 }]
let ys = xs.filter(n => if n.score >= 80 { true } else { false }).map(n => n.name)
"#,
        )
        .expect("extract");
        let expanded = expand_closure_details(longest_graph(&graphs));
        let labels = node_labels(&expanded);
        assert!(labels.iter().any(|label| label.starts_with("body: ")));
        assert!(labels.iter().any(|label| label.starts_with("condition: ")));
        assert!(labels
            .iter()
            .any(|label| label.starts_with("field: n.score")));
        assert!(expanded
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::ClosureDetail));
        let condition = expanded
            .nodes
            .iter()
            .find(|node| node.label.starts_with("condition: "))
            .expect("condition detail");
        assert_eq!(
            condition
                .output_type
                .as_ref()
                .expect("condition type")
                .display,
            "bool"
        );
        let field = expanded
            .nodes
            .iter()
            .find(|node| node.label.starts_with("field: n.score"))
            .expect("field detail");
        assert_eq!(
            field.output_type.as_ref().expect("field type").display,
            "number"
        );
    }

    #[test]
    fn test_type_list_filter_map() {
        let graphs = analyze_source(
            r#"
struct Student { name: string, score: number }
let students: list<Student> = [Student { name: "A", score: 90 }]
let names = students |> filter(s => s.score >= 80) |> map(s => s.name)
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        assert_eq!(
            graph.nodes[0].output_type.as_ref().expect("source").display,
            "list<Student>"
        );
        assert_eq!(
            graph.nodes[1].output_type.as_ref().expect("filter").display,
            "list<Student>"
        );
        assert_eq!(
            graph.nodes[2].output_type.as_ref().expect("map").display,
            "list<string>"
        );
        assert_eq!(
            graph.nodes[1]
                .data_info
                .as_ref()
                .expect("filter info")
                .state,
            DataState::MaybeEmpty
        );
    }

    #[test]
    fn test_type_anon_struct_notes() {
        let graphs = analyze_source(
            r#"
struct Student { name: string, score: number }
let students: list<Student> = [Student { name: "A", score: 90 }]
let cards = students |> map(s => { name: s.name, score: s.score })
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        let map = graph
            .nodes
            .iter()
            .find(|node| node.label.starts_with("map("))
            .expect("map node");
        assert!(map
            .notes
            .iter()
            .any(|note| note == "anon struct fields: { name: string, score: number }"));
    }

    #[test]
    fn test_type_anon_struct_nested_field_path() {
        let graphs = analyze_source(
            r#"
struct Meta { score: number }
struct Event { id: number, meta: Meta }
let events: list<Event> = [Event { id: 1, meta: Meta { score: 9 } }]
let cards = events |> map(e => { id: e.id, score: e.meta.score })
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        let map = graph
            .nodes
            .iter()
            .find(|node| node.label.starts_with("map("))
            .expect("map node");
        assert_eq!(
            map.output_type.as_ref().expect("map output").display,
            "list<{ id: number, score: number }>"
        );
        assert!(map
            .notes
            .iter()
            .any(|note| note == "anon struct fields: { id: number, score: number }"));
    }

    #[test]
    fn test_type_find_returns_option() {
        let graphs = analyze_source(
            r#"
struct Student { name: string }
let students: list<Student> = [Student { name: "A" }]
let one = students |> find(s => s.name == "A")
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        assert_eq!(
            graph.nodes[1].output_type.as_ref().expect("find").display,
            "Student?"
        );
        assert_eq!(
            graph.nodes[1].data_info.as_ref().expect("find info").state,
            DataState::MaybeNone
        );
    }

    #[test]
    fn test_type_option_unwrap_or() {
        let graphs = analyze_source(
            r#"
struct Student { name: string }
let maybe_student: Student? = none
let name = maybe_student |> unwrap_or(Student { name: "fallback" })
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        assert_eq!(
            graph.nodes[1]
                .output_type
                .as_ref()
                .expect("unwrap_or")
                .display,
            "Student"
        );
        assert_eq!(
            graph.nodes[1]
                .data_info
                .as_ref()
                .expect("unwrap_or info")
                .state,
            DataState::Definite
        );
    }

    #[test]
    fn test_type_mismatch_field_on_number() {
        let graphs = analyze_source(
            r#"
let numbers: list<number> = [1, 2, 3]
let names = numbers |> map(s => s.name)
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        let map_node = &graph.nodes[1];
        assert_eq!(map_node.status, NodeStatus::Error);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diag| diag.code == "InvalidFieldAccess"));
    }

    #[test]
    fn test_type_unknown_method() {
        let graphs = analyze_source(
            r#"
let numbers: list<number> = [1, 2, 3]
let names = numbers |> mystery()
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        assert_eq!(graph.nodes[1].status, NodeStatus::Error);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diag| diag.code == "UnknownMethod"));
    }

    #[test]
    fn test_type_binding_from_function_return_annotation() {
        let graphs = analyze_source(
            r#"
fn top() -> list<string> { ["a"] }
let top2 = top()
let size = top2.len()
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        assert_eq!(graph.nodes[0].label, "size");
        assert_eq!(
            graph.nodes[0].output_type.as_ref().expect("source").display,
            "list<string>"
        );
        assert_eq!(
            graph.nodes[1].output_type.as_ref().expect("len").display,
            "number"
        );
    }

    #[test]
    fn test_broken_pipeline_mermaid() {
        let graphs = analyze_source(
            r#"
let numbers: list<number> = [1, 2, 3]
let names = numbers |> map(s => s.name)
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        let output = render_mermaid(graph);
        assert!(output.contains(":::error"));
        assert!(output.contains("classDef error fill:#f88"));
    }

    #[test]
    fn test_broken_pipeline_save() {
        let graphs = analyze_source(
            r#"
let numbers: list<number> = [1, 2, 3]
let names = numbers |> map(s => s.name)
"#,
        )
        .expect("typed extract");
        let graph = longest_graph(&graphs);
        let file = NamedTempFile::new().expect("tempfile");
        save_pipeline(graph, file.path(), OutputFormat::Mermaid).expect("save succeeds");
        let content = fs::read_to_string(file.path()).expect("read output");
        assert!(content.contains("```mermaid"));
        assert!(content.contains(":::error"));
    }

    #[test]
    fn test_diag_unknown_symbol() {
        // `undefined_var` has no annotation — should produce UnknownSymbol
        let graphs = analyze_source(
            r#"
let names = undefined_var |> map(s => s.name)
"#,
        )
        .expect("extract");
        let graph = longest_graph(&graphs);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diag| diag.code == "UnknownSymbol"),
            "expected UnknownSymbol diagnostic, got: {:?}",
            graph.diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_diag_type_mismatch_and_then_option() {
        // and_then closure returns a non-Option value — TypeMismatch
        let graphs = analyze_source(
            r#"
struct User { name: string }
let maybe_user: User? = none
let result = maybe_user |> and_then(u => u.name)
"#,
        )
        .expect("extract");
        let graph = longest_graph(&graphs);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diag| diag.code == "TypeMismatch"),
            "expected TypeMismatch diagnostic, got: {:?}",
            graph.diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_diag_unsupported_pipeline_shape() {
        // calling a list method on a Tuple shape — UnsupportedPipelineShape
        let graphs = analyze_source(
            r#"
struct Student { name: string }
let students: list<Student> = [Student { name: "A" }]
let pairs = students |> partition(s => s.name == "A") |> map(s => s.name)
"#,
        )
        .expect("extract");
        let graph = longest_graph(&graphs);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diag| diag.code == "UnsupportedPipelineShape"),
            "expected UnsupportedPipelineShape diagnostic, got: {:?}",
            graph.diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_diag_invalid_closure_return_filter() {
        // filter closure returns a non-bool value — InvalidClosureReturn
        let graphs = analyze_source(
            r#"
struct Student { name: string }
let students: list<Student> = [Student { name: "A" }]
let names = students |> filter(s => s.name)
"#,
        )
        .expect("extract");
        let graph = longest_graph(&graphs);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diag| diag.code == "InvalidClosureReturn"),
            "expected InvalidClosureReturn diagnostic, got: {:?}",
            graph.diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
}
