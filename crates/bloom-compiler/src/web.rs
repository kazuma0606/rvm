use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BloomSourceFile {
    pub abs_path: PathBuf,
    pub rel_path: PathBuf,
}

pub fn collect_bloom_files(source_root: &Path) -> Result<Vec<BloomSourceFile>, String> {
    fn walk(
        dir: &Path,
        source_root: &Path,
        files: &mut Vec<BloomSourceFile>,
    ) -> Result<(), String> {
        let mut entries = fs::read_dir(dir)
            .map_err(|e| format!("{}: {}", dir.display(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, source_root, files)?;
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("bloom") {
                continue;
            }
            let rel_path = path
                .strip_prefix(source_root)
                .map_err(|_| format!("{} is outside {}", path.display(), source_root.display()))?
                .to_path_buf();
            files.push(BloomSourceFile {
                abs_path: path,
                rel_path,
            });
        }

        Ok(())
    }

    let mut files = Vec::new();
    walk(source_root, source_root, &mut files)?;
    Ok(files)
}

pub fn generated_forge_path(rel_path: &Path) -> PathBuf {
    rel_path.with_extension("forge")
}

pub fn wasm_output_path(rel_path: &Path) -> PathBuf {
    rel_path.with_extension("wasm")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmRenderPlan {
    pub state_name: String,
    pub initial_value: i32,
    pub dynamic_text_target: Option<String>,
    pub static_texts: Vec<(String, String)>,
    pub listeners: Vec<(String, String, String)>,
    pub increment_handlers: Vec<String>,
}

fn quoted_strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i <= bytes.len() {
            out.push(line[start..i].to_string());
        }
        i += 1;
    }
    out
}

pub fn parse_generated_forge_to_plan(source: &str) -> Result<WasmRenderPlan, String> {
    let mut state_name = None;
    let mut initial_value = 0i32;
    let mut dynamic_text_target = None;
    let mut static_texts = Vec::new();
    let mut listeners = Vec::new();
    let mut increment_handlers = Vec::new();

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("state ") {
            let parts = rest.split('=').collect::<Vec<_>>();
            if parts.len() == 2 {
                let lhs = parts[0].trim();
                let name = lhs
                    .split(':')
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| format!("invalid state line: {}", line))?;
                state_name = Some(name.to_string());
                initial_value = parts[1].trim().parse::<i32>().map_err(|e| {
                    format!("invalid state initializer '{}': {}", parts[1].trim(), e)
                })?;
            }
            continue;
        }

        if let Some(after_fn) = line.strip_prefix("fn ") {
            if let Some(name_end) = after_fn.find('(') {
                let handler_name = after_fn[..name_end].trim();
                if let Some(state) = state_name.as_deref() {
                    if line.contains(&format!("{} += 1", state)) {
                        increment_handlers.push(handler_name.to_string());
                    }
                }
            }
            continue;
        }

        if line.starts_with("dom::set_text(") {
            let strings = quoted_strings(line);
            if line.contains("string(") {
                if let Some(target) = strings.first() {
                    dynamic_text_target = Some(target.clone());
                }
            } else if strings.len() >= 2 {
                static_texts.push((strings[0].clone(), strings[1].clone()));
            }
            continue;
        }

        if line.starts_with("dom::add_listener(") {
            let strings = quoted_strings(line);
            if strings.len() >= 3 {
                listeners.push((strings[0].clone(), strings[1].clone(), strings[2].clone()));
            }
        }
    }

    let state_name =
        state_name.ok_or_else(|| "no state declaration found in generated forge".to_string())?;
    Ok(WasmRenderPlan {
        state_name,
        initial_value,
        dynamic_text_target,
        static_texts,
        listeners,
        increment_handlers,
    })
}

fn rust_bytes(name: &str, value: &str) -> String {
    format!("const {}: &[u8] = b\"{}\";", name, value.escape_default())
}

