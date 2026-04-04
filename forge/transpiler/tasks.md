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

## Phase B-5: struct / data / enum / impl 変換

> 前提: `forge/typedefs/` の型定義実装（T-1〜T-5）が完了済み
> 参照: `forge/typedefs/spec.md`

### B-5-A: struct 変換

- [ ] `struct Name { field: Type, ... }` → Rust `struct Name { field: Type, ... }`
- [ ] `impl Name { fn method(...) }` → Rust `impl Name { fn method(...) }`
- [ ] `self` 参照 → Rust `&self` / `&mut self`（`state self` は `&mut self`）
- [ ] `Name { field: expr, ... }` インスタンス化 → Rust の struct 初期化構文
- [ ] `expr.field` フィールドアクセス → Rust の `.field` アクセス
- [ ] スナップショットテスト: `struct_basic`
- [ ] スナップショットテスト: `struct_impl`

### B-5-B: @derive 変換

- [ ] `@derive(Debug)` → `#[derive(Debug)]`
- [ ] `@derive(Clone)` → `#[derive(Clone)]`
- [ ] `@derive(Eq)` → `#[derive(PartialEq, Eq)]`
- [ ] `@derive(Hash)` → `#[derive(Hash)]`
- [ ] `@derive(Ord)` → `#[derive(PartialOrd, Ord)]`
- [ ] `@derive(Default)` → `#[derive(Default)]`
- [ ] `@derive(Accessor)` → getter/setter メソッドの `impl` ブロックを生成
- [ ] `@derive(Singleton)` → `once_cell::sync::Lazy` を使った静的インスタンスを生成
- [ ] スナップショットテスト: `struct_derive`

### B-5-C: data 変換

- [ ] `data Name { field: Type, ... }` → `#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)] struct Name { ... }`
- [ ] `validate` ブロック → `.validate() -> Result<(), String>` メソッドを生成
  - `length(min..max)` → `if self.field.len() < min || self.field.len() > max { return Err(...) }`
  - `email_format` → 簡易正規表現チェック
  - `not_empty` / `alphanumeric` / `contains_digit` / `contains_uppercase` など
- [ ] スナップショットテスト: `data_basic`
- [ ] スナップショットテスト: `data_validate`

### B-5-D: enum 変換

- [ ] `enum Name { Variant }` → Rust `enum Name { Variant }`（Unit バリアント）
- [ ] `enum Name { Variant(Type) }` → Rust `enum Name { Variant(Type) }`（Tuple バリアント）
- [ ] `enum Name { Variant { field: Type } }` → Rust の名前付きフィールドバリアント
- [ ] `Name::Variant` / `Name::Variant(expr)` / `Name::Variant { field: expr }` → Rust 構文
- [ ] match パターン内の enum バリアント → Rust のパターンマッチ
- [ ] スナップショットテスト: `enum_basic`
- [ ] スナップショットテスト: `enum_match`

### B-5-E: trait / mixin 変換

- [ ] `trait Name { fn method() -> Type }` → Rust `trait Name { fn method(&self) -> Type; }`
- [ ] `trait Name { fn default_method() { body } }` → Rust デフォルト実装
- [ ] `mixin Name { fn method() { body } }` → Rust `trait Name { fn method(&self) { body } }`（デフォルト実装のみの trait）
- [ ] `impl Trait for Type { ... }` → Rust `impl Trait for Type { ... }`
- [ ] `impl Mixin for Type`（本体なし）→ Rust `impl Mixin for Type {}`
- [ ] スナップショットテスト: `trait_impl`
- [ ] スナップショットテスト: `mixin_impl`

### B-5-F: ラウンドトリップテスト

- [ ] ラウンドトリップ: struct の定義・インスタンス化・フィールドアクセス
- [ ] ラウンドトリップ: enum の定義・match
- [ ] ラウンドトリップ: data の定義・validate

---

## Phase B-6: モジュールシステム変換

> 前提: `forge/modules/` の M-0〜M-7 が完了済み
> 参照: `forge/modules/spec.md`（セクション6: forge build での Rust 変換）

### B-6-A: use 文変換

- [ ] `use ./utils/helper.add` → `use crate::utils::helper::add;`
- [ ] `use ./utils/helper.{add, subtract}` → `use crate::utils::helper::{add, subtract};`
- [ ] `use ./utils/helper.*` → `use crate::utils::helper::*;`
- [ ] `use ./utils/helper.add as add_fn` → `use crate::utils::helper::add as add_fn;`
- [ ] `use serde.{Serialize}` → `use serde::Serialize;`（外部クレート）
- [ ] `pub use helper.{add}` → `pub use helper::{add};`（re-export）
- [ ] スナップショットテスト: `use_local`
- [ ] スナップショットテスト: `use_external`

### B-6-B: ファイル → Rust mod 変換

- [ ] `src/utils/helper.forge` → `src/utils/helper.rs` として出力
- [ ] `src/utils/mod.forge` → `src/utils/mod.rs` として出力
- [ ] ディレクトリ構造を `mod` ツリーに変換（`mod utils;` を main.rs に自動挿入）
- [ ] `pub` キーワードをそのまま Rust に引き継ぎ

### B-6-C: when キーワード変換

- [ ] `when platform.linux { ... }` → `#[cfg(target_os = "linux")] ...`
- [ ] `when platform.windows { ... }` → `#[cfg(target_os = "windows")] ...`
- [ ] `when platform.macos { ... }` → `#[cfg(target_os = "macos")] ...`
- [ ] `when feature.xxx { ... }` → `#[cfg(feature = "xxx")] ...`
- [ ] `when env.dev { ... }` → `#[cfg(debug_assertions)] ...`
- [ ] `when env.prod { ... }` → `#[cfg(not(debug_assertions))] ...`
- [ ] `when test { ... }` → `#[cfg(test)] ...`
- [ ] `when not feature.xxx { ... }` → `#[cfg(not(feature = "xxx"))] ...`
- [ ] スナップショットテスト: `when_platform`
- [ ] スナップショットテスト: `when_test`

### B-6-D: use raw {} 変換

- [ ] `use raw { ... }` → ブロック内の生 Rust コードをそのまま出力
- [ ] スナップショットテスト: `use_raw`

### B-6-E: test ブロック変換

- [ ] `test "name" { body }` → `#[cfg(test)] mod tests { #[test] fn test_name() { body } }`
- [ ] `assert_eq(a, b)` → `assert_eq!(a, b)`
- [ ] `assert(expr)` → `assert!(expr)`
- [ ] スナップショットテスト: `test_block`

### B-6-F: ラウンドトリップテスト

- [ ] ラウンドトリップ: 複数ファイル構成（main + utils モジュール）
- [ ] ラウンドトリップ: pub/非公開の境界

---

## Phase B-7 以降（将来）

- [ ] **B-7**: `async` / `await` / tokio 自動挿入
- [ ] **B-8**: `typestate` / `@derive(Singleton)` / `when` の Rust 変換（PhantomData / OnceLock / cfg）
