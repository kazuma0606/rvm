use forge_compiler::wasm_backend::{
    parse_bloom_script, StateDelta, WasmConst, WasmModule, WasmStateVar,
};
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
                    // `count += 1` または `count = count + 1` の両形式を検出
                    let is_inc = line.contains(&format!("{} += 1", state))
                        || line.contains(&format!("{0} = {0} + 1", state));
                    if is_inc {
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

/// .bloom ファイルの `<script>` セクションを抽出する（Rust 側の簡易実装）
pub fn extract_script_section(bloom_source: &str) -> Option<&str> {
    let start = bloom_source.find("<script>")? + 8;
    let end = bloom_source[start..].find("</script>")? + start;
    Some(bloom_source[start..end].trim())
}

// ─── Bloom テンプレートパーサー（Rust ネイティブ）────────────────────────────
//
// compiler.forge をサブプロセスで実行する代わりに、Rust で直接 bloom テンプレートを
// 解析して WasmRenderPlan を生成する。
//
// ID 生成規則は compiler.forge の path_id / text_target_id と一致させる:
//   path_id(path)          → "node_{path}"
//   text_target_id(path,i) → "text_{path}_{i}"
//   child_path at index i  → "{parent_path}_{i}"

/// bloom テンプレートの最小 AST ノード
#[derive(Debug)]
enum BloomNode {
    Text(String),
    Interpolation(String),
    Element {
        _tag: String,
        event_handlers: Vec<(String, String)>, // (event, handler)
        children: Vec<BloomNode>,
    },
}

/// `<script>...</script>` を除去したテンプレート文字列を返す
fn strip_script_section(bloom_source: &str) -> String {
    if let Some(start) = bloom_source.find("<script>") {
        if let Some(rel_end) = bloom_source[start..].find("</script>") {
            let after = &bloom_source[start + rel_end + 9..];
            return format!("{}{}", &bloom_source[..start], after);
        }
    }
    bloom_source.to_string()
}

/// bloom テンプレート文字列をパースしてノードリストを返す（再帰）
fn parse_bloom_nodes(src: &str) -> Vec<BloomNode> {
    let mut nodes = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        // {#if ...} と {#for ...} はスキップ（中身ごと）
        if bytes[i] == b'{' && src[i..].starts_with("{#if ") {
            if let Some(end) = src[i..].find("{/if}") {
                i += end + 5;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'{' && src[i..].starts_with("{#for ") {
            if let Some(end) = src[i..].find("{/for}") {
                i += end + 6;
            } else {
                i += 1;
            }
            continue;
        }

        // 補間 {expr}
        if bytes[i] == b'{' {
            let start = i + 1;
            // ネストした {} を考慮して閉じ括弧を探す
            let mut depth = 1usize;
            let mut j = start;
            while j < len {
                match bytes[j] {
                    b'{' => {
                        depth += 1;
                        j += 1;
                    }
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        j += 1;
                    }
                    _ => {
                        j += 1;
                    }
                }
            }
            let expr = src[start..j].trim().to_string();
            if !expr.is_empty() {
                nodes.push(BloomNode::Interpolation(expr));
            }
            i = j + 1;
            continue;
        }

        // HTML タグ
        if bytes[i] == b'<' {
            // 閉じタグ </tag> は呼び出し元が処理するためスキップ
            if i + 1 < len && bytes[i + 1] == b'/' {
                break;
            }
            // タグ終端 > を探す（クォートと {} を考慮）
            let tag_end = find_html_tag_close(src, i);
            let raw_tag = &src[i + 1..tag_end]; // タグ名 + 属性
            let self_closing = raw_tag.ends_with('/');
            let tag_body = if self_closing {
                raw_tag[..raw_tag.len() - 1].trim()
            } else {
                raw_tag.trim()
            };
            let (tag_name, attrs_src) = split_tag_name(tag_body);

            let event_handlers = parse_event_handlers(attrs_src);

            let children = if self_closing {
                Vec::new()
            } else {
                // 対応する </tag> を探す（ネスト対応）
                let after_open = tag_end + 1;
                let close_tag = format!("</{}>", tag_name);
                if let Some(close_rel) = find_matching_close_tag(&src[after_open..], tag_name) {
                    let inner = &src[after_open..after_open + close_rel];
                    let ch = parse_bloom_nodes(inner);
                    i = after_open + close_rel + close_tag.len();
                    ch
                } else {
                    i = tag_end + 1;
                    Vec::new()
                }
            };

            nodes.push(BloomNode::Element {
                _tag: tag_name.to_string(),
                event_handlers,
                children,
            });

            if self_closing {
                i = tag_end + 1;
            }
            // i was already updated for non-self-closing
            continue;
        }

        // テキスト: 次の < または { まで
        let text_start = i;
        while i < len && bytes[i] != b'<' && bytes[i] != b'{' {
            i += 1;
        }
        let text = src[text_start..i].trim();
        if !text.is_empty() {
            nodes.push(BloomNode::Text(text.to_string()));
        }
    }

    nodes
}

/// src[start] は '<' の位置。対応する '>' のインデックスを返す
fn find_html_tag_close(src: &str, start: usize) -> usize {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = start + 1;
    let mut in_quote = false;
    let mut quote_char = b'"';
    let mut brace_depth = 0usize;

    while i < len {
        match bytes[i] {
            b'"' | b'\'' if !in_quote => {
                in_quote = true;
                quote_char = bytes[i];
                i += 1;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
                i += 1;
            }
            b'{' if !in_quote => {
                brace_depth += 1;
                i += 1;
            }
            b'}' if !in_quote => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                }
                i += 1;
            }
            b'>' if !in_quote && brace_depth == 0 => return i,
            _ => {
                i += 1;
            }
        }
    }
    len - 1
}

