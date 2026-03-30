# ForgeScript タスク一覧

> [ ] 未完了 / [x] 完了
> 各 Phase の単体テスト・結合テストは Phase 内に決め打ち

---

## Phase 0：基盤整備

### 0-A: クレート再編

- [x] `forge-compiler` クレート作成（`Cargo.toml` + `src/lib.rs`）
- [x] `forge-vm` クレート作成
- [x] `forge-stdlib` クレート作成
- [x] `forge-cli` クレート作成（バイナリクレート）
- [x] workspace `Cargo.toml` を5クレート構成に更新
- [x] `fixtures/` ディレクトリ作成（`.forge` テストファイル置き場）
- [x] `fixtures/hello.forge` を作成（最初の動作確認ファイル）

```forge
// fixtures/hello.forge
let msg = "Hello, ForgeScript!"
print(msg)
```

### 0-B: テスト基盤

- [x] E2Eテストランナー作成（`forge-cli/tests/e2e.rs`）
  - `.forge` ファイルを実行し、stdout を文字列比較する仕組み（Phase 2-D で本実装）
- [x] `cargo test --workspace` がエラーなく通ることを確認

#### Phase 0 単体テスト

| テスト | 場所 | 内容 |
|---|---|---|
| `test_workspace_builds` | `forge-cli/tests/e2e.rs` | `cargo build --workspace` が通る |

---

## Phase 1：Lexer / Parser / AST

### 1-A: Lexer

- [x] `forge-compiler/src/lexer/tokens.rs` に `TokenKind` 定義
  - [x] 数値リテラル: `Int(i64)`, `Float(f64)`
  - [x] 文字列リテラル: `Str(String)`
  - [x] 真偽値: `True`, `False`
  - [x] キーワード: `Let`, `State`, `Const`, `Fn`, `Return`
  - [x] キーワード: `If`, `Else`, `For`, `In`, `While`
  - [x] キーワード: `Match`, `None`, `Some`, `Ok`, `Err`
  - [x] 演算子: `Plus`, `Minus`, `Star`, `Slash`, `Percent`
  - [x] 演算子: `EqEq`, `BangEq`, `Lt`, `Gt`, `LtEq`, `GtEq`
  - [x] 演算子: `And`, `Or`, `Bang`
  - [x] 記号: `Arrow(=>)`, `ThinArrow(->)`, `Question(?)`
  - [x] 記号: `Colon`, `Dot`, `DotDot`, `DotDotEq`
  - [x] 記号: `LBracket`, `RBracket`
  - [x] コメント: `//` をスキップ
  - [x] 文字列補間: `"Hello, {name}"` を `StrPart` のリストにパース

#### Phase 1-A 単体テスト（`forge-compiler/src/lexer/mod.rs` 内 `#[cfg(test)]`）

- [x] `test_lex_integer` — `42` → `[Int(42), Eof]`
- [x] `test_lex_float` — `3.14` → `[Float(3.14), Eof]`
- [x] `test_lex_bool` — `true` → `[True, Eof]`、`false` → `[False, Eof]`
- [x] `test_lex_string` — `"hello"` → `[Str("hello"), Eof]`
- [x] `test_lex_keywords` — `let state const fn return if else for in while match` 各キーワード
- [x] `test_lex_operators` — `+ - * / % == != < > <= >= && || !`
- [x] `test_lex_symbols` — `=> -> ? : . .. ..= [ ]`
- [x] `test_lex_comment` — `// comment\nlet x = 1` でコメントがスキップされる
- [x] `test_lex_string_interpolation` — `"Hello, {name}!"` が正しく分割される
- [x] `test_lex_unknown_char` — `@` → `LexError::UnexpectedChar`
- [x] `test_lex_unterminated_string` — `"hello` → `LexError::UnterminatedString`
- [x] `test_lex_span` — 各トークンの span（開始・終了位置）が正しい

### 1-B: AST 定義

- [x] `forge-compiler/src/ast/mod.rs` に `Stmt` 定義
  - [x] `Stmt::Let`, `Stmt::State`, `Stmt::Const`
  - [x] `Stmt::Fn`（パラメータ・戻り値型・ボディ）
  - [x] `Stmt::Return`
  - [x] `Stmt::Expr`
