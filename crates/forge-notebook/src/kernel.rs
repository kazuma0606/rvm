use std::io::{self, BufRead, Write};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use forge_compiler::parser::parse_source;
use forge_goblet::{analyze_source as goblet_analyze_source, PipelineGraph};
use forge_vm::interpreter::{
    CorruptedRecord, DisplayOutput, Interpreter, PipelineTraceEvent, PipelineTraceNodeRef,
    PipelineTraceOutcome,
};
use forge_vm::value::Value;
use serde::{Deserialize, Serialize};

use crate::output::{OutputItem, PipelineTraceCorruption, PipelineTraceStage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: KernelRequestParams,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KernelRequestParams {
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KernelResponse {
    pub id: u64,
    pub status: String,
    pub outputs: Vec<OutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellExecution {
    pub status: String,
    pub outputs: Vec<OutputItem>,
    pub duration_ms: u128,
}

pub type KernelOutput = OutputItem;

pub struct KernelSession {
    interpreter: Interpreter,
    shutdown: bool,
    current_request_id: Arc<Mutex<Option<u64>>>,
    partial_sink: Arc<dyn Fn(KernelResponse) + Send + Sync>,
    display_outputs: Arc<Mutex<Vec<OutputItem>>>,
}

impl KernelSession {
    pub fn new() -> Self {
        Self::with_partial_sink(|_| {})
    }

    pub fn with_partial_sink<F>(sink: F) -> Self
    where
        F: Fn(KernelResponse) + Send + Sync + 'static,
    {
        let current_request_id = Arc::new(Mutex::new(None));
        let current_request_id_for_listener = Arc::clone(&current_request_id);
        let sink: Arc<dyn Fn(KernelResponse) + Send + Sync> = Arc::new(sink);
        let sink_for_listener = Arc::clone(&sink);
        let display_outputs = Arc::new(Mutex::new(Vec::new()));
        let display_outputs_for_listener = Arc::clone(&display_outputs);
        let (interpreter, _) = Interpreter::with_output_capture_and_display_listener(
            move |value| {
                let id = current_request_id_for_listener
                    .lock()
                    .ok()
                    .and_then(|guard| *guard);
                if let Some(id) = id {
                    sink_for_listener(KernelResponse {
                        id,
                        status: "partial".to_string(),
                        outputs: vec![OutputItem::Text { value }],
                        duration_ms: None,
                    });
                }
            },
            move |output| {
                if let Ok(mut outputs) = display_outputs_for_listener.lock() {
                    outputs.push(display_output_to_item(output));
                }
            },
        );
        Self {
            interpreter,
            shutdown: false,
            current_request_id,
            partial_sink: sink,
            display_outputs,
        }
    }

    fn reset_interpreter(&mut self) {
        let current_request_id_for_listener = Arc::clone(&self.current_request_id);
        let sink_for_listener = Arc::clone(&self.partial_sink);
        let display_outputs = Arc::new(Mutex::new(Vec::new()));
        let display_outputs_for_listener = Arc::clone(&display_outputs);
        let (interpreter, _) = Interpreter::with_output_capture_and_display_listener(
            move |value| {
                let id = current_request_id_for_listener
                    .lock()
                    .ok()
                    .and_then(|guard| *guard);
                if let Some(id) = id {
                    sink_for_listener(KernelResponse {
                        id,
                        status: "partial".to_string(),
                        outputs: vec![OutputItem::Text { value }],
                        duration_ms: None,
                    });
                }
            },
            move |output| {
                if let Ok(mut outputs) = display_outputs_for_listener.lock() {
                    outputs.push(display_output_to_item(output));
                }
            },
        );
        self.interpreter = interpreter;
        self.display_outputs = display_outputs;
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub fn handle_request(&mut self, req: KernelRequest) -> KernelResponse {
        match req.method.as_str() {
            "execute" => {
                if let Ok(mut current) = self.current_request_id.lock() {
                    *current = Some(req.id);
                }
                let exec = self.execute(&req.params.code);
                if let Ok(mut current) = self.current_request_id.lock() {
                    *current = None;
                }
                KernelResponse {
                    id: req.id,
                    status: exec.status,
                    outputs: exec.outputs,
                    duration_ms: Some(exec.duration_ms),
                }
            }
            "reset" => {
                self.reset_interpreter();
                KernelResponse {
                    id: req.id,
                    status: "ok".to_string(),
                    outputs: Vec::new(),
                    duration_ms: None,
                }
            }
            "shutdown" => {
                self.shutdown = true;
                KernelResponse {
                    id: req.id,
                    status: "ok".to_string(),
                    outputs: Vec::new(),
                    duration_ms: None,
                }
            }
            _ => KernelResponse {
                id: req.id,
                status: "error".to_string(),
                outputs: vec![OutputItem::Error {
                    message: format!("unknown method: {}", req.method),
                    line: None,
                }],
                duration_ms: None,
            },
        }
    }

    pub fn execute(&mut self, code: &str) -> CellExecution {
        let started = Instant::now();
        let graphs = goblet_analyze_source(code).unwrap_or_default();
        self.interpreter.set_trace_mode(!graphs.is_empty());
        self.interpreter
            .set_pipeline_trace_nodes(collect_pipeline_trace_nodes(&graphs));
        let outputs = match parse_source(code) {
            Ok(module) => match self.interpreter.eval(&module) {
                Ok(value) => {
                    let mut outputs = take_stdout(&mut self.interpreter);
                    outputs.extend(take_display_outputs(&self.display_outputs));
                    outputs.extend(build_pipeline_trace_outputs(
                        code,
                        &graphs,
                        self.interpreter.take_pipeline_trace_events(),
                    ));
                    if outputs.is_empty() && !matches!(value, forge_vm::value::Value::Unit) {
                        outputs.push(OutputItem::Text {
                            value: format!("{}\n", value),
                        });
                    }
                    return CellExecution {
                        status: "ok".to_string(),
                        outputs,
                        duration_ms: started.elapsed().as_millis(),
                    };
                }
                Err(error) => {
                    let mut outputs = take_stdout(&mut self.interpreter);
                    outputs.extend(take_display_outputs(&self.display_outputs));
                    outputs.extend(build_pipeline_trace_outputs(
                        code,
                        &graphs,
                        self.interpreter.take_pipeline_trace_events(),
                    ));
                    outputs.push(OutputItem::Error {
                        message: error.to_string(),
                        line: None,
                    });
                    outputs
                }
            },
            Err(error) => vec![OutputItem::Error {
                message: error.to_string(),
                line: None,
            }],
        };

        CellExecution {
            status: "error".to_string(),
            outputs,
            duration_ms: started.elapsed().as_millis(),
        }
    }
}

pub fn run_kernel_stdio() {
    let stdin = io::stdin();
    let mut session = KernelSession::with_partial_sink(|response| {
        let mut out = io::stdout().lock();
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string(&response).unwrap_or_else(|_| {
                "{\"id\":0,\"status\":\"error\",\"outputs\":[],\"duration_ms\":null}".to_string()
            })
        );
        let _ = out.flush();
    });

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<KernelRequest>(&line) {
            Ok(request) => session.handle_request(request),
            Err(error) => KernelResponse {
                id: 0,
                status: "error".to_string(),
                outputs: vec![OutputItem::Error {
                    message: format!("invalid request: {}", error),
                    line: None,
                }],
                duration_ms: None,
            },
        };

        let mut out = io::stdout().lock();
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string(&response).unwrap_or_else(|_| {
                "{\"id\":0,\"status\":\"error\",\"outputs\":[],\"duration_ms\":null}".to_string()
            })
        );
        let _ = out.flush();

        if session.is_shutdown() {
            break;
        }
    }
}

fn take_stdout(interpreter: &mut Interpreter) -> Vec<OutputItem> {
    if let Some(buffer) = &interpreter.output_buffer {
        if let Ok(mut text) = buffer.lock() {
            if text.is_empty() {
                return Vec::new();
            }
            let value = std::mem::take(&mut *text);
            return vec![OutputItem::Text { value }];
        }
    }
    Vec::new()
}

fn take_display_outputs(buffer: &Arc<Mutex<Vec<OutputItem>>>) -> Vec<OutputItem> {
    buffer
        .lock()
        .map(|mut outputs| std::mem::take(&mut *outputs))
        .unwrap_or_default()
}

fn display_output_to_item(output: DisplayOutput) -> OutputItem {
    match output {
        DisplayOutput::Text { value } => OutputItem::Text { value },
        DisplayOutput::Html { value } => OutputItem::Html { value },
        DisplayOutput::Json { value } => OutputItem::Json { value },
        DisplayOutput::Table { columns, rows } => OutputItem::Table { columns, rows },
        DisplayOutput::Image { mime, data } => OutputItem::Image { mime, data },
        DisplayOutput::Markdown { value } => OutputItem::Markdown { value },
    }
}

fn collect_pipeline_trace_nodes(graphs: &[PipelineGraph]) -> Vec<PipelineTraceNodeRef> {
    graphs
        .iter()
        .flat_map(|graph| {
            graph.nodes.iter().filter_map(|node| {
                node.span.as_ref().map(|span| PipelineTraceNodeRef {
                    node_id: node.id.0,
                    start: span.start,
                    end: span.end,
                    line: span.line,
                    col: span.col,
                })
            })
        })
        .collect()
}

fn build_pipeline_trace_outputs(
    code: &str,
    graphs: &[PipelineGraph],
    events: Vec<PipelineTraceEvent>,
) -> Vec<OutputItem> {
    graphs
        .iter()
        .filter_map(|graph| build_pipeline_trace_output(code, graph, &events))
        .collect()
}

fn build_pipeline_trace_output(
    code: &str,
    graph: &PipelineGraph,
    events: &[PipelineTraceEvent],
) -> Option<OutputItem> {
    if graph.nodes.is_empty() {
        return None;
    }

    let (source_snippet, start_line) = pipeline_source_snippet(code, graph);
    let pipeline_name = graph
        .function_name
        .clone()
        .or_else(|| graph.roots.first().and_then(|id| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == *id)
                .map(|node| node.label.clone())
        }))
        .unwrap_or_else(|| format!("pipeline_{}", graph.roots.first().map(|id| id.0).unwrap_or(1)));

    let mut previous_out = 0usize;
    let mut saw_count = false;
    let mut traced_stage_count = 0usize;
    let mut stages = Vec::new();
    let mut corruptions = Vec::new();
    let mut records_by_stage = BTreeMap::new();

    for node in &graph.nodes {
        let node_events = events
            .iter()
            .filter(|event| event.node_id == Some(node.id.0))
            .collect::<Vec<_>>();
        let last_count = node_events
            .iter()
            .rev()
            .find_map(|event| event.item_count)
            .unwrap_or(previous_out);
        if node_events
            .iter()
            .any(|event| is_runtime_pipeline_method(&event.method))
        {
            traced_stage_count += 1;
        }

        let stage_in = if saw_count { previous_out } else { last_count };
        let stage_out = last_count;
        if node_events.iter().any(|event| event.item_count.is_some()) {
            saw_count = true;
        }

        for (offset, event) in node_events.iter().enumerate() {
            if event.corrupted.is_empty() {
                if event.outcome == PipelineTraceOutcome::Ok {
                    continue;
                }
                corruptions.push(PipelineTraceCorruption {
                    stage: node.label.clone(),
                    index: offset + 1,
                    reason: pipeline_event_reason(event),
                });
                continue;
            }

            for record in &event.corrupted {
                records_by_stage
                    .entry(node.label.clone())
                    .or_insert_with(Vec::new)
                    .push(corrupted_record_to_json(record));
                corruptions.push(PipelineTraceCorruption {
                    stage: node.label.clone(),
                    index: record.index,
                    reason: record.reason.clone(),
                });
            }
        }

        stages.push(PipelineTraceStage {
            name: node.label.clone(),
            r#in: stage_in,
            out: stage_out,
            corrupted: node_events
                .iter()
                .map(|event| {
                    if event.corrupted.is_empty() {
                        usize::from(event.outcome != PipelineTraceOutcome::Ok)
                    } else {
                        event.corrupted.len()
                    }
                })
                .sum(),
            line: node
                .span
                .as_ref()
                .map(|span| span.line.saturating_sub(start_line).saturating_add(1)),
        });
        previous_out = stage_out;
    }

    if traced_stage_count == 0 {
        return None;
    }

    let total_records = stages.first().map(|stage| stage.out).unwrap_or(0);
    let total_corrupted = corruptions.len();
    Some(OutputItem::PipelineTrace {
        pipeline_name,
        source_snippet,
        stages,
        total_records,
        total_corrupted,
        corruptions,
        records_by_stage,
    })
}

