# ForgeScript アプリケーション素案集

> ForgeScript の強みが活きるアプリケーションのアイデアをまとめたドキュメント。
> 各案は「どの言語機能を活用するか」を明示し、実装着手の判断基準とする。

---

## 凡例

| 記号 | 意味 |
|---|---|
| 🌐 | Web / API |
| 🛠️ | CLI / 開発ツール |
| ⚙️ | 業務ロジック / バックエンド |
| 📦 | ライブラリ / フレームワーク |

---

## 1. forge-todo — CLI タスクマネージャ 🛠️

**概要**: コマンドラインで動く Todo 管理ツール。`~/.forge-todo.json` に永続化。

**活用する言語機能**:
- `data` 型 + `validate` ブロック（タスクの不変条件保証）
- `typestate`（`Pending → InProgress → Done` の状態遷移）
- `forge run` によるスクリプト感覚での実行

```forge
data Task {
    id:      string
    title:   string
    status:  TaskStatus

    validate {
        title.len() > 0,
        title.len() <= 200,
    }
}

typestate TaskStatus {
    Pending -> InProgress -> Done
    Pending -> Cancelled
}
```

**コマンド例**:
```bash
forge run todo.forge add "ドキュメントを書く"
forge run todo.forge list
forge run todo.forge done 3
```

---

## 2. anvil-rest-template — REST API スターターキット 🌐

**概要**: Anvil を使った本番運用可能な REST API のテンプレートプロジェクト。
`forge new --template rest-api` で一発生成できる形を目指す。

**活用する言語機能**:
- Anvil（ルーティング・ミドルウェア・CORS）
- `data` 型（リクエスト/レスポンスの型安全なバリデーション）
- `forge/std/json.{ parse }`
- `forge build` → Rust バイナリとして配布

```forge
data CreateUserRequest {
    name:  string
    email: string

    validate {
        name.len() >= 2,
        email.contains("@"),
    }
}

fn create_user(req: Request<string>) -> Response<string>! {
    let body = parse(req.raw_body)?
    let input = CreateUserRequest::from(body)?
    // ...
    ok(Response::json(user))
}
```

---

## 3. forge-env — 環境変数バリデータ ⚙️

**概要**: `.env` ファイルや環境変数を読み込み、スキーマに対して型検証・必須チェックを行う CLI ツール。
12-factor app の設計で欠かせない「起動時バリデーション」をForgeで表現する。

**活用する言語機能**:
- `data` 型 + `validate` ブロック（設定スキーマの宣言的な定義）
- `T!` 型（エラーを型として扱う）
- `forge check` / `forge run` のどちらでも動作

```forge
data AppConfig {
    database_url: string
    port:         number
    log_level:    string

    validate {
        port >= 1024,
        port <= 65535,
        ["debug", "info", "warn", "error"].contains(log_level),
    }
}

fn load_config() -> AppConfig! {
    let url   = env("DATABASE_URL")?
    let port  = number(env("PORT")?)?
    let level = env("LOG_LEVEL") |> or("info")
    ok(AppConfig { database_url: url, port: port, log_level: level })
}
```

---

## 4. forge-pipeline — データ変換パイプライン CLI ⚙️

**概要**: CSV / JSON を読み込んでフィルタ・変換・集計し、別形式で出力する ETL ツール。
`jq` の ForgeScript 版に近いイメージ。

**活用する言語機能**:
- コレクション API（`map` / `filter` / `fold` / `order_by` / `group_by`）
- パイプライン演算子 `|>`
- `forge run` によるスクリプト実行

```forge
let result = read_csv("sales.csv")
    |> map(fn(row) { parse_row(row) })
    |> filter(fn(r) { r.amount > 1000 })
    |> group_by(fn(r) { r.region })
    |> map(fn(g) { { region: g.key, total: g.values |> fold(0, fn(acc, r) { acc + r.amount }) } })
    |> order_by(fn(g) { -g.total })

write_json("report.json", result)
```

---

## 5. forge-webhook — Webhook レシーバー 🌐

**概要**: GitHub / Stripe / Slack 等のWebhookを受信し、イベント種別に応じて処理を振り分けるサーバ。
Anvil のルーティング + パターンマッチで宣言的に記述できる。

**活用する言語機能**:
- Anvil（HTTP サーバ・ミドルウェア）
- `match` による enum パターンマッチ
- `data` 型（Webhook ペイロードの型定義）

```forge
enum GitHubEvent {
    PushEvent    { ref: string, commits: list<Commit> }
    PullRequest  { action: string, number: number }
    Issues       { action: string, title: string }
}

fn webhook_handler(req: Request<string>) -> Response<string>! {
    let event = parse_github_event(req)?
    match event {
        PushEvent { ref, commits }      => handle_push(ref, commits),
        PullRequest { action, number }  => handle_pr(action, number),
        Issues { action, title }        => handle_issue(action, title),
    }
}
```

---

## 6. forge-cron — ジョブスケジューラ ⚙️

**概要**: cron 式で定義したジョブを定期実行するデーモンプロセス。
systemd / Docker と組み合わせてバックグラウンドサービスとして動作させる。

**活用する言語機能**:
- `typestate`（ジョブの `Scheduled → Running → Succeeded / Failed` 遷移）
- `forge build` → 単一バイナリとして systemd サービス化
- エラー伝播（`?` 演算子）

```forge
typestate JobState {
    Scheduled -> Running -> Succeeded
    Running   -> Failed
    Failed    -> Scheduled   // リトライ
}

data Job {
    name:     string
    schedule: string   // cron 式
    command:  string

    validate {
        name.len() > 0,
        is_valid_cron(schedule),
    }
}
```

---