pub fn generate_counter_wasm_rust(plan: &WasmRenderPlan) -> Result<String, String> {
    let dynamic_target = plan
        .dynamic_text_target
        .as_ref()
        .ok_or_else(|| "dynamic text target is required for wasm output".to_string())?;
    let click_listener = plan
        .listeners
        .iter()
        .find(|(_, event, handler)| {
            event == "click" && plan.increment_handlers.iter().any(|name| name == handler)
        })
        .ok_or_else(|| {
            "click listener with increment handler is required for wasm output".to_string()
        })?;

    let mut consts = vec![
        rust_bytes("DYNAMIC_TARGET", dynamic_target),
        rust_bytes("CLICK_TARGET", &click_listener.0),
        rust_bytes("CLICK_EVENT", &click_listener.1),
    ];
    for (index, (_, text)) in plan.static_texts.iter().enumerate() {
        consts.push(rust_bytes(&format!("STATIC_TEXT_{}", index), text));
    }
    for (index, (target, _)) in plan.static_texts.iter().enumerate() {
        consts.push(rust_bytes(&format!("STATIC_TARGET_{}", index), target));
    }

    let mut body = String::new();
    body.push_str(
        r#"
const OP_SET_TEXT: i32 = 1;
const OP_ADD_LISTENER: i32 = 3;
const OP_ATTACH: i32 = 9;
const EVENT_CLICK: i32 = 1;

static mut COUNT: i32 = 0;
static mut COMMANDS: [i32; 32] = [0; 32];
static mut COMMAND_LEN: i32 = 0;
static mut COUNT_BUF: [u8; 12] = [0; 12];

"#,
    );
    for line in consts {
        body.push_str(&line);
        body.push('\n');
    }
    body.push_str(&format!(
        r#"
fn write_count_bytes() -> i32 {{
    let mut value = unsafe {{ COUNT }};
    if value == 0 {{
        unsafe {{ COUNT_BUF[0] = b'0'; }}
        return 1;
    }}
    let mut digits = [0u8; 12];
    let mut len = 0usize;
    while value > 0 {{
        digits[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
    }}
    let mut i = 0usize;
    while i < len {{
        unsafe {{ COUNT_BUF[i] = digits[len - i - 1]; }}
        i += 1;
    }}
    len as i32
}}

unsafe fn push_i32(cursor: &mut usize, value: i32) {{
    COMMANDS[*cursor] = value;
    *cursor += 1;
}}

unsafe fn render() {{
    let mut cursor = 0usize;
    let count_len = write_count_bytes();
    push_i32(&mut cursor, OP_SET_TEXT);
    push_i32(&mut cursor, DYNAMIC_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, DYNAMIC_TARGET.len() as i32);
    push_i32(&mut cursor, COUNT_BUF.as_ptr() as i32);
    push_i32(&mut cursor, count_len);
"#
    ));
    for (index, _) in plan.static_texts.iter().enumerate() {
        body.push_str(&format!(
            r#"
    push_i32(&mut cursor, OP_SET_TEXT);
    push_i32(&mut cursor, STATIC_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TARGET_{0}.len() as i32);
    push_i32(&mut cursor, STATIC_TEXT_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TEXT_{0}.len() as i32);
"#,
            index
        ));
    }
    body.push_str(
        r#"
    push_i32(&mut cursor, OP_ADD_LISTENER);
    push_i32(&mut cursor, CLICK_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, CLICK_TARGET.len() as i32);
    push_i32(&mut cursor, CLICK_EVENT.as_ptr() as i32);
    push_i32(&mut cursor, CLICK_EVENT.len() as i32);
    push_i32(&mut cursor, 1);

    COMMAND_LEN = cursor as i32;
}

unsafe fn attach() {
    let mut cursor = 0usize;
    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, DYNAMIC_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, DYNAMIC_TARGET.len() as i32);
"#,
    );
    for (index, _) in plan.static_texts.iter().enumerate() {
        body.push_str(&format!(
            r#"
    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, STATIC_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TARGET_{0}.len() as i32);
"#,
            index
        ));
    }
    body.push_str(
        r#"
    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, CLICK_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, CLICK_TARGET.len() as i32);
    push_i32(&mut cursor, OP_ADD_LISTENER);
    push_i32(&mut cursor, CLICK_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, CLICK_TARGET.len() as i32);
    push_i32(&mut cursor, CLICK_EVENT.as_ptr() as i32);
    push_i32(&mut cursor, CLICK_EVENT.len() as i32);
    push_i32(&mut cursor, 1);

    COMMAND_LEN = cursor as i32;
}

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn __forge_init() {
    unsafe {
        COUNT = "#,
    );
    body.push_str(&plan.initial_value.to_string());
    body.push_str(
        r#";
        render();
    }
}

#[no_mangle]
pub extern "C" fn __forge_attach() {
    unsafe {
        COUNT = "#,
    );
    body.push_str(&plan.initial_value.to_string());
    body.push_str(
        r#";
        attach();
    }
}

#[no_mangle]
pub extern "C" fn __forge_pull_commands_ptr() -> *const i32 {
    unsafe { COMMANDS.as_ptr() }
}

#[no_mangle]
pub extern "C" fn __forge_pull_commands_len() -> i32 {
    unsafe { COMMAND_LEN }
}

#[no_mangle]
pub extern "C" fn __forge_receive_events(kind: i32, target_ptr: *const u8, target_len: usize) {
    if kind != EVENT_CLICK {
        return;
    }
    let target = unsafe { std::slice::from_raw_parts(target_ptr, target_len) };
    if target != CLICK_TARGET {
        return;
    }
    unsafe {
        COUNT += 1;
        render();
    }
}
"#,
    );
    Ok(body)
}

