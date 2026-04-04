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
- [x] 既存 E2E テスト 29 本をラウンドトリップテストとして転用（13本実装・全通過）
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

- [x] `struct Name { field: Type, ... }` → Rust `struct Name { field: Type, ... }`
- [x] `impl Name { fn method(...) }` → Rust `impl Name { fn method(...) }`
- [x] `self` 参照 → Rust `&self` / `&mut self`（`state self` は `&mut self`）
- [x] `Name { field: expr, ... }` インスタンス化 → Rust の struct 初期化構文
- [x] `expr.field` フィールドアクセス → Rust の `.field` アクセス
- [x] スナップショットテスト: `struct_basic`
- [x] スナップショットテスト: `struct_impl`

### B-5-B: @derive 変換

- [x] `@derive(Debug)` → `#[derive(Debug)]`
- [x] `@derive(Clone)` → `#[derive(Clone)]`
- [x] `@derive(Eq)` → `#[derive(PartialEq, Eq)]`
- [x] `@derive(Hash)` → `#[derive(Hash)]`
- [x] `@derive(Ord)` → `#[derive(PartialOrd, Ord)]`
- [x] `@derive(Default)` → `#[derive(Default)]`
- [x] `@derive(Accessor)` → getter/setter メソッドの `impl` ブロックを生成
- [x] `@derive(Singleton)` → `once_cell::sync::Lazy` を使った静的インスタンスを生成
- [x] スナップショットテスト: `struct_derive`

### B-5-C: data 変換

- [x] `data Name { field: Type, ... }` → `#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)] struct Name { ... }`
- [x] `validate` ブロック → `.validate() -> Result<(), String>` メソッドを生成
  - `length(min..max)` → `if self.field.len() < min || self.field.len() > max { return Err(...) }`
  - `email_format` → 簡易正規表現チェック
  - `not_empty` / `alphanumeric` / `contains_digit` / `contains_uppercase` など
- [x] スナップショットテスト: `data_basic`
- [x] スナップショットテスト: `data_validate`

### B-5-D: enum 変換

- [x] `enum Name { Variant }` → Rust `enum Name { Variant }`（Unit バリアント）
- [x] `enum Name { Variant(Type) }` → Rust `enum Name { Variant(Type) }`（Tuple バリアント）
- [x] `enum Name { Variant { field: Type } }` → Rust の名前付きフィールドバリアント
- [x] `Name::Variant` / `Name::Variant(expr)` / `Name::Variant { field: expr }` → Rust 構文
- [x] match パターン内の enum バリアント → Rust のパターンマッチ
- [x] スナップショットテスト: `enum_basic`
- [x] スナップショットテスト: `enum_match`

### B-5-E: trait / mixin 変換

- [x] `trait Name { fn method() -> Type }` → Rust `trait Name { fn method(&self) -> Type; }`
- [x] `trait Name { fn default_method() { body } }` → Rust デフォルト実装
- [x] `mixin Name { fn method() { body } }` → Rust `trait Name { fn method(&self) { body } }`（デフォルト実装のみの trait）
- [x] `impl Trait for Type { ... }` → Rust `impl Trait for Type { ... }`
- [x] `impl Mixin for Type`（本体なし）→ Rust `impl Mixin for Type {}`
- [x] スナップショットテスト: `trait_impl`
- [x] スナップショットテスト: `mixin_impl`

### B-5-F: ラウンドトリップテスト

- [x] ラウンドトリップ: struct の定義・インスタンス化・フィールドアクセス
- [x] ラウンドトリップ: enum の定義・match
- [x] ラウンドトリップ: data の定義・validate

---

## Phase B-6: モジュールシステム変換

> 前提: `forge/modules/` の M-0〜M-7 が完了済み
> 参照: `forge/modules/spec.md`（セクション6: forge build での Rust 変換）

### B-6-A: use 文変換

