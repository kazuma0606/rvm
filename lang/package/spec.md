# ForgeScript パッケージ管理仕様（forge.toml）

> バージョン対象: v0.3.0
> ステータス: 設計済み・未実装

---

## 1. 設計方針

- `forge.toml` をプロジェクトルートに置く
- `forge build` はエントリポイントを `forge.toml` から読む
- Rust クレートの依存は `use external_name` 検出で自動追記（M-3 の延長）
- マルチパッケージワークスペースは v2 以降

---

## 2. forge.toml フォーマット

```toml
[package]
name    = "anvil"
version = "0.1.0"
forge   = "0.2.0"          # 要求する ForgeScript バージョン
entry   = "src/main.forge"  # エントリポイント（省略時: src/main.forge）

[build]
output  = "target/anvil"    # 出力バイナリパス（省略時: target/<name>）
edition = "2021"            # 生成 Rust の edition（省略時: 2021）

[dependencies]
# use serde.{Serialize} のように外部クレートを use すると自動追記される
# 手動追記も可能
serde       = { version = "1", features = ["derive"] }
tokio       = { version = "1", features = ["full"] }
anyhow      = "1"
```

---

## 3. CLI との統合

```bash
# forge.toml があるディレクトリで実行（エントリポイントを自動解決）
forge build                          # → target/<name> を生成
forge run                            # → インタープリタで実行
forge test                           # → インラインテストを収集・実行
forge transpile                      # → Rust コードを stdout に出力

# 明示的にパスを指定
forge build packages/anvil/          # → forge.toml を読む
forge build packages/anvil/src/main.forge  # → 単一ファイル（forge.toml 不要）
```

---

## 4. 依存解決フロー

```
forge build packages/anvil/
    ↓
1. forge.toml を読む
2. entry = "src/main.forge" を解決
3. use 文を走査して外部クレートを検出（M-3 相当）
4. Cargo.toml を生成（[dependencies] に検出クレートを追記）
5. .forge → .rs 変換（既存 forge-transpiler）
6. cargo build で Rust をコンパイル
7. target/<name> にバイナリを生成
```

---

## 5. 生成される Cargo.toml

`forge build` は以下の `Cargo.toml` を一時ディレクトリに生成してコンパイルする：

```toml
[package]
name    = "anvil"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "anvil"
path = "src/main.rs"   # トランスパイル後の .rs ファイル

[dependencies]
anyhow = "1"
# forge.toml [dependencies] の内容がマージされる
```

---

## 6. ディレクトリ規約

```
packages/anvil/
  forge.toml          ← パッケージマニフェスト
  src/
    main.forge        ← エントリポイント
    *.forge           ← モジュール
  tests/
    *.test.forge      ← コンパニオンテスト（FT-2 実装後）
  target/
    anvil             ← 生成バイナリ（.gitignore 対象）
    Cargo.toml        ← 一時生成（.gitignore 対象）
    src/              ← 一時生成 .rs ファイル（.gitignore 対象）
```

---

## 7. 制約（v1 スコープ）

| 機能 | v1 | 将来 |
|---|---|---|
| 単一パッケージ | ✅ | |
| エントリポイント指定 | ✅ | |
| 外部クレート自動追記 | ✅（M-3 延長）| |
| バイナリ出力先指定 | ✅ | |
| ワークスペース（複数パッケージ）| ❌ | ✅ |
| lib クレート出力 | ❌ | ✅ |
| features フラグ | ❌ | ✅ |
| publish（crates.io）| ❌ | ✅ |