pub fn compile_generated_forge_to_wasm(source: &str, out_path: &Path) -> Result<(), String> {
    let plan = parse_generated_forge_to_plan(source)?;
    let rust_source = generate_counter_wasm_rust(&plan)?;

    let mut temp_dir = std::env::temp_dir();
    temp_dir.push(format!("bloom_wasm_{}", std::process::id()));
    temp_dir.push(format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(temp_dir.join("src"))
        .map_err(|e| format!("{}: {}", temp_dir.display(), e))?;

    let result = (|| {
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"bloom-web-module\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(temp_dir.join("src/lib.rs"), rust_source).map_err(|e| e.to_string())?;

        let status = Command::new("cargo")
            .args([
                "build",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "--manifest-path",
                temp_dir
                    .join("Cargo.toml")
                    .to_str()
                    .ok_or_else(|| "invalid temp manifest path".to_string())?,
            ])
            .status()
            .map_err(|e| format!("cargo wasm build failed: {}", e))?;
        if !status.success() {
            return Err(format!(
                "cargo wasm build failed with exit code {:?}",
                status.code()
            ));
        }

        let built = temp_dir
            .join("target")
            .join("wasm32-unknown-unknown")
            .join("release")
            .join("bloom_web_module.wasm");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {}", parent.display(), e))?;
        }
        fs::copy(&built, out_path)
            .map_err(|e| format!("{} -> {}: {}", built.display(), out_path.display(), e))?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

/// PascalCase コンポーネント名をスネークケースに変換する。
/// 例: "Counter" -> "counter", "MyComponent" -> "my_component"
fn pascal_to_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            out.push(ch);
        }
    }
    out
}