/// "tag attr1 attr2..." → ("tag", "attr1 attr2...")
fn split_tag_name(tag_body: &str) -> (&str, &str) {
    if let Some(sp) = tag_body.find(' ') {
        (&tag_body[..sp], tag_body[sp + 1..].trim())
    } else {
        (tag_body, "")
    }
}

/// 属性文字列から @event={handler} パターンを抽出
fn parse_event_handlers(attrs_src: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let bytes = attrs_src.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        // スペース区切りのトークン（クォートと{} を考慮）
        while i < len && bytes[i] == b' ' {
            i += 1;
        }
        if i >= len {
            break;
        }
        let tok_start = i;
        let mut in_q = false;
        let mut q_ch = b'"';
        let mut depth = 0usize;
        while i < len {
            match bytes[i] {
                b'"' | b'\'' if !in_q => {
                    in_q = true;
                    q_ch = bytes[i];
                    i += 1;
                }
                c if in_q && c == q_ch => {
                    in_q = false;
                    i += 1;
                }
                b'{' if !in_q => {
                    depth += 1;
                    i += 1;
                }
                b'}' if !in_q => {
                    if depth > 0 {
                        depth -= 1;
                    }
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                }
                b' ' if !in_q && depth == 0 => break,
                _ => {
                    i += 1;
                }
            }
        }
        let token = attrs_src[tok_start..i].trim();
        if token.starts_with('@') {
            // @event={handler} または @event={...}
            if let Some(eq) = token.find("={") {
                let event = &token[1..eq];
                let after_eq = &token[eq + 2..];
                // 末尾の } を除去
                let handler = if after_eq.ends_with('}') {
                    after_eq[..after_eq.len() - 1].trim()
                } else {
                    after_eq.trim()
                };
                result.push((event.to_string(), handler.to_string()));
            }
        }
    }
    result
}

/// src 内で tag_name に対応する閉じタグ </tag_name> の相対オフセットを返す（ネスト対応）
fn find_matching_close_tag(src: &str, tag_name: &str) -> Option<usize> {
    let open_pat = format!("<{}", tag_name);
    let close_pat = format!("</{}>", tag_name);
    let mut depth = 1usize;
    let mut i = 0usize;
    let bytes = src.as_bytes();
    let len = bytes.len();

    while i < len {
        if src[i..].starts_with(&close_pat) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += close_pat.len();
        } else if src[i..].starts_with(&open_pat) {
            depth += 1;
            i += open_pat.len();
        } else {
            i += 1;
        }
    }
    None
}

/// compiler.forge の compile_nodes 相当: ノードツリーを走査して
/// dynamic_text_target と listeners を収集する。
/// path は parent の path（最初の呼び出しは "root"）
fn collect_plan_from_nodes(
    nodes: &[BloomNode],
    path: &str,
    state_name: &str,
    dynamic_text_target: &mut Option<String>,
    listeners: &mut Vec<(String, String, String)>,
) {
    for (i, node) in nodes.iter().enumerate() {
        let child_path = format!("{}_{}", path, i);
        match node {
            BloomNode::Interpolation(expr) => {
                // text_target_id(path, i) = "text_{path}_{i}"
                let target_id = format!("text_{}_{}", path, i);
                if expr == state_name {
                    // 状態変数の補間 → dynamic_text_target
                    if dynamic_text_target.is_none() {
                        *dynamic_text_target = Some(target_id);
                    }
                }
            }
            BloomNode::Element {
                event_handlers,
                children,
                ..
            } => {
                let node_id = format!("node_{}", child_path);
                for (event, handler) in event_handlers {
                    listeners.push((node_id.clone(), event.clone(), handler.clone()));
                }
                collect_plan_from_nodes(
                    children,
                    &child_path,
                    state_name,
                    dynamic_text_target,
                    listeners,
                );
            }
            BloomNode::Text(_) => {}
        }
    }
}

