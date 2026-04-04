# ForgeScript モジュールシステム タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `use ./path/module.symbol` でローカルモジュールを分割でき、
>             `forge run` / `forge build` どちらでも動作すること

---

## Phase M-0: ファイル解決基盤

### M-0-A: Lexer 拡張

- [x] `use` キーワードトークンを追加
- [x] `pub` キーワードトークンを追加
- [x] `as` キーワードトークンを追加（エイリアス用）
- [x] `when` キーワードトークンを追加

### M-0-B: AST 拡張

- [x] `Stmt::UseDecl { path: UsePath, symbols: UseSymbols, alias: Option<String> }` を追加
- [x] `UsePath::Local(String)` / `UsePath::External(String)` / `UsePath::Stdlib(String)` を追加
- [x] `UseSymbols::Single(String)` / `UseSymbols::Multiple(...)` / `UseSymbols::All` を追加

### M-0-C: パーサー拡張

- [x] `use ./path/module.symbol` のパース
- [x] `use ./path/module.{sym1, sym2}` のパース
- [x] `use ./path/module.*` のパース
- [x] `use ./path/module.symbol as alias` のパース
- [x] `pub use ...` のパース

### M-0-D: モジュールローダー実装

- [x] `forge-compiler/src/loader/` ディレクトリを作成
- [x] `ModuleLoader` 構造体を実装（プロジェクトルートの保持・ファイル読み込み）
- [x] パス解決: `use ./utils/helper` → `{root}/src/utils/helper.forge`
- [x] パース済み AST のキャッシュ（二重読み込み防止）
- [x] `UseSymbols::All` のワイルドカード展開

### M-0-E: インタープリタ拡張

- [x] `UseDecl` の評価: モジュールファイルを読み込んで評価
- [x] インポートしたシンボルを現在のスコープに束縛
- [x] `as` エイリアスでスコープに束縛
- [x] `UseSymbols::All` で pub シンボル全てをスコープに追加

### M-0-F: テスト

- [x] テスト: `test_use_local_single` — 単一シンボルのインポートと使用
- [x] テスト: `test_use_local_multiple` — 複数シンボルのインポート
- [x] テスト: `test_use_alias` — エイリアス付きインポート
- [x] テスト: `test_use_wildcard` — `use ./module.*` のインポート
- [x] E2E テスト: `modules/basic/` — ローカルモジュールを使った基本的な実行

---

## Phase M-1: pub 可視性

### M-1-A: AST 拡張

- [x] `Stmt::FnDef` に `is_pub: bool` フラグを追加
- [x] `Stmt::Let` / `Stmt::Const` に `is_pub: bool` フラグを追加
- [x] `Stmt::StructDef` / `Stmt::EnumDef` / `Stmt::DataDef` に `is_pub: bool` フラグを追加

### M-1-B: パーサー拡張

- [x] `pub fn ...` のパース
- [x] `pub let ...` / `pub const ...` のパース
- [x] `pub struct ...` / `pub enum ...` / `pub data ...` のパース

### M-1-C: モジュールローダー拡張

- [x] インポート時に `is_pub` フラグを確認
- [x] 非公開シンボルへのアクセスでエラー: `"<symbol> は非公開です（pub キーワードがありません）"`

### M-1-D: テスト

- [x] テスト: `test_pub_import_success` — pub シンボルのインポート成功
- [x] テスト: `test_pub_import_private_error` — 非公開シンボルのインポートでエラー
- [x] E2E テスト: `modules/pub_visibility/` — pub/非公開の境界テスト

---

## Phase M-2: mod.forge サポート・re-export

### M-2-A: モジュールローダー拡張

- [x] `use ./utils` のようにディレクトリ指定時に `utils/mod.forge` の存在を確認
- [x] `mod.forge` がある場合はそれ経由でシンボルを解決
- [x] `mod.forge` がない場合はディレクトリ内の全 pub シンボルをアクセス可能に

### M-2-B: AST 拡張

- [x] `Stmt::PubUse { path: UsePath, symbols: UseSymbols }` — re-export 宣言を追加（既存 `Stmt::UseDecl { is_pub: true, ... }` で実装）

### M-2-C: パーサー拡張

- [x] `pub use helper.{add, subtract}` のパース（mod.forge 内）
- [x] `pub use helper.*` のパース

### M-2-D: インタープリタ拡張

- [x] `PubUse` の評価: re-export シンボルをモジュールの公開 API に追加

### M-2-E: テスト

