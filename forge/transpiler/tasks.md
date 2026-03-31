# ForgeScript トランスパイラ タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `forge build src/main.forge` でネイティブバイナリが生成され、
>             `forge run` と出力が完全一致すること
> **方針**: Phase B-1〜B-4 を順番に実装し、各フェーズ完了後にラウンドトリップテストで検証

---

## Phase B-0: クレート準備

- [x] `forge-transpiler` クレートを新規作成
- [x] `Cargo.toml` に `forge-compiler` を依存として追加
- [x] `forge-transpiler/src/lib.rs` に `pub fn transpile(source: &str) -> Result<String, TranspileError>` を定義
- [x] `forge-cli/Cargo.toml` に `forge-transpiler` を追加
- [x] `forge-cli` に `forge transpile` コマンドを追加（Rust コードを stdout 出力）
- [x] `forge-cli` に `forge build` コマンドを追加（`rustc` 呼び出し・バイナリ生成）

---

## Phase B-1: 基本変換

### B-1-A: CodeGenerator 基盤

- [x] `CodeGenerator` 構造体を定義（`src/codegen.rs`）
- [x] `generate_module(module: &Module) -> String` を実装
- [x] インデント管理ユーティリティ

### B-1-B: バインディング変換

- [x] `let x = expr` → `let x: T = expr;`
- [x] `state x = expr` → `let mut x: T = expr;`
- [x] `const X = expr` → `const X: T = expr;`
- [x] 型注釈あり・なし両方に対応

### B-1-C: 型変換

- [x] `number` → `i64`
- [x] `float` → `f64`
- [x] `string` → `String`
- [x] `bool` → `bool`
- [x] 型注釈の変換（`TypeAnn` → Rust 型文字列）

### B-1-D: 関数定義変換

- [x] `fn name(params) -> RetType { body }` → Rust `fn`
- [x] 戻り値なし（`Unit`）→ 戻り値型省略
- [x] `fn main()` → `fn main() -> Result<(), anyhow::Error>`
- [x] `return expr` → `return expr;`
- [x] 最後の式が戻り値（セミコロンなし）

### B-1-E: 制御フロー変換

- [x] `if expr { } else { }` → Rust `if`
- [x] `if` 式（値を返す）→ Rust の `if` 式
- [x] `else if` チェーン
- [x] `while cond { }` → Rust `while`
- [x] `for x in expr { }` → `for x in &expr { }`

### B-1-F: match 変換

- [x] `match val { pat => expr, }` → Rust `match`
- [x] `some(n) =>` → `Some(n) =>`
- [x] `none =>` → `None =>`
- [x] `ok(v) =>` → `Ok(v) =>`
- [x] `err(e) =>` → `Err(e) =>`
- [x] `_` ワイルドカード
- [x] 範囲パターン（`0..=9 =>`）

### B-1-G: 文字列補間変換

- [x] `"Hello, {name}!"` → `format!("Hello, {}!", name)`
- [x] 複数の補間 `"a={a}, b={b}"` → `format!("a={}, b={}", a, b)`
- [x] 式の補間 `"{a + b}"` → `format!("{}", a + b)`

### B-1-H: 組み込み関数変換

- [x] `print(x)` → `print!("{}", x)`
- [x] `println(x)` → `println!("{}", x)`
- [x] `string(x)` → `x.to_string()`
- [x] `number(x)` → `x.to_string().parse::<i64>()?`
- [x] `float(x)` → `x.to_string().parse::<f64>()?`
- [x] `len(x)` → `x.len()`
- [x] `type_of(x)` → `std::any::type_name_of_val(&x)`

### B-1-I: テスト

- [x] スナップショットテスト: `let_binding`
- [x] スナップショットテスト: `fn_definition`
- [x] スナップショットテスト: `if_expression`
- [x] スナップショットテスト: `for_loop`
- [x] スナップショットテスト: `match_expression`
- [x] スナップショットテスト: `string_interpolation`
- [x] スナップショットテスト: `builtin_functions`
- [x] `forge transpile fixtures/hello.forge` の目視確認
- [x] 生成された Rust コードが `rustc` でコンパイル可能なこと

---

## Phase B-2: 型システム

- [x] `T?` → `Option<T>` の型変換
- [x] `T!` → `Result<T, anyhow::Error>` の型変換
- [x] `some(x)` → `Some(x)`
- [x] `none` → `None`
- [x] `ok(x)` → `Ok(x)`
- [x] `err(msg)` → `Err(anyhow::anyhow!(msg))`
- [x] `?` 演算子 → Rust の `?`
- [x] `x.is_some()` / `x.is_none()` → そのまま
- [x] `x.is_ok()` / `x.is_err()` → そのまま
- [x] `x.unwrap_or(default)` → `x.unwrap_or(default)`
- [x] `x.map(f)` → `x.map(f)`（Option/Result コンテキスト）
- [x] スナップショットテスト: `option_result`
- [x] ラウンドトリップテスト: Option / Result を使う E2E テストの転用

