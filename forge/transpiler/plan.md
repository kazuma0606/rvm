# ForgeScript トランスパイラ 実装計画

---

## フェーズ構成

```
Phase B-1: 基本変換（コア言語機能）
Phase B-2: 型システム（Option / Result / ? 演算子）
Phase B-3: クロージャ（Fn / FnMut / FnOnce 推論）
Phase B-4: コレクション（Vec + イテレータチェーン）
Phase B-5: 複合型（struct / data / enum）       ← 将来
Phase B-6: モジュール / use raw / 外部クレート   ← 将来
Phase B-7: async / await                        ← 将来
```

Phase B-1〜B-4 が完了した時点で、現在の `forge run` で動く ForgeScript プログラムの
大半が `forge build` でもネイティブバイナリになる。

---

## Phase B-1: 基本変換

### 目標
`let` / `fn` / `if` / `for` / `while` / `match` / 文字列補間 / 組み込み関数が
正しく Rust コードに変換されること。

### 成果物
```
forge-transpiler/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── codegen.rs      ← CodeGenerator（AST → Rust 文字列）
    └── builtin.rs      ← 組み込み関数の変換テーブル
```

### 実装ステップ
1. `forge-transpiler` クレートを新規作成、`forge-compiler` を依存に追加
2. `CodeGenerator` 構造体と `generate(module: &Module) -> String` を実装
3. バインディング変換（`let` / `state` / `const`）
4. 型変換（`number`→`i64` / `float`→`f64` / `string`→`String` / `bool`→`bool`）
5. 関数定義変換（`fn` → Rust `fn`、エントリーポイントは `-> Result<(), anyhow::Error>`）
6. `if` / `for` / `while` 変換
7. `match` 変換（`some` → `Some` / `none` → `None` / `ok` → `Ok` / `err` → `Err`）
8. 文字列補間 → `format!(...)`
9. 組み込み関数変換（`println` → `println!` など）
10. スナップショットテスト作成
11. `forge-cli` に `forge transpile` コマンド追加（Rust コード出力のみ）
12. `forge-cli` に `forge build` コマンド追加（`rustc` 呼び出し）

### 検証
- スナップショットテスト（生成コードの文字列比較）
- `forge transpile fixtures/hello.forge` で Rust コードを目視確認
- 生成されたコードを `rustc` でコンパイルして実行できること

---

## Phase B-2: 型システム

### 目標
`T?` / `T!` / `?` 演算子 / `some` / `ok` / `err` / Option・Result メソッドが
正しく変換されること。

### 実装ステップ
1. `T?` → `Option<T>` / `T!` → `Result<T, anyhow::Error>` の型変換
2. `some(x)` → `Some(x)` / `none` → `None` の変換
3. `ok(x)` → `Ok(x)` / `err(msg)` → `Err(anyhow::anyhow!(msg))` の変換
4. `?` 演算子 → Rust の `?` に変換
5. Option / Result メソッド（`is_some` / `unwrap_or` / `map` など）の変換
6. ラウンドトリップテスト（Option / Result を使う E2E テストの転用）

---

## Phase B-3: クロージャ

### 目標
クロージャのキャプチャを解析して `Fn` / `FnMut` / `FnOnce` を推論し、
正しい Rust クロージャに変換されること。

### 実装ステップ
1. クロージャのキャプチャ変数を収集する解析パスを追加
2. キャプチャ変数が `let` のみ → `Fn`（`|x| ...`）
3. キャプチャ変数に `state` が含まれる → `FnMut`（`move |x| ...`）
4. キャプチャ変数が消費される → `FnOnce`（`move |x| ...`）
5. 高階関数に渡すクロージャの型を引数型から逆算する
6. クロージャ変換テスト

---

## Phase B-4: コレクション

### 目標
`list<T>` → `Vec<T>` に変換され、コレクションメソッドが
Rust のイテレータチェーンに正しく変換されること。

### 実装ステップ
1. `[1, 2, 3]` → `vec![1_i64, 2, 3]` のリテラル変換
2. 範囲リテラル `[1..=10]` → `(1..=10).collect::<Vec<i64>>()`
3. `map` / `filter` / `flat_map` → `.iter().map(...).collect()`
4. `fold` / `sum` / `count` / `any` / `all` の変換
5. `first` / `last` / `nth` → `.first()` / `.last()` など
6. `order_by` / `reverse` / `distinct` の変換
7. コレクション変換テスト + ラウンドトリップテスト

---

## Phase B-5 以降（将来）

| Phase | 内容 | 前提 |
|---|---|---|
| B-5 | `struct` / `data` / `enum` / `impl` | 型定義仕様の確定 |
| B-6 | モジュールシステム / `use raw` / 外部クレート | モジュール仕様の確定 |
| B-7 | `async` / `await` / tokio 自動挿入 | 非同期仕様の確定 |
| B-8 | `typestate` / `mixin` / `when` | 独自機能仕様の確定 |

---

## テスト方針

### スナップショットテスト
```
forge-transpiler/tests/snapshots/
  let_binding.forge      ← 入力
  let_binding.rs.snap    ← 期待出力
  fn_definition.forge
  fn_definition.rs.snap
  ...
```

### ラウンドトリップテスト
`forge-cli/tests/e2e.rs` の既存テストを `run_forge` と `run_built` の両方で実行し、
出力が一致することを検証する。

```
forge run  [source] → stdout_a
forge build + run   → stdout_b
assert_eq!(stdout_a, stdout_b)
```

---

## `forge-cli` への統合

```bash
forge transpile src/main.forge          # Rust コードを stdout に出力
forge transpile src/main.forge -o out.rs  # ファイルに出力
forge build src/main.forge              # ビルドして target/forge/main を生成
forge build src/main.forge -o myapp     # 出力バイナリ名を指定
```
