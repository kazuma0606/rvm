# ForgeScript 規約アプリ 実装ロードマップ

> 参照: `dev/convention-apps.md`
> 方針: 既存仕様を壊さず、`job / event / emit / app` を順番に積み上げる

---

## 現在地（実装済み）

| 機能 | 状態 | 備考 |
|---|---|---|
| `forge/validator` | ✅ 81/81 | `Validator::new().rule().validate_all()` |
| DI / `container { bind X to Y }` | ✅ 67/67 | `@service` / `@repository` / `@on` / `@timed` |
| Pipe演算子 `\|>` | ✅ | `expr \|> fn()` |
| Anvil HTTP | ✅ 120/120 | `@validate` ミドルウェア統合済み |
| Crucible DB | ✅ 118/118 | `Crucible::connect()` |
| Notebook | ✅ 121/121 | `.fnb` 形式 |
| Ember ECS | ✅ 104/104 | ゲームエンジン、WASM対応 |
| DAP デバッガー | ✅ 111/111 | VS Code ブレークポイント |
| Bloom / SSR | ✅ 157/158 | WASM + ホットリロード |
| `extends` / mixin | 🔄 84/102 | 一部残 |
| `generics` | ❌ 0/103 | 未着手 |

---

## Phase JOB-1: `job` キーワード + `forge job` CLI

**ゴール**: `jobs/import_users.forge` に書いた job が `forge job import-users` で実行できる

### 言語（コンパイラ・VM）

- `job`, `input`, `run` をレキサー・パーサーに追加
- AST: `Stmt::Job { name, inputs: Vec<JobInput>, body }`
- `JobInput { name, type_ann, default }` — CLIオプション自動生成の素材
- インタープリター: `job` 宣言を関数として登録、`input` を引数として扱う
- `run JobName { key: val, ... }` 実行構文のパース・評価

### CLI

- `forge job <name>` コマンド追加
- `jobs/*.forge` を自動スキャンして job 一覧を収集
- `input` 宣言から `--path`, `--dry-run` 等の CLI オプションを自動生成
- `forge job --list` で利用可能 job の一覧表示

### 依存: なし（最初に実装）

---

## Phase EVT-1: `event` / `emit` システム

**ゴール**: `emit RowInvalid { ... }` したイベントが `@on(RowInvalid)` ハンドラーで受け取れる

### 言語

- `event` をレキサー・パーサーに追加
- AST: `Stmt::Event { name, fields: Vec<Field> }` — `data` に近い構造
- `emit` をレキサー・パーサーに追加
- AST: `Stmt::Emit { event_name, fields }` — struct リテラルに近い構造
- インタープリター: `emit` 実行時にランタイムのイベントキューに投入
- 既存の `@on(Event)` / `container` を通じてハンドラーを呼び出す

### ランタイム

- `EventQueue`: メモリ上の FIFO キュー
- job 実行後にキューをフラッシュしてハンドラーを順に呼ぶ
- `Run.events.count(RowInvalid)` — 実行中のイベント数を参照できる

### 依存: JOB-1（job の実行モデルの上に乗る）

---

## Phase RUN-1: run_id / event log / `runs/`

**ゴール**: job 実行のたびに `runs/2026-04-24T10-22-31/events.jsonl` が記録される

### ランタイム

- `run_id` を UUID または `YYYY-MM-DDTHH-MM-SS` 形式で生成
- `runs/<run_id>/` ディレクトリを自動作成
- `emit` されたイベントを `events.jsonl` に追記（JSON Lines形式）
- job 完了時に `result.json` を出力（`{ status, started_at, finished_at, args }`）

### CLI

- `forge job <name>` 実行時に自動で `runs/` 記録
- `--no-log` フラグで無効化可能
- `runs/latest/` シンボリックリンク（または最新 run_id のエイリアス）

### 依存: EVT-1（emit がある前提）

---

## Phase APP-1: `app.forge` / 規約ディレクトリ

**ゴール**: `app.forge` を置くと `jobs/`, `events/`, `handlers/` が自動ロードされる

### 言語

- `app` キーワード追加
- AST: `Stmt::App { name, loads: Vec<GlobPattern>, provide_blocks, container_block, wire_blocks }`
- `load validators/*` — glob パターンで複数ファイルを一括インポート
- `provide key = Expr` — インフラサービスをアプリ全体に供給（後述）
- `wire JobName { key: val }` — pluggable サービスの明示的注入宣言
- `container { bind X to Y }` — 差し替え可能サービスのバインド（既存仕様維持）

### CLI

- `forge job` 実行時に `app.forge` を自動検出してロード
- `forge run` / `forge test` でも `app.forge` を起点にする
- `--app production` オプション（将来: `app Production { }` ブロックの切り替え）

### 依存: JOB-1, EVT-1

---

## 設計補足: `provide`（インフラ）と `bind`（pluggable）の分離

> SQS / Lambda パターンから得た洞察。Job の `input` 宣言をどこまで求めるかの設計判断。

### 背景

AWS SQS + Lambda では、メッセージは `{ Action: "CreateUser", payload: {...} }` という形で届き、
Lambda（= job）は DB 接続を引数で受け取らない。DB は環境変数・インフラ設定で与えられる。

ForgeScript の job も同じ区別が有効：

```text
infrastructure（インフラ固定）:
  Crucible DB 接続        → provide で供給、job の input 不要
  EventQueue              → provide で供給、emit が自動的に使う

pluggable（差し替え可能）:
  Notifier（Slack/Null）  → container { bind } + job input で明示
  Report（Html/Memory）   → container { bind } + job input で明示
```

### `provide` の動作

