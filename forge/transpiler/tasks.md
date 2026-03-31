# ForgeScript トランスパイラ タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `forge build src/main.forge` でネイティブバイナリが生成され、
>             `forge run` と出力が完全一致すること
> **方針**: Phase B-1〜B-4 を順番に実装し、各フェーズ完了後にラウンドトリップテストで検証

---

## Phase B-0: クレート準備

- [ ] `forge-transpiler` クレートを新規作成
- [ ] `Cargo.toml` に `forge-compiler` を依存として追加
- [ ] `forge-transpiler/src/lib.rs` に `pub fn transpile(source: &str) -> Result<String, TranspileError>` を定義
- [ ] `forge-cli/Cargo.toml` に `forge-transpiler` を追加
- [ ] `forge-cli` に `forge transpile` コマンドを追加（Rust コードを stdout 出力）
- [ ] `forge-cli` に `forge build` コマンドを追加（`rustc` 呼び出し・バイナリ生成）

---

## Phase B-1: 基本変換

### B-1-A: CodeGenerator 基盤

- [ ] `CodeGenerator` 構造体を定義（`src/codegen.rs`）
- [ ] `generate_module(module: &Module) -> String` を実装
- [ ] インデント管理ユーティリティ

### B-1-B: バインディング変換

- [ ] `let x = expr` → `let x: T = expr;`
- [ ] `state x = expr` → `let mut x: T = expr;`
- [ ] `const X = expr` → `const X: T = expr;`
- [ ] 型注釈あり・なし両方に対応

### B-1-C: 型変換

- [ ] `number` → `i64`
- [ ] `float` → `f64`
- [ ] `string` → `String`
- [ ] `bool` → `bool`
- [ ] 型注釈の変換（`TypeAnn` → Rust 型文字列）

### B-1-D: 関数定義変換

- [ ] `fn name(params) -> RetType { body }` → Rust `fn`
- [ ] 戻り値なし（`Unit`）→ 戻り値型省略
- [ ] `fn main()` → `fn main() -> Result<(), anyhow::Error>`
- [ ] `return expr` → `return expr;`
- [ ] 最後の式が戻り値（セミコロンなし）

### B-1-E: 制御フロー変換

- [ ] `if expr { } else { }` → Rust `if`
- [ ] `if` 式（値を返す）→ Rust の `if` 式
- [ ] `else if` チェーン
- [ ] `while cond { }` → Rust `while`
- [ ] `for x in expr { }` → `for x in &expr { }`

### B-1-F: match 変換

- [ ] `match val { pat => expr, }` → Rust `match`
- [ ] `some(n) =>` → `Some(n) =>`
- [ ] `none =>` → `None =>`
- [ ] `ok(v) =>` → `Ok(v) =>`
- [ ] `err(e) =>` → `Err(e) =>`
- [ ] `_` ワイルドカード
- [ ] 範囲パターン（`0..=9 =>`）

### B-1-G: 文字列補間変換

- [ ] `"Hello, {name}!"` → `format!("Hello, {}!", name)`
- [ ] 複数の補間 `"a={a}, b={b}"` → `format!("a={}, b={}", a, b)`
- [ ] 式の補間 `"{a + b}"` → `format!("{}", a + b)`

### B-1-H: 組み込み関数変換

- [ ] `print(x)` → `print!("{}", x)`
- [ ] `println(x)` → `println!("{}", x)`
- [ ] `string(x)` → `x.to_string()`
- [ ] `number(x)` → `x.to_string().parse::<i64>()?`
- [ ] `float(x)` → `x.to_string().parse::<f64>()?`
- [ ] `len(x)` → `x.len()`
- [ ] `type_of(x)` → `std::any::type_name_of_val(&x)`

### B-1-I: テスト

- [ ] スナップショットテスト: `let_binding`
- [ ] スナップショットテスト: `fn_definition`
- [ ] スナップショットテスト: `if_expression`
- [ ] スナップショットテスト: `for_loop`
- [ ] スナップショットテスト: `match_expression`
- [ ] スナップショットテスト: `string_interpolation`
- [ ] スナップショットテスト: `builtin_functions`
- [ ] `forge transpile fixtures/hello.forge` の目視確認
- [ ] 生成された Rust コードが `rustc` でコンパイル可能なこと

