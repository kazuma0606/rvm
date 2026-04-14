# ForgeScript 拡張 v2 タスク一覧

> **参照**: `lang/extends/v2/spec.md`
> **実装順序**: E2-2 → E2-1 → E2-3

---

## Phase E2-2: Option 型メソッド拡充

> **変更ファイル**: `crates/forge-vm/src/interpreter.rs` のみ
> **リスク**: 低（既存コードへの影響なし）

### E2-2-A: `unwrap_or`
- [x] `Value::Option(None)` → `default` を返す
- [x] `Value::Option(Some(v))` → `v` を返す
- [x] テスト: `test_option_unwrap_or_none`
- [x] テスト: `test_option_unwrap_or_some`

### E2-2-B: `unwrap`
- [x] `some(v)` → `v` を返す
- [x] `none` → ランタイムエラー（`"unwrap called on none"`)
- [x] テスト: `test_option_unwrap_some`
- [x] テスト: `test_option_unwrap_none_panics`

### E2-2-C: `map`
- [x] `some(v)` → `fn(v)` を呼び `some(result)` を返す
- [x] `none` → `none` をそのまま返す
- [x] テスト: `test_option_map_some`
- [x] テスト: `test_option_map_none`
- [x] テスト: `test_option_map_chain`（パイプと組み合わせ）

### E2-2-D: `and_then`
- [x] `some(v)` → `fn(v)` を呼ぶ（戻り値は `T?`）
- [x] `none` → `none` をそのまま返す
- [x] テスト: `test_option_and_then_some`
- [x] テスト: `test_option_and_then_none`
- [x] テスト: `test_option_and_then_chain`

### E2-2-E: `is_some` / `is_none`
- [x] `is_some()`: `some(_)` → `true`, `none` → `false`
- [x] `is_none()`: `none` → `true`, `some(_)` → `false`
- [x] テスト: `test_option_is_some`
- [x] テスト: `test_option_is_none`

### E2-2-F: `or` / `filter`
- [x] `or(default_opt)`: `none` → `default_opt`, `some(v)` → `some(v)`
- [x] `filter(fn)`: `some(v)` で `fn(v)` が false → `none`, true → `some(v)`
- [x] テスト: `test_option_or_none`
- [x] テスト: `test_option_or_some`
- [x] テスト: `test_option_filter_true`
- [x] テスト: `test_option_filter_false`

### E2-2-G: E2E 統合テスト
- [x] テスト: `test_option_pipeline_find_map_unwrap_or`
  ```forge
  let name = students |> find(s => s.score >= 90) |> map(s => s.name) |> unwrap_or("なし")
  assert_eq(name, "Alice")
  ```
- [x] テスト: `test_option_pipeline_none_path`
  ```forge
  let name = students |> find(s => s.score >= 100) |> map(s => s.name) |> unwrap_or("なし")
  assert_eq(name, "なし")
  ```
- [x] テスト: `test_option_and_then_find_chain`

---

## Phase E2-1: 分割代入

> **変更ファイル**: `forge-ast`, `forge-parser`, `forge-vm`, `forge-transpiler`
> **リスク**: 中（パーサー変更）

### E2-1-A: AST
- [x] `Pat` 列挙型を追加
  ```rust
  pub enum Pat {
      Ident(String),           // 単純な変数束縛
      Wildcard,                // _
      Tuple(Vec<Pat>),         // (a, b, c)
      List(Vec<Pat>),          // [a, b, c]  ← (a,b,c) と同義
      Rest(String),            // ..name（残余パターン）
  }
  ```
- [x] `Stmt::Let` の `name: String` を `pat: Pat` に変更（後方互換を維持）
- [x] テスト: AST 構造の確認

### E2-1-B: パーサー
- [x] `let (a, b) = expr` → `Stmt::Let { pat: Pat::Tuple([Ident("a"), Ident("b")]), ... }`
- [x] `let [a, b] = expr` → `Stmt::Let { pat: Pat::List([...]) }`（Tuple と同義扱い）
- [x] `let _` → `Pat::Wildcard`
- [x] `let (head, ..tail)` → `Pat::Tuple([Ident("head"), Rest("tail")])`
- [x] 後方互換: `let x = expr` は `Pat::Ident("x")` として従来通り動作
- [x] テスト: `test_parse_destructure_tuple_2`
- [x] テスト: `test_parse_destructure_tuple_3`
- [x] テスト: `test_parse_destructure_wildcard`
- [x] テスト: `test_parse_destructure_rest`
- [x] テスト: `test_parse_destructure_list_bracket`

