use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

fn next_port() -> u16 {
    18_080 + (std::process::id() as u16 % 1_000) + PORT_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("repo root")
        .to_path_buf()
}

fn wait_for_http(port: u16, path: &str) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        path, port
    );

    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            stream.write_all(request.as_bytes()).expect("write request");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("read response");
            if response.contains("HTTP/1.1 200") || response.contains("HTTP/1.0 200") {
                return Ok(response);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    Err(format!("server did not respond on port {}{}", port, path))
}

fn stop_child(child: Child) -> std::process::Output {
    let mut child = child;
    let _ = child.kill();
    child.wait_with_output().expect("wait child")
}

#[test]
fn error_messages_forge_reports_source_location() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("error_messages.forge");
    fs::write(&file, "let count = 1\nprintln(cout)\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("run forge-new");

    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error_messages.forge:2:9"), "{}", stderr);
    assert!(stderr.contains("println(cout)"), "{}", stderr);
    assert!(stderr.contains("hint: did you mean `count`?"), "{}", stderr);
}

#[test]
fn verbose_run_emits_trace_lines() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("trace.forge");
    fs::write(&file, "state count = 0\ncount = count + 1\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("run")
        .arg("--verbose")
        .arg(&file)
        .output()
        .expect("run forge-new");

    assert!(output.status.success(), "expected success");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[TRACE] statement"), "{}", stderr);
    assert!(stderr.contains("[TRACE] define count = 0"), "{}", stderr);
    assert!(
        stderr.contains("[TRACE] assign count: 0 -> 1"),
        "{}",
        stderr
    );
}

#[test]
fn serve_command_runs_entrypoint_like_run() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("serve.forge");
    fs::write(&file, "println(\"served\")\n").expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("serve")
        .arg("--port")
        .arg("8123")
        .arg(&file)
        .output()
        .expect("run forge-new");

    assert!(output.status.success(), "expected success");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("served"), "{}", stdout);
}

#[test]
fn serve_help_describes_server_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("serve")
        .arg("--help")
        .output()
        .expect("run forge-new");

    assert!(output.status.success(), "expected success");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("forge serve [path]"), "{}", stdout);
    assert!(stdout.contains("--port"), "{}", stdout);
    assert!(stdout.contains("--wasm-trace"), "{}", stdout);
}

#[test]
fn anvil_server_logger_middleware_emits_request_log() {
    let port = next_port();
    let example = repo_root().join("examples").join("anvil");
    let child = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg(&example)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn forge-new serve");

    let response = match wait_for_http(port, "/health") {
        Ok(response) => response,
        Err(err) => {
            let output = stop_child(child);
            panic!(
                "{}\nstdout:\n{}\nstderr:\n{}",
                err,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };
    assert!(response.contains("\"healthy\""), "{}", response);

    let output = stop_child(child);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GET /health -> 200"), "{}", stdout);
}

#[test]
fn serve_wasm_trace_emits_trace_log() {
    let example = repo_root().join("examples").join("anvil-bloom-ssr");
    let build = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .arg("--web")
        .arg(&example)
        .output()
        .expect("build bloom example");
    assert!(
        build.status.success(),
        "build stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let port = next_port();
    let child = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("serve")
        .arg("--wasm-trace")
        .arg("--port")
        .arg(port.to_string())
        .arg(&example)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn forge-new serve --wasm-trace");

    let response = match wait_for_http(port, "/counter") {
        Ok(response) => response,
        Err(err) => {
            let output = stop_child(child);
            panic!(
                "{}\nstdout:\n{}\nstderr:\n{}",
                err,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };
    assert!(response.contains("HTTP/1.1 200"), "{}", response);

    let output = stop_child(child);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[WASM TRACE] load"), "{}", stderr);
    assert!(
        stderr.contains("[WASM TRACE] call __forge_init"),
        "{}",
        stderr
    );
    assert!(stderr.contains("[WASM TRACE] command buffer"), "{}", stderr);
}

#[test]
fn build_web_dump_ast_and_forge_prints_intermediates() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("counter.bloom");
    fs::write(
        &file,
        "<script>\nstate count = 0\nfn increment() { count += 1 }\n</script>\n<button @click={increment}>{count}</button>\n",
    )
    .expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .arg("--web")
        .arg("--dump-ast")
        .arg("--dump-forge")
        .arg(&file)
        .output()
        .expect("run forge-new");

    assert!(output.status.success(), "expected success");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"state\":{\"name\":\"count\",\"initial\":0}"),
        "{}",
        stdout
    );
    assert!(stdout.contains("use bloom/dom"), "{}", stdout);
    assert!(stdout.contains("fn __bloom_render()"), "{}", stdout);
}

#[test]
fn build_web_dump_prints_intermediates_on_compile_error() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("broken.bloom");
    fs::write(
        &file,
        "<script>\nstate count = 0\n</script>\n{#iff count > 0}\n",
    )
    .expect("write");

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .arg("--web")
        .arg("--dump-ast")
        .arg("--dump-forge")
        .arg(&file)
        .output()
        .expect("run forge-new");

    assert!(!output.status.success(), "expected failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"stage\":\"plan\""), "{}", stdout);
    assert!(stdout.contains("\"error\":"), "{}", stdout);
    assert!(stdout.contains("state count = 0"), "{}", stdout);
}