- [x] `forge-compiler/src/ast/mod.rs` に `Expr` 定義
  - [x] `Expr::Literal`（Int / Float / Str / Bool）
  - [x] `Expr::Ident`
  - [x] `Expr::BinOp`, `Expr::UnaryOp`
  - [x] `Expr::If`, `Expr::While`, `Expr::For`
  - [x] `Expr::Block`
  - [x] `Expr::Match`（アームのリスト）
  - [x] `Expr::Call`, `Expr::MethodCall`
  - [x] `Expr::Index`（リストインデックス）
  - [x] `Expr::Closure`
  - [x] `Expr::Interpolation`（文字列補間）
  - [x] `Expr::Range`（`..` / `..=`）
  - [x] `Expr::List`（リストリテラル）
  - [x] `Expr::Question`（`?` 演算子）
- [x] `TypeAnn` 型定義（`number` / `float` / `string` / `bool` / `T?` / `T!` / `list<T>`）
- [x] `Pattern` 型定義（マッチパターン）
  - [x] `Pattern::Literal`, `Pattern::Wildcard`
  - [x] `Pattern::Some(Pattern)`, `Pattern::None`
  - [x] `Pattern::Ok(Pattern)`, `Pattern::Err(Pattern)`
  - [x] `Pattern::Range`

### 1-C: Parser

- [ ] `forge-compiler/src/parser/mod.rs` に再帰降下パーサー実装
  - [ ] `parse_module()` — 文のリストをパース
  - [ ] `parse_stmt()` — 文のディスパッチ
  - [ ] `parse_let()` / `parse_state()` / `parse_const()`
  - [ ] `parse_fn()` — 関数定義
  - [ ] `parse_return()`
  - [ ] `parse_expr()` — 式（演算子優先順位対応）
  - [ ] `parse_if()`
  - [ ] `parse_while()`
  - [ ] `parse_for()`
  - [ ] `parse_match()`
  - [ ] `parse_block()`
  - [ ] `parse_closure()` — `x => expr` / `(x, y) => expr` / `() => expr`
  - [ ] `parse_call()` / `parse_method_call()`
  - [ ] `parse_list_literal()` / `parse_range_literal()`
  - [ ] `parse_string_interpolation()`
  - [ ] `parse_type_ann()` — 型注釈
  - [ ] `parse_pattern()` — match パターン

#### Phase 1-C 単体テスト（`forge-compiler/src/parser/mod.rs` 内 `#[cfg(test)]`）

- [ ] `test_parse_let` — `let x = 10` → `Stmt::Let`
- [ ] `test_parse_state` — `state count = 0` → `Stmt::State`
- [ ] `test_parse_const` — `const MAX = 100` → `Stmt::Const`
- [ ] `test_parse_fn` — `fn add(a: number, b: number) -> number { a + b }` → `Stmt::Fn`
- [ ] `test_parse_if_expr` — `if x > 0 { "pos" } else { "neg" }` → `Expr::If`
- [ ] `test_parse_while` — `while i < 10 { i = i + 1 }` → `Stmt::While` ... `Expr::While`
- [ ] `test_parse_for` — `for x in items { print(x) }` → `Expr::For`
- [ ] `test_parse_match` — `match x { some(v) => v, none => 0 }` → `Expr::Match`
- [ ] `test_parse_closure_single` — `x => x * 2` → `Expr::Closure`
- [ ] `test_parse_closure_multi_arg` — `(a, b) => a + b` → `Expr::Closure`
- [ ] `test_parse_closure_no_arg` — `() => print("hi")` → `Expr::Closure`
- [ ] `test_parse_closure_block` — `x => { let y = x * 2; y + 1 }` → ブロックボディ
- [ ] `test_parse_method_call` — `items.map(x => x * 2)` → `Expr::MethodCall`
- [ ] `test_parse_question_op` — `parse(s)?` → `Expr::Question`
- [ ] `test_parse_string_interpolation` — `"Hello, {name}!"` → `Expr::Interpolation`
- [ ] `test_parse_range` — `[1..=10]` → `Expr::Range { inclusive: true }`
- [ ] `test_parse_list_literal` — `[1, 2, 3]` → `Expr::List`
- [ ] `test_parse_operator_precedence` — `1 + 2 * 3` → `1 + (2 * 3)`
- [ ] `test_parse_type_ann` — `let x: number?` → `TypeAnn::Option(number)`
- [ ] `test_parse_error_unexpected_token` — 不正な構文で `ParseError` を返す

#### Phase 1 結合テスト（`forge-compiler/tests/integration.rs`）

- [ ] `test_full_parse_hello` — `let msg = "Hello!"\nprint(msg)` がエラーなくパースされる
- [ ] `test_full_parse_fn_with_match` — 関数定義 + match 式をパース
- [ ] `test_full_parse_closures` — クロージャを含むコードをパース
- [ ] `test_full_parse_all_literals` — 全リテラル型（int/float/bool/string）をパース

