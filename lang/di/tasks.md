# ForgeScript DI タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `container {}` / `@service` / `@repository` / 依存方向チェックが動作し、
>             クリーンアーキテクチャのデモプロジェクトが `forge run` / `forge build` で動くこと

---

## Phase DI-1: 依存方向チェック

### DI-1-A: `forge.toml` パーサー拡張

- [ ] `[architecture]` セクションの読み取り
- [ ] `layers = [...]` の順序付きリストとして解析
- [ ] `[architecture.naming]` セクションの読み取り
- [ ] `naming_rules = "warn" | "error"` オプションの解析

### DI-1-B: 依存グラフ構築

- [ ] 対象プロジェクトの `.forge` ファイルから `use` 宣言を収集
- [ ] ファイルパスをレイヤーに分類（`forge.toml` の `layers` に基づく）
- [ ] 有向グラフとして依存関係を構築

### DI-1-C: 違反検出

- [ ] 上位層から下位層への逆依存を検出してエラー報告
- [ ] 循環依存の検出（Tarjan の強連結成分アルゴリズム）
- [ ] エラーメッセージにファイルパス・依存先・違反ルールを含める

### DI-1-D: 命名規則チェック

- [ ] `[architecture.naming]` の `suffix` リストを型名に照合
- [ ] 違反を warning として報告
- [ ] `naming_rules = "error"` 時はエラーとして報告

### DI-1-E: `forge check` への統合

- [ ] 既存の型チェックに加えてアーキテクチャチェックを実行
- [ ] `--arch-only` フラグの追加

### DI-1-F: テスト

- [ ] テスト: `test_arch_valid` — 依存方向が正しいプロジェクトでエラーなし
- [ ] テスト: `test_arch_violation` — 逆依存でエラー検出
- [ ] テスト: `test_arch_circular` — 循環依存でエラー検出
- [ ] テスト: `test_arch_naming_warn` — 命名規則違反で警告
- [ ] E2E テスト: サンプルプロジェクトで `forge check` が通ること

---

## Phase DI-2: `forge new --template clean-arch`

### DI-2-A: テンプレートファイルの作成

