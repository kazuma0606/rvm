use serde_json::{json, Map, Value};

use crate::output::NotebookOutput;
use crate::parser::Cell;
use crate::{OutputItem, PipelineTraceCorruption, PipelineTraceStage};

pub fn export_ipynb(cells: &[Cell], output: Option<&NotebookOutput>) -> Value {
    let mut execution_count = 1usize;
    let exported_cells = cells
        .iter()
        .map(|cell| match cell {
            Cell::Markdown(markdown) => json!({
                "cell_type": "markdown",
                "id": format!("cell-{}", markdown.index),
                "metadata": {},
                "source": source_lines(&markdown.content),
            }),
            Cell::Code(code) => {
                let output_cell =
                    output.and_then(|out| out.cells.iter().find(|cell| cell.index == code.index));
                let has_execution = output_cell
                    .is_some_and(|cell| cell.status != "pending" && cell.status != "skipped");
                let cell_execution_count = if has_execution {
                    let current = execution_count;
                    execution_count += 1;
                    Some(current)
                } else {
                    None
                };

                json!({
                    "cell_type": "code",
                    "execution_count": cell_execution_count,
                    "id": code.name,
                    "metadata": {
                        "forge": {
                            "hidden": code.hidden,
                            "skip": code.skip,
                            "start_line": code.start_line,
                        }
                    },
                    "outputs": output_cell
                        .map(|cell| cell.outputs.iter().map(export_output_item).collect::<Vec<_>>())
                        .unwrap_or_default(),
                    "source": source_lines(&code.source),
                })
            }
        })
        .collect::<Vec<_>>();

    json!({
        "cells": exported_cells,
        "metadata": {
            "kernelspec": {
                "display_name": "ForgeScript",
                "language": "forge",
                "name": "forge"
            },
            "language_info": {
                "file_extension": ".forge",
                "mimetype": "text/x-forge",
                "name": "forge"
            }
        },
        "nbformat": 4,
        "nbformat_minor": 5
    })
}

fn export_output_item(item: &OutputItem) -> Value {
    match item {
        OutputItem::Text { value } => json!({
            "name": "stdout",
            "output_type": "stream",
            "text": source_lines(value),
        }),
        OutputItem::Error { message, .. } => json!({
            "ename": "ForgeError",
            "evalue": message,
            "output_type": "error",
            "traceback": source_lines(message),
        }),
        OutputItem::Json { value } => display_data(vec![
            ("application/json", value.clone()),
            ("text/plain", Value::String(pretty_json(value))),
        ]),
        OutputItem::Html { value } => display_data(vec![
            ("text/html", Value::String(value.clone())),
            ("text/plain", Value::String(value.clone())),
        ]),
        OutputItem::Markdown { value } => display_data(vec![
            ("text/markdown", Value::String(value.clone())),
            ("text/plain", Value::String(value.clone())),
        ]),
        OutputItem::Image { mime, data } => display_data(vec![
            (mime.as_str(), Value::String(data.clone())),
            ("text/plain", Value::String(format!("<image {}>", mime))),
        ]),
        OutputItem::Table { columns, rows } => display_data(vec![
            (
                "application/vnd.forge.table+json",
                json!({
                    "columns": columns,
                    "rows": rows,
                }),
            ),
            (
                "text/markdown",
                Value::String(render_markdown_table(columns, rows)),
            ),
        ]),
        OutputItem::PipelineTrace {
            pipeline_name,
            source_snippet,
            stages,
            total_records,
            total_corrupted,
            corruptions,
            records_by_stage,
        } => display_data(vec![
            (
                "application/vnd.forge.pipeline-trace+json",
                json!({
                    "pipeline_name": pipeline_name,
                    "source_snippet": source_snippet,
                    "stages": stages,
                    "total_records": total_records,
                    "total_corrupted": total_corrupted,
                    "corruptions": corruptions,
                    "records_by_stage": records_by_stage,
                }),
            ),
            (
                "text/plain",
                Value::String(render_pipeline_trace_text(
                    pipeline_name,
                    stages,
                    *total_corrupted,
                    corruptions,
                )),
            ),
        ]),
    }
}