---

## Phase 2：RVM インタプリタ

### 2-A: Value 型

- [ ] `forge-vm/src/value.rs` に `Value` 定義
  - [ ] `Int(i64)`, `Float(f64)`, `String(String)`, `Bool(bool)`
  - [ ] `List(Rc<RefCell<Vec<Value>>>)`
  - [ ] `Option(Option<Box<Value>>)`
  - [ ] `Result(Result<Box<Value>, String>)`
  - [ ] `Closure { params, body, env }`
  - [ ] `NativeFunction(NativeFn)`
  - [ ] `Unit`（表示なし）
- [ ] `Value` に `Display` 実装（print 用）
- [ ] `Value` に `type_name()` 実装

#### Phase 2-A 単体テスト（`forge-vm/src/value.rs` 内 `#[cfg(test)]`）

- [ ] `test_value_display_int` — `Value::Int(42)` → `"42"`
- [ ] `test_value_display_float` — `Value::Float(3.14)` → `"3.14"`
- [ ] `test_value_display_bool` — `Value::Bool(true)` → `"true"`
- [ ] `test_value_display_string` — `Value::String("hi")` → `"hi"`
- [ ] `test_value_display_none` — `Value::Option(None)` → `"none"`
- [ ] `test_value_display_some` — `Value::Option(Some(Int(1)))` → `"some(1)"`
- [ ] `test_value_display_list` — `[1, 2, 3]` → `"[1, 2, 3]"`
- [ ] `test_no_nil` — `Value` に `Nil` バリアントが存在しないことを確認（コンパイル確認）

### 2-B: インタプリタ

- [ ] `forge-vm/src/interpreter/mod.rs` にツリーウォーカー実装
  - [ ] `Env`（スコープチェーン）の実装
  - [ ] `eval_literal()` — リテラルの評価
  - [ ] `eval_binop()` — 二項演算
  - [ ] `eval_unary()` — 単項演算
  - [ ] `eval_let()` / `eval_state()` / `eval_const()`
  - [ ] `eval_assign()` — `state` 変数への再代入
  - [ ] `eval_if()` — if 式の評価
  - [ ] `eval_while()`
  - [ ] `eval_for()` — イテレータを消費してブロックを繰り返す
  - [ ] `eval_match()` — パターンマッチの評価
  - [ ] `eval_block()` — ブロック式の評価
  - [ ] `eval_fn_def()` — 関数定義を環境に登録
  - [ ] `eval_call()` — 関数・クロージャ呼び出し
  - [ ] `eval_method_call()` — メソッド呼び出し（Phase 3 で本格実装）
  - [ ] `eval_closure()` — クロージャの生成（環境のキャプチャ）
  - [ ] `eval_question()` — `?` 演算子（Result 伝播）
  - [ ] `eval_interpolation()` — 文字列補間
  - [ ] `eval_range()` — 範囲をリストに展開
  - [ ] `eval_list()` — リストリテラルの評価
  - [ ] `RuntimeError` 型の定義（型エラー・未定義変数等）

#### Phase 2-B 単体テスト（`forge-vm/src/interpreter/mod.rs` 内 `#[cfg(test)]`）

- [ ] `test_eval_arithmetic` — `1 + 2 * 3` → `Int(7)`
- [ ] `test_eval_string_concat` — `"foo" + "bar"` → `String("foobar")`
- [ ] `test_eval_comparison` — `1 < 2` → `Bool(true)`
- [ ] `test_eval_logical` — `true && false` → `Bool(false)`
- [ ] `test_eval_let_binding` — `let x = 10; x` → `Int(10)`
- [ ] `test_eval_state_reassign` — `state x = 0; x = 5; x` → `Int(5)`
- [ ] `test_eval_let_immutable` — `let x = 1; x = 2` → `RuntimeError::Immutable`
- [ ] `test_eval_if_expr` — `if true { 1 } else { 2 }` → `Int(1)`
- [ ] `test_eval_if_else_chain` — else if チェーンが正しく評価される
- [ ] `test_eval_while` — ループが正しく実行される
- [ ] `test_eval_for_range` — `for i in [1..=3] { i }` が `[1, 2, 3]` を生成
- [ ] `test_eval_block_expr` — ブロックの最後の式が戻り値になる
- [ ] `test_eval_fn_call` — `fn add(a, b) { a + b }; add(1, 2)` → `Int(3)`
- [ ] `test_eval_closure` — `let f = x => x * 2; f(5)` → `Int(10)`
- [ ] `test_eval_closure_capture` — クロージャが外側の変数をキャプチャ
- [ ] `test_eval_match_literal` — `match 2 { 1 => "one", 2 => "two", _ => "other" }` → `String("two")`
- [ ] `test_eval_match_option_some` — `match some(42) { some(v) => v, none => 0 }` → `Int(42)`
- [ ] `test_eval_match_option_none` — none パターンが正しくマッチする
- [ ] `test_eval_match_result_ok` — `match ok(1) { ok(v) => v, err(e) => 0 }`
- [ ] `test_eval_match_result_err` — err パターンが正しくマッチする
- [ ] `test_eval_question_ok` — `?` が `ok(v)` を unwrap する
- [ ] `test_eval_question_err` — `?` が `err(e)` で即 return する
- [ ] `test_eval_string_interpolation` — `"Hello, {name}!"` が正しく展開される
- [ ] `test_eval_shadowing` — 同名変数のシャドーイングが動作する
- [ ] `test_eval_scope` — スコープ外の変数にアクセスしてエラー