## 7. forge-config — 設定ファイル DSL ⚙️

**概要**: YAML / TOML の代替として、ForgeScript 自体を設定言語として使う仕組み。
`forge run config.forge` で設定オブジェクトを生成し、アプリに渡す。
Dhall / CUE に近いコンセプト。

**活用する言語機能**:
- `data` 型（スキーマ定義 + バリデーション）
- 変数・関数による DRY な設定記述
- `forge check` による静的検証

```forge
let base_db = {
    host:    "localhost",
    port:    5432,
    pool:    10,
}

let production = {
    ...base_db,
    host:    "db.prod.example.com",
    pool:    50,
    ssl:     true,
}

let config = AppConfig {
    env:      "production",
    database: production,
    log:      "warn",
}
```

---

## 8. forge-audit — ログ解析 / セキュリティ監査ツール 🛠️

**概要**: アクセスログや認証ログを読み込み、異常なパターン（連続失敗・短時間の大量リクエスト等）を検出して報告する CLI ツール。

**活用する言語機能**:
- コレクション API（`group_by` / `filter` / `fold`）
- `data` 型（ログエントリのスキーマ）
- `forge run` によるスクリプト実行（cron で定期実行）

```forge
let anomalies = read_lines("auth.log")
    |> map(parse_log_entry)
    |> filter(fn(e) { e.result == "FAILED" })
    |> group_by(fn(e) { e.ip })
    |> filter(fn(g) { g.values.len() >= 10 })   // 10回以上失敗

anomalies |> each(fn(g) {
    println("[ALERT] {g.key} : {g.values.len()} failures")
})
```

---

## 9. forge-migrate — DB マイグレーション管理ツール ⚙️

**概要**: SQL マイグレーションファイルをバージョン管理し、適用・ロールバックを安全に行う CLI ツール。
`typestate` でマイグレーション状態を型レベルで保証する。

**活用する言語機能**:
- `typestate`（`Pending → Applied → RolledBack`）
- `data` 型（マイグレーションのメタデータ）
- `forge build` → CI/CD パイプラインに組み込み可能なバイナリ

---

## 10. forge-notify — 通知ルーター ⚙️🌐

**概要**: Slack / メール / LINE / PagerDuty 等の複数通知チャネルへのルーティングを一元管理するサービス。
ルールを ForgeScript で宣言的に記述できる。

**活用する言語機能**:
- Anvil（Webhook エンドポイントで通知を受信）
- `match` / enum（チャネル振り分け）
- ミドルウェア（認証・レート制限）

```forge
fn route(event: NotifyEvent) -> list<Channel>! {
    match event.severity {
        "critical" => ok([Slack("oncall"), PagerDuty("p1")]),
        "warning"  => ok([Slack("alerts")]),
        _          => ok([Slack("general")]),
    }
}
```

---

## 11. forge-tutorial — インタラクティブ言語チュートリアル 🛠️

**概要**: `.fnb`（ForgeScript Notebook）形式で書かれた公式チュートリアル集。
コードを読むだけでなく、その場で書き換えて実行できる。言語の採用障壁を下げる最重要コンテンツ。

**活用する言語機能**:
- ノートブック `.fnb` 形式（`display()` / セル実行）
- 言語機能の全範囲（let / fn / match / typestate / data 等）
- `forge check` によるセル内エラー表示

**想定コンテンツ**:
```
tutorials/
  01_hello_world.fnb      ← print / println / 変数
  02_types.fnb            ← T? / T! / match
  03_functions.fnb        ← fn / クロージャ / |>
  04_collections.fnb      ← list / map / filter / fold
  05_structs.fnb          ← struct / impl / trait
  06_data_types.fnb       ← data / validate
  07_typestate.fnb        ← typestate / 状態遷移
  08_modules.fnb          ← use / pub / mod.forge
  09_anvil_basics.fnb     ← HTTP サーバー入門
  10_error_handling.fnb   ← ? 演算子 / Result 型
```

**推奨度**: ★★★（`.fnb` 実装後に最初に作るべきコンテンツ）

---

## 12. forge-notebook-gallery — ノートブックギャラリー 🛠️⚙️

**概要**: ForgeScript で書かれた `.fnb` ノートブックのサンプル集。
データ処理・API連携・アルゴリズム可視化など、実用的なデモを揃えることで言語の表現力を示す。

**例**:
```
gallery/
  data_pipeline.fnb       ← CSV 読み込み・加工・集計
  api_client.fnb          ← forge-http で REST API を叩く
  sorting_algorithms.fnb  ← ソートアルゴリズムの比較
  state_machine.fnb       ← typestate の動的可視化
```

---

## 優先度マトリクス

| アプリ | 実装コスト | 言語機能デモ効果 | 実用性 | 推奨度 |
|---|---|---|---|---|
| forge-todo | 低 | typestate / data | ◯ | ★★★ |
| anvil-rest-template | 低 | Anvil フル活用 | ◎ | ★★★ |
| forge-env | 低 | data + validate | ◎ | ★★★ |
| forge-tutorial | 低（.fnb実装後） | 言語全機能 | ◎ | ★★★ |
| forge-pipeline | 中 | コレクション API | ◯ | ★★ |
| forge-webhook | 中 | Anvil + match | ◎ | ★★ |
| forge-cron | 中 | typestate | ◯ | ★★ |
| forge-config | 中 | DSL 表現力 | ◯ | ★★ |
| forge-audit | 低 | コレクション API | ◯ | ★★ |
| forge-notify | 中 | Anvil + match | ◎ | ★★ |
| forge-notebook-gallery | 低（.fnb実装後） | ノートブック活用 | ◯ | ★★ |
| forge-migrate | 高 | typestate | ◎ | ★ |