- [x] `use ./utils/helper.add` → `use crate::utils::helper::add;`
- [x] `use ./utils/helper.{add, subtract}` → `use crate::utils::helper::{add, subtract};`
- [x] `use ./utils/helper.*` → `use crate::utils::helper::*;`
- [x] `use ./utils/helper.add as add_fn` → `use crate::utils::helper::add as add_fn;`
- [x] `use serde.{Serialize}` → `use serde::Serialize;`（外部クレート）
- [x] `pub use helper.{add}` → `pub use helper::{add};`（re-export）
- [x] スナップショットテスト: `use_local`
- [x] スナップショットテスト: `use_external`

### B-6-B: ファイル → Rust mod 変換

- [x] `src/utils/helper.forge` → `src/utils/helper.rs` として出力
- [x] `src/utils/mod.forge` → `src/utils/mod.rs` として出力
- [x] ディレクトリ構造を `mod` ツリーに変換（`mod utils;` を main.rs に自動挿入）
- [x] `pub` キーワードをそのまま Rust に引き継ぎ

### B-6-C: when キーワード変換

- [x] `when platform.linux { ... }` → `#[cfg(target_os = "linux")] ...`
- [x] `when platform.windows { ... }` → `#[cfg(target_os = "windows")] ...`
- [x] `when platform.macos { ... }` → `#[cfg(target_os = "macos")] ...`
- [x] `when feature.xxx { ... }` → `#[cfg(feature = "xxx")] ...`
- [x] `when env.dev { ... }` → `#[cfg(debug_assertions)] ...`
- [x] `when env.prod { ... }` → `#[cfg(not(debug_assertions))] ...`
- [x] `when test { ... }` → `#[cfg(test)] ...`
- [x] `when not feature.xxx { ... }` → `#[cfg(not(feature = "xxx"))] ...`
- [x] スナップショットテスト: `when_platform`
- [x] スナップショットテスト: `when_test`

### B-6-D: use raw {} 変換

- [x] `use raw { ... }` → ブロック内の生 Rust コードをそのまま出力
- [x] スナップショットテスト: `use_raw`

### B-6-E: test ブロック変換

- [x] `test "name" { body }` → `#[cfg(test)] mod tests { #[test] fn test_name() { body } }`
- [x] `assert_eq(a, b)` → `assert_eq!(a, b)`
- [x] `assert(expr)` → `assert!(expr)`
- [x] スナップショットテスト: `test_block`

### B-6-F: ラウンドトリップテスト

- [x] ラウンドトリップ: 複数ファイル構成（main + utils モジュール）
- [x] ラウンドトリップ: pub/非公開の境界

---

## Phase B-7: async / await

> 仕様: `forge/transpiler/spec.md` セクション 13
> 前提: B-0〜B-6 完了済み

### B-7-A: 解析パス

- [ ] 関数本体を走査して `.await` 式を含む関数を収集する解析パスを実装
- [ ] 呼び出しグラフを構築し、async fn を呼び出して `.await` している関数も async に昇格（固定点反復）

### B-7-B: async fn 変換

- [ ] 昇格対象関数を `async fn` として出力
- [ ] `fn main()` が昇格対象なら `#[tokio::main] async fn main()` に変換
- [ ] 通常の async fn には `#[tokio::main]` を付与しない

### B-7-C: Cargo.toml 自動更新

- [ ] `.await` が存在するプロジェクトに `tokio = { version = "1", features = ["full"] }` を自動追加
- [ ] 既に tokio が存在する場合は重複追加しない

### B-7-D: .await 式変換

- [ ] `expr.await` → `expr.await`（Rust 構文そのまま出力）
- [ ] `expr.await?` → `expr.await?`

### B-7-E: async 再帰対応

- [ ] async fn の直接再帰を検出
- [ ] 再帰 async fn を `Box::pin(async move { ... })` 形式に変換
- [ ] スナップショットテスト: `async_recursive`

### B-7-F: test ブロック内 await