### 2-C: 標準ライブラリ（ネイティブ関数）

- [ ] `print(v)` — 任意の値を stdout に出力
- [ ] `println(v)` — 改行付き出力
- [ ] `string(v)` — 任意の値を string に変換
- [ ] `number(v)` — string / float → number!
- [ ] `float(v)` — string / number → float!
- [ ] `len(v)` — string / list の長さ
- [ ] `type_of(v)` — 型名を string で返す

#### Phase 2-C 単体テスト

- [ ] `test_native_print` — `print(42)` が stdout に `"42\n"` を出力
- [ ] `test_native_string` — `string(42)` → `"42"`、`string(true)` → `"true"`
- [ ] `test_native_number` — `number("42")` → `ok(42)`、`number("abc")` → `err(...)`
- [ ] `test_native_float` — `float("3.14")` → `ok(3.14)`
- [ ] `test_native_len_string` — `len("hello")` → `5`
- [ ] `test_native_len_list` — `len([1,2,3])` → `3`
- [ ] `test_native_type_of` — `type_of(42)` → `"number"`

### 2-D: forge-cli

- [ ] `forge run file.forge` — ファイルを読み込んで実行
- [ ] `forge repl` — 対話型 REPL（複数行入力・状態保持）
- [ ] `forge help` — コマンド一覧
- [ ] エラー時の適切なメッセージ表示（行番号・カラム付き）

#### Phase 2 E2E テスト（`forge-cli/tests/e2e.rs`）

各テストは `.forge` ファイルを `forge run` で実行し stdout を検証する。

- [ ] `e2e_hello_world` — `print("Hello, World!")` → `"Hello, World!\n"`
- [ ] `e2e_arithmetic` — 四則演算・余り
- [ ] `e2e_string_concat` — 文字列連結
- [ ] `e2e_bool_logic` — `&&` `||` `!`
- [ ] `e2e_let_state` — let は再代入不可・state は再代入可
- [ ] `e2e_const` — const 定数の使用
- [ ] `e2e_if_else_expr` — if/else が式として値を返す
- [ ] `e2e_while_loop` — while ループ
- [ ] `e2e_for_range` — `for i in [1..=5]` のループ
- [ ] `e2e_for_expr` — for が値のリストを返す
- [ ] `e2e_function_def` — 関数定義と呼び出し
- [ ] `e2e_function_return` — return 文
- [ ] `e2e_closure_basic` — クロージャの基本動作
- [ ] `e2e_closure_capture` — クロージャによるキャプチャ
- [ ] `e2e_match_literal` — リテラルのパターンマッチ
- [ ] `e2e_match_option` — Option のパターンマッチ
- [ ] `e2e_match_result` — Result のパターンマッチ
- [ ] `e2e_question_op` — `?` 演算子によるエラー伝播
- [ ] `e2e_string_interpolation` — `"Hello, {name}!"`
- [ ] `e2e_recursion` — 再帰関数（フィボナッチ等）
- [ ] `e2e_nested_scope` — ネストしたスコープとシャドーイング
- [ ] `e2e_type_of` — `type_of` 組み込み関数

---

## Phase 3：コレクション API

### 3-A: リストメソッド実装（`forge-stdlib/src/collections/`）

