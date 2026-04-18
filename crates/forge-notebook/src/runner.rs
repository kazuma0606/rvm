use std::time::Instant;

use forge_compiler::parser::parse_source;
use forge_vm::interpreter::Interpreter;

use crate::parser::Cell;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunOptions {
    pub cell_filter: Option<String>,
    pub stop_on_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellStatus {
    Ok,
    Error,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellResult {
    pub index: usize,
    pub name: String,
    pub status: CellStatus,
    pub stdout: String,
    pub error: Option<String>,
    pub duration_ms: u128,
}

pub fn run_notebook(cells: &[Cell], opts: RunOptions) -> Vec<CellResult> {
    let (mut interp, output_buf) = Interpreter::with_output_capture();
    let mut results = Vec::new();

    for cell in cells {
        let Cell::Code(code) = cell else {
            continue;
        };

        if opts
            .cell_filter
            .as_deref()
            .is_some_and(|filter| filter != code.name)
        {
            continue;
        }

        if code.skip {
            results.push(CellResult {
                index: code.index,
                name: code.name.clone(),
                status: CellStatus::Skipped,
                stdout: String::new(),
                error: None,
                duration_ms: 0,
            });
            continue;
        }

        clear_output(&output_buf);
        let started = Instant::now();

        let result = match parse_source(&code.source) {
            Ok(module) => match interp.eval(&module) {
                Ok(_) => CellResult {
                    index: code.index,
                    name: code.name.clone(),
                    status: CellStatus::Ok,
                    stdout: read_output(&output_buf),
                    error: None,
                    duration_ms: started.elapsed().as_millis(),
                },
                Err(error) => CellResult {
                    index: code.index,
                    name: code.name.clone(),
                    status: CellStatus::Error,
                    stdout: read_output(&output_buf),
                    error: Some(error.to_string()),
                    duration_ms: started.elapsed().as_millis(),
                },
            },
            Err(error) => CellResult {
                index: code.index,
                name: code.name.clone(),
                status: CellStatus::Error,
                stdout: String::new(),
                error: Some(error.to_string()),
                duration_ms: started.elapsed().as_millis(),
            },
        };

        let should_stop = result.status == CellStatus::Error && opts.stop_on_error;
        results.push(result);
        if should_stop {
            break;
        }
    }

    results
}

fn clear_output(output_buf: &std::sync::Arc<std::sync::Mutex<String>>) {
    if let Ok(mut buffer) = output_buf.lock() {
        buffer.clear();
    }
}

fn read_output(output_buf: &std::sync::Arc<std::sync::Mutex<String>>) -> String {
    output_buf
        .lock()
        .map(|buffer| buffer.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_notebook;

    #[test]
    fn test_run_shared_scope() {
        let cells = parse_notebook("```forge\nlet x = 42\n```\n```forge\nprintln(x)\n```");
        let results = run_notebook(&cells, RunOptions::default());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, CellStatus::Ok);
        assert_eq!(results[1].status, CellStatus::Ok);
        assert_eq!(results[1].stdout, "42\n");
    }

    #[test]
    fn test_run_skip_cell() {
        let cells = parse_notebook(
            "```forge\nlet x = 1\n```\n```forge skip=true\nprintln(x)\n```\n```forge\nprintln(x)\n```",
        );
        let results = run_notebook(&cells, RunOptions::default());

        assert_eq!(results.len(), 3);
        assert_eq!(results[1].status, CellStatus::Skipped);
        assert_eq!(results[2].stdout, "1\n");
    }

    #[test]
    fn test_run_stop_on_error() {
        let cells = parse_notebook(
            "```forge\nlet x = 1\n```\n```forge\nprintln(y)\n```\n```forge\nprintln(x)\n```",
        );
        let results = run_notebook(
            &cells,
            RunOptions {
                stop_on_error: true,
                ..RunOptions::default()
            },
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[1].status, CellStatus::Error);
    }

    #[test]
    fn test_run_continue_on_error() {
        let cells = parse_notebook("```forge\nprintln(y)\n```\n```forge\nprintln(\"ok\")\n```");
        let results = run_notebook(&cells, RunOptions::default());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, CellStatus::Error);
        assert_eq!(results[1].status, CellStatus::Ok);
        assert_eq!(results[1].stdout, "ok\n");
    }
}
