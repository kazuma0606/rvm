use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", std::process::id(), ts, seq)
}

fn make_project_dir(label: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "forge_build_toml_{}_{}_{}",
        label,
        std::process::id(),
        unique_suffix()
    ));
    std::fs::create_dir_all(path.join("src")).expect("create src dir");
    path
}

fn write_project(path: &std::path::Path, output: Option<&str>) {
    let build_section = output
        .map(|output| format!("\n[build]\noutput = \"{}\"\nedition = \"2021\"\n", output))
        .unwrap_or_default();

    let forge_toml = format!(
        "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n{}\n[dependencies]\nanyhow = \"1\"\n",
        build_section
    );
    std::fs::write(path.join("forge.toml"), forge_toml).expect("write forge.toml");
    std::fs::write(
        path.join("src/main.forge"),
        "fn main() {\n    println(\"hello from toml\")\n}\n\nmain()\n",
    )
    .expect("write main.forge");
}

fn write_bloom_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join("src/app")).expect("create app dir");
    std::fs::write(
        path.join("forge.toml"),
        "[package]\nname = \"demo-bloom\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n",
    )
    .expect("write forge.toml");
    std::fs::write(path.join("src/main.forge"), "fn main() {}\n").expect("write main.forge");
    std::fs::write(
        path.join("src/app/page.bloom"),
        "<p>{count}</p><button @click={increment}>+</button><script>\nstate count: i32 = 0\nfn increment() { count += 1 }\n</script>\n",
    )
    .expect("write page.bloom");
}

fn write_bloom_ssr_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join("src")).expect("create src dir");
    let bloom_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .parent()
        .expect("workspace root")
        .join("packages")
        .join("bloom");
    let forge_toml = format!(
        "[package]\nname = \"demo-bloom-ssr\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n\n[dependencies]\nbloom = {{ path = \"{}\" }}\n",
        bloom_path.display().to_string().replace('\\', "/")
    );
    std::fs::write(path.join("forge.toml"), forge_toml).expect("write forge.toml");
    std::fs::write(
        path.join("src/main.forge"),
        "use bloom/ssr.{ render_source, hydrate_script }\n\nfn main() {\n    let html = render_source(\"<p>Hello</p>\", {})\n    assert(html.contains(\"node_root_0\"))\n    assert(hydrate_script().contains(\"/forge.min.js\"))\n}\n\nmain()\n",
    )
    .expect("write main.forge");
}

fn write_bloom_render_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join("src/components")).expect("create components dir");
    std::fs::create_dir_all(path.join("tests")).expect("create tests dir");
    let bloom_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .parent()
        .expect("workspace root")
        .join("packages")
        .join("bloom");
    let forge_toml = format!(
        "[package]\nname = \"demo-bloom-render\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n\n[dependencies]\nbloom = {{ path = \"{}\" }}\n",
        bloom_path.display().to_string().replace('\\', "/")
    );
    std::fs::write(path.join("forge.toml"), forge_toml).expect("write forge.toml");
    std::fs::write(
        path.join("src/main.forge"),
        "use forge/std/fs.{ read_file }\nuse bloom/ssr.{ render_source }\n\nfn main() -> number! {\n    let html = render(<CounterPage />)\n    assert(html.contains(\"Counter\"))\n    assert(html.contains(\"node_root_0\"))\n    ok(0)\n}\n\nmain()?\n",
    )
    .expect("write main.forge");
    std::fs::write(
        path.join("src/components/counter_page.bloom"),
        "<div><h1>Counter</h1><p>{count}</p><button @click={increment}>+</button></div><script>\nstate count: i32 = 1\nfn increment() { count += 1 }\n</script>\n",
    )
    .expect("write counter_page.bloom");
    std::fs::write(
        path.join("tests/render.test.forge"),
        "use forge/std/fs.{ read_file }\nuse bloom/ssr.{ render_source }\n\ntest \"render macro in tests\" {\n    let html = render(<CounterPage />)\n    assert(html.contains(\"Counter\"))\n}\n",
    )
    .expect("write render.test.forge");
}