fn pipeline_source_snippet(code: &str, graph: &PipelineGraph) -> (String, usize) {
    let mut min_line = usize::MAX;
    let bounds = graph
        .nodes
        .iter()
        .filter_map(|node| {
            node.span.as_ref().map(|span| {
                min_line = min_line.min(span.line);
                (span.start, span.end)
            })
        })
        .fold(None, |acc: Option<(usize, usize)>, (start, end)| match acc {
            Some((min_start, max_end)) => Some((min_start.min(start), max_end.max(end))),
            None => Some((start, end)),
        });

    let snippet = bounds
        .and_then(|(start, end)| code.get(start..end))
        .map(str::trim)
        .filter(|snippet| !snippet.is_empty())
        .unwrap_or_else(|| code.trim())
        .to_string();
    let start_line = if min_line == usize::MAX { 1 } else { min_line };
    (snippet, start_line)
}

fn pipeline_event_reason(event: &PipelineTraceEvent) -> String {
    match event.outcome {
        PipelineTraceOutcome::FindNone => "find returned none".to_string(),
        PipelineTraceOutcome::ResultErr => event
            .message
            .clone()
            .map(|message| format!("result error: {message}"))
            .unwrap_or_else(|| "result error".to_string()),
        PipelineTraceOutcome::Ok => event
            .message
            .clone()
            .unwrap_or_else(|| "runtime warning".to_string()),
    }
}

