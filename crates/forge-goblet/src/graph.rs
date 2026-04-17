use forge_compiler::lexer::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NodeKind {
    Source,
    MethodCall,
    FunctionCall,
    Closure,
    ClosureDetail,
    Filter,
    Map,
    Fold,
    Find,
    OptionOp,
    ResultOp,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NodeStatus {
    Ok,
    Warning,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TypeSummary {
    pub display: String,
    pub nullable: bool,
    pub fallible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DataShape {
    Scalar(String),
    List(Box<DataShape>),
    Option(Box<DataShape>),
    Result(Box<DataShape>),
    Struct {
        name: String,
        fields: Vec<(String, DataShape)>,
    },
    AnonStruct(Vec<(String, DataShape)>),
    Tuple(Vec<DataShape>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DataState {
    Definite,
    MaybeNone,
    MaybeErr,
    MaybeEmpty,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeDataInfo {
    pub param_name: Option<String>,
    pub param_shape: Option<DataShape>,
    pub shape: DataShape,
    pub state: DataState,
}

/// forge_compiler::lexer::Span の Eq + Serialize 対応ラッパー。
/// フィールドは Span と同一。file が必要になった時点で source_file と組み合わせて解決する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

impl From<Span> for SourceSpan {
    fn from(value: Span) -> Self {
        Self {
            start: value.start,
            end: value.end,
            line: value.line,
            col: value.col,
        }
    }
}

impl From<&Span> for SourceSpan {
    fn from(value: &Span) -> Self {
        Self {
            start: value.start,
            end: value.end,
            line: value.line,
            col: value.col,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub node_id: Option<NodeId>,
    pub code: String,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineNode {
    pub id: NodeId,
    pub label: String,
    pub kind: NodeKind,
    pub span: Option<SourceSpan>,
    pub input_type: Option<TypeSummary>,
    pub output_type: Option<TypeSummary>,
    pub data_info: Option<NodeDataInfo>,
    pub status: NodeStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineGraph {
    pub roots: Vec<NodeId>,
    pub nodes: Vec<PipelineNode>,
    pub edges: Vec<PipelineEdge>,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub function_name: Option<String>,
}

impl PipelineGraph {
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
            source_file: None,
            function_name: None,
        }
    }

    pub fn add_node(&mut self, mut node: PipelineNode) -> NodeId {
        let id = NodeId(self.nodes.len() + 1);
        node.id = id;
        self.nodes.push(node);
        id
    }

    pub fn add_edge(&mut self, from: NodeId, to: NodeId, label: Option<String>) {
        self.edges.push(PipelineEdge { from, to, label });
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

impl Default for PipelineGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeSummary {
    pub fn new(display: impl Into<String>) -> Self {
        Self {
            display: display.into(),
            nullable: false,
            fallible: false,
        }
    }
}

impl PipelineNode {
    pub fn new(label: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: NodeId(0),
            label: label.into(),
            kind,
            span: None,
            input_type: None,
            output_type: None,
            data_info: None,
            status: NodeStatus::Unknown,
            notes: Vec::new(),
        }
    }
}