fn write_anvil_bloom_ssr_project(path: &std::path::Path) {
    std::fs::create_dir_all(path.join("src/components")).expect("create components dir");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .parent()
        .expect("workspace root");
    let bloom_path = root.join("packages").join("bloom");
    let anvil_path = root.join("packages").join("anvil");
    let forge_toml = format!(
        "[package]\nname = \"demo-anvil-bloom-ssr\"\nversion = \"0.1.0\"\nforge = \"0.1.0\"\nentry = \"src/main.forge\"\n\n[dependencies]\nanvil = {{ path = \"{}\" }}\nbloom = {{ path = \"{}\" }}\n\n[bloom]\ncomponents = \"src/components\"\npages = \"src/app\"\n",
        anvil_path.display().to_string().replace('\\', "/"),
        bloom_path.display().to_string().replace('\\', "/")
    );
    std::fs::write(path.join("forge.toml"), forge_toml).expect("write forge.toml");
    std::fs::write(
        path.join("src/main.forge"),
        "use anvil/ssr.{ layout }\nuse bloom/ssr.{ render_source, hydrate_script_with }\n\nfn main() -> number! {\n    let html = render(<CounterPage />)\n    let doc = layout(html, hydrate_script_with(\"/components/counter_page.wasm\"))\n    assert(doc.contains(\"Counter\"))\n    assert(doc.contains(\"/forge.min.js\"))\n    assert(doc.contains(\"/components/counter_page.wasm\"))\n    ok(0)\n}\n\nmain()?\n",
    )
    .expect("write main.forge");
    std::fs::write(
        path.join("src/components/counter_page.bloom"),
        "<div><h1>Counter</h1><button @click={increment}>+</button><span>{count}</span></div><script>\nstate count: i32 = 0\nfn increment() { count += 1 }\n</script>\n",
    )
    .expect("write counter_page.bloom");
}

fn write_browser_fixture(path: &std::path::Path) {
    std::fs::write(
        path.join("dist/index.html"),
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Bloom Counter</title>
  </head>
  <body>
    <p id="text_root_0_0"></p>
    <button id="node_root_1" type="button"></button>
    <p id="status">pending</p>
    <script src="./forge.min.js"></script>
    <script>
      (async () => {
        const app = await globalThis.ForgeBloom.load("./app/page.wasm");
        document.getElementById("status").textContent =
          "ready:" + document.getElementById("text_root_0_0").textContent;
        document.getElementById("node_root_1").click();
        document.getElementById("status").textContent +=
          "->" + document.getElementById("text_root_0_0").textContent;
      })().catch((err) => {
        document.getElementById("status").textContent = "error:" + err;
      });
    </script>
  </body>
</html>
"#,
    )
    .expect("write browser fixture");
}

fn write_ssr_browser_fixture(path: &std::path::Path) {
    std::fs::write(
        path.join("dist/index.html"),
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Bloom SSR Counter</title>
  </head>
  <body>
    <p id="node_root_0"><span id="text_root_0_0">0</span></p>
    <button id="node_root_1" type="button"><span id="text_root_1_0">+</span></button>
    <p id="status">pending</p>
    <script src="./forge.min.js"></script>
    <script>
      window.addEventListener("DOMContentLoaded", async () => {
        try {
          await globalThis.ForgeBloom.load("./app/page.wasm");
          const count = document.getElementById("text_root_0_0");
          const button = document.getElementById("node_root_1");
          document.getElementById("status").textContent =
            "ready:" + count.textContent + ":" + button.getAttribute("data-bloom-attached");
          button.click();
          document.getElementById("status").textContent += "->" + count.textContent;
        } catch (err) {
          document.getElementById("status").textContent = "error:" + err;
        }
      });
    </script>
  </body>
</html>
"#,
    )
    .expect("write SSR browser fixture");
}