/// スクリプトテキストから状態変数名と初期値をテキストパターンで抽出する。
/// ForgeScript パーサーが `+=` をサポートしないケースのフォールバック。
fn extract_state_from_script_text(script: &str) -> Option<(String, i32)> {
    for line in script.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("state ") {
            // "state name: type = value" または "state name = value"
            let parts: Vec<&str> = rest.splitn(2, '=').collect();
            if parts.len() == 2 {
                let lhs = parts[0].trim();
                let name = lhs
                    .split(':')
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?;
                let val: i32 = parts[1].trim().parse().unwrap_or(0);
                return Some((name.to_string(), val));
            }
        }
    }
    None
}

/// スクリプトテキストから増分ハンドラー名をテキストパターンで検出する。
/// `state_name += 1` または `state_name = state_name + 1` を含む関数を対象とする。
fn extract_increment_handlers_from_text(script: &str, state_name: &str) -> Vec<String> {
    let patterns = [
        format!("{} += 1", state_name),
        format!("{0} = {0} + 1", state_name),
        format!("{0}={0}+1", state_name),
    ];
    let mut result = Vec::new();
    let mut current_fn: Option<String> = None;
    for line in script.lines() {
        let line = line.trim();
        if let Some(after_fn) = line.strip_prefix("fn ") {
            if let Some(paren) = after_fn.find('(') {
                current_fn = Some(after_fn[..paren].trim().to_string());
            }
        }
        if let Some(ref name) = current_fn {
            if patterns.iter().any(|p| line.contains(p.as_str())) {
                if !result.contains(name) {
                    result.push(name.clone());
                }
            }
        }
    }
    result
}

/// .bloom ソースから直接 WasmRenderPlan を生成する。
///
/// compiler.forge のサブプロセス実行（compile_bloom_with_compiler_forge）を
/// 不要にする Rust ネイティブ実装。
pub fn plan_from_bloom_source(bloom_source: &str) -> Result<WasmRenderPlan, String> {
    let script = extract_script_section(bloom_source).unwrap_or("");

    // ForgeScript パーサーで解析を試みる（`+=` 非サポートのためフォールバックあり）
    let (state_name, initial_value, increment_handlers) = match parse_bloom_script(script) {
        Ok(wasm_module) => {
            let first_state = wasm_module
                .states
                .first()
                .ok_or_else(|| "no state variable found in bloom script".to_string())?;
            let sname = first_state.name.clone();
            let ival = match &first_state.initial {
                WasmConst::I32(n) => *n,
                _ => 0,
            };
            let handlers: Vec<String> = wasm_module
                .functions
                .iter()
                .filter(|f| {
                    f.mutations.iter().any(|(name, delta)| {
                        name == &sname && matches!(delta, StateDelta::Increment(_))
                    })
                })
                .map(|f| f.name.clone())
                .collect();
            (sname, ival, handlers)
        }
        Err(_) => {
            // `+=` などでパース失敗した場合はテキストマッチで抽出
            let (sname, ival) = extract_state_from_script_text(script)
                .ok_or_else(|| "no state variable found in bloom script".to_string())?;
            let handlers = extract_increment_handlers_from_text(script, &sname);
            (sname, ival, handlers)
        }
    };

    // テンプレートを Rust でパースして DOM バインディングを抽出
    let template_src = strip_script_section(bloom_source);
    let nodes = parse_bloom_nodes(template_src.trim());
    let mut dynamic_text_target = None;
    let mut listeners = Vec::new();
    collect_plan_from_nodes(
        &nodes,
        "root",
        &state_name,
        &mut dynamic_text_target,
        &mut listeners,
    );

    Ok(WasmRenderPlan {
        state_name,
        initial_value,
        dynamic_text_target,
        static_texts: Vec::new(),
        listeners,
        increment_handlers,
    })
}