- [x] テスト: `test_mod_forge_routing` — mod.forge 経由のシンボル解決
- [x] テスト: `test_reexport_chain` — A → mod.forge → B の re-export チェーン
- [x] テスト: `test_reexport_depth_warning` — 3段階超で警告
- [x] E2E テスト: `modules/mod_forge/` — mod.forge を使った公開 API 制御

---

## Phase M-3: 外部クレート

### M-3-A: パーサー拡張

- [x] `use serde` のパース（`./` なし → `UsePath::External`）
- [x] `use reqwest.{Client, Response}` のパース
- [x] `use forge/std/io.read_file` のパース（`UsePath::Stdlib`）

### M-3-B: 依存関係マネージャー実装

- [x] `forge-compiler/src/deps/` ディレクトリを作成
- [x] `DepsManager` 構造体を実装（外部クレート名の収集）
- [x] `forge.toml` の `[dependencies]` への追記（べき等）
- [x] 重複クレートのスキップ

### M-3-C: テスト

- [x] テスト: `test_external_crate_detection` — 外部クレート名の収集
- [x] テスト: `test_cargo_toml_update` — Cargo.toml への追記（重複なし）

---

## Phase M-4: 静的解析

### M-4-A: 依存グラフ構築

- [x] `forge-compiler/src/analysis/` ディレクトリを作成
- [x] `use` 文を収集して有向グラフを構築する `DependencyGraph` 実装

### M-4-B: 循環参照検出

- [x] トポロジカルソートで閉路を検出
- [x] エラーメッセージにパスとファイル・行番号を含める

### M-4-C: 未使用インポート検出

- [x] インポートシンボルの使用状況トラッキング
- [x] 未使用シンボル → 警告メッセージ

### M-4-D: シンボル衝突検出

- [x] 同名シンボルの複数インポート → エラー
- [x] `use *` 衝突 → 警告（エラーではない）

### M-4-E: テスト

- [x] テスト: `test_circular_ref_detection` — 循環参照のエラー検出
- [x] テスト: `test_unused_import_warning` — 未使用インポートの警告
- [x] テスト: `test_symbol_collision_error` — 同名シンボルの衝突エラー
- [x] テスト: `test_wildcard_collision_warning` — `use *` 衝突の警告
- [x] E2E テスト: `modules/circular/` — 循環参照ファイルの実行エラー

---

## Phase M-5: when キーワード

### M-5-A: AST 拡張

- [x] `Stmt::When { condition: WhenCondition, body: Vec<Stmt> }` を追加
- [x] `WhenCondition::Platform(String)` を追加
- [x] `WhenCondition::Feature(String)` を追加
- [x] `WhenCondition::Env(String)` を追加
- [x] `WhenCondition::Test` を追加
- [x] `WhenCondition::Not(Box<WhenCondition>)` を追加

### M-5-B: パーサー拡張

- [x] `when platform.linux { ... }` のパース
- [x] `when feature.debug { ... }` のパース
- [x] `when env.dev { ... }` のパース
- [x] `when test { ... }` のパース
- [x] `when not feature.debug { ... }` のパース

### M-5-C: インタープリタ拡張

- [x] `When` の評価: 実行時の `platform`/`feature`/`env` に基づいてブロックを実行
- [x] `when test` は `forge test` コマンドでのみ実行（`forge run` ではスキップ）

### M-5-D: テスト

- [x] テスト: `test_when_platform` — platform 条件の評価
- [x] テスト: `test_when_test_skipped` — `forge run` では `when test` がスキップ
- [x] テスト: `test_when_not` — `when not` の反転

---

## Phase M-6: use raw {} ブロック

### M-6-A: AST 拡張

- [ ] `Stmt::UseRaw { rust_code: String }` を追加

### M-6-B: パーサー拡張

- [ ] `use raw { ... }` のパース（ブロック内は生文字列として保持）

### M-6-C: インタープリタ拡張

- [ ] `UseRaw` の評価: `forge run` では警告を出してスキップ

### M-6-D: テスト

- [ ] テスト: `test_use_raw_skipped_in_run` — `forge run` でのスキップと警告

---

## Phase M-7: REPL でのモジュールインポート

### M-7-A: REPL 拡張

- [ ] `use ./utils/helper.add` 入力 → ロード・スコープに追加
- [ ] `:modules` コマンド → ロード済みモジュール一覧の表示
- [ ] `:reload utils/helper` コマンド → モジュールの再読み込み
- [ ] `:unload utils/helper` コマンド → アンロード

### M-7-B: テスト

- [ ] テスト: `test_repl_module_load` — REPL でのモジュールロード
- [ ] テスト: `test_repl_module_reload` — `:reload` による再読み込み
