use std::env;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::kernel::{KernelRequest, KernelRequestParams, KernelResponse};

pub struct KernelClient {
    process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl KernelClient {
    pub fn spawn() -> Result<Self, String> {
        let current_exe = env::current_exe().map_err(|e| e.to_string())?;
        let mut process = Command::new(current_exe)
            .arg("notebook")
            .arg("--kernel")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| "failed to capture child stdin".to_string())?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| "failed to capture child stdout".to_string())?;

        Ok(Self {
            process,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    pub fn execute(&mut self, code: &str) -> Result<KernelResponse, String> {
        self.send(
            "execute",
            KernelRequestParams {
                code: code.to_string(),
            },
        )
    }

    pub fn reset(&mut self) -> Result<KernelResponse, String> {
        self.send("reset", KernelRequestParams::default())
    }

    pub fn shutdown(&mut self) -> Result<KernelResponse, String> {
        let response = self.send("shutdown", KernelRequestParams::default())?;
        let _ = self.process.wait();
        Ok(response)
    }

    fn send(
        &mut self,
        method: &str,
        params: KernelRequestParams,
    ) -> Result<KernelResponse, String> {
        let request = KernelRequest {
            id: self.next_id,
            method: method.to_string(),
            params,
        };
        self.next_id += 1;

        let body = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{}", body).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;

        let mut streamed_outputs = Vec::new();
        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            let mut response: KernelResponse =
                serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
            if response.status == "partial" {
                streamed_outputs.extend(response.outputs);
                continue;
            }
            if !streamed_outputs.is_empty() {
                let mut outputs = streamed_outputs;
                outputs.extend(response.outputs);
                response.outputs = outputs;
            }
            return Ok(response);
        }
    }
}