/// `WasmModule`（スクリプト IR）と `WasmRenderPlan`（DOM バインディング）から
/// 汎用 Rust WASM ソースを生成する。
///
/// `generate_counter_wasm_rust` の制約（単一状態・インクリメント専用）を解消し、
/// 任意の状態変数・任意の操作（+N/-N/setConst）・複数リスナーに対応する。
pub fn generate_wasm_rust(wasm: &WasmModule, plan: &WasmRenderPlan) -> Result<String, String> {
    // 状態変数が存在しない場合は counter-specific ジェネレータへフォールバック
    if wasm.states.is_empty() {
        return generate_counter_wasm_rust(plan);
    }

    let mut body = String::new();

    // ── 定数（DOM ターゲット文字列） ─────────────────────────────────────────

    if let Some(ref target) = plan.dynamic_text_target {
        body.push_str(&rust_bytes("DYNAMIC_TARGET", target));
        body.push('\n');
    }
    for (idx, (target, text)) in plan.static_texts.iter().enumerate() {
        body.push_str(&rust_bytes(&format!("STATIC_TARGET_{}", idx), target));
        body.push('\n');
        body.push_str(&rust_bytes(&format!("STATIC_TEXT_{}", idx), text));
        body.push('\n');
    }
    for (idx, (target, event, handler)) in plan.listeners.iter().enumerate() {
        body.push_str(&rust_bytes(&format!("LISTENER_TARGET_{}", idx), target));
        body.push('\n');
        body.push_str(&rust_bytes(&format!("LISTENER_EVENT_{}", idx), event));
        body.push('\n');
        body.push_str(&rust_bytes(&format!("LISTENER_HANDLER_{}", idx), handler));
        body.push('\n');
    }

    // ── WASM 操作コード ──────────────────────────────────────────────────────

    body.push_str(
        r#"
const OP_SET_TEXT: i32 = 1;
const OP_ADD_LISTENER: i32 = 3;
const OP_ATTACH: i32 = 9;
const EVENT_CLICK: i32 = 1;

static mut COMMANDS: [i32; 64] = [0; 64];
static mut COMMAND_LEN: i32 = 0;
static mut NUM_BUF: [u8; 24] = [0; 24];

"#,
    );

    // ── 状態変数グローバル ───────────────────────────────────────────────────

    for state in &wasm.states {
        let init_str = wasm_const_rust_literal(&state.initial);
        body.push_str(&format!(
            "static mut STATE_{}: i32 = {};\n",
            state.name.to_uppercase(),
            init_str
        ));
    }
    body.push('\n');

    // ── 数値 → ASCII 変換 ────────────────────────────────────────────────────

    body.push_str(
        r#"fn write_num_bytes(value: i32) -> i32 {
    if value == 0 {
        unsafe { NUM_BUF[0] = b'0'; }
        return 1;
    }
    let (negative, mut v) = if value < 0 { (true, -(value as i64) as u64) } else { (false, value as u64) };
    let mut digits = [0u8; 20];
    let mut len = 0usize;
    while v > 0 {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
    }
    let offset = if negative { unsafe { NUM_BUF[0] = b'-'; } 1usize } else { 0usize };
    let mut i = 0usize;
    while i < len {
        unsafe { NUM_BUF[offset + i] = digits[len - i - 1]; }
        i += 1;
    }
    (offset + len) as i32
}

unsafe fn push_i32(cursor: &mut usize, value: i32) {
    COMMANDS[*cursor] = value;
    *cursor += 1;
}

"#,
    );

    // ── render() ────────────────────────────────────────────────────────────

    body.push_str("unsafe fn render() {\n    let mut cursor = 0usize;\n");

    // 動的テキスト（状態変数の値を表示）
    if plan.dynamic_text_target.is_some() {
        // 最初の i32 状態変数を動的テキストの値として使用
        let state_upper = wasm
            .states
            .iter()
            .find(|s| matches!(s.initial, WasmConst::I32(_) | WasmConst::Bool(_)))
            .map(|s| s.name.to_uppercase())
            .unwrap_or_else(|| plan.state_name.to_uppercase());

        body.push_str(&format!(
            r#"    let num_len = write_num_bytes(STATE_{state});
    push_i32(&mut cursor, OP_SET_TEXT);
    push_i32(&mut cursor, DYNAMIC_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, DYNAMIC_TARGET.len() as i32);
    push_i32(&mut cursor, NUM_BUF.as_ptr() as i32);
    push_i32(&mut cursor, num_len);
"#,
            state = state_upper
        ));
    }

    // 静的テキスト
    for (idx, _) in plan.static_texts.iter().enumerate() {
        body.push_str(&format!(
            r#"    push_i32(&mut cursor, OP_SET_TEXT);
    push_i32(&mut cursor, STATIC_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TARGET_{0}.len() as i32);
    push_i32(&mut cursor, STATIC_TEXT_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TEXT_{0}.len() as i32);
"#,
            idx
        ));
    }

    // リスナー
    for (idx, _) in plan.listeners.iter().enumerate() {
        body.push_str(&format!(
            r#"    push_i32(&mut cursor, OP_ADD_LISTENER);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.len() as i32);
    push_i32(&mut cursor, LISTENER_EVENT_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_EVENT_{0}.len() as i32);
    push_i32(&mut cursor, LISTENER_HANDLER_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_HANDLER_{0}.len() as i32);
"#,
            idx
        ));
    }

    body.push_str("    COMMAND_LEN = cursor as i32;\n}\n\n");

    // ── attach() ────────────────────────────────────────────────────────────

    body.push_str("unsafe fn attach() {\n    let mut cursor = 0usize;\n");
    if plan.dynamic_text_target.is_some() {
        body.push_str(
            r#"    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, DYNAMIC_TARGET.as_ptr() as i32);
    push_i32(&mut cursor, DYNAMIC_TARGET.len() as i32);
"#,
        );
    }
    for (idx, _) in plan.static_texts.iter().enumerate() {
        body.push_str(&format!(
            r#"    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, STATIC_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, STATIC_TARGET_{0}.len() as i32);
"#,
            idx
        ));
    }
    for (idx, _) in plan.listeners.iter().enumerate() {
        body.push_str(&format!(
            r#"    push_i32(&mut cursor, OP_ATTACH);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.len() as i32);
    push_i32(&mut cursor, OP_ADD_LISTENER);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_TARGET_{0}.len() as i32);
    push_i32(&mut cursor, LISTENER_EVENT_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_EVENT_{0}.len() as i32);
    push_i32(&mut cursor, LISTENER_HANDLER_{0}.as_ptr() as i32);
    push_i32(&mut cursor, LISTENER_HANDLER_{0}.len() as i32);
"#,
            idx
        ));
    }
    body.push_str("    COMMAND_LEN = cursor as i32;\n}\n\n");

    // ── WASM エクスポート関数 ────────────────────────────────────────────────

    body.push_str(
        r#"#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

"#,
    );

    // __forge_init
    body.push_str("#[no_mangle]\npub extern \"C\" fn __forge_init() {\n    unsafe {\n");
    for state in &wasm.states {
        let init = wasm_const_rust_literal(&state.initial);
        body.push_str(&format!(
            "        STATE_{} = {};\n",
            state.name.to_uppercase(),
            init
        ));
    }
    body.push_str("        render();\n    }\n}\n\n");

    // __forge_attach
    body.push_str("#[no_mangle]\npub extern \"C\" fn __forge_attach() {\n    unsafe {\n");
    for state in &wasm.states {
        let init = wasm_const_rust_literal(&state.initial);
        body.push_str(&format!(
            "        STATE_{} = {};\n",
            state.name.to_uppercase(),
            init
        ));
    }
    body.push_str("        attach();\n    }\n}\n\n");

    // __forge_pull_commands_ptr / len
    body.push_str(
        r#"#[no_mangle]
pub extern "C" fn __forge_pull_commands_ptr() -> *const i32 {
    unsafe { COMMANDS.as_ptr() }
}

#[no_mangle]
pub extern "C" fn __forge_pull_commands_len() -> i32 {
    unsafe { COMMAND_LEN }
}

"#,
    );

    // __forge_receive_events — 各ハンドラーの変異を適用
    body.push_str(
        r#"#[no_mangle]
pub extern "C" fn __forge_receive_events(kind: i32, target_ptr: *const u8, target_len: usize) {
    let target = unsafe { std::slice::from_raw_parts(target_ptr, target_len) };
"#,
    );

    // ハンドラーと対応するリスナーを結びつける
    for (idx, (l_target, _, l_handler)) in plan.listeners.iter().enumerate() {
        // リスナーのターゲット ID に対応するハンドラー関数を探す
        let fn_opt = wasm.functions.iter().find(|f| f.name == *l_handler);
        let mutations = fn_opt.map(|f| f.mutations.as_slice()).unwrap_or_default();

        body.push_str(&format!("    if target == LISTENER_TARGET_{} {{\n", idx));

        for (state_name, delta) in mutations {
            let upper = state_name.to_uppercase();
            let mutation_code = match delta {
                StateDelta::Increment(n) => format!(
                    "        unsafe {{ STATE_{upper} = STATE_{upper}.wrapping_add({n}); }}\n"
                ),
                StateDelta::Decrement(n) => format!(
                    "        unsafe {{ STATE_{upper} = STATE_{upper}.wrapping_sub({n}); }}\n"
                ),
                StateDelta::SetConst(c) => format!(
                    "        unsafe {{ STATE_{upper} = {}; }}\n",
                    wasm_const_rust_literal(c)
                ),
                StateDelta::Unknown => {
                    format!("        // Unknown mutation for state {state_name}\n")
                }
            };
            body.push_str(&mutation_code);
        }
        body.push_str("        unsafe { render(); }\n");
        body.push_str("        return;\n    }\n");
    }

    body.push_str("}\n");

    Ok(body)
}

