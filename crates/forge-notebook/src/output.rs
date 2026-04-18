use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotebookOutput {
    pub version: u32,
    pub file: String,
    pub executed_at: DateTime<Utc>,
    pub cells: Vec<CellOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellOutput {
    pub index: usize,
    pub name: String,
    pub status: String,
    pub outputs: Vec<OutputItem>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineTraceStage {
    pub name: String,
    pub r#in: usize,
    pub out: usize,
    pub corrupted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineTraceCorruption {
    pub stage: String,
    pub index: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineTraceOutput {
    pub pipeline_name: String,
    pub source_snippet: String,
    pub stages: Vec<PipelineTraceStage>,
    pub total_records: usize,
    pub total_corrupted: usize,
    pub corruptions: Vec<PipelineTraceCorruption>,
    pub records_by_stage: BTreeMap<String, Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum OutputItem {
    #[serde(rename = "text")]
    Text { value: String },
    #[serde(rename = "html")]
    Html { value: String },
    #[serde(rename = "json")]
    Json { value: serde_json::Value },
    #[serde(rename = "table")]
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<serde_json::Value>>,
    },
    #[serde(rename = "image")]
    Image { mime: String, data: String },
    #[serde(rename = "markdown")]
    Markdown { value: String },
    #[serde(rename = "pipeline_trace")]
    PipelineTrace {
        pipeline_name: String,
        source_snippet: String,
        stages: Vec<PipelineTraceStage>,
        total_records: usize,
        total_corrupted: usize,
        corruptions: Vec<PipelineTraceCorruption>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        records_by_stage: BTreeMap<String, Vec<serde_json::Value>>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<usize>,
    },
}

pub fn output_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.out.json", path.display()))
}

pub fn save_output(path: &Path, output: &NotebookOutput) -> Result<(), String> {
    let json = serde_json::to_string_pretty(output).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_output(path: &Path) -> Result<NotebookOutput, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_json_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("demo.fnb.out.json");
        let output = NotebookOutput {
            version: 1,
            file: "demo.fnb".to_string(),
            executed_at: Utc::now(),
            cells: vec![CellOutput {
                index: 0,
                name: "cell_0".to_string(),
                status: "ok".to_string(),
                outputs: vec![OutputItem::Text {
                    value: "42\n".to_string(),
                }],
                duration_ms: 12,
            }],
        };

        save_output(&path, &output).expect("save");
        let loaded = load_output(&path).expect("load");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.file, "demo.fnb");
        assert_eq!(loaded.cells[0].status, "ok");
        assert_eq!(
            loaded.cells[0].outputs,
            vec![OutputItem::Text {
                value: "42\n".to_string()
            }]
        );
    }

    #[test]
    fn test_output_pipeline_trace_json() {
        let item = OutputItem::PipelineTrace {
            pipeline_name: "names".to_string(),
            source_snippet: "items |> map(|item| item.name)".to_string(),
            stages: vec![
                PipelineTraceStage {
                    name: "source".to_string(),
                    r#in: 3,
                    out: 3,
                    corrupted: 0,
                    line: Some(1),
                },
                PipelineTraceStage {
                    name: "map".to_string(),
                    r#in: 3,
                    out: 2,
                    corrupted: 1,
                    line: Some(1),
                },
            ],
            total_records: 3,
            total_corrupted: 1,
            corruptions: vec![PipelineTraceCorruption {
                stage: "map".to_string(),
                index: 1,
                reason: "oops".to_string(),
            }],
            records_by_stage: BTreeMap::from([(
                "map".to_string(),
                vec![serde_json::json!({ "name": "alice", "score": 90 })],
            )]),
        };

        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(value["type"], "pipeline_trace");
        assert_eq!(value["pipeline_name"], "names");
        assert_eq!(value["stages"][1]["name"], "map");
        assert_eq!(value["corruptions"][0]["reason"], "oops");
        assert_eq!(value["records_by_stage"]["map"][0]["name"], "alice");
    }
}
