# `job` 実装タスク

> 参照: `lang/job/spec.md`, `lang/job/plan.md`
> 進捗: 0/56

---

## JOB-1: 言語拡張

### JOB-1-A: レキサー (0/4)

- [ ] `job` キーワードトークンを追加
- [ ] `input` キーワードトークンを追加
- [ ] `run` キーワードトークンを追加
- [ ] レキサーテスト: `job`, `input`, `run` が正しくトークン化される

### JOB-1-B: AST (0/5)

- [ ] `Stmt::Job { name, inputs, body, span }` を定義
- [ ] `JobInput { name, type_ann, default, span }` を定義
- [ ] `Stmt::RunJob { name, args, span }` を定義
- [ ] AST の Display / Debug 実装
- [ ] AST テスト: ノード構造が正しく構築される

### JOB-1-C: パーサー (0/8)

- [ ] `job PascalName { ... }` のパース
- [ ] `input name: Type` のパース（デフォルトなし）
- [ ] `input name: Type = default` のパース（デフォルトあり）
- [ ] `input name: Type?` のパース（Optional型）
- [ ] `run { ... }` ブロックのパース
- [ ] `run JobName { key: expr, ... }` 実行構文のパース
- [ ] パーサーテスト: 正常ケース
- [ ] パーサーテスト: エラーケース（malformed input 等）

### JOB-1-D: 型チェッカー (0/5)

- [ ] `job` 宣言をスコープに関数型として登録
- [ ] `input` の型アノテーション検証
- [ ] `run { }` ブロック内の式を fn body として型チェック
- [ ] `run JobName { ... }` 実行時の引数型一致検証
- [ ] 型チェッカーテスト: `input` 型不一致エラー

### JOB-1-E: インタープリター (0/6)

- [ ] `job` 宣言を Interpreter の job レジストリに登録
- [ ] `input` の型から CLI/DI の種別を判別（primitive → CLI、trait/struct → DI）
- [ ] `run JobName { ... }` で job を実行
- [ ] `provide` されたインフラを RunContext から自動供給
- [ ] job の戻り値を `Result<Value>` として処理
- [ ] インタープリターテスト: job 実行の基本動作

---

## JOB-2: CLI 拡張

### JOB-2-A: `forge job` コマンド (0/4)

- [ ] `forge-cli` に `job` サブコマンドを追加
- [ ] `jobs/*.forge` を自動スキャンして job 宣言を収集
- [ ] `forge job <name>` で対象 job を実行
- [ ] 存在しない job 名へのエラーメッセージ

### JOB-2-B: CLI オプション自動生成 (0/5)

- [ ] `string` → `--name <value>`（必須オプション）
- [ ] `string?` → `--name <value>`（省略可オプション）
- [ ] `bool` → `--flag / --no-flag`
- [ ] `bool = true` → `--flag / --no-flag`（デフォルト true）
- [ ] `number` → `--count <value>`
- [ ] snake_case → `--kebab-case` 変換

### JOB-2-C: `forge job --list` (0/2)

- [ ] 利用可能な job 一覧を表示
- [ ] job ごとの説明文（コメントから抽出）を表示

---

## JOB-3: RunContext / `runs/` ディレクトリ

### JOB-3-A: RunContext (0/5)

- [ ] `RunContext { run_id, job_name, started_at, finished_at, args, event_queue }` 構造体を定義
- [ ] `run_id` の生成: `YYYY-MM-DDTHH-MM-SS` 形式
- [ ] job 実行前に RunContext を初期化
- [ ] job 完了時に `finished_at` を記録
- [ ] RunContext テスト: run_id フォーマット検証

### JOB-3-B: `runs/` ディレクトリ出力 (0/6)

- [ ] `runs/<run_id>/` ディレクトリを自動作成
- [ ] job 完了時に `result.json` を書き出す（`{ status, started_at, finished_at, args }`）
- [ ] `runs/latest` → 最新 run_id へのポインタファイルを更新
- [ ] `--no-log` フラグで `runs/` 記録を無効化
- [ ] ディレクトリ出力テスト
- [ ] `--no-log` テスト: ディレクトリが作成されないことを確認

### JOB-3-C: `Run.events` API (0/3)

- [ ] `Run.events.count(EventType)` — emit 件数を返す
- [ ] `Run.events.all()` — 全イベントのリスト
- [ ] `Run.id` — 現在の run_id を返す

---

## JOB-4: `--plan / --apply / --dry-run`

### JOB-4-A: `--plan` モード (0/3)

- [ ] RunContext に `dry_run: true` をセット
- [ ] Crucible 書き込み系操作をインターセプトして予定を収集
- [ ] Plan レポートを表示

### JOB-4-B: `--apply` フロー (0/2)

- [ ] `--plan` 内容を表示してから確認を求める
- [ ] `--yes` フラグで対話をスキップ

### JOB-4-C: `input dry_run: bool` との連携 (0/2)

- [ ] `--dry-run` フラグを `dry_run` input に自動的に渡す
- [ ] job 内で `if dry_run { ... }` で副作用をスキップできることを確認

---

## JOB-5: テスト・サンプル

### JOB-5-A: ユニットテスト (0/4)

- [ ] job 宣言のパーステスト（全 input 型パターン）
- [ ] input 型判別テスト（CLI/DI 分類）
- [ ] `run JobName { }` 実行テスト
- [ ] デフォルト値テスト

### JOB-5-B: E2E テスト (0/3)

- [ ] `examples/job-demo/` で `forge job import-users` が動く
- [ ] `runs/` ディレクトリが生成される
- [ ] `result.json` に正しい結果が記録される

### JOB-5-C: サンプル (0/2)

- [ ] `examples/job-demo/jobs/import_users.forge` — CSV インポート job
- [ ] `examples/job-demo/jobs/generate_report.forge` — レポート生成 job