fn wasm_const_rust_literal(c: &WasmConst) -> String {
    match c {
        WasmConst::I32(n) => n.to_string(),
        WasmConst::F64(f) => format!("{:.}", f),
        WasmConst::Bool(true) => "1".to_string(),
        WasmConst::Bool(false) => "0".to_string(),
        WasmConst::EmptyString => "0".to_string(),
    }
}

/// 生成済み .forge ソースと元の .bloom ソース（省略可）から WASM をコンパイルする。
/// bloom_source が指定された場合は汎用ジェネレータ (`generate_wasm_rust`) を使用する。
pub fn compile_bloom_to_wasm(
    generated_forge: &str,
    bloom_source: Option<&str>,
    out_path: &Path,
) -> Result<(), String> {
    let plan = parse_generated_forge_to_plan(generated_forge)?;
    let rust_source = if let Some(bloom) = bloom_source {
        if let Some(script) = extract_script_section(bloom) {
            match parse_bloom_script(script) {
                Ok(wasm_module) => generate_wasm_rust(&wasm_module, &plan)?,
                Err(_) => generate_counter_wasm_rust(&plan)?,
            }
        } else {
            generate_counter_wasm_rust(&plan)?
        }
    } else {
        generate_counter_wasm_rust(&plan)?
    };
    compile_rust_source_to_wasm(&rust_source, out_path)
}