---

## Phase B-3: クロージャ

- [x] キャプチャ変数収集の解析パスを実装
- [x] `x => expr` → `|x: T| expr`（型は型チェッカーから取得）
- [x] `(a, b) => expr` → `|a: T, b: U| expr`
- [x] `() => expr` → `|| expr`
- [x] `let` キャプチャのみ → `Fn`（`|x| ...`）
- [ ] `state` キャプチャあり → `FnMut`（`move |x| ...`）
  <!-- TODO: state キーワード自体は v0.0.1 で実装済み（lexer/parser/interpreter すべて対応）。
       トランスパイラ側でクロージャのキャプチャ解析時に「state 変数を変更しているか」を
       判定して move |x| ... に変換する必要がある。
       現状は全クロージャを Fn として生成するため、state をクロージャ内で再代入する
       コードは forge build でコンパイルエラーになる。
       forge-transpiler/src/codegen.rs の gen_closure() を修正する。 -->
- [ ] 消費キャプチャ → `FnOnce`（`move |x| ...`）
  <!-- TODO: クロージャが変数を「移動（move）」させる場合（spawn等、1回限りの呼び出し）に
       FnOnce に変換する。現時点では ForgeScript に spawn 構文が未実装のため、
       モジュールシステム・async 実装後に合わせて対応する。優先度: 低。 -->
- [x] スナップショットテスト: `closure_fn`
- [x] スナップショットテスト: `closure_fnmut`
- [x] ラウンドトリップテスト: クロージャを使う E2E テストの転用

---

## Phase B-4: コレクション

- [x] `[1, 2, 3]` → `vec![1_i64, 2, 3]`
- [x] `[1..=10]` → `(1_i64..=10).collect::<Vec<_>>()`
- [x] `[0..10]` → `(0_i64..10).collect::<Vec<_>>()`
- [x] `.map(f)` → `.iter().map(f).collect::<Vec<_>>()`
- [x] `.filter(f)` → `.iter().filter(|x| f(*x)).collect::<Vec<_>>()`
- [x] `.flat_map(f)` → `.iter().flat_map(f).collect::<Vec<_>>()`
- [x] `.fold(init, f)` → `.iter().fold(init, f)`
- [x] `.sum()` → `.iter().sum::<i64>()`
- [x] `.count()` → `.len()`
- [x] `.any(f)` → `.iter().any(|x| f(*x))`
- [x] `.all(f)` → `.iter().all(|x| f(*x))`
- [x] `.first()` → `.first().copied()`
- [x] `.last()` → `.last().copied()`
- [x] `.take(n)` → `.iter().take(n).copied().collect::<Vec<_>>()`
- [x] `.skip(n)` → `.iter().skip(n).copied().collect::<Vec<_>>()`
- [x] `.reverse()` → `{ let mut v = x.clone(); v.reverse(); v }`
- [x] `.distinct()` → 重複除去の変換
- [x] `.enumerate()` → `.iter().enumerate()`
- [x] `.zip(other)` → `.iter().zip(other.iter())`
- [x] スナップショットテスト: `list_literal`
- [x] スナップショットテスト: `collection_methods`
- [x] ラウンドトリップテスト: コレクション E2E テストの転用

---

## ラウンドトリップテスト（B-1〜B-4 完了後）

- [x] `run_forge(src)` と `run_built(src)` を比較するヘルパー関数を実装
- [x] 既存 E2E テスト 29 本をラウンドトリップテストとして転用（9本実装・全通過）
  <!-- TODO: 残り20本の内訳と方針：
       - forge check 系（3本）→ forge build はコンパイル成功/失敗で別途検証が必要。
         run_built() が Err を返すかどうかで代替できる可能性あり。要検討。
       - forge repl 系 → forge build の対象外。ラウンドトリップ不要。
       - 型チェック系（型エラー検出）→ forge build 時に rustc がエラーを出す形になるが、
         エラーメッセージが日本語ではなく Rust のエラーになる。別途対応を検討。
       - コレクション高度系（order_by / then_by 等）→ 順次追加可能。
       優先度: 中。B-5（struct/data）実装後に合わせて整理する。 -->
- [ ] 全 E2E テストで `forge run` == `forge build + run` が成立すること

---

## Phase B-5 以降（将来）

- [ ] **B-5**: `struct` / `data` / `enum` / `impl` 変換
- [ ] **B-6**: モジュールシステム / `use ./module` / `use raw {}` / 外部クレート
- [ ] **B-7**: `async` / `await` / tokio 自動挿入
- [ ] **B-8**: `typestate` / `mixin` / `@derive` / `when` / `Validated<T>`