### E2-1-C: 評価（インタープリタ）
- [x] `Pat::Ident` → 従来通りの束縛
- [x] `Pat::Tuple / Pat::List` → リスト値を展開して各変数に束縛
- [x] `Pat::Wildcard` → 何もしない
- [x] `Pat::Rest` → 残りの要素をリストとして束縛
- [x] 要素数不足時のエラーメッセージを適切に出す
- [x] テスト: `test_eval_destructure_basic`
- [x] テスト: `test_eval_destructure_partition`
- [x] テスト: `test_eval_destructure_wildcard`
- [x] テスト: `test_eval_destructure_rest`
- [x] テスト: `test_eval_destructure_zip`
- [x] テスト: `test_eval_destructure_too_few_elements_error`

### E2-1-D: `for` ループへの分割代入拡張
- [x] `for (a, b) in list_of_pairs` に対応
- [x] パーサー: `for` のループ変数にも `Pat` を使えるよう変更
- [x] 評価器: 各イテレーションで要素を分割代入
- [x] テスト: `test_eval_for_destructure_enumerate`
  ```forge
  for (i, v) in [10, 20, 30] |> enumerate() {
      println("{i}: {v}")
  }
  ```
- [x] テスト: `test_eval_for_destructure_zip`
  ```forge
  for (k, v) in ["a","b"] |> zip([1, 2]) {
      println("{k}={v}")
  }
  ```

### E2-1-E: トランスパイル
- [x] `Pat::Tuple` → 一時変数 + 添字アクセスに展開
  ```rust
  // let (a, b) = expr;
  let _tmp = expr;
  let a = _tmp[0].clone();
  let b = _tmp[1].clone();
  ```
- [x] `Pat::Rest` → スライス展開
  ```rust
  // let (head, ..tail) = expr;
  let _tmp = expr;
  let head = _tmp[0].clone();
  let tail = _tmp[1..].to_vec();
  ```
- [x] テスト: `test_transpile_destructure_tuple`
- [x] テスト: `test_transpile_destructure_rest`

### E2-1-F: E2E 統合テスト
- [x] テスト: `test_e2e_destructure_partition`
  ```forge
  let nums = [1, 2, 3, 4, 5, 6]
  let (evens, odds) = nums |> partition(n => n % 2 == 0)
  assert_eq(evens, [2, 4, 6])
  assert_eq(odds,  [1, 3, 5])
  ```
- [x] テスト: `test_e2e_destructure_zip_for`
  ```forge
  let keys   = ["a", "b", "c"]
  let values = [1, 2, 3]
  let result: list<string> = []
  for (k, v) in keys |> zip(values) {
      result = result |> concat(["{k}={v}"])
  }
  assert_eq(result, ["a=1", "b=2", "c=3"])
  ```
- [x] テスト: `test_e2e_destructure_chunk`
  ```forge
  let (first, second, third) = [10, 20, 30]
  assert_eq(first + second + third, 60)
  ```

---

## Phase E2-3: 匿名 struct

> **変更ファイル**: `forge-lexer`, `forge-parser`, `forge-ast`, `forge-vm`, `forge-transpiler`
> **リスク**: 高（全層に変更）

### E2-3-A: レキサー
- [x] `{` が型注釈文脈でも使えるよう確認（既存トークンの流用で対応可能なはず）
- [x] テスト: `test_lex_anon_struct_type_context`

### E2-3-B: AST
- [x] `TypeExpr::AnonStruct(Vec<(String, TypeExpr)>)` を追加
- [x] `Expr::AnonStruct(Vec<(String, Expr)>)` を追加（リテラル）
- [x] `Expr::AnonStruct` のショートハンド: `(String, None)` で変数参照を表す

