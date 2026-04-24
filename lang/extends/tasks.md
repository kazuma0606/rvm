# ForgeScript 拡張タスク一覧

> **参照**: `lang/extends/spec.md`
> **目標**: Phase E-1～E-7 を順番に実装

---

## Phase E-1: `|>` パイプ演算子

### E-1-A: レキサー
- [x] `TokenKind::PipeArrow` 追加（`|>` を認識）
- [x] `TokenKind::Pipe` との区別
- [x] `test_lex_pipe_arrow`

### E-1-B: パーサー
- [x] `PipeArrow` を `Expr::MethodCall` へつなげる
- [x] 引数なし呼び出しを `Expr::MethodCall { args: [] }` に展開
- [x] チェイン `lhs |> method |> other` に対応
- [x] `test_parse_pipe_arrow_method`
- [x] `test_parse_pipe_arrow_no_args`
- [x] `test_parse_pipe_arrow_chain`

### E-1-C: E2E スナップショット
- [x] `test_pipe_arrow_filter_map_fold`
- [x] `test_pipe_arrow_equals_method_chain`
- [x] `test_transpile_pipe_arrow`

---

## Phase E-2: `?.` / `??` オペレーター

### E-2-A: レキサー
- [x] `TokenKind::QuestionDot`
- [x] `TokenKind::QuestionQuestion`
- [x] `?` を単体記号としても扱う
- [x] `test_lex_question_dot`
- [x] `test_lex_question_question`

### E-2-B: AST
- [x] `Expr::OptionalChain { object, chain: ChainKind }`
- [x] `enum ChainKind { Field(String), Method { name, args } }`
- [x] `Expr::NullCoalesce { value, default }`

### E-2-C: パーサー
- [x] `expr?.field` → `OptionalChain(Field)`
- [x] `expr?.method(args)` → `OptionalChain(Method)`
- [x] `expr ?? default` → `Expr::NullCoalesce`
- [x] 二項の優先順位で `??` を `||` と分ける
- [x] `test_parse_optional_chain_field`
- [x] `test_parse_optional_chain_method`
- [x] `test_parse_null_coalesce`
- [x] `test_parse_optional_chain_nested`

### E-2-D: 評価
- [x] OptionalChain(Field) は `None` を保持し `Some(v)` ならフィールド取得
- [x] OptionalChain(Method) はメソッド呼び出し
- [x] NullCoalesce は `None` ならデフォルト
- [x] `test_eval_optional_chain_none_propagates`
- [x] `test_eval_optional_chain_some_accesses`
- [x] `test_eval_null_coalesce_none`
- [x] `test_eval_null_coalesce_some`
- [x] `test_eval_optional_chain_nested`

### E-2-E: トランスパイル
- [x] OptionalChain(Field) → `.and_then(|v| Some(v.field))`
- [x] OptionalChain(Method) → `.and_then(|v| Some(v.method(args)))`
- [x] NullCoalesce → `.unwrap_or(default)`
- [x] `test_transpile_optional_chain`
- [x] `test_transpile_null_coalesce`
- [x] `test_transpile_optional_chain_nested`

---

## Phase E-3: `operator` 宣言

### E-3-A: レキサー
- [x] `TokenKind::Operator`（キーワード `operator`）
- [x] `test_lex_operator_keyword`

### E-3-B: AST
- [x] `ImplItem::OperatorDef { op: OperatorKind, params, ret, body }`
- [x] `enum OperatorKind { Add, Sub, Mul, Div, Rem, Eq, Lt, Index, Neg }`

### E-3-C: パーサー
- [x] `impl operator +(...)` を受け取る
- [x] `operator unary-` など複数対応
- [x] `test_parse_operator_add`
- [x] `test_parse_operator_eq`
- [x] `test_parse_operator_index`
- [x] `test_parse_operator_unary_neg`

### E-3-D: 評価
- [x] `operator` 定義の解釈
- [x] `@derive(Eq)` との衝突検出
- [x] `test_eval_operator_add`
- [x] `test_eval_operator_mul`
- [x] `test_eval_operator_eq`
- [x] `test_eval_operator_index`
- [x] `test_eval_operator_unary_neg`
- [x] `test_eval_operator_conflict_derive_eq_error`

