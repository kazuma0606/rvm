# ForgeScript DI タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `container {}` / `@service` / `@repository` / 依存方向チェックが動作し、
>             クリーンアーキテクチャのデモプロジェクトが `forge run` / `forge build` で動くこと

---

## Phase DI-1: 依存方向チェック

### DI-1-A: `forge.toml` パーサー拡張

- [x] `[architecture]` セクションの読み取り
- [x] `layers = [...]` の順序付きリストとして解析
- [x] `[architecture.naming]` セクションの読み取り
- [x] `naming_rules = "warn" | "error"` オプションの解析

### DI-1-B: 依存グラフ構築

- [x] 対象プロジェクトの `.forge` ファイルから `use` 宣言を収集
- [x] ファイルパスをレイヤーに分類（`forge.toml` の `layers` に基づく）
- [x] 有向グラフとして依存関係を構築

### DI-1-C: 違反検出

- [x] 上位層から下位層への逆依存を検出してエラー報告
- [x] 循環依存の検出（Tarjan の強連結成分アルゴリズム）
- [x] エラーメッセージにファイルパス・依存先・違反ルールを含める

### DI-1-D: 命名規則チェック

- [x] `[architecture.naming]` の `suffix` リストを型名に照合
- [x] 違反を warning として報告
- [x] `naming_rules = "error"` 時はエラーとして報告

### DI-1-E: `forge check` への統合

- [x] 既存の型チェックに加えてアーキテクチャチェックを実行
- [x] `--arch-only` フラグの追加

### DI-1-F: テスト

- [x] テスト: `test_arch_valid` — 依存方向が正しいプロジェクトでエラーなし
- [x] テスト: `test_arch_violation` — 逆依存でエラー検出
- [x] テスト: `test_arch_circular` — 循環依存でエラー検出
- [x] テスト: `test_arch_naming_warn` — 命名規則違反で警告
- [x] E2E テスト: サンプルプロジェクトで `forge check` が通ること

---

## Phase DI-2: `forge new --template clean-arch`

### DI-2-A: テンプレートファイルの作成

