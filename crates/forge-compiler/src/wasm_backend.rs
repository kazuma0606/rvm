/// ForgeScript → WASM コンパイルバックエンド
///
/// .bloom コンポーネントの `<script>` セクションを解析し、
/// WASM モジュール IR に変換する。
///
/// 型マッピング:
///   number  → i32
///   float   → f64
///   bool    → i32 (0/1)
///   string  → (ptr: i32, len: i32) 線形メモリ上のポインタ+長さペア
use crate::ast::{BinOp, Expr, Literal, Stmt, TypeAnn};
use crate::parser::parse_source;

// ─── WASM 型システム ─────────────────────────────────────────────────────────

/// ForgeScript の型を WASM の値型にマッピングした結果
#[derive(Debug, Clone, PartialEq)]
pub enum WasmType {
    /// number / i32 / int — WASM i32
    I32,
    /// float / f64 — WASM f64
    F64,
    /// bool — WASM i32 として表現（0 = false, 1 = true）
    Bool,
    /// string — 線形メモリ上の (ptr: i32, len: i32) ペアとして渡す
    StringRef,
}

/// WASM モジュールに埋め込む定数値
#[derive(Debug, Clone, PartialEq)]
pub enum WasmConst {
    I32(i32),
    F64(f64),
    Bool(bool),
    /// 空文字列（StringRef の初期値）
    EmptyString,
}

// ─── WASM モジュール IR ──────────────────────────────────────────────────────

/// ForgeScript `state X = Y` から抽出した状態変数
#[derive(Debug, Clone)]
pub struct WasmStateVar {
    /// 変数名（ForgeScript のまま）
    pub name: String,
    /// WASM での型表現
    pub wasm_type: WasmType,
    /// 初期値
    pub initial: WasmConst,
}

/// 関数が状態変数に与える変化量
#[derive(Debug, Clone, PartialEq)]
pub enum StateDelta {
    /// `state = state + N`
    Increment(i64),
    /// `state = state - N`
    Decrement(i64),
    /// `state = constant`
    SetConst(WasmConst),
    /// 静的解析では決定できない変更
    Unknown,
}

/// ForgeScript `fn name() { ... }` から抽出した関数
#[derive(Debug, Clone)]
pub struct WasmFn {
    pub name: String,
    /// (state_name, delta) のリスト — 関数が各状態変数に与える変更
    pub mutations: Vec<(String, StateDelta)>,
    /// `log("...")` や `log(count)` のようなログ出力
    pub logs: Vec<WasmLogExpr>,
}

/// WASM 側で扱う簡易ログ式
#[derive(Debug, Clone, PartialEq)]
pub enum WasmLogExpr {
    Static(String),
    State(String),
}

/// JS 環境から WASM にインポートする DOM 操作関数
#[derive(Debug, Clone)]
pub struct WasmDomImport {
    /// JS 側の関数名（`env.dom_set_text` の `dom_set_text` 部分）
    pub js_name: String,
    /// 引数の型リスト
    pub params: Vec<WasmType>,
}

/// .bloom `<script>` セクションから生成した WASM モジュール IR
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub states: Vec<WasmStateVar>,
    pub functions: Vec<WasmFn>,
    pub dom_imports: Vec<WasmDomImport>,
    pub init_logs: Vec<WasmLogExpr>,
}

// ─── 型変換ヘルパー ──────────────────────────────────────────────────────────

/// ForgeScript の型注釈を WASM 型にマッピングする
pub fn forge_type_to_wasm(ann: Option<&TypeAnn>) -> WasmType {
    match ann {
        Some(TypeAnn::Float) => WasmType::F64,
        Some(TypeAnn::Bool) => WasmType::Bool,
        Some(TypeAnn::String) => WasmType::StringRef,
        _ => WasmType::I32, // number / None / その他はすべて i32
    }
}

