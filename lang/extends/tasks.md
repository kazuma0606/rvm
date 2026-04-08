# ForgeScript 拡張ロードマップ

> [ ] 仕様確認 / [x] 実装中
> **参照**: `lang/extends/spec.md`
> **計画**: `lang/extends/plan.md`
> **対象フェーズ**: Phase E-1 ～ Phase E-6
---

## Phase E-1: `|>` パイプ演算子
### E-1-A: レキサー
- [x] `TokenKind::PipeArrow` を追加して `|>` を単一トークン化
- [x] `|` を既存の `TokenKind::Pipe` と区別
- [x] `test_lex_pipe_arrow` を追加

### E-1-B: パーサ
- [x] `parse_expr` に `PipeArrow` の構文を盛り込む
- [x] `lhs |> method(args)` を `Expr::MethodCall` に変換
- [x] 引数なしの呼び出しでも `Expr::MethodCall { args: [] }`
- [x] 連続パイプをパースできるよう `lhs |> method |> other` に対応
- [x] `test_parse_pipe_arrow_method`
- [x] `test_parse_pipe_arrow_no_args`
- [x] `test_parse_pipe_arrow_chain`

### E-1-C: E2E・Snapshot
- [x] `test_pipe_arrow_filter_map_fold` で filter/map/fold の動作を確認
- [x] `test_pipe_arrow_equals_method_chain` でメソッド連鎖を保持
- [x] `test_transpile_pipe_arrow` でトランスパイル結果をスナップショット化

---

## Phase E-2: `?.` / `??`
### E-2-A: レキサー
- [x] `TokenKind::QuestionDot` を追加して `?.` を認識
- [x] `TokenKind::QuestionQuestion` を追加して `??` を認識
- [x] `?` と組み合わせた文脈と区別
- [x] `test_lex_question_dot`
- [x] `test_lex_question_question`

### E-2-B: AST
- [x] `Expr::OptionalChain { object, chain: ChainKind }`
- [x] `enum ChainKind { Field(String), Method { name, args } }`
- [x] `Expr::NullCoalesce { value, default }`

### E-2-C: パーサ
- [x] `expr?.field` を `OptionalChain(Field)` に構築
- [x] `expr?.method(args)` を `OptionalChain(Method)`
- [x] `expr ?? default` を `Expr::NullCoalesce`
- [x] `??` を `||` に似たショートサーキットで扱う
- [x] `test_parse_optional_chain_field`
- [x] `test_parse_optional_chain_method`
- [x] `test_parse_null_coalesce`
- [x] `test_parse_optional_chain_nested`

### E-2-D: 評価
- [x] `OptionalChain(Field)` が `None` を伝播し、 `Some(v)` なら `.field`
- [x] `OptionalChain(Method)` が `Some(v.method(...))`
- [x] `NullCoalesce` で `None` 時にデフォルトを返す
- [x] `test_eval_optional_chain_none_propagates`
- [x] `test_eval_optional_chain_some_accesses`
- [x] `test_eval_null_coalesce_none`
- [x] `test_eval_null_coalesce_some`
- [x] `test_eval_optional_chain_nested`

### E-2-E: トランスパイラ
- [x] `OptionalChain(Field)` を `.and_then(|v| Some(v.field))`
- [x] `OptionalChain(Method)` を `.and_then(|v| Some(v.method(args)))`
- [x] `NullCoalesce` を `.unwrap_or(default)`
- [x] `test_transpile_optional_chain`
- [x] `test_transpile_null_coalesce`
- [x] `test_transpile_optional_chain_nested`

---

## Phase E-3: `operator` キーワード
### E-3-A: レキサー
- [x] `operator` を `TokenKind::Operator` にトークン化
- [x] `test_lex_operator_keyword`

### E-3-B: AST
- [x] `ImplItem::OperatorDef { op: OperatorKind, params, ret, body }`
- [x] `enum OperatorKind { Add, Sub, Mul, Div, Rem, Eq, Lt, Index, Neg }`

