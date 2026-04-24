# `app` / `forge explain` 実装タスク

> 参照: `lang/app/spec.md`, `lang/app/plan.md`
> 進捗: 0/65

---

## APP-1: 言語拡張

### APP-1-A: レキサー (0/5)

- [ ] `app` キーワードトークンを追加
- [ ] `load` キーワードトークンを追加
- [ ] `provide` キーワードトークンを追加
- [ ] `wire` キーワードトークンを追加
- [ ] レキサーテスト: 全キーワードが正しくトークン化される

### APP-1-B: AST (0/7)

- [ ] `Stmt::App { name, loads, provides, container, wires, span }` を定義
- [ ] `ProvideDecl { name, value, span }` を定義
- [ ] `WireDecl { job_name, bindings, span }` を定義（`bindings: Vec<(String, String)>`）
- [ ] `loads: Vec<String>` — glob パターンのリスト
- [ ] 既存 `ContainerBlock` を `Stmt::App` で再利用
- [ ] AST の Display / Debug 実装
- [ ] AST テスト: ノード構造が正しく構築される

### APP-1-C: パーサー (0/8)

- [ ] `app Name { ... }` のパース
- [ ] `load glob/pattern` のパース（文字列またはパスパターン）
- [ ] `provide key = Expr` のパース
- [ ] `wire JobName { key: ServiceName }` のパース
- [ ] `container { ... }` は既存パーサーに委譲
- [ ] 複数 `app` ブロックのパース（同一ファイル内に複数）
- [ ] パーサーテスト: 正常ケース（全構文要素）
- [ ] パーサーテスト: エラーケース

### APP-1-D: 型チェッカー (0/5)

- [ ] `provide` の型を推論して RunContext に登録
- [ ] `wire` の binding が `container { bind }` の対象と一致することを検証
- [ ] `load` のパターンが有効なパスパターンであることを検証
- [ ] 型チェッカーテスト: wire binding 不一致エラー
- [ ] 型チェッカーテスト: 無効な load パターンエラー

### APP-1-E: インタープリター (0/6)

- [ ] `load glob/*` → glob 展開してファイルをロード
- [ ] ロード順: 英数字ソート順を保証
- [ ] `provide key = Expr` → `RunContext.provided` に登録
- [ ] `container { bind }` → 既存 Container と統合
- [ ] `wire JobName { ... }` → job 実行前に `input` の不足分を補完
- [ ] インタープリターテスト: provide/wire の連携動作

---

## APP-2: CLI 統合

### APP-2-A: `app.forge` 自動検出 (0/6)

- [ ] `forge job <name>` 実行時にカレントディレクトリから `app.forge` を検索
- [ ] `forge test` 実行時も `app.forge` を起点にする
- [ ] 上位ディレクトリへの再帰的な検索（`forge.toml` がある階層まで）
- [ ] 見つかった場合は AppConfig を構築して job に適用
- [ ] 見つからない場合は AppConfig なしで job のみ実行
- [ ] 自動検出テスト: ディレクトリ階層をまたいだ検出

### APP-2-B: `--app` フラグ (0/4)

- [ ] `forge job import-users --app production` のパース
- [ ] `app.forge` 内から指定名の `app` ブロックを選択
- [ ] 未指定時のデフォルト選択（唯一なら自動、複数なら `Production` or 最初）
- [ ] `--app` フラグテスト: 環境切り替えが正しく動作する

### APP-2-C: `forge test` コマンド (0/3)

- [ ] `forge test` サブコマンドを追加
- [ ] `--app test` 相当の AppConfig（Test 環境）を自動選択
- [ ] `forge test` テスト: Test 環境の provide/bind が正しく適用される

### APP-2-D: `provide` の job への自動供給 (0/4)

- [ ] AppConfig の `provides` から `RunContext.provided` を構築
- [ ] job の `input` を走査して `provided` のキーと一致するものを補完
- [ ] `wire` で明示的に接続されているものを補完
- [ ] 残った `input` は CLI オプションから補完

---

## APP-3: `forge explain` コマンド

### APP-3-A: 静的解析 (0/5)

- [ ] `app.forge` を解析して `load` パターンから全 `.forge` ファイルを収集
- [ ] 各ファイルから `job`, `event`, `@service + @on` を抽出
- [ ] `provide`, `bind` を抽出
- [ ] 依存関係グラフを構築
- [ ] 静的解析テスト: 各要素が正しく抽出される

### APP-3-B: テキスト出力 (0/6)

- [ ] App 名と環境のヘッダーを出力
- [ ] Jobs セクション: name / inputs（供給元付き）/ emits / uses
- [ ] input 供給元の表示: `<- CLI option` / `<- app Production (provide)` / `<- app Production (wire)`
- [ ] Events セクション: name / fields / handled by
- [ ] Services セクション: provide / pluggable
- [ ] テキスト出力テスト: 全セクションの出力形式確認

### APP-3-C: JSON 出力 (0/4)

- [ ] `forge explain --json` フラグを追加
- [ ] `{ "app", "env", "jobs", "events", "services" }` JSON を出力
- [ ] jobs / events / services の各フィールドを正しく JSON 化
- [ ] JSON 出力テスト: スキーマ整合性確認

---

## APP-4: `forge new --recipe` テンプレート

### APP-4-A: `csv-import` レシピ (0/9)

- [ ] `forge new app <name> --recipe csv-import` コマンドを追加
- [ ] `app.forge` テンプレートを生成
- [ ] `forge.toml` テンプレートを生成
- [ ] `schemas/record.forge` テンプレートを生成
- [ ] `validators/record_schema.forge` テンプレートを生成
- [ ] `jobs/import.forge` テンプレートを生成
- [ ] `events/row_invalid.forge` テンプレートを生成
- [ ] `events/import_finished.forge` テンプレートを生成
- [ ] `handlers/report_errors.forge` テンプレートを生成
- [ ] `fixtures/sample.valid.csv` と `fixtures/sample.invalid.csv` を生成

---

## APP-5: テスト・サンプル

### APP-5-A: ユニットテスト (0/5)

- [ ] `app` 宣言のパーステスト
- [ ] `load` の glob 展開テスト
- [ ] `provide` の RunContext 登録テスト
- [ ] `wire` の input 補完テスト
- [ ] 複数 `app` ブロックの環境切り替えテスト

### APP-5-B: 統合テスト (0/3)

- [ ] `app.forge` を起点にした job 実行（provide 経由の Crucible アクセス）
- [ ] `forge explain` の出力内容検証
- [ ] `forge explain --json` の出力内容検証

### APP-5-C: E2E テスト (0/1)

- [ ] `forge new app --recipe csv-import` で生成したプロジェクトが `forge job import` で動く