- [ ] `crates/forge-cli/templates/clean-arch/forge.toml` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/main.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/domain/mod.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/domain/user.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/usecase/mod.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/usecase/register_user_usecase.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/interface/mod.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/interface/user_handler.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/infrastructure/mod.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/infrastructure/postgres_user_repository.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/src/infrastructure/smtp_email_service.forge` の作成
- [ ] `crates/forge-cli/templates/clean-arch/tests/register_user_test.forge` の作成

### DI-2-B: `forge-cli` の `new` サブコマンド拡張

- [ ] `--template` フラグの追加
- [ ] テンプレートディレクトリのコピー処理
- [ ] `{{name}}` プレースホルダーの置換処理

### DI-2-C: `--template anvil-clean` の追加

- [ ] `crates/forge-cli/templates/anvil-clean/` の作成
- [ ] `interface/` 層が `AnvilRouter` を使う雛形ファイルの作成

### DI-2-D: テスト

- [ ] テスト: `test_new_clean_arch` — 雛形生成後に `forge check` が通ること
- [ ] E2E テスト: `forge new sample --template clean-arch && forge run src/main.forge` が動くこと

---

## Phase DI-3: `@service` / `@repository` デコレータ

### DI-3-A: Lexer 拡張

- [ ] `@service` トークンの追加
- [ ] `@repository` トークンの追加
- [ ] 既存の `@derive` と統一的に扱えるよう整理

### DI-3-B: AST 拡張

- [ ] `Decorator::Service` を追加
- [ ] `Decorator::Repository` を追加
- [ ] `Decorator::On { event_type: String }` を追加
- [ ] `Decorator::Timed { metric: String }` を追加
- [ ] `Stmt::StructDef` の derives を `decorators: Vec<Decorator>` に拡張

### DI-3-C: パーサー拡張

- [ ] `@service struct Name { ... }` のパース
- [ ] `@repository struct Name { ... }` のパース
- [ ] 複数デコレータの組み合わせのパース（`@service @derive(Debug)`）

### DI-3-D: インタープリタ拡張

- [ ] 型レジストリに `is_service: bool` フラグを追加
- [ ] 型レジストリに `is_repository: bool` フラグを追加
- [ ] デコレータ情報をメタデータとして保持

### DI-3-E: テスト

- [ ] テスト: `test_decorator_service` — `@service` の付与と型レジストリへの登録
- [ ] テスト: `test_decorator_repository` — `@repository` の付与と型レジストリへの登録
- [ ] テスト: `test_decorator_multi` — 複数デコレータの組み合わせ
- [ ] E2E テスト: `decorator_service.forge`

---

## Phase DI-4: `container {}` パーサー + トランスパイル

### DI-4-A: Lexer 拡張

- [ ] `container` キーワードトークンの追加
- [ ] `bind` キーワードトークンの追加
- [ ] `to` キーワードトークンの追加（既存の用途と衝突しないよう確認）

### DI-4-B: AST 拡張

- [ ] `Stmt::ContainerDef { bindings: Vec<Binding> }` を追加
- [ ] `Binding { trait_name: String, implementation: Expr }` を追加

### DI-4-C: パーサー拡張

- [ ] `container { bind Trait to Impl }` のパース
- [ ] `bind Trait to match expr { ... }` のパース（環境切り替え）
- [ ] 複数 `bind` 宣言のパース

### DI-4-D: インタープリタ拡張

- [ ] `ContainerDef` → コンテナレジストリへの登録
- [ ] `@service` struct のインスタンス化時に trait 型をコンテナから解決して自動注入

### DI-4-E: Rust トランスパイル（`crates/forge-compiler`）

- [ ] `container {}` → `struct Container { ... }` の生成
- [ ] `Container::new()` メソッドの生成
- [ ] `@service` struct → `Arc<dyn Trait>` フィールドへの変換
- [ ] `Container::register_<service>()` メソッドの自動生成

### DI-4-F: テスト

- [ ] テスト: `test_container_basic` — `bind` と自動注入の基本動作
- [ ] テスト: `test_container_match` — `match` による環境切り替え
- [ ] テスト: `test_container_multi` — 複数 `bind` の組み合わせ
- [ ] E2E テスト: `container_basic.forge`
- [ ] E2E テスト: `container_env_switch.forge`

---

## Phase DI-5: `typestate` との統合

### DI-5-A: `container` をインライン式として扱えるよう拡張

- [ ] `container { ... }` を `Expr` として扱えるようパーサーを拡張
- [ ] `fn configure(self, c: container) -> Configured` のシグネチャ対応

### DI-5-B: インタープリタ拡張

- [ ] `container` 型の値を `typestate` の `configure` メソッドに渡せるよう対応
- [ ] `Unconfigured` 状態で `start()` を呼んだ場合のランタイムエラー

### DI-5-C: Rust トランスパイル

- [ ] `configure` メソッドが `Container` struct を受け取るコードを生成
- [ ] typestate の状態チェックが Rust の型システムで表現されることを確認

### DI-5-D: テスト

- [ ] テスト: `test_typestate_container_integration` — 正常な configure → start の遷移
- [ ] テスト: `test_typestate_container_unconfigured` — configure なしの start でエラー
- [ ] E2E テスト: `app_typestate_di.forge`

---

## Phase DI-6: `@on` / `@timed` デコレータ

### DI-6-A: `@on` デコレータ

- [ ] パーサーで `@on(EventType)` を認識
- [ ] `container` 初期化時にイベント購読の自動登録コードを生成（トランスパイラ）
- [ ] テスト: `test_on_decorator` — `@on` によるイベントハンドラ登録

### DI-6-B: `@timed` デコレータ

- [ ] パーサーで `@timed(metric: "name")` を認識
- [ ] `Instant::now()` + `.elapsed()` + `metrics.histogram(...)` でラップするコードを生成（トランスパイラ）
- [ ] テスト: `test_timed_decorator` — `@timed` によるメトリクス記録

### DI-6-C: E2E テスト

- [ ] E2E テスト: `event_on_decorator.forge`
- [ ] E2E テスト: `metrics_timed_decorator.forge`

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| DI-1: 依存方向チェック | 0 / 10 |
| DI-2: forge new テンプレート | 0 / 16 |
| DI-3: @service / @repository | 0 / 13 |
| DI-4: container {} | 0 / 15 |
| DI-5: typestate 統合 | 0 / 7 |
| DI-6: @on / @timed | 0 / 6 |
| **合計** | **0 / 67** |
