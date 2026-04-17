use crate::graph::PipelineGraph;

pub fn render_json(graph: &PipelineGraph) -> String {
    serde_json::to_string_pretty(graph).expect("PipelineGraph serialization must succeed")
}
