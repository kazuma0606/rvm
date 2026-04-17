use crate::graph::{DataShape, NodeStatus, PipelineGraph};

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

pub fn sanitize_label(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('"', "&quot;")
}

pub fn render_mermaid(graph: &PipelineGraph) -> String {
    let mut lines = vec!["flowchart LR".to_string()];

    for node in &graph.nodes {
        let mut label_lines = vec![node.label.clone()];

        if let Some(ty) = &node.output_type {
            label_lines.push(ty.display.clone());
        }

        if let Some(info) = &node.data_info {
            label_lines.push(shape_display(&info.shape));
        }

        let label = sanitize_label(&label_lines.join("\n"));
        let class = match node.status {
            NodeStatus::Ok => ":::ok",
            NodeStatus::Warning => ":::warning",
            NodeStatus::Error => ":::error",
            NodeStatus::Unknown => ":::unknown",
        };

        lines.push(format!("    N{}[\"{}\"]{}", node.id.0, label, class));
    }

    for edge in &graph.edges {
        let label = edge
            .label
            .as_ref()
            .map(|label| format!("|\"{}\"|", sanitize_label(label)))
            .unwrap_or_default();
        lines.push(format!("    N{} -->{} N{}", edge.from.0, label, edge.to.0));
    }

    lines.push("    classDef ok fill:#c8e6c9,stroke:#2e7d32,color:#1b5e20".to_string());
    lines.push("    classDef warning fill:#fff3cd,stroke:#f9a825,color:#795548".to_string());
    lines.push("    classDef error fill:#f88,stroke:#b71c1c,color:#7f0000".to_string());
    lines.push("    classDef unknown fill:#eceff1,stroke:#607d8b,color:#263238".to_string());

    format!("{}\n", lines.join("\n"))
}
