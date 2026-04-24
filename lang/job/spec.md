# ForgeScript `job` 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-25
> 参照: `dev/convention-apps.md`, `dev/convention-roadmap.md`

---

## 概要

`job` は ForgeScript における「意味のある実行単位」。`fn` より上位の概念で、以下の特性を持つ。

- CLI から `forge job <name>` で直接実行できる
- `input` 宣言から CLI オプションを自動生成する
- `emit` でイベントを出し、run_id / event log と統合される
- `--plan / --apply / --dry-run` フラグを標準でサポートする
- `app.forge` の `provide` / `wire` で依存が供給される

---

## 構文

### job 宣言

```forge
job JobName {
    // オプション: 入力宣言（0個以上）
    input field_name: Type
    input field_name: Type = default_value

    // オプション: 制約（将来）
    require role: "admin"
    require env:  "production"
    confirm "本当に実行しますか？"

    // 必須: 実行ブロック
    run {
        // 処理
    }
}
```

### 実行構文

```forge
// app.forge や main.forge から直接実行
run ImportUsers {
    path: "users.csv",
    dry_run: true,
}

// 引数なし job
run GenerateReport
```

---

## `input` 宣言

### 型による自動判別

`input` の型によって、CLI オプションか DI 注入かを自動判別する。

| 型 | 供給元 | 例 |
|---|---|---|
| `string` / `bool` / `number` / `float` | CLI オプション | `--path users.csv` |
| `string?` / `bool?` 等 Option 型 | CLI オプション（省略可） | `--since 2026-01-01` |
| trait / struct 型 | DI（`container` / `wire` から注入） | `input notifier: Notifier` |
| `Crucible::Connection` | **不要**。`provide db` で自動供給 | — |

### デフォルト値

```forge
input dry_run: bool = true    // --dry-run を省略すると true
input limit:   number = 1000  // --limit を省略すると 1000
```

### CLI オプション生成ルール

- `snake_case` の field 名 → `--kebab-case` オプション
- `bool` 型 → `--flag`（true）/ `--no-flag`（false）フラグ
- デフォルトなし必須フィールドは未指定でエラー

```forge
job ImportUsers {
    input path:    string       // → --path <value>（必須）
    input dry_run: bool = true  // → --dry-run / --no-dry-run（省略時 true）
    input limit:   number = 0   // → --limit <value>（省略時 0 = 無制限）
}
```

```bash
forge job import-users --path users.csv
forge job import-users --path users.csv --no-dry-run --limit 500
```

---

## `provide` との統合

`app.forge` で `provide` されたインフラサービスは、job の `input` に書かなくても全 job から使える。

```forge
// app.forge
app Production {
    provide db    = Crucible::connect(env("DATABASE_URL"))
    provide queue = EventQueue::new()
}
```

```forge
// jobs/import_users.forge
job ImportUsers {
    input path: string   // CLIから受け取る

    run {
        // provide db があるので input db 不要
        let rows = Crucible::table("raw_imports").insert_csv(path)
        emit ImportStarted { total: rows.len() }
    }
}
```

---

## `wire` との統合

pluggable サービスは `app.forge` の `wire` で job に接続する。

```forge
// app.forge
app Production {
    provide db = Crucible::connect(env("DATABASE_URL"))

    container {
        bind Notifier to SlackNotifier::new(env("SLACK_TOKEN"))
        bind Report   to HtmlReport::new("target/report.html")
    }

    wire ImportUsers {
        notifier: Notifier
        report:   Report
    }
}
```

```forge
// jobs/import_users.forge
job ImportUsers {
    input path:     string
    input dry_run:  bool = true
    input notifier: Notifier   // wire から注入
    input report:   Report     // wire から注入

    run {
        ...
        notifier.send("import finished")
    }
}
```

---

## `--plan / --apply / --dry-run`

### `--plan`

副作用を実行せず、予定を表示して終了する。

```bash
forge job import-users --path users.csv --plan
```

出力例：

```text
Plan: ImportUsers
  Read CSV:        users.csv (estimated 120 rows)
  Insert users:    120
  Skip invalid:    8
  Emit RowInvalid: 8
  Emit ImportFinished: 1

Run `forge job import-users --apply` to execute.
```