/// Anvil の .forge ファイル内に含まれる render(<Foo />) を
/// render_source(bytes_to_str([..]), {}) に変換する。
///
/// props 付き: render(<Counter count={5} />) -> render_source(..., {"count": count})
pub fn preprocess_render_calls(source: &str, project_root: &Path) -> Result<String, String> {
    let re = Regex::new(r#"render\(<([A-Z][A-Za-z0-9]*)(\s[^/]*)?\s*/>\)"#)
        .map_err(|e| format!("regex error: {}", e))?;

    let mut result = source.to_string();
    // 後ろから置換することで位置ずれを防ぐ
    let matches: Vec<_> = re
        .captures_iter(source)
        .map(|cap| {
            let full = cap.get(0).unwrap();
            let component_name = cap.get(1).unwrap().as_str();
            let props_raw = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            (
                full.start(),
                full.end(),
                component_name.to_string(),
                props_raw.to_string(),
            )
        })
        .collect();

    // 後ろから置換
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for (start, end, component_name, props_raw) in matches {
        let snake = pascal_to_snake(&component_name);
        let bloom_path = project_root
            .join("src")
            .join("components")
            .join(format!("{}.bloom", snake));
        let bloom_source = fs::read_to_string(&bloom_path)
            .map_err(|e| format!("{}: {}", bloom_path.display(), e))?;
        let bytes_literal = bloom_source
            .as_bytes()
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        // props_raw をシンプルな map リテラルに変換 (key={expr} -> "key": expr)
        let props_map = if props_raw.is_empty() {
            "{}".to_string()
        } else {
            let kv_re =
                Regex::new(r#"(\w+)=\{([^}]+)\}"#).map_err(|e| format!("regex error: {}", e))?;
            let mut pairs = Vec::new();
            for kv in kv_re.captures_iter(&props_raw) {
                let key = kv.get(1).unwrap().as_str();
                let val = kv.get(2).unwrap().as_str();
                pairs.push(format!("\"{}\": {}", key, val));
            }
            if pairs.is_empty() {
                "{}".to_string()
            } else {
                format!("{{{}}}", pairs.join(", "))
            }
        };

        let replacement = format!(
            "render_source(bytes_to_str([{}]), {})",
            bytes_literal, props_map
        );
        replacements.push((start, end, replacement));
    }

    // 後ろから適用
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }

    Ok(result)
}

/// `forge.min.js` の先頭に critical CSS をインライン注入するコードを付加する。
/// CSS が空文字の場合は元の JS をそのまま返す。
pub fn inline_critical_css(js_source: &str, css: &str) -> String {
    if css.is_empty() {
        return js_source.to_string();
    }
    let escaped = css
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('$', "\\$");
    let injector = format!(
        "(function(){{if(typeof document==='undefined'||typeof document.createElement!=='function')return;const s=document.createElement('style');s.textContent=`{}`;document.head.appendChild(s);}})();\n",
        escaped
    );
    format!("{}{}", injector, js_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "bloom_compiler_web_{}_{}_{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn collect_bloom_files_finds_nested_sources() {
        let dir = temp_dir("scan");
        fs::create_dir_all(dir.join("src/app")).expect("mkdir");
        fs::write(dir.join("src/app/page.bloom"), "<p>hi</p>").expect("write");
        fs::write(dir.join("src/app/skip.forge"), "fn main() {}").expect("write");

        let files = collect_bloom_files(&dir.join("src")).expect("collect");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, PathBuf::from("app/page.bloom"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn generated_forge_path_rewrites_extension() {
        assert_eq!(
            generated_forge_path(Path::new("app/page.bloom")),
            PathBuf::from("app/page.forge")
        );
    }

    #[test]
    fn wasm_output_path_rewrites_extension() {
        assert_eq!(
            wasm_output_path(Path::new("app/page.bloom")),
            PathBuf::from("app/page.wasm")
        );
    }

    #[test]
    fn parse_generated_forge_to_plan_extracts_counter_shape() {
        let plan = parse_generated_forge_to_plan(
            "use bloom/dom\n\nstate count: i32 = 0\nfn increment() { count += 1 }\nfn __bloom_render() {\n  dom::set_text(\"text_root_0_0\", string(count))\n  dom::add_listener(\"node_root_1\", \"click\", \"increment\")\n  dom::set_text(\"text_root_1_0\", \"+\")\n}\n",
        )
        .expect("plan");
        assert_eq!(plan.state_name, "count");
        assert_eq!(plan.initial_value, 0);
        assert_eq!(plan.dynamic_text_target.as_deref(), Some("text_root_0_0"));
        assert_eq!(
            plan.static_texts,
            vec![("text_root_1_0".to_string(), "+".to_string())]
        );
        assert_eq!(
            plan.listeners,
            vec![(
                "node_root_1".to_string(),
                "click".to_string(),
                "increment".to_string()
            )]
        );
    }

    #[test]
    fn generate_counter_wasm_rust_contains_runtime_exports() {
        let rust = generate_counter_wasm_rust(&WasmRenderPlan {
            state_name: "count".to_string(),
            initial_value: 0,
            dynamic_text_target: Some("text_root_0_0".to_string()),
            static_texts: vec![("text_root_1_0".to_string(), "+".to_string())],
            listeners: vec![(
                "node_root_1".to_string(),
                "click".to_string(),
                "increment".to_string(),
            )],
            increment_handlers: vec!["increment".to_string()],
        })
        .expect("rust");
        assert!(rust.contains("pub extern \"C\" fn __forge_init()"));
        assert!(rust.contains("pub extern \"C\" fn __forge_attach()"));
        assert!(rust.contains("pub extern \"C\" fn __forge_receive_events"));
        assert!(rust.contains("CLICK_TARGET"));
    }

    #[test]
    fn test_preprocess_render_calls() {
        let source = r#"use bloom/ssr.{ render_source, hydrate_script }
let html = render(<Counter />)
"#;
        let dir = temp_dir("preprocess_render");
        fs::create_dir_all(dir.join("src/components")).expect("mkdir");
        fs::write(dir.join("src/components/counter.bloom"), "<p>hello</p>").expect("write");
        let result = preprocess_render_calls(source, &dir).expect("preprocess");
        assert!(
            result.contains("render_source("),
            "render_source が含まれていない: {}",
            result
        );
        assert!(
            result.contains("bytes_to_str(["),
            "bytes_to_str が含まれていない: {}",
            result
        );
        assert!(
            result.contains("104, 101, 108, 108, 111"),
            "コンポーネント本文が埋め込まれていない: {}",
            result
        );
        assert!(
            !result.contains("render(<Counter />)"),
            "元の render(<...>) が残っている: {}",
            result
        );
    }

    #[test]
    fn test_preprocess_render_calls_with_props() {
        let source = "render(<Counter count={5} />)";
        let dir = temp_dir("preprocess_render_props");
        fs::create_dir_all(dir.join("src/components")).expect("mkdir");
        fs::write(dir.join("src/components/counter.bloom"), "<p>hello</p>").expect("write");
        let result = preprocess_render_calls(source, &dir).expect("preprocess");
        assert!(
            result.contains("\"count\": 5"),
            "props が変換されていない: {}",
            result
        );
        assert!(result.contains("bytes_to_str(["), "{}", result);
    }

    #[test]
    fn test_hydrate_script_contains_forge_min() {
        // hydrate_script() の出力確認は ForgeScript 側のテストだが、
        // Rust 側では hydrate_script_with を模倣してアサートする
        // wasm_output_path は単純な拡張子変換なので forge.min.js 検証はインライン化で確認
        let js = "// forge.min.js runtime";
        let css = "body{background:oklch(0.145 0 0);color:white}";
        let result = inline_critical_css(js, css);
        assert!(
            result.contains("document.createElement('style')"),
            "style 注入コードが含まれていない: {}",
            result
        );
        assert!(
            result.contains("body{background:oklch"),
            "CSS が含まれていない: {}",
            result
        );
        assert!(
            result.contains("forge.min.js"),
            "js 本体が含まれていない: {}",
            result
        );
    }

    #[test]
    fn test_inline_critical_css_empty_css_unchanged() {
        let js = "var x = 1;";
        let result = inline_critical_css(js, "");
        assert_eq!(result, js);
    }

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(pascal_to_snake("Counter"), "counter");
        assert_eq!(pascal_to_snake("MyComponent"), "my_component");
        assert_eq!(pascal_to_snake("A"), "a");
    }

    #[test]
    fn test_flux_file_detected() {
        // .flux.bloom ファイルが collect_bloom_files で検出される
        let dir = temp_dir("flux");
        fs::create_dir_all(dir.join("src/stores")).expect("mkdir");
        fs::write(dir.join("src/stores/cart.flux.bloom"), "store Cart {}").expect("write");
        fs::write(dir.join("src/stores/cart.forge"), "fn noop() {}").expect("write");

        let files = collect_bloom_files(&dir.join("src")).expect("collect");
        // .flux.bloom は "bloom" 拡張子を持たないため collect_bloom_files では検出されない。
        // collect_bloom_files は .bloom 拡張子のみを対象とする。
        // .flux.bloom は "flux.bloom" という複合拡張子を持つが、
        // Path::extension() は最後のピリオド以降のみを返すため "bloom" と判断される。
        let flux_found = files.iter().any(|f| {
            f.rel_path
                .to_str()
                .map(|s| s.contains("cart.flux"))
                .unwrap_or(false)
        });
        // flux.bloom ファイルが検出されることを確認
        assert!(
            flux_found,
            ".flux.bloom ファイルが検出されなかった: {:?}",
            files
        );

        let _ = fs::remove_dir_all(dir);
    }
}