```forge
app Production {
    // インフラ: DB・キューは provide で一度だけ設定
    provide db    = Crucible::connect(env("DATABASE_URL"))
    provide queue = EventQueue::new()

    // pluggable: 差し替え可能なサービスは container で宣言
    container {
        bind Notifier to SlackNotifier::new(env("SLACK_TOKEN"))
        bind Report   to HtmlReport::new("target/report.html")
    }
}
```

`provide` されたものはすべての job から `Crucible::action(...)` 等で直接使える。
job の `input` に `db: Crucible::Connection` を書く必要がない。

```forge
// provide があれば input db 不要
job CreateUser {
    input name:  string
    input email: string

    run {
        Crucible::table("users").insert({ name: name, email: email })
        emit UserCreated { name: name }
    }
}
```

### SQSとの対応

```text
SQS メッセージ:
  { Action: "CreateUser", payload: { name: "Alice", email: "..." } }
      ↓
forge job create-user --name Alice --email ...
      ↓ （または）
emit CreateUser { name: "Alice", email: "..." }  →  @on(CreateUser) job が受け取る
```

`emit` → `@on(Event)` のパスにより、同期 CLI 実行と非同期イベント駆動の両方を同一 job で表現できる。

### `forge explain` での表示

```text
Job: CreateUser
  inputs:  name: string, email: string   ← CLI options
  emits:   UserCreated
  uses db: Crucible (provided by app Production)  ← provide から自動解決
```

依存が隠れずに `forge explain` で可視化できるため、「どこから来たか分からない」問題を回避できる。

---

## Phase CLI-2: `forge job --plan / --apply` + `forge explain`

**ゴール**: `--plan` で副作用プレビュー、`forge explain` でアプリ構造を出力

### `forge job --plan`

- job 実行前に「実行予定」を表示して止まる
- Crucible: `--plan` 時は DryRunDb モードで SQL を発行せず表示だけ
- `emit` されるイベントの一覧を予測表示

### `forge job --apply`

- `--plan` 確認後に実際に実行するフロー
- 危険操作の `require role: / confirm:` はここで対話的に確認

### `forge explain`

- `app.forge` を解析して以下を出力:
  - Jobs: 名前・input・emits・uses
  - Events: 名前・フィールド・handled by
  - Services: 名前・型
  - 依存の供給元（`<- CLI option` / `<- app Production provides`）
- `forge explain --json` で機械可読形式

### 依存: APP-1, RUN-1

---

## Phase CLI-3: `--lineage / --profile / --sample`

**ゴール**: データパイプラインの追跡・デバッグフラグ

- `--lineage`: run_id + input file hash + pipeline steps をレポート
- `--profile`: 各ステップの実行時間を計測
- `--sample N`: 先頭 N 行だけで試し実行
- `--limit N`: 処理行数上限
- `inspect("label")` パイプステップ: 中間データを Notebook / dashboard に出力

### 依存: RUN-1, pipe 実装

---

## Phase SUG-1: シュガー構文（将来・優先度低）

**方針**: canonical syntax に展開できる糖衣として、実需が出てから追加

| 糖衣構文 | 展開先 | 採用条件 |
|---|---|---|
| `validate x with Schema` | `Schema.validate(x)` | job構文が安定したら |
| `on Event as e { }` (top-level) | `@service Handler { @on(Event) fn handle }` | handlers/ 規約が定着したら |
| `expect_event(E, where: fn)` | assert on event log | RUN-1 完了後 |
| `expect_snapshot(x)` | JSON/Notebook diff | Notebook統合後 |
| `\|> validate UserSchema`（括弧なし） | **採用しない** | パーサー曖昧性のため |

---

## 実装順まとめ

```
JOB-1: job キーワード + forge job CLI
  ↓
EVT-1: event / emit / @on との統合
  ↓
RUN-1: run_id / events.jsonl / runs/
  ↓
APP-1: app.forge / load * / wire
  ↓
CLI-2: --plan / --apply / forge explain
  ↓
CLI-3: --lineage / --profile / inspect
  ↓
SUG-1: シュガー構文（実需次第）
```

---

## 各フェーズの成果物とデモ

### JOB-1 完了時
```bash
forge job import-users --path users.csv --dry-run
```

### EVT-1 完了時
```forge
job ImportUsers {
    run {
        for row in csv.read(path) {
            if !UserSchema.validate(row).ok {
                emit RowInvalid { row: row.index, message: "invalid" }
            }
        }
    }
}
```

### RUN-1 完了時
```
runs/2026-04-24T10-22-31/
  events.jsonl   ← RowInvalid × n 件
  result.json    ← { status: "ok", total: 100, failed: 3 }
```

### APP-1 + CLI-2 完了時（MVP デモ）
```bash
forge new app user-importer --recipe csv-import
forge job import-users --path users.csv --plan
forge explain
forge dashboard
```

---

## 既存仕様との関係

| 既存仕様 | 扱い |
|---|---|
| `container { bind X to Y }` | 維持。pluggable サービスの差し替えに使う |
| `@service` / `@on(Event)` | canonical。ハンドラーはこの形式で実装 |
| `v.validate_all(form)` | canonical。`validate x with Y` は将来 sugar |
| `test "..." { }` | canonical。`expect_event` は後から追加 |
| `input db: Crucible::Connection` | **原則不要**。`provide db` で app レベルに移す |

## `provide` と `input` の使い分け

| 種別 | 手段 | Job での書き方 |
|---|---|---|
| DB（Crucible） | `app { provide db = ... }` | input 不要、`Crucible::table(...)` で直接使う |
| EventQueue | `app { provide queue = ... }` | input 不要、`emit` が自動的に使う |
| Notifier | `container { bind Notifier to ... }` | `input notifier: Notifier` で明示 |
| Report | `container { bind Report to ... }` | `input report: Report` で明示 |
| path / flags | — | `input path: string` で CLI オプションとして受け取る |