### E2-3-C: パーサー（型注釈）
- [x] `type_expr` の解析で `{` が来た場合に `AnonStruct` 型として解析
  - 例: `-> { name: string, score: number }`
  - 例: `list<{ id: number, name: string }>`
- [x] `field_type` : `IDENT ":" type_expr` のパース
- [x] テスト: `test_parse_anon_struct_type_return`
- [x] テスト: `test_parse_anon_struct_type_in_generic`
- [x] テスト: `test_parse_anon_struct_type_in_state`

### E2-3-D: パーサー（リテラル）
- [x] 式文脈で `{` が来た場合、`IDENT ":"` が続けば `AnonStruct` リテラルと解析
  - `{` + `IDENT` + `:` → AnonStruct
  - `{` + `IDENT` + `,` or `}` → ショートハンド AnonStruct
  - `{` + その他 → ブロック（既存の動作を維持）
- [x] ショートハンド `{ x, y }` → `{ x: x, y: y }` として AST 構築
- [x] テスト: `test_parse_anon_struct_literal`
- [x] テスト: `test_parse_anon_struct_literal_shorthand`
- [x] テスト: `test_parse_anon_struct_literal_mixed`（通常フィールドとショートハンド混在）
- [x] テスト: `test_parse_block_not_confused_with_struct`（ブロックとの区別）

### E2-3-E: 評価（インタープリタ）
- [x] `Expr::AnonStruct` → `Value::Struct { type_name: "<anon>", fields: HashMap }`
- [x] フィールドアクセス `.field` は名前付き struct と同一のコードパスを使う
- [x] 型チェック: 型名が `"<anon>"` でも `named_struct` と同様にフィールドアクセス可能
- [x] ショートハンドの評価: 変数参照に変換してから評価
- [x] テスト: `test_eval_anon_struct_literal`
- [x] テスト: `test_eval_anon_struct_field_access`
- [x] テスト: `test_eval_anon_struct_shorthand`
- [x] テスト: `test_eval_anon_struct_in_list`
- [x] テスト: `test_eval_anon_struct_as_return_value`
- [x] テスト: `test_eval_anon_struct_pipe_map`

### E2-3-F: トランスパイル
- [x] 匿名 struct 型を自動命名した Rust struct に変換
  - フィールド名を辞書順ソートして型名を生成: `{ name: string, score: number }` → `AnonStruct_name_score`
  - 同じフィールドセットは同じ struct 名にする
- [x] `Expr::AnonStruct` → 対応する Rust struct のインスタンス化
- [x] テスト: `test_transpile_anon_struct_return_type`
- [x] テスト: `test_transpile_anon_struct_literal`
- [x] テスト: `test_transpile_anon_struct_dedup`（同一フィールドセットの struct は1回だけ生成）

### E2-3-G: E2E 統合テスト
- [x] テスト: `test_e2e_anon_struct_map`
  ```forge
  struct Student { name: string, score: number }
  let students = [
      Student { name: "Alice", score: 92 },
      Student { name: "Bob",   score: 78 },
  ]
  let summaries = students |> map(s => { name: s.name, passed: s.score >= 80 })
  assert_eq(summaries[0].name,   "Alice")
  assert_eq(summaries[0].passed, true)
  assert_eq(summaries[1].passed, false)
  ```
- [x] テスト: `test_e2e_anon_struct_state`
  ```forge
  state users: list<{ id: number, name: string }> = []
  users = users |> concat([{ id: 1, name: "Alice" }])
  assert_eq(users[0].name, "Alice")
  ```
- [x] テスト: `test_e2e_anon_struct_shorthand`
  ```forge
  let name = "Alice"
  let score = 92
  let s = { name, score }
  assert_eq(s.name, "Alice")
  assert_eq(s.score, 92)
  ```

---

## 進捗サマリ

| Phase | 内容               | 完了 / 全体 |
|-------|--------------------|-------------|
| E2-2  | Option メソッド    | 27 / 27     |
| E2-1  | 分割代入           | 32 / 32     |
| E2-3  | 匿名 struct        | 33 / 33     |
| **合計** |                 | **92 / 92** |