### E-3-E: トランスパイル
- [x] `operator +` → `impl std::ops::Add for Type`
- [x] `operator -` → `impl std::ops::Sub for Type`
- [x] `operator *` → `impl std::ops::Mul for Type`
- [x] `operator /` → `impl std::ops::Div for Type`
- [x] `operator ==` → `impl PartialEq for Type`
- [x] `operator <` → `impl PartialOrd for Type`
- [x] `operator []` → `impl std::ops::Index`
- [x] `operator unary-` → `impl std::ops::Neg`
- [x] `test_transpile_operator_overload_vector2`

---

## Phase E-4: `spawn` / 非同期

### E-4-A: レキサー
- [x] `spawn` キーワードのトークン化
- [x] `test_lex_spawn`

### E-4-B: AST
- [x] `Expr::Spawn { body: Block }`

### E-4-C: パーサー
- [x] `spawn { ... }` → `Expr::Spawn`
- [x] `test_parse_spawn_block`

### E-4-D: 評価
- [x] `spawn {}` で `Value::Option(Some(result))`
- [x] `test_eval_spawn_sequential`

### E-4-E: トランスパイル
- [x] `await` → `tokio::spawn`, `async move`
- [x] Snapshot: `test_transpile_async_closure`
- [x] Snapshot: `test_transpile_spawn`

---

## Phase E-5: `const fn`

### E-5-A: AST
- [x] `Stmt::FnDecl` に `is_const` フラグ

### E-5-B: パーサー
- [x] `const fn` 宣言を解析
- [x] `test_parse_const_fn`
- [x] `test_parse_const_var_with_const_fn_call`

### E-5-C: 評価
- [x] `const fn` による定数実行
- [x] `test_eval_const_fn_basic`
- [x] `test_eval_const_fn_in_const_var`

### E-5-D: トランスパイル
- [x] `const fn` を `const` 表現へ
- [x] Snapshot: `test_transpile_const_fn`
- [x] Snapshot: `test_transpile_const_var_with_const_fn`

---

## Phase E-6: `yield` / ジェネレータ

### E-6-A: レキサー
- [x] `TokenKind::Yield`
- [x] `test_lex_yield`

### E-6-B: AST
- [x] `Stmt::Yield` と `TypeExpr::Generate`

### E-6-C: パーサー
- [x] `yield expr`
- [x] `-> generate<T>`
- [x] `test_parse_yield`
- [x] `test_parse_generate_return_type`

### E-6-D: 評価
- [x] `generate<T>` は `Vec<T>` を返す
- [x] `test_eval_generator_finite`
- [x] `test_eval_generator_with_take`
- [x] `test_eval_generator_filter_map`
- [x] `test_eval_generator_fibonacci`

### E-6-E: トランスパイル
- [x] `Iterator` と `std::iter::from_fn`
- [x] Snapshot: `test_transpile_generator_fibonacci`
- [x] Snapshot: `test_transpile_generator_with_take`

---

## Phase E-7: `defer`

### E-7-A: レキサー
- [x] `TokenKind::Defer`
- [x] `test_lex_defer`

### E-7-B: AST
- [x] `Stmt::Defer { body: DeferBody }`
- [x] `enum DeferBody { Expr(Box<Expr>), Block(Block) }`

### E-7-C: パーサー
- [x] `defer expr`
- [x] `defer { ... }`
- [x] `test_parse_defer_expr`
- [x] `test_parse_defer_block`

### E-7-D: 評価
- [x] `defer_stack: Vec<DeferBody>` で LIFO
- [x] `test_defer_runs_on_normal_exit`
- [x] `test_defer_runs_on_early_return`
- [x] `test_defer_lifo_order`
- [x] `test_defer_block_syntax`
- [x] `test_defer_error_in_defer_ignored`

### E-7-E: トランスパイル
- [x] `scopeguard::defer` を活用
- [x] `@defer(cleanup: "method_name")` サポート
- [x] Snapshot: `test_transpile_defer_single`
- [x] Snapshot: `test_transpile_defer_multiple_lifo`
- [x] Snapshot: `test_transpile_defer_decorator`

---

## 進捗サマリ
| Phase | 内容 | 進捗 |
|---|---|---|
| E-1 | `|>` パイプ | 10 / 10 |
| E-2 | `?.` / `??` | 22 / 22 |
| E-3 | `operator` 宣言 | 20 / 20 |
| E-4 | `spawn` / 非同期 | 11 / 11 |
| E-5 | `const fn` | 9 / 9 |
| E-6 | `yield` / ジェネレータ | 12 / 12 |
| E-7 | `defer` | 18 / 18 |
| **合計** | | **102 / 102** |