fn display_data(entries: Vec<(&str, Value)>) -> Value {
    let mut data = Map::new();
    for (mime, value) in entries {
        data.insert(mime.to_string(), value);
    }

    json!({
        "data": data,
        "metadata": {},
        "output_type": "display_data",
    })
}

fn source_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let normalized = text.replace("\r\n", "\n");
    let mut lines = normalized
        .split_inclusive('\n')
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if !normalized.ends_with('\n') {
        if let Some(last) = lines.last_mut() {
            last.push('\n');
        }
    }
    lines
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn render_markdown_table(columns: &[String], rows: &[Vec<Value>]) -> String {
    if columns.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push(format!("| {} |", columns.join(" | ")));
    lines.push(format!(
        "| {} |",
        columns
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    for row in rows {
        let values = row.iter().map(render_plain_value).collect::<Vec<_>>();
        lines.push(format!("| {} |", values.join(" | ")));
    }
    format!("{}\n", lines.join("\n"))
}

fn render_pipeline_trace_text(
    pipeline_name: &str,
    stages: &[PipelineTraceStage],
    total_corrupted: usize,
    corruptions: &[PipelineTraceCorruption],
) -> String {
    let flow = stages
        .iter()
        .map(|stage| {
            if stage.corrupted > 0 {
                format!("{}({}) !{}", stage.name, stage.out, stage.corrupted)
            } else {
                format!("{}({})", stage.name, stage.out)
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut lines = vec![format!("[pipeline: {}] {}", pipeline_name, flow)];
    if total_corrupted > 0 {
        lines.push(format!("! {} corrupted records detected", total_corrupted));
        for corruption in corruptions {
            lines.push(format!(
                "  #{} [{}] {}",
                corruption.index, corruption.stage, corruption.reason
            ));
        }
    }
    format!("{}\n", lines.join("\n"))
}

fn render_plain_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(string) => string.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::output::CellOutput;
    use crate::parser::parse_notebook;

    #[test]
    fn test_export_ipynb_valid_json() {
        let cells = parse_notebook("# Title\n\n```forge\nprintln(42)\n```");
        let output = NotebookOutput {
            version: 1,
            file: "demo.fnb".to_string(),
            executed_at: Utc::now(),
            cells: vec![CellOutput {
                index: 1,
                name: "cell_1".to_string(),
                status: "ok".to_string(),
                outputs: vec![OutputItem::Text {
                    value: "42\n".to_string(),
                }],
                duration_ms: 12,
            }],
        };
        let exported = export_ipynb(&cells, Some(&output));
        let text = serde_json::to_string(&exported).expect("serialize");
        let parsed: Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(parsed["nbformat"], 4);
    }

    #[test]
    fn test_export_ipynb_cell_count() {
        let cells = parse_notebook("# Title\n\n```forge\nprintln(42)\n```");
        let exported = export_ipynb(&cells, None);
        assert_eq!(exported["cells"].as_array().expect("cells").len(), 2);
    }

    #[test]
    fn test_export_ipynb_code_cell() {
        let cells = parse_notebook("```forge name=\"setup\"\nprintln(42)\n```");
        let output = NotebookOutput {
            version: 1,
            file: "demo.fnb".to_string(),
            executed_at: Utc::now(),
            cells: vec![CellOutput {
                index: 0,
                name: "setup".to_string(),
                status: "ok".to_string(),
                outputs: vec![OutputItem::Text {
                    value: "42\n".to_string(),
                }],
                duration_ms: 1,
            }],
        };
        let exported = export_ipynb(&cells, Some(&output));

        let cell = &exported["cells"][0];
        assert_eq!(cell["cell_type"], "code");
        assert_eq!(cell["id"], "setup");
        assert_eq!(cell["execution_count"], 1);
        assert_eq!(cell["outputs"][0]["output_type"], "stream");
        assert_eq!(cell["outputs"][0]["text"][0], "42\n");
    }

    #[test]
    fn test_export_ipynb_markdown_cell() {
        let cells = parse_notebook("# Title\n\ntext");
        let exported = export_ipynb(&cells, None);

        let cell = &exported["cells"][0];
        assert_eq!(cell["cell_type"], "markdown");
        assert_eq!(cell["source"][0], "# Title\n");
        assert_eq!(cell["source"][1], "\n");
        assert_eq!(cell["source"][2], "text\n");
    }
}