### E-3-C: パーサ
- [x] `impl` ブロック内で `operator +(...)` を認識
- [x] `operator unary-(self)` を扱う
- [x] `+` `-` `*` `/` `%` `==` `<` `[]` `unary-` をすべて解析
- [x] `test_parse_operator_add`
- [x] `test_parse_operator_eq`
- [x] `test_parse_operator_index`
- [x] `test_parse_operator_unary_neg`

### E-3-D: 評価
- [x] 構造体の `operator` 定義を呼び出し可能にする
- [x] `@derive(Eq)` との衝突を検出
- [x] `test_eval_operator_add`
- [x] `test_eval_operator_mul`
- [x] `test_eval_operator_eq`
- [x] `test_eval_operator_index`
- [x] `test_eval_operator_unary_neg`
- [x] `test_eval_operator_conflict_derive_eq_error`

### E-3-E: トランスパイラ
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
# E-4-A lines not handled ??? 

### E-4-B: AST
- [x] `Expr::Spawn { body: Block }`

### E-4-C: パーサ
- [x] `spawn { ... }` を `Expr::Spawn`
- [x] `test_parse_spawn_block`

### E-4-D: 評価
- [x] `spawn { }` を非同期タスクで実行し `Value::Option(Some(result))`
- [x] `test_eval_spawn_sequential`

### E-4-E: トランスパイラ
- [x] `await` を `|args| async move { ... }` や `tokio::spawn` に変換
- [x] Snapshot: `test_transpile_async_closure`
- [x] Snapshot: `test_transpile_spawn`

---

## Phase E-5: `const fn`
### E-5-A: AST
- [x] `Stmt::FnDecl` に `is_const: bool`

### E-5-B: パーサ
- [x] `const fn name(...)` を `is_const: true` で扱う
- [x] `test_parse_const_fn`
- [x] `test_parse_const_var_with_const_fn_call`

### E-5-C: 評価
- [x] `const fn` でコンパイル時実行を担保
- [x] `test_eval_const_fn_basic`
- [x] `test_eval_const_fn_in_const_var`

### E-5-D: トランスパイラ
- [x] `const fn` 呼び出しを `const` で保持
- [x] Snapshot: `test_transpile_const_fn`
- [x] Snapshot: `test_transpile_const_var_with_const_fn`

---

## Phase E-6: `yield` / ジェネレータ
### E-6-A: レキサー
- [x] `yield` を `TokenKind::Yield` に追加
- [x] `test_lex_yield`

### E-6-B: AST
- [x] `Stmt::Yield { value: Box<Expr> }`
- [x] `TypeExpr::Generate(Box<TypeExpr>)`

### E-6-C: パーサ
- [x] `yield expr` を `Stmt::Yield`
- [x] `-> generate<T>` を `TypeExpr::Generate(T)`
- [x] `test_parse_yield`
- [x] `test_parse_generate_return_type`

### E-6-D: 評価
- [x] `generate<T>` を `Vec<T>` に変換
- [x] `test_eval_generator_finite`
- [x] `test_eval_generator_with_take`
- [x] `test_eval_generator_filter_map`
- [x] `test_eval_generator_fibonacci`

### E-6-E: トランスパイラ
- [x] ジェネレータを `Iterator` + `std::iter::from_fn` に変換
- [x] Snapshot: `test_transpile_generator_fibonacci`
- [x] Snapshot: `test_transpile_generator_with_take`

---

## 進捗ダッシュボード

| Phase | 内容 | 完了 / 全体 |
|---|---|---|
| E-1 | `|>` パイプ演算子 | 10 / 10 |
| E-2 | `?.` / `??` | 22 / 22 |
| E-3 | `operator` キーワード | 20 / 20 |
| E-4 | `spawn` / 非同期 | 11 / 11 |
| E-5 | `const fn` | 9 / 9 |
| E-6 | `yield` / ジェネレータ | 12 / 12 |
| **合計** | 進捗 | **84 / 84** |