/// Bloom コンポーネントのデフォルト DOM インポート定義
///
/// ```wat
/// (import "env" "dom_set_text" (func (param i32 i32 i32 i32)))
/// (import "env" "dom_add_listener" (func (param i32 i32 i32 i32 i32 i32)))
/// ```
pub fn bloom_dom_imports() -> Vec<WasmDomImport> {
    vec![
        WasmDomImport {
            js_name: "dom_set_text".to_string(),
            // (target_ptr, target_len, value_ptr, value_len)
            params: vec![WasmType::I32, WasmType::I32, WasmType::I32, WasmType::I32],
        },
        WasmDomImport {
            js_name: "dom_add_listener".to_string(),
            // (target_ptr, target_len, event_ptr, event_len, handler_ptr, handler_len)
            params: vec![
                WasmType::I32,
                WasmType::I32,
                WasmType::I32,
                WasmType::I32,
                WasmType::I32,
                WasmType::I32,
            ],
        },
    ]
}

// ─── スクリプトパーサー ──────────────────────────────────────────────────────

/// .bloom `<script>` セクションの ForgeScript ソースを解析し `WasmModule` を返す
///
/// サポートする構文:
/// - `state name = <literal>` — i32 状態変数
/// - `state name: <type> = <literal>` — 型付き状態変数
/// - `fn name() { <body> }` — 引数なしのイベントハンドラー関数
///
/// 関数ボディの解析:
/// - `state = state + N` → `StateDelta::Increment(N)`
/// - `state = state - N` → `StateDelta::Decrement(N)`
/// - `state = <literal>` → `StateDelta::SetConst(...)`
/// - その他 → `StateDelta::Unknown`
pub fn parse_bloom_script(source: &str) -> Result<WasmModule, String> {
    let module = parse_source(source).map_err(|e| e.to_string())?;

    // まず状態変数名一覧を収集（関数解析で参照するため）
    let state_names: Vec<String> = module
        .stmts
        .iter()
        .filter_map(|s| {
            if let Stmt::State { name, .. } = s {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    let mut states = Vec::new();
    let mut functions = Vec::new();
    let mut init_logs = Vec::new();

    for stmt in &module.stmts {
        match stmt {
            Stmt::State {
                name,
                type_ann,
                value,
                ..
            } => {
                let wasm_type = forge_type_to_wasm(type_ann.as_ref());
                let initial = expr_to_wasm_const(value).unwrap_or(WasmConst::I32(0));
                states.push(WasmStateVar {
                    name: name.clone(),
                    wasm_type,
                    initial,
                });
            }
            Stmt::Fn { name, body, .. } => {
                let mutations = extract_mutations_from_expr(body, &state_names);
                let logs = extract_logs_from_expr(body, &state_names);
                functions.push(WasmFn {
                    name: name.clone(),
                    mutations,
                    logs,
                });
            }
            Stmt::Expr(expr) => {
                init_logs.extend(extract_logs_from_expr(expr, &state_names));
            }
            _ => {}
        }
    }

    Ok(WasmModule {
        states,
        functions,
        dom_imports: bloom_dom_imports(),
        init_logs,
    })
}

/// `Expr` から WASM 定数値を抽出する（リテラルのみ）
fn expr_to_wasm_const(expr: &Expr) -> Option<WasmConst> {
    match expr {
        Expr::Literal(Literal::Int(n), _) => Some(WasmConst::I32(*n as i32)),
        Expr::Literal(Literal::Float(f), _) => Some(WasmConst::F64(*f)),
        Expr::Literal(Literal::Bool(b), _) => Some(WasmConst::Bool(*b)),
        Expr::Literal(Literal::String(s), _) if s.is_empty() => Some(WasmConst::EmptyString),
        Expr::UnaryOp {
            op: crate::ast::UnaryOp::Neg,
            operand,
            ..
        } => match operand.as_ref() {
            Expr::Literal(Literal::Int(n), _) => Some(WasmConst::I32(-(*n as i32))),
            _ => None,
        },
        _ => None,
    }
}

/// 式の中から状態変数への代入を再帰的に探索し、変化量を返す
fn extract_mutations_from_expr(expr: &Expr, state_names: &[String]) -> Vec<(String, StateDelta)> {
    let mut out = Vec::new();
    collect_mutations(expr, state_names, &mut out);
    out
}

fn extract_logs_from_expr(expr: &Expr, state_names: &[String]) -> Vec<WasmLogExpr> {
    let mut out = Vec::new();
    collect_logs(expr, state_names, &mut out);
    out
}

fn collect_mutations(expr: &Expr, state_names: &[String], out: &mut Vec<(String, StateDelta)>) {
    match expr {
        // ブロック式: 各文を順番に処理
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                collect_mutations_from_stmt(stmt, state_names, out);
            }
            if let Some(tail_expr) = tail {
                collect_mutations(tail_expr, state_names, out);
            }
        }
        // 代入式: `name = value`
        Expr::Assign { name, value, .. } => {
            if state_names.contains(name) {
                let delta = classify_delta(name, value);
                out.push((name.clone(), delta));
            }
        }
        // if/else: 両方のブランチを解析
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            collect_mutations(cond, state_names, out);
            collect_mutations(then_block, state_names, out);
            if let Some(e) = else_block {
                collect_mutations(e, state_names, out);
            }
        }
        _ => {}
    }
}