---

## Phase B-2: 型システム

- [ ] `T?` → `Option<T>` の型変換
- [ ] `T!` → `Result<T, anyhow::Error>` の型変換
- [ ] `some(x)` → `Some(x)`
- [ ] `none` → `None`
- [ ] `ok(x)` → `Ok(x)`
- [ ] `err(msg)` → `Err(anyhow::anyhow!(msg))`
- [ ] `?` 演算子 → Rust の `?`
- [ ] `x.is_some()` / `x.is_none()` → そのまま
- [ ] `x.is_ok()` / `x.is_err()` → そのまま
- [ ] `x.unwrap_or(default)` → `x.unwrap_or(default)`
- [ ] `x.map(f)` → `x.map(f)`
- [ ] スナップショットテスト: `option_result`
- [ ] ラウンドトリップテスト: Option / Result を使う E2E テストの転用

---

## Phase B-3: クロージャ

- [ ] キャプチャ変数収集の解析パスを実装
- [ ] `x => expr` → `|x: T| expr`（型は型チェッカーから取得）
- [ ] `(a, b) => expr` → `|a: T, b: U| expr`
- [ ] `() => expr` → `|| expr`
- [ ] `let` キャプチャのみ → `Fn`（`|x| ...`）
- [ ] `state` キャプチャあり → `FnMut`（`move |x| ...`）
- [ ] 消費キャプチャ → `FnOnce`（`move |x| ...`）
- [ ] スナップショットテスト: `closure_fn`
- [ ] スナップショットテスト: `closure_fnmut`
- [ ] ラウンドトリップテスト: クロージャを使う E2E テストの転用

---

## Phase B-4: コレクション

- [ ] `[1, 2, 3]` → `vec![1_i64, 2, 3]`
- [ ] `[1..=10]` → `(1_i64..=10).collect::<Vec<_>>()`
- [ ] `[0..10]` → `(0_i64..10).collect::<Vec<_>>()`
- [ ] `.map(f)` → `.iter().map(f).collect::<Vec<_>>()`
- [ ] `.filter(f)` → `.iter().filter(|x| f(*x)).collect::<Vec<_>>()`
- [ ] `.flat_map(f)` → `.iter().flat_map(f).collect::<Vec<_>>()`
- [ ] `.fold(init, f)` → `.iter().fold(init, f)`
- [ ] `.sum()` → `.iter().sum::<i64>()`
- [ ] `.count()` → `.len()`
- [ ] `.any(f)` → `.iter().any(|x| f(*x))`
- [ ] `.all(f)` → `.iter().all(|x| f(*x))`
- [ ] `.first()` → `.first().copied()`
- [ ] `.last()` → `.last().copied()`
- [ ] `.take(n)` → `.iter().take(n).copied().collect::<Vec<_>>()`
- [ ] `.skip(n)` → `.iter().skip(n).copied().collect::<Vec<_>>()`
- [ ] `.reverse()` → `{ let mut v = x.clone(); v.reverse(); v }`
- [ ] `.distinct()` → 重複除去の変換
- [ ] `.enumerate()` → `.iter().enumerate()`
- [ ] `.zip(other)` → `.iter().zip(other.iter())`
- [ ] スナップショットテスト: `list_literal`
- [ ] スナップショットテスト: `collection_methods`
- [ ] ラウンドトリップテスト: コレクション E2E テストの転用

---

## ラウンドトリップテスト（B-1〜B-4 完了後）

- [ ] `run_forge(src)` と `run_built(src)` を比較するヘルパー関数を実装
- [ ] 既存 E2E テスト 29 本をラウンドトリップテストとして転用
- [ ] 全 E2E テストで `forge run` == `forge build + run` が成立すること

---

## Phase B-5 以降（将来）

- [ ] **B-5**: `struct` / `data` / `enum` / `impl` 変換
- [ ] **B-6**: モジュールシステム / `use ./module` / `use raw {}` / 外部クレート
- [ ] **B-7**: `async` / `await` / tokio 自動挿入
- [ ] **B-8**: `typestate` / `mixin` / `@derive` / `when` / `Validated<T>`