- [ ] `.map(f)`
- [ ] `.filter(f)`
- [ ] `.flat_map(f)`
- [ ] `.filter_map(f)`
- [ ] `.take(n)` / `.skip(n)`
- [ ] `.take_while(f)` / `.skip_while(f)`
- [ ] `.enumerate()`
- [ ] `.zip(other)`
- [ ] `.sum()` / `.count()`
- [ ] `.fold(seed, f)`
- [ ] `.any(f)` / `.all(f)` / `.none(f)`
- [ ] `.first()` / `.last()` / `.nth(n)`
- [ ] `.min()` / `.max()` / `.min_by(f)` / `.max_by(f)`
- [ ] `.order_by(f)` / `.order_by_descending(f)`
- [ ] `.then_by(f)` / `.then_by_descending(f)`
- [ ] `.reverse()`
- [ ] `.distinct()`
- [ ] `.collect()`

#### Phase 3 単体テスト（`forge-stdlib/tests/`）

- [ ] `test_map` — `[1,2,3].map(x => x * 2)` → `[2, 4, 6]`
- [ ] `test_filter` — `[1,2,3,4].filter(x => x % 2 == 0)` → `[2, 4]`
- [ ] `test_fold` — `[1,2,3].fold(0, (acc, x) => acc + x)` → `6`
- [ ] `test_sum` — `[1,2,3,4,5].sum()` → `15`
- [ ] `test_count` — `[1,2,3].count()` → `3`
- [ ] `test_any_all` — `any` / `all` の真偽値検証
- [ ] `test_first_last` — 空リストで `none` を返すことを確認
- [ ] `test_order_by` — ソートの正確性
- [ ] `test_take_skip` — 境界値（0・リスト長超過）
- [ ] `test_distinct` — 重複除去
- [ ] `test_zip` — 長さが異なる場合は短い方に合わせる
- [ ] `test_flat_map` — ネストしたリストの展開
- [ ] `test_method_chain` — `.filter().map().fold()` のチェーン

#### Phase 3 E2E テスト

- [ ] `e2e_collection_pipeline` — フィルタ→マップ→集計のパイプライン
- [ ] `e2e_for_plus_collection` — for 式とコレクションメソッドの組み合わせ
- [ ] `e2e_nested_closures` — ネストしたクロージャ
- [ ] `e2e_range_methods` — 範囲リテラルにメソッドを適用

---

## Phase 4：型チェッカー

### 4-A: 型定義・型推論

- [ ] `forge-compiler/src/typechecker/types.rs` に `Type` enum 定義
- [ ] リテラルからの型推論
- [ ] 関数シグネチャの型検査
- [ ] `T?` の match 網羅性チェック（some/none 両方必須）
- [ ] `T!` の match 網羅性チェック（ok/err 両方必須）
- [ ] 型不一致エラーの生成（行番号付き）

#### Phase 4 単体テスト

- [ ] `test_type_infer_int` — `let x = 42` → `x: number`
- [ ] `test_type_infer_float` — `let x = 3.14` → `x: float`
- [ ] `test_type_check_binop` — `1 + "hello"` → 型エラー
- [ ] `test_type_check_fn_return` — 戻り値型と実際の型の検査
- [ ] `test_type_check_option_match` — none ケースなしで警告
- [ ] `test_type_check_result_match` — err ケースなしで警告

### 4-B: forge check コマンド

- [ ] `forge check file.forge` — 型チェックのみ（実行しない）
- [ ] エラー一覧を行番号付きで出力

#### Phase 4 E2E テスト

- [ ] `e2e_check_no_error` — 正しいコードで exit code 0
- [ ] `e2e_check_type_error` — 型エラーがあるコードで exit code 1、エラーメッセージを出力
- [ ] `e2e_check_match_exhaustion` — match の網羅性エラーを検出

---

## 進捗サマリー

| Phase | 状態 | 完了タスク数 |
|---|---|---|
| Phase 0 | [ ] 未着手 | 0 / 8 |
| Phase 1-A Lexer | [x] 完了   | 14 / 14 |
| Phase 1-B AST | [x] 完了   | 18 / 18 |
| Phase 1-C Parser | [ ] 未着手 | 0 / 24 |
| Phase 2-A Value | [ ] 未着手 | 0 / 9 |
| Phase 2-B Interpreter | [ ] 未着手 | 0 / 24 |
| Phase 2-C Stdlib | [ ] 未着手 | 0 / 8 |
| Phase 2-D CLI | [ ] 未着手 | 0 / 4 |
| Phase 3 Collections | [ ] 未着手 | 0 / 20 |
| Phase 4 TypeChecker | [ ] 未着手 | 0 / 10 |