fn collect_logs(expr: &Expr, state_names: &[String], out: &mut Vec<WasmLogExpr>) {
    match expr {
        Expr::Block { stmts, tail, .. } => {
            for stmt in stmts {
                collect_logs_from_stmt(stmt, state_names, out);
            }
            if let Some(tail_expr) = tail {
                collect_logs(tail_expr, state_names, out);
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            collect_logs(cond, state_names, out);
            collect_logs(then_block, state_names, out);
            if let Some(expr) = else_block {
                collect_logs(expr, state_names, out);
            }
        }
        Expr::While { cond, body, .. } => {
            collect_logs(cond, state_names, out);
            collect_logs(body, state_names, out);
        }
        Expr::Loop { body, .. } => collect_logs(body, state_names, out),
        Expr::For { iter, body, .. } => {
            collect_logs(iter, state_names, out);
            collect_logs(body, state_names, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_logs(scrutinee, state_names, out);
            for arm in arms {
                collect_logs(&arm.body, state_names, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            if matches!(callee.as_ref(), Expr::Ident(name, _) if name == "log") {
                if let Some(first) = args.first() {
                    match first {
                        Expr::Literal(Literal::String(value), _) => {
                            out.push(WasmLogExpr::Static(value.clone()));
                        }
                        Expr::Ident(name, _) if state_names.contains(name) => {
                            out.push(WasmLogExpr::State(name.clone()));
                        }
                        _ => {}
                    }
                }
            }
            collect_logs(callee, state_names, out);
            for arg in args {
                collect_logs(arg, state_names, out);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_logs(object, state_names, out);
            for arg in args {
                collect_logs(arg, state_names, out);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_logs(left, state_names, out);
            collect_logs(right, state_names, out);
        }
        Expr::UnaryOp { operand, .. } => collect_logs(operand, state_names, out),
        Expr::Assign { value, .. } => collect_logs(value, state_names, out),
        Expr::Field { object, .. } => collect_logs(object, state_names, out),
        Expr::Index { object, index, .. } => {
            collect_logs(object, state_names, out);
            collect_logs(index, state_names, out);
        }
        Expr::Closure { body, .. } => collect_logs(body, state_names, out),
        _ => {}
    }
}

fn collect_mutations_from_stmt(
    stmt: &Stmt,
    state_names: &[String],
    out: &mut Vec<(String, StateDelta)>,
) {
    match stmt {
        Stmt::Expr(e) => collect_mutations(e, state_names, out),
        Stmt::Let { value, .. } => collect_mutations(value, state_names, out),
        _ => {}
    }
}

fn collect_logs_from_stmt(stmt: &Stmt, state_names: &[String], out: &mut Vec<WasmLogExpr>) {
    match stmt {
        Stmt::Expr(expr) => collect_logs(expr, state_names, out),
        Stmt::Let { value, .. } | Stmt::State { value, .. } => {
            collect_logs(value, state_names, out)
        }
        _ => {}
    }
}

/// `name = <expr>` の右辺を見て変化量を分類する
///
/// - `name + N` → Increment(N)
/// - `name - N` → Decrement(N)
/// - `<literal>` → SetConst(...)
/// - その他 → Unknown
fn classify_delta(state_name: &str, value: &Expr) -> StateDelta {
    match value {
        // state = state + N
        Expr::BinOp {
            op: BinOp::Add,
            left,
            right,
            ..
        } => {
            if is_ident(left, state_name) {
                if let Some(n) = int_const(right) {
                    return StateDelta::Increment(n);
                }
            }
            if is_ident(right, state_name) {
                if let Some(n) = int_const(left) {
                    return StateDelta::Increment(n);
                }
            }
            StateDelta::Unknown
        }
        // state = state - N
        Expr::BinOp {
            op: BinOp::Sub,
            left,
            right,
            ..
        } => {
            if is_ident(left, state_name) {
                if let Some(n) = int_const(right) {
                    return StateDelta::Decrement(n);
                }
            }
            StateDelta::Unknown
        }
        // state = <literal>
        other => expr_to_wasm_const(other)
            .map(StateDelta::SetConst)
            .unwrap_or(StateDelta::Unknown),
    }
}

fn is_ident(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Ident(n, _) if n == name)
}

fn int_const(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(n), _) => Some(*n),
        _ => None,
    }
}

// ─── テスト ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_counter_script() {
        let source = "state count = 0\nfn increment() {\n    count = count + 1\n}\nfn decrement() {\n    count = count - 1\n}\n";
        let module = parse_bloom_script(source).expect("parse");

        assert_eq!(module.states.len(), 1);
        assert_eq!(module.states[0].name, "count");
        assert_eq!(module.states[0].wasm_type, WasmType::I32);
        assert_eq!(module.states[0].initial, WasmConst::I32(0));

        assert_eq!(module.functions.len(), 2);
        let inc = &module.functions[0];
        assert_eq!(inc.name, "increment");
        assert_eq!(inc.mutations.len(), 1);
        assert_eq!(inc.mutations[0].0, "count");
        assert_eq!(inc.mutations[0].1, StateDelta::Increment(1));

        let dec = &module.functions[1];
        assert_eq!(dec.name, "decrement");
        assert_eq!(dec.mutations[0].1, StateDelta::Decrement(1));
    }

    #[test]
    fn parse_typed_state_variable() {
        let source = "state score: float = 3.14\n";
        let module = parse_bloom_script(source).expect("parse");
        assert_eq!(module.states[0].wasm_type, WasmType::F64);
        assert_eq!(module.states[0].initial, WasmConst::F64(3.14));
    }

    #[test]
    fn parse_bool_state_variable() {
        let source = "state active = true\n";
        let module = parse_bloom_script(source).expect("parse");
        assert_eq!(module.states[0].wasm_type, WasmType::I32); // bool → i32
        assert_eq!(module.states[0].initial, WasmConst::Bool(true));
    }

    #[test]
    fn forge_type_to_wasm_mappings() {
        assert_eq!(forge_type_to_wasm(None), WasmType::I32);
        assert_eq!(forge_type_to_wasm(Some(&TypeAnn::Number)), WasmType::I32);
        assert_eq!(forge_type_to_wasm(Some(&TypeAnn::Float)), WasmType::F64);
        assert_eq!(forge_type_to_wasm(Some(&TypeAnn::Bool)), WasmType::Bool);
        assert_eq!(
            forge_type_to_wasm(Some(&TypeAnn::String)),
            WasmType::StringRef
        );
    }

    #[test]
    fn bloom_dom_imports_has_set_text_and_add_listener() {
        let imports = bloom_dom_imports();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].js_name, "dom_set_text");
        assert_eq!(imports[1].js_name, "dom_add_listener");
        assert_eq!(imports[0].params.len(), 4); // (ptr, len, ptr, len)
        assert_eq!(imports[1].params.len(), 6); // (ptr, len, ptr, len, ptr, len)
    }

    #[test]
    fn parse_set_const_mutation() {
        let source = "state count = 0\nfn reset() {\n    count = 0\n}\n";
        let module = parse_bloom_script(source).expect("parse");
        let reset = &module.functions[0];
        assert_eq!(
            reset.mutations[0].1,
            StateDelta::SetConst(WasmConst::I32(0))
        );
    }
}