#[test]
fn e2e_build_directory_uses_forge_toml() {
    let project_dir = make_project_dir("dir");
    write_project(&project_dir, Some("dist/demo-app"));

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        project_dir.join("dist/demo-app.exe").exists()
            || project_dir.join("dist/demo-app").exists()
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_uses_current_directory_forge_toml() {
    let project_dir = make_project_dir("cwd");
    write_project(&project_dir, None);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .current_dir(&project_dir)
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        project_dir.join("target/demo-app.exe").exists()
            || project_dir.join("target/demo-app").exists()
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_web_emits_generated_forge_and_runtime() {
    let project_dir = make_project_dir("web");
    write_bloom_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", "--web", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build --web");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = project_dir.join("dist/generated/app/page.forge");
    assert!(
        generated.exists(),
        "missing generated file: {}",
        generated.display()
    );
    let generated_src = std::fs::read_to_string(&generated).expect("read generated");
    assert!(generated_src.contains("use bloom/dom"), "{}", generated_src);
    assert!(
        generated_src.contains("dom::add_listener"),
        "{}",
        generated_src
    );
    assert!(project_dir.join("dist/forge.min.js").exists());
    assert!(project_dir.join("dist/app/page.wasm").exists());

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_web_counter_wasm_updates_after_click() {
    let project_dir = make_project_dir("web-counter");
    write_bloom_project(&project_dir);

    let build = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", "--web", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build --web");

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let script = format!(
        r#"
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const root = process.argv[2];
const js = fs.readFileSync(path.join(root, "dist", "forge.min.js"), "utf8");
const wasmBytes = fs.readFileSync(path.join(root, "dist", "app", "page.wasm"));

const elements = new Map();
function makeElement(id) {{
  return {{
    id,
    textContent: "",
    listeners: new Map(),
    attributes: new Map(),
    setAttribute(name, value) {{ this.attributes.set(name, value); }},
    addEventListener(name, listener) {{ this.listeners.set(name, listener); }},
    removeEventListener(name) {{ this.listeners.delete(name); }},
  }};
}}
elements.set("text_root_0_0", makeElement("text_root_0_0"));
elements.set("node_root_1", makeElement("node_root_1"));
elements.set("text_root_1_0", makeElement("text_root_1_0"));

const context = {{
  TextDecoder,
  TextEncoder,
  Uint8Array,
  Int32Array,
  WebAssembly,
  console,
  document: {{
    getElementById(id) {{
      return elements.get(id) ?? null;
    }},
  }},
  globalThis: null,
}};
context.globalThis = context;
vm.runInNewContext(js, context, {{ filename: "forge.min.js" }});

(async () => {{
  const instance = await WebAssembly.instantiate(wasmBytes, {{}});
  const exports = instance.instance.exports;
  const runtime = context.ForgeBloom.createRuntime(exports.memory, exports);
  exports.__forge_init();
  runtime.applyPendingCommands();

  if (elements.get("text_root_0_0").textContent !== "0") {{
    throw new Error("expected initial count 0, got " + elements.get("text_root_0_0").textContent);
  }}
  const click = elements.get("node_root_1").listeners.get("click");
  if (typeof click !== "function") {{
    throw new Error("click listener missing");
  }}
  click();
  if (elements.get("text_root_0_0").textContent !== "1") {{
    throw new Error("expected updated count 1, got " + elements.get("text_root_0_0").textContent);
  }}
  if (elements.get("text_root_1_0").textContent !== "+") {{
    throw new Error("expected button text +, got " + elements.get("text_root_1_0").textContent);
  }}
}})().catch((err) => {{
  console.error(String(err && err.stack || err));
  process.exit(1);
}});
"#
    );
    let script_path = project_dir.join("counter_runtime_check.cjs");
    std::fs::write(&script_path, script).expect("write node script");

    let node = Command::new("node")
        .arg(&script_path)
        .arg(&project_dir)
        .output()
        .expect("run node e2e");

    assert!(
        node.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&node.stderr)
    );

    let _ = std::fs::remove_file(script_path);
    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_bloom_ssr_module_imports_from_dependency_project() {
    let project_dir = make_project_dir("bloom-ssr-import");
    write_bloom_ssr_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("run")
        .current_dir(&project_dir)
        .output()
        .expect("run forge");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_run_project_preprocesses_render_component_calls() {
    let project_dir = make_project_dir("bloom-render-run");
    write_bloom_render_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("run")
        .current_dir(&project_dir)
        .output()
        .expect("run forge");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let transformed =
        std::fs::read_to_string(project_dir.join("src/main.forge")).expect("read transformed main");
    assert!(transformed.contains("render_source("), "{}", transformed);
    assert!(transformed.contains("bytes_to_str(["), "{}", transformed);

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_test_project_preprocesses_render_component_calls() {
    let project_dir = make_project_dir("bloom-render-test");
    write_bloom_render_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("test")
        .current_dir(&project_dir)
        .output()
        .expect("run forge test");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let transformed = std::fs::read_to_string(project_dir.join("tests/render.test.forge"))
        .expect("read transformed test");
    assert!(transformed.contains("render_source("), "{}", transformed);

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_project_emits_bloom_dist_artifacts_without_web_flag() {
    let project_dir = make_project_dir("bloom-render-build");
    write_bloom_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .current_dir(&project_dir)
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(project_dir.join("dist/forge.min.js").exists());
    assert!(project_dir.join("dist/app/page.wasm").exists());
    assert!(project_dir.join("dist/generated/app/page.forge").exists());

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_run_anvil_bloom_ssr_project_renders_html() {
    let project_dir = make_project_dir("anvil-bloom-run");
    write_anvil_bloom_ssr_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("run")
        .current_dir(&project_dir)
        .output()
        .expect("run forge");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let transformed =
        std::fs::read_to_string(project_dir.join("src/main.forge")).expect("read transformed main");
    assert!(transformed.contains("render_source("), "{}", transformed);
    assert!(transformed.contains("bytes_to_str(["), "{}", transformed);

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_build_anvil_bloom_ssr_project_emits_binary_and_wasm() {
    let project_dir = make_project_dir("anvil-bloom-build");
    write_anvil_bloom_ssr_project(&project_dir);

    let output = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .arg("build")
        .current_dir(&project_dir)
        .output()
        .expect("run forge build");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(project_dir.join("dist/forge.min.js").exists());
    assert!(project_dir
        .join("dist/components/counter_page.wasm")
        .exists());
    assert!(
        project_dir.join("target/demo-anvil-bloom-ssr.exe").exists()
            || project_dir.join("target/demo-anvil-bloom-ssr").exists()
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
fn e2e_bloom_runtime_attach_and_replace_inner() {
    let project_dir = make_project_dir("bloom-attach");
    write_bloom_project(&project_dir);

    let build = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", "--web", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build --web");

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let script = format!(
        r#"
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const root = process.argv[2];
const js = fs.readFileSync(path.join(root, "dist", "forge.min.js"), "utf8");
const wasmBytes = fs.readFileSync(path.join(root, "dist", "app", "page.wasm"));

const elements = new Map();
function makeElement(id) {{
  return {{
    id,
    textContent: "",
    innerHTML: "",
    listeners: new Map(),
    attributes: new Map(),
    setAttribute(name, value) {{ this.attributes.set(name, value); }},
    addEventListener(name, listener) {{ this.listeners.set(name, listener); }},
    removeEventListener(name) {{ this.listeners.delete(name); }},
  }};
}}
elements.set("text_root_0_0", makeElement("text_root_0_0"));
elements.set("node_root_1", makeElement("node_root_1"));
elements.set("text_root_1_0", makeElement("text_root_1_0"));
elements.get("text_root_0_0").textContent = "0";
elements.get("text_root_1_0").textContent = "+";

const context = {{
  TextDecoder,
  TextEncoder,
  Uint8Array,
  Int32Array,
  WebAssembly,
  console,
  fetch: async () => {{
    return {{
      arrayBuffer: async () => wasmBytes,
    }};
  }},
  document: {{
    getElementById(id) {{
      return elements.get(id) ?? null;
    }},
  }},
  globalThis: null,
}};
context.globalThis = context;
vm.runInNewContext(js, context, {{ filename: "forge.min.js" }});

;(async () => {{
  const app = await context.ForgeBloom.load("./app/page.wasm");
  const count = elements.get("text_root_0_0");
  const button = elements.get("node_root_1");
  const label = elements.get("text_root_1_0");
  if (count.textContent !== "0") {{
    throw new Error("attach path should preserve SSR text, got " + count.textContent);
  }}
  if (count.attributes.get("data-bloom-attached") !== "true") {{
    throw new Error("expected count attach marker");
  }}
  if (button.attributes.get("data-bloom-attached") !== "true") {{
    throw new Error("expected button attach marker");
  }}
  if (typeof button.listeners.get("click") !== "function") {{
    throw new Error("expected click listener after attach");
  }}
  button.listeners.get("click")();
  if (count.textContent !== "1") {{
    throw new Error("expected updated count 1 after click, got " + count.textContent);
  }}
  if (label.textContent !== "+") {{
    throw new Error("expected static label to remain +, got " + label.textContent);
  }}
}})().catch((err) => {{
  console.error(String(err && err.stack || err));
  process.exit(1);
}});
"#
    );
    let script_path = project_dir.join("attach_runtime_check.cjs");
    std::fs::write(&script_path, script).expect("write node script");

    let node = Command::new("node")
        .arg(&script_path)
        .arg(&project_dir)
        .output()
        .expect("run node attach check");

    assert!(
        node.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&node.stderr)
    );

    let _ = std::fs::remove_file(script_path);
    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
#[ignore = "requires local Edge browser profile access; run manually in browser verification"]
fn e2e_build_web_counter_runs_in_browser() {
    let project_dir = make_project_dir("web-browser");
    write_bloom_project(&project_dir);

    let build = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", "--web", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build --web");

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    write_browser_fixture(&project_dir);

    let screenshot = project_dir.join("dist/browser-counter.png");
    let html = project_dir.join("dist/index.html");
    let file_url = format!("file:///{}", html.to_string_lossy().replace('\\', "/"));

    let edge = Command::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--allow-file-access-from-files",
            "--virtual-time-budget=4000",
            &format!("--screenshot={}", screenshot.display()),
            &file_url,
        ])
        .output()
        .expect("run edge headless");

    assert!(
        edge.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&edge.stderr)
    );
    assert!(
        screenshot.exists(),
        "missing screenshot: {}",
        screenshot.display()
    );

    let _ = std::fs::remove_dir_all(project_dir);
}

#[test]
#[ignore = "requires local Edge browser; run manually for SSR attach browser verification"]
fn e2e_build_web_ssr_counter_attaches_in_browser() {
    let project_dir = make_project_dir("web-ssr-browser");
    write_bloom_project(&project_dir);

    let build = Command::new(env!("CARGO_BIN_EXE_forge-new"))
        .args(["build", "--web", project_dir.to_str().expect("project dir")])
        .output()
        .expect("run forge build --web");

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    write_ssr_browser_fixture(&project_dir);

    let html = project_dir.join("dist/index.html");
    let file_url = format!("file:///{}", html.to_string_lossy().replace('\\', "/"));

    let edge = Command::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
        .args([
            "--headless=new",
            "--disable-gpu",
            "--allow-file-access-from-files",
            "--virtual-time-budget=4000",
            "--dump-dom",
            &file_url,
        ])
        .output()
        .expect("run edge headless");

    assert!(
        edge.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&edge.stderr)
    );
    let dom = String::from_utf8_lossy(&edge.stdout);
    assert!(
        dom.contains("ready:0:true-&gt;1") || dom.contains("ready:0:true->1"),
        "dumped DOM: {}",
        dom
    );

    let _ = std::fs::remove_dir_all(project_dir);
}