- [ ] `.await` を含む test ブロック → `#[tokio::test] async fn test_xxx() -> Result<(), anyhow::Error>`
- [ ] スナップショットテスト: `async_test_block`

### B-7-G: クロージャ内 await の禁止

- [ ] クロージャ本体内で `.await` を検出したらコンパイルエラーを返す
  - エラーメッセージ: `"クロージャ内での .await はサポートされていません"`
- [ ] E2E テスト: `closure_with_await_compile_error`

### B-7-H: forge run フォールバック

- [ ] インタープリタで `Expr::Await { expr }` を `expr` の評価結果をそのまま返す no-op として実装
- [ ] （組み込み非同期関数が追加された時点でブロッキング実装を追加する）

### B-7-I: テスト

- [ ] スナップショットテスト: `async_basic`（単一 .await を持つ関数）
- [ ] スナップショットテスト: `async_propagation`（呼び出し元も async に昇格）
- [ ] スナップショットテスト: `async_tokio_main`（main が async になる）
- [ ] スナップショットテスト: `async_recursive`（Box::pin 自動挿入）
- [ ] スナップショットテスト: `async_test_block`（#[tokio::test]）
- [ ] E2E テスト: `closure_with_await_compile_error`（クロージャ内 await はエラー）

---

## Phase B-8: typestate 変換

> 仕様: `forge/transpiler/spec.md` セクション 14
> 前提: B-5（型定義変換）完了済み

### 制約（必ず守ること）

1. `states:` に列挙する状態は Unit 型のみ（フィールドを持つ状態は**コンパイルエラー**）
2. ジェネリクス付き typestate（`typestate Foo<T>`）は**コンパイルエラー**
3. `@derive` on typestate は**コンパイルエラー**
4. `any {}` ブロックは1つのみ（複数あれば**コンパイルエラー**）
5. コンストラクタ `::new()` は `states:` の最初の状態で生成される

### B-8-A: 制約チェック

- [ ] 状態にフィールドを持つ定義を検出 → エラー: `"typestate の状態は Unit 型のみサポートされます"`
- [ ] ジェネリクス付き typestate を検出 → エラー: `"ジェネリクス付き typestate は未サポートです"`
- [ ] `@derive` on typestate を検出 → エラー: `"typestate への @derive は未サポートです"`
- [ ] `any {}` ブロックが複数 → エラー: `"any ブロックは1つのみ定義できます"`

### B-8-B: 状態マーカー型の生成

- [ ] `states: [A, B, C]` → `struct A; struct B; struct C;` を生成
- [ ] `use std::marker::PhantomData;` を自動挿入

### B-8-C: 本体 struct の生成

- [ ] `typestate Name { fields... }` → `struct Name<S> { fields..., _state: PhantomData<S> }` を生成
- [ ] 初期状態（`states:` 最初）のコンストラクタ `pub fn new(fields...) -> Name<InitialState>` を生成

### B-8-D: 状態別 impl の生成

- [ ] 各状態ブロック `StateA { fn method() }` → `impl Name<StateA> { fn method() }` を生成
- [ ] 遷移メソッド（戻り値型が別状態）: `self` を消費（`fn method(self) -> Name<NextState>`）
- [ ] 参照メソッド（戻り値がプリミティブ / 同状態）: `&self` に変換

### B-8-E: any ブロックの展開

- [ ] `any { fn method() }` → 全状態ごとに同一の `impl Name<StateX> { fn method() }` を生成

### B-8-F: テスト

- [ ] スナップショットテスト: `typestate_basic`（2状態・1遷移）
- [ ] スナップショットテスト: `typestate_transitions`（3状態・複数遷移）
- [ ] スナップショットテスト: `typestate_any_block`（any ブロック展開）
- [ ] E2E テスト: `typestate_constraint_unit_state_error`（Unit 以外の状態はエラー）
- [ ] E2E テスト: `typestate_constraint_generic_error`（ジェネリクスはエラー）
- [ ] ラウンドトリップテスト: `roundtrip_typestate_basic`