### `--apply`

`--plan` の内容を実際に実行する。危険操作の `confirm` がある場合は対話的に確認する。

```bash
forge job import-users --path users.csv --apply
```

### `--dry-run`

`input dry_run: bool` フィールドが存在する場合、`--dry-run` フラグで `dry_run = true` を渡す。
`--plan` とは異なり、job の `run` ブロックは実行される（job 内で `dry_run` を見て副作用をスキップする）。

---

## エラーハンドリング

`run {}` ブロックから `err(...)` が返った場合、job は失敗として終了する。

```forge
job ImportUsers {
    input path: string

    run {
        let rows = csv.read(path)?          // ? でエラー伝播
        let valid = UserSchema.validate_all(rows)?
        Crucible::table("users").insert_many(valid)
    }
}
```

CLI での終了コード：
- 成功: exit 0
- `err(...)` による失敗: exit 1
- パニック: exit 2

---

## `event` との統合

`run {}` ブロック内で `emit` できる。

```forge
job ImportUsers {
    input path: string

    run {
        let rows = csv.read(path)

        for row in rows {
            match UserSchema.validate(row) {
                ok(_)  => Crucible::table("users").insert(row)
                err(e) => emit RowInvalid {
                    row:     row.index,
                    message: e.message,
                }
            }
        }

        emit ImportFinished {
            total:  rows.len(),
            failed: Run.events.count(RowInvalid),
        }
    }
}
```

---

## run_id / event log との統合

`forge job <name>` の実行ごとに自動的に記録される。

```text
runs/2026-04-25T10-22-31/
  events.jsonl   ← emit されたイベントの JSON Lines
  result.json    ← { status, started_at, finished_at, args, exit_code }
```

`result.json` の例：

```json
{
  "job":         "ImportUsers",
  "run_id":      "2026-04-25T10-22-31",
  "status":      "ok",
  "started_at":  "2026-04-25T10:22:31Z",
  "finished_at": "2026-04-25T10:22:45Z",
  "args":        { "path": "users.csv", "dry_run": false },
  "exit_code":   0
}
```

---

## `forge explain` での表示

```text
Job: ImportUsers

Inputs:
  path:     string              <- CLI option (required)
  dry_run:  bool = true         <- CLI option
  notifier: Notifier            <- app Production (wire)

Emits:
  RowInvalid      (row: int, message: string)
  ImportFinished  (total: int, failed: int)

Uses:
  db: Crucible  <- app Production (provide)
```

---

## 危険操作の宣言（将来）

```forge
job DeleteOldUsers {
    require role: "admin"
    require env:  "production"
    confirm "2年以上前のユーザーを削除します。よろしいですか？"

    run {
        Crucible::table("users")
            .where("created_at < ?", [two_years_ago()])
            .delete()
    }
}
```

CLI での対話：

```text
2年以上前のユーザーを削除します。よろしいですか？
"DeleteOldUsers" と入力して確認:
```

---

## ディレクトリ規約

```text
jobs/
  import_users.forge     ← job ImportUsers { ... }
  generate_report.forge  ← job GenerateReport { ... }
  delete_old_users.forge ← job DeleteOldUsers { ... }
```

- ファイル名は `snake_case`、job 名は `PascalCase`
- 1 ファイル 1 job を推奨（複数定義も可）
- `forge job` は `jobs/*.forge` を自動スキャン

---

## Rust 変換方針

- `job` 宣言 → インタープリターに関数として登録（`fn run_import_users(inputs: JobInputs) -> Result<Value>`）
- `input` → `JobInput { name, type_ann, default, source: Cli | Di }` として AST に保持
- `run {}` → 通常の `fn` body と同様に評価
- `emit` → `interpreter.emit_event(event_name, fields)` を呼び出す
- `Run.events` → インタープリターの `RunContext` から参照

---

## 実装フェーズ（convention-roadmap.md より）

```
JOB-1-A: lexer に job / input / run トークン追加
JOB-1-B: parser に Stmt::Job / JobInput / Stmt::Run 追加
JOB-1-C: interpreter に job 登録・実行を追加
JOB-1-D: forge-cli に forge job コマンド追加
JOB-1-E: input から CLI オプション自動生成
JOB-1-F: forge job --list
```
