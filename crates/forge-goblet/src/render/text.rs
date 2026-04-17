use crate::graph::{DataShape, DataState, NodeStatus, PipelineGraph};

fn shape_display(shape: &DataShape) -> String {
    match shape {
        DataShape::Scalar(name) => name.clone(),
        DataShape::List(inner) => format!("list<{}>", shape_display(inner)),
        DataShape::Option(inner) => format!("{}?", shape_display(inner)),
        DataShape::Result(inner) => format!("{}!", shape_display(inner)),
        DataShape::Struct { fields, .. } | DataShape::AnonStruct(fields) => {
            let fields = fields
                .iter()
                .map(|(name, shape)| format!("{name}: {}", shape_display(shape)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {fields} }}")
        }
        DataShape::Tuple(items) => {
            let items = items
                .iter()
                .map(shape_display)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({items})")
        }
        DataShape::Unknown => "unknown".to_string(),
    }
}

fn state_suffix(state: &DataState) -> &'static str {
    match state {
        DataState::Definite => "(Definite)",
        DataState::MaybeNone => "(MaybeNone)",
        DataState::MaybeErr => "(MaybeErr)",
        DataState::MaybeEmpty => "(MaybeEmpty)",
        DataState::Unknown => "(Unknown)",
    }
}

pub fn render_text(graph: &PipelineGraph) -> String {
    let mut lines = Vec::new();

    for node in &graph.nodes {
        let type_display = node
            .output_type
            .as_ref()
            .or(node.input_type.as_ref())
            .map(|summary| summary.display.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let shape_display = node
            .data_info
            .as_ref()
            .map(|info| shape_display(&info.shape))
            .unwrap_or_else(|| "unknown".to_string());

        let state_display = node
            .data_info
            .as_ref()
            .map(|info| state_suffix(&info.state))
            .unwrap_or("(Unknown)");

        let mut line = format!(
            "[{}] {}    {}    {}    {}",
            node.id.0, node.label, type_display, shape_display, state_display
        );

        if node.status == NodeStatus::Error {
            if let Some(diagnostic) = graph
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.node_id == Some(node.id))
            {
                line.push_str(&format!("    error: {}", diagnostic.message));
            }
        }

        lines.push(line);
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}