fn is_runtime_pipeline_method(method: &str) -> bool {
    matches!(
        method,
        "map"
            | "filter"
            | "take"
            | "skip"
            | "find"
            | "fold"
            | "flat_map"
            | "and_then"
            | "unwrap_or"
    )
}

fn corrupted_record_to_json(record: &CorruptedRecord) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("index".to_string(), serde_json::Value::from(record.index));
    object.insert(
        "reason".to_string(),
        serde_json::Value::String(record.reason.clone()),
    );
    for (name, value) in &record.fields {
        object.insert(name.clone(), runtime_value_to_json(value));
    }
    serde_json::Value::Object(object)
}

fn runtime_value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Int(value) => serde_json::Value::from(*value),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(value) => serde_json::Value::String(value.clone()),
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Unit => serde_json::Value::Null,
        Value::Option(Some(value)) => runtime_value_to_json(value),
        Value::Option(None) => serde_json::Value::Null,
        Value::Result(Ok(value)) => runtime_value_to_json(value),
        Value::Result(Err(message)) => serde_json::json!({ "error": message }),
        Value::List(items) => serde_json::Value::Array(
            items.borrow().iter().map(runtime_value_to_json).collect(),
        ),
        Value::Map(entries) => serde_json::Value::Object(
            entries
                .iter()
                .filter_map(|(key, value)| match key {
                    Value::String(name) => Some((name.clone(), runtime_value_to_json(value))),
                    _ => None,
                })
                .collect(),
        ),
        Value::Set(items) => {
            serde_json::Value::Array(items.iter().map(runtime_value_to_json).collect())
        }
        Value::Struct { fields, .. } | Value::Typestate { fields, .. } => serde_json::Value::Object(
            fields
                .borrow()
                .iter()
                .map(|(name, value)| (name.clone(), runtime_value_to_json(value)))
                .collect(),
        ),
        Value::Enum {
            type_name,
            variant,
            ..
        } => serde_json::json!({
            "_type": type_name,
            "variant": variant
        }),
        Value::Closure { .. } => serde_json::Value::String("<closure>".to_string()),
        Value::NativeFunction(_) => serde_json::Value::String("<native_fn>".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_execute() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 1,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "println(42)".to_string(),
            },
        });

        assert_eq!(response.status, "ok");
        assert_eq!(
            response.outputs,
            vec![OutputItem::Text {
                value: "42\n".to_string()
            }]
        );
    }

    #[test]
    fn test_kernel_reset() {
        let mut session = KernelSession::new();
        let _ = session.handle_request(KernelRequest {
            id: 1,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "let x = 42".to_string(),
            },
        });
        let _ = session.handle_request(KernelRequest {
            id: 2,
            method: "reset".to_string(),
            params: KernelRequestParams::default(),
        });
        let response = session.handle_request(KernelRequest {
            id: 3,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "println(x)".to_string(),
            },
        });

        assert_eq!(response.status, "error");
    }

    #[test]
    fn test_kernel_shutdown() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 1,
            method: "shutdown".to_string(),
            params: KernelRequestParams::default(),
        });

        assert_eq!(response.status, "ok");
        assert!(session.is_shutdown());
    }

    #[test]
    fn test_kernel_partial_response() {
        let partials = Arc::new(Mutex::new(Vec::<KernelResponse>::new()));
        let partials_for_sink = Arc::clone(&partials);
        let mut session = KernelSession::with_partial_sink(move |response| {
            partials_for_sink.lock().expect("lock").push(response);
        });

        let response = session.handle_request(KernelRequest {
            id: 7,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "println(\"step 1\")\nprintln(\"step 2\")".to_string(),
            },
        });

        assert_eq!(response.status, "ok");
        let partials = partials.lock().expect("lock");
        assert_eq!(partials.len(), 2);
        assert_eq!(partials[0].status, "partial");
        assert_eq!(partials[0].id, 7);
        assert_eq!(
            partials[0].outputs,
            vec![OutputItem::Text {
                value: "step 1\n".to_string()
            }]
        );
        assert_eq!(
            partials[1].outputs,
            vec![OutputItem::Text {
                value: "step 2\n".to_string()
            }]
        );
    }

    #[test]
    fn test_display_kernel_json() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 8,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "display::json({ name: \"alice\", score: 90 })".to_string(),
            },
        });

        assert_eq!(response.status, "ok");
        assert_eq!(
            response.outputs,
            vec![OutputItem::Json {
                value: serde_json::json!({
                    "name": "alice",
                    "score": 90
                })
            }]
        );
    }

    #[test]
    fn test_display_kernel_table() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 9,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: r#"
                    display::table([
                      { name: "alice", score: 90 },
                      { name: "bob", score: 75 }
                    ])
                "#
                .to_string(),
            },
        });

        assert_eq!(response.status, "ok");
        assert_eq!(
            response.outputs,
            vec![OutputItem::Table {
                columns: vec!["name".to_string(), "score".to_string()],
                rows: vec![
                    vec![serde_json::json!("alice"), serde_json::json!(90)],
                    vec![serde_json::json!("bob"), serde_json::json!(75)]
                ]
            }]
        );
    }

    #[test]
    fn test_pipeline_trace_output_for_clean_pipeline() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 10,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: r#"
                    [1, 2, 3].map(n => n * 2).take(2)
                "#
                .to_string(),
            },
        });

        assert_eq!(response.status, "ok");
        let trace = response
            .outputs
            .iter()
            .find_map(|output| match output {
                OutputItem::PipelineTrace {
                    pipeline_name,
                    stages,
                    total_corrupted,
                    ..
                } => Some((pipeline_name.clone(), stages.clone(), *total_corrupted)),
                _ => None,
            })
            .expect("pipeline trace");
        assert!(trace.0.starts_with("pipeline_") || !trace.0.is_empty());
        assert!(!trace.1.is_empty());
        assert_eq!(trace.2, 0);
    }

    #[test]
    fn test_pipeline_trace_stage_counts() {
        let mut session = KernelSession::new();
        let response = session.handle_request(KernelRequest {
            id: 11,
            method: "execute".to_string(),
            params: KernelRequestParams {
                code: "[1, 2, 3].map(n => n * 2).take(2)".to_string(),
            },
        });

        let stages = response
            .outputs
            .iter()
            .find_map(|output| match output {
                OutputItem::PipelineTrace { stages, .. } => Some(stages.clone()),
                _ => None,
            })
            .expect("pipeline trace");

        let map_stage = stages
            .iter()
            .find(|stage| stage.name.contains("map"))
            .expect("map");
        let take_stage = stages
            .iter()
            .find(|stage| stage.name.contains("take"))
            .expect("take");
        assert_eq!(map_stage.r#in, 3);
        assert_eq!(map_stage.out, 3);
        assert_eq!(take_stage.r#in, 3);
        assert_eq!(take_stage.out, 2);
    }
}