- [x] `crates/forge-cli/templates/clean-arch/forge.toml` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/main.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/domain/mod.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/domain/user.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/usecase/mod.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/usecase/register_user_usecase.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/interface/mod.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/interface/user_handler.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/infrastructure/mod.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/infrastructure/postgres_user_repository.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/src/infrastructure/smtp_email_service.forge` の作成
- [x] `crates/forge-cli/templates/clean-arch/tests/register_user_test.forge` の作成

### DI-2-B: `forge-cli` の `new` サブコマンド拡張

- [x] `--template` フラグの追加
- [x] テンプレートディレクトリのコピー処理
- [x] `{{name}}` プレースホルダーの置換処理

### DI-2-C: `--template anvil-clean` の追加

- [x] `crates/forge-cli/templates/anvil-clean/` の作成
- [x] `interface/` 層が `AnvilRouter` を使う雛形ファイルの作成

### DI-2-D: テスト

- [x] テスト: `test_new_clean_arch` — 雛形生成後に `forge check` が通ること
- [x] E2E テスト: `forge new sample --template clean-arch && forge run src/main.forge` が動くこと

---

## Phase DI-3: `@service` / `@repository` デコレータ

### DI-3-A: Lexer 拡張

- [x] `@service` トークンの追加
- [x] `@repository` トークンの追加
- [x] 既存の `@derive` と統一的に扱えるよう整理

### DI-3-B: AST 拡張

- [x] `Decorator::Service` を追加
- [x] `Decorator::Repository` を追加
- [x] `Decorator::On { event_type: String }` を追加
- [x] `Decorator::Timed { metric: String }` を追加
- [x] `Stmt::StructDef` の derives を `decorators: Vec<Decorator>` に拡張

### DI-3-C: パーサー拡張

- [x] `@service struct Name { ... }` のパース
- [x] `@repository struct Name { ... }` のパース
- [x] 複数デコレータの組み合わせのパース（`@service @derive(Debug)`）

### DI-3-D: インタープリタ拡張

- [x] 型レジストリに `is_service: bool` フラグを追加
- [x] 型レジストリに `is_repository: bool` フラグを追加
- [x] デコレータ情報をメタデータとして保持

### DI-3-E: テスト

- [x] テスト: `test_decorator_service` — `@service` の付与と型レジストリへの登録
- [x] テスト: `test_decorator_repository` — `@repository` の付与と型レジストリへの登録
- [x] テスト: `test_decorator_multi` — 複数デコレータの組み合わせ
- [x] E2E テスト: `decorator_service.forge`

---

## Phase DI-4: `container {}` パーサー + トランスパイル

### DI-4-A: Lexer 拡張

- [x] `container` キーワードトークンの追加
- [x] `bind` キーワードトークンの追加
- [x] `to` キーワードトークンの追加（既存の用途と衝突しないよう確認）

### DI-4-B: AST 拡張

- [x] `Stmt::ContainerDef { bindings: Vec<Binding> }` を追加
- [x] `Binding { trait_name: String, implementation: Expr }` を追加

### DI-4-C: パーサー拡張

- [x] `container { bind Trait to Impl }` のパース
- [x] `bind Trait to match expr { ... }` のパース（環境切り替え）
- [x] 複数 `bind` 宣言のパース

### DI-4-D: インタープリタ拡張

- [x] `ContainerDef` → コンテナレジストリへの登録
- [x] `@service` struct のインスタンス化時に trait 型をコンテナから解決して自動注入

### DI-4-E: Rust トランスパイル（`crates/forge-compiler`）

- [x] `container {}` → `struct Container { ... }` の生成
- [x] `Container::new()` メソッドの生成
- [x] `@service` struct → `Arc<dyn Trait>` フィールドへの変換
- [x] `Container::register_<service>()` メソッドの自動生成

### DI-4-F: テスト

- [x] テスト: `test_container_basic` — `bind` と自動注入の基本動作
- [x] テスト: `test_container_match` — `match` による環境切り替え
- [x] テスト: `test_container_multi` — 複数 `bind` の組み合わせ
- [x] E2E テスト: `container_basic.forge`
- [x] E2E テスト: `container_env_switch.forge`

---

## Phase DI-5: `typestate` との統合

### DI-5-A: `container` をインライン式として扱えるよう拡張

- [x] `container { ... }` を `Expr` として扱えるようパーサーを拡張
- [x] `fn configure(self, c: container) -> Configured` のシグネチャ対応

### DI-5-B: インタープリタ拡張

- [x] `container` 型の値を `typestate` の `configure` メソッドに渡せるよう対応
- [x] `Unconfigured` 状態で `start()` を呼んだ場合のランタイムエラー

### DI-5-C: Rust トランスパイル

- [x] `configure` メソッドが `Container` struct を受け取るコードを生成
- [x] typestate の状態チェックが Rust の型システムで表現されることを確認

### DI-5-D: テスト

- [x] テスト: `test_typestate_container_integration` — 正常な configure → start の遷移
- [x] テスト: `test_typestate_container_unconfigured` — configure なしの start でエラー
- [x] E2E テスト: `app_typestate_di.forge`

---

## Phase DI-6: `@on` / `@timed` デコレータ

### DI-6-A: `@on` デコレータ

- [x] パーサーで `@on(EventType)` を認識
- [x] `container` 初期化時にイベント購読の自動登録コードを生成（トランスパイラ）
- [x] テスト: `test_on_decorator` — `@on` によるイベントハンドラ登録

### DI-6-B: `@timed` デコレータ

- [x] パーサーで `@timed(metric: "name")` を認識
- [x] `Instant::now()` + `.elapsed()` + `metrics.histogram(...)` でラップするコードを生成（トランスパイラ）
- [x] テスト: `test_timed_decorator` — `@timed` によるメトリクス記録

### DI-6-C: E2E テスト

- [x] E2E テスト: `event_on_decorator.forge`
- [x] E2E テスト: `metrics_timed_decorator.forge`

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| DI-1: 依存方向チェック | 10 / 10 |
| DI-2: forge new テンプレート | 16 / 16 |
| DI-3: @service / @repository | 13 / 13 |
| DI-4: container {} | 15 / 15 |
| DI-5: typestate 統合 | 7 / 7 |
| DI-6: @on / @timed | 6 / 6 |
| **合計** | **67 / 67** |