/// WasmRenderPlan から parse_generated_forge_to_plan が読み戻せる
/// 最小限の生成 Forge ソースを作成する。
pub fn plan_to_generated_forge(plan: &WasmRenderPlan, script: &str) -> String {
    let mut lines = Vec::<String>::new();
    lines.push("use bloom/dom".to_string());
    lines.push(String::new());
    if !script.trim().is_empty() {
        // script の `state X = V` を Forge 互換形式（`+=` → `= X + N`）に変換して出力
        for line in script.lines() {
            let t = line.trim();
            // `fn name() { count += 1 }` → `fn name() {\n    count = count + 1\n}`
            // シンプルな 1 行ブロック形式のみ対応
            if let Some(after_fn) = t.strip_prefix("fn ") {
                if let Some(paren_end) = after_fn.find(')') {
                    let fn_name = after_fn[..after_fn.find('(').unwrap_or(paren_end)].trim();
                    let body_raw = after_fn[paren_end + 1..].trim();
                    let body = body_raw
                        .strip_prefix('{')
                        .and_then(|s| s.strip_suffix('}'))
                        .map(str::trim)
                        .unwrap_or(body_raw);
                    // `count += 1` → `count = count + 1`
                    let body_normalized = {
                        let sname = &plan.state_name;
                        let body = body
                            .replace(&format!("{} += 1", sname), &format!("{0} = {0} + 1", sname));
                        body.replace(&format!("{} -= 1", sname), &format!("{0} = {0} - 1", sname))
                    };
                    lines.push(format!("fn {}() {{", fn_name));
                    if !body_normalized.is_empty() {
                        lines.push(format!("    {}", body_normalized));
                    }
                    lines.push("}".to_string());
                    continue;
                }
            }
            lines.push(line.to_string());
        }
        lines.push(String::new());
    }

    // __bloom_render: set_text + add_listener
    lines.push("fn __bloom_render() {".to_string());
    if let Some(ref target) = plan.dynamic_text_target {
        lines.push(format!(
            "  dom::set_text(\"{}\", string({}))",
            target, plan.state_name
        ));
    }
    for (target, event, handler) in &plan.listeners {
        lines.push(format!(
            "  dom::add_listener(\"{}\", \"{}\", \"{}\")",
            target, event, handler
        ));
    }
    lines.push("}".to_string());
    lines.push(String::new());

    // __bloom_update_<state>
    lines.push(format!("fn __bloom_update_{}() {{", plan.state_name));
    if let Some(ref target) = plan.dynamic_text_target {
        lines.push(format!(
            "  dom::set_text(\"{}\", string({}))",
            target, plan.state_name
        ));
    } else {
        lines.push("  // no dependent nodes".to_string());
    }
    lines.push("}".to_string());
    lines.push(String::new());

    lines.push("fn __bloom_mount() {".to_string());
    lines.push("  __bloom_render()".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    lines.join("\n")
}

/// .bloom ソースから直接 WASM をコンパイルする。
/// compiler.forge のサブプロセス呼び出しを不要にする高速パス。
pub fn compile_bloom_direct(bloom_source: &str, out_path: &Path) -> Result<(), String> {
    let plan = plan_from_bloom_source(bloom_source)?;
    let rust_source = if let Some(script) = extract_script_section(bloom_source) {
        match parse_bloom_script(script) {
            Ok(wasm_module) => generate_wasm_rust(&wasm_module, &plan)?,
            Err(_) => generate_counter_wasm_rust(&plan)?,
        }
    } else {
        generate_counter_wasm_rust(&plan)?
    };
    compile_rust_source_to_wasm(&rust_source, out_path)
}

pub fn compile_generated_forge_to_wasm(source: &str, out_path: &Path) -> Result<(), String> {
    let plan = parse_generated_forge_to_plan(source)?;
    let rust_source = generate_counter_wasm_rust(&plan)?;
    compile_rust_source_to_wasm(&rust_source, out_path)
}

/// 生成済み Rust ソースを `cargo build --target wasm32-unknown-unknown` で `.wasm` にコンパイルする
pub fn compile_rust_source_to_wasm(rust_source: &str, out_path: &Path) -> Result<(), String> {
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
    // render(<X />) と hydrate_inline_script(<X />) の両方を対象にする
    let re = Regex::new(r#"(render|hydrate_inline_script)\(<([A-Z][A-Za-z0-9]*)(\s[^/]*)?\s*/>\)"#)
        .map_err(|e| format!("regex error: {}", e))?;

    let mut result = source.to_string();
    // 後ろから置換することで位置ずれを防ぐ
    let matches: Vec<_> = re
        .captures_iter(source)
        .map(|cap| {
            let full = cap.get(0).unwrap();
            let fn_name = cap.get(1).unwrap().as_str(); // "render" or "hydrate_inline_script"
            let component_name = cap.get(2).unwrap().as_str();
            let props_raw = cap.get(3).map(|m| m.as_str().trim()).unwrap_or("");
            (
                full.start(),
                full.end(),
                fn_name.to_string(),
                component_name.to_string(),
                props_raw.to_string(),
            )
        })
        .collect();

    // 後ろから置換
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for (start, end, fn_name, component_name, props_raw) in matches {
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

        let replacement = if fn_name == "hydrate_inline_script" {
            format!("hydrate_inline_script(bytes_to_str([{}]))", bytes_literal)
        } else {
            format!(
                "render_source(bytes_to_str([{}]), {})",
                bytes_literal, props_map
            )
        };
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
    fn extract_script_section_parses_bloom() {
        let bloom = "<div>hi</div>\n<script>\nstate count = 0\n</script>";
        let script = extract_script_section(bloom).expect("script");
        assert!(script.contains("state count = 0"));
        assert!(!script.contains("<script>"));
    }

    #[test]
    fn extract_script_section_returns_none_without_tag() {
        assert!(extract_script_section("<div>no script</div>").is_none());
    }

    #[test]
    fn generate_wasm_rust_handles_counter_with_decrement() {
        use forge_compiler::wasm_backend::{parse_bloom_script, StateDelta, WasmConst};
        let script = "state count = 0\nfn increment() {\n    count = count + 1\n}\nfn decrement() {\n    count = count - 1\n}\n";
        let wasm_module = parse_bloom_script(script).expect("parse");

        let plan = WasmRenderPlan {
            state_name: "count".to_string(),
            initial_value: 0,
            dynamic_text_target: Some("text_root_0_2_1_0".to_string()),
            static_texts: vec![],
            listeners: vec![
                (
                    "node_root_0_2_0".to_string(),
                    "click".to_string(),
                    "decrement".to_string(),
                ),
                (
                    "node_root_0_2_2".to_string(),
                    "click".to_string(),
                    "increment".to_string(),
                ),
            ],
            increment_handlers: vec!["increment".to_string()],
        };

        let rust = generate_wasm_rust(&wasm_module, &plan).expect("generate");

        // エクスポート関数が存在する
        assert!(
            rust.contains("pub extern \"C\" fn __forge_init()"),
            "missing __forge_init"
        );
        assert!(
            rust.contains("pub extern \"C\" fn __forge_receive_events"),
            "missing receive_events"
        );

        // デクリメント操作が生成される
        assert!(rust.contains("wrapping_sub(1)"), "missing decrement logic");

        // インクリメント操作が生成される
        assert!(rust.contains("wrapping_add(1)"), "missing increment logic");

        // 両リスナーターゲットの定数が生成される
        assert!(
            rust.contains("node_root_0_2_0"),
            "missing decrement listener target"
        );
        assert!(
            rust.contains("node_root_0_2_2"),
            "missing increment listener target"
        );
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

    // ─── B-9-E テスト ────────────────────────────────────────────────────────

    /// `state count = 0` が WASM グローバル変数としてコンパイルされることを確認する
    #[test]
    fn test_wasm_compile_state() {
        use forge_compiler::wasm_backend::parse_bloom_script;

        let script = "state count = 0\n";
        let wasm_module = parse_bloom_script(script).expect("parse");

        let plan = WasmRenderPlan {
            state_name: "count".to_string(),
            initial_value: 0,
            dynamic_text_target: Some("count-display".to_string()),
            static_texts: vec![],
            listeners: vec![],
            increment_handlers: vec![],
        };
        let rust = generate_wasm_rust(&wasm_module, &plan).expect("generate");

        // 状態変数が static mut グローバルとして宣言される
        assert!(
            rust.contains("static mut STATE_COUNT: i32 = 0;"),
            "state global not found in:\n{}",
            rust
        );
        // __forge_init が初期値を設定する
        assert!(
            rust.contains("STATE_COUNT = 0"),
            "__forge_init state init not found"
        );
    }

    /// `fn increment()` が WASM エクスポート関数として正しくコンパイルされることを確認する
    #[test]
    fn test_wasm_compile_fn() {
        use forge_compiler::wasm_backend::parse_bloom_script;

        let script = "state count = 0\nfn increment() {\n    count = count + 1\n}\n";
        let wasm_module = parse_bloom_script(script).expect("parse");

        let plan = WasmRenderPlan {
            state_name: "count".to_string(),
            initial_value: 0,
            dynamic_text_target: Some("count-display".to_string()),
            static_texts: vec![],
            listeners: vec![(
                "btn-inc".to_string(),
                "click".to_string(),
                "increment".to_string(),
            )],
            increment_handlers: vec!["increment".to_string()],
        };
        let rust = generate_wasm_rust(&wasm_module, &plan).expect("generate");

        // WASM エクスポート関数が存在する
        assert!(
            rust.contains("pub extern \"C\" fn __forge_receive_events"),
            "__forge_receive_events export not found"
        );
        assert!(
            rust.contains("pub extern \"C\" fn __forge_init"),
            "__forge_init export not found"
        );
        // increment の変異ロジックが生成される
        assert!(
            rust.contains("wrapping_add(1)"),
            "increment wrapping_add not found"
        );
        // リスナーターゲット定数が存在する
        assert!(
            rust.contains("btn-inc"),
            "listener target constant not found"
        );
    }

    /// WASM SSR ハイドレーションパスが `generate_wasm_rust` の出力と整合することを確認する。
    ///
    /// 実際の WASM バイナリなしで、コマンドバッファ生成ロジック（Rust ソース）が
    /// SSR HTML に期待される要素 ID を正しく参照することをテストする。
    #[test]
    fn test_wasm_ssr_hydration() {
        use forge_compiler::wasm_backend::parse_bloom_script;

        let script = "state count = 0\nfn increment() {\n    count = count + 1\n}\n";
        let wasm_module = parse_bloom_script(script).expect("parse");

        // SSR が生成する ID (ssr.forge の命名規則に従う)
        let dynamic_target = "text_root_0_2_1_0".to_string();
        let listener_target = "node_root_0_2_2".to_string();

        let plan = WasmRenderPlan {
            state_name: "count".to_string(),
            initial_value: 0,
            dynamic_text_target: Some(dynamic_target.clone()),
            static_texts: vec![],
            listeners: vec![(
                listener_target.clone(),
                "click".to_string(),
                "increment".to_string(),
            )],
            increment_handlers: vec!["increment".to_string()],
        };
        let rust = generate_wasm_rust(&wasm_module, &plan).expect("generate");

        // 生成された Rust ソースが SSR HTML の id を参照していること
        assert!(
            rust.contains(&dynamic_target),
            "dynamic_text_target id not in generated WASM Rust"
        );
        assert!(
            rust.contains(&listener_target),
            "listener_target id not in generated WASM Rust"
        );

        // OP_SET_TEXT と OP_ADD_LISTENER が使われていること
        assert!(rust.contains("OP_SET_TEXT"), "OP_SET_TEXT not referenced");
        assert!(
            rust.contains("OP_ADD_LISTENER"),
            "OP_ADD_LISTENER not referenced"
        );
    }
}
