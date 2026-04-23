# ForgeScript 規約付きアプリ構想

> 状態: アイディア整理
>
> テーマ: ForgeScript を「柔軟な部品箱」ではなく、「検証・ジョブ・イベント・レポートを規約でまとめる小型アプリ実行環境」として育てる。

---

## 1. 背景

ForgeScript は Rust エコシステムの上に立つ言語だが、単に「Rust より書きやすいスクリプト」を目指すだけでは弱い。

Rust は低レイヤ・基盤・高速処理に強い。一方で、業務フロー、データ検証、自動化、通知、レポート生成、Notebook 確認、WASM 共有などの「小さなアプリ」を Rust で書くと、配線や設計判断が重くなりやすい。

ForgeScript が狙うべき価値は、Rails が Web アプリに対して提供したような「迷う場所を減らす規約」を、Web だけでなく小型アプリ全般に提供すること。

```text
Rails:
  Web アプリの標準形を規約で提供する

ForgeScript:
  検証・ジョブ・イベント・外部連携・レポートの標準形を規約で提供する
```

---

## 2. 中心思想

ForgeScript は柔軟さよりも規約を重視する。

```text
柔軟な部品箱ではなく、
よくある小型アプリの形を最初から持った言語。
```

重要なのは、何でもできる DI コンテナや過剰な抽象化ではなく、よくある処理の流れを最短で安全に書けること。

```text
データを読む
検証する
ジョブとして実行する
イベントを出す
ハンドラが反応する
必要なら外部サービスや DB に明示的に触る
実行結果をログ・Notebook・Web に残す
```

この標準形を言語と CLI の規約として持つ。

---

## 3. 主要概念

### validator

データが正しいかを扱う機能。

既に ForgeScript における重要機能として定義済み。

役割:

- 入力データの形と制約を表現する
- CSV / JSON / DB レコード / API レスポンスを検証する
- ジョブの安全性を支える
- イベントやレポートの元になる

### job

意味のある実行単位。

普通の関数よりも「CLI / Notebook / Web UI / スケジューラから実行される処理」として扱える。

```forge
job ImportUsers {
    input path: string
    input dry_run: bool = true

    run {
        let rows = csv.read(path)
        let result = UserSchema.validate_all(rows)
        return result.summary()
    }
}
```

嬉しい点:

- `ImportUsers` や `GenerateReport` のように処理に名前が付く
- `input` から CLI オプションや UI フォームを生成できる
- retry / timeout / dry-run / plan / progress / report を共通で乗せられる
- テストしやすい
- 実行履歴を残しやすい

### event

何かが起きたことを型付きで流す仕組み。

```forge
event RowInvalid {
    row: int
    field: string
    message: string
}

emit RowInvalid {
    row: 42,
    field: "email",
    message: "invalid email",
}
```

嬉しい点:

- 主処理と副作用を分離できる
- ログ、通知、レポート生成、UI 更新を後付けできる
- テストで「このイベントが出たか」を検証できる
- Notebook / dashboard / event log に流用できる

### handler

イベントへの反応。

```forge
@service
struct RowInvalidReportHandler {
    report: Report
}

impl RowInvalidReportHandler {
    @on(RowInvalid)
    fn handle(self, e: RowInvalid) -> unit! {
        self.report.add_error(e.row, e.message)
    }
}
```

規約上、`jobs/` は主処理、`handlers/` は副作用の置き場にする。

### service

外部機能や共有リソースを名前付きで初期化する仕組み。

ただし、暗黙注入のための DI コンテナにはしない。

```forge
service report {
    create {
        HtmlReport::new("target/report.html")
    }
}
```

役割:

- Slack / Mail / HTTP client / report / storage / cache などの初期化を整理する
- テスト時の差し替えポイントにする
- アプリが依存する外部機能を見える化する

避けるべきこと:

```forge
job ImportUsers {
    run {
        db.insert_many("users", rows) // db がどこから来たか分からない
    }
}
```

ジョブの依存は `input` や `app.forge` に明示する。

---

## 4. Crucible との関係

DB は副作用が大きいため、Crucible は明示的に接続する方向が良い。

```forge
fn main() {
    let db = Crucible::connect(env("DATABASE_URL"))

    run ImportUsers {
        db: db,
        path: "users.csv",
    }
}
```

ジョブ側も明示的に受け取る。

```forge
job ImportUsers {
    input db: Crucible::Connection
    input path: string

    run {
        let rows = csv.read(path)
        match UserSchema.validate_all(rows) {
            ok(_) => db.insert_many("users", rows)
            err(errors) => return err(errors)
        }
    }
}
```

方針:

```text
DB:
  Crucible::connect(...) で明示的に作る
  job/input に明示的に渡す

service:
  DB 接続を隠すものではなく、外部リソース初期化を整理するもの

job:
  DB 接続や service を input として明示的に受け取る

event:
  DB 更新後の通知・ログ・レポートなどを分離する
```

---

## 5. 規約ディレクトリ

ForgeScript アプリは以下のような構成を標準にする。

```text
forge.toml
app.forge
schemas/
validators/
jobs/
events/
handlers/
services/
fixtures/
tests/
notebooks/
runs/
```

役割:

```text
app.forge:
  アプリの入口、読み込み規約、環境、標準サービスの宣言

validators/:
  データ検証

jobs/:
  実行単位

events/:
  型付きイベント

handlers/:
  イベントへの反応、副作用

services/:
  外部機能や共有リソースの初期化

fixtures/:
  テスト・デモ用データ

runs/:
  実行履歴、イベントログ、レポート
```

---

## 6. app.forge

Rails の `config/application.rb` や `routes.rb` に近い、アプリ全体の入口。

例:

```forge
app UserImporter {
    load validators/*
    load jobs/*
    load events/*
    load handlers/*
    load services/*
}
```

目的:

- アプリの全体像を一箇所で見えるようにする
- 規約読み込みの入口にする
- 環境設定や標準サービスをまとめる

---

## 7. CLI 規約

`jobs/*.forge` にある `job` は自動発見され、CLI から直接実行できる。

```bash
forge job import-users --path users.csv --dry-run
forge job sync-orders --since 2026-04-01
forge job generate-report
```

`input` から CLI オプションを自動生成する。

```forge
job ImportUsers {
    input path: string
    input dry_run: bool = true
}
```

将来的なコマンド:

```bash
forge explain
forge dashboard
forge generate job import-users
forge generate event row-invalid
forge generate validator user
forge job import-users --plan
forge job import-users --apply
```

---

## 8. plan / apply / dry-run

業務スクリプトや DB 更新では dry-run が重要。

ForgeScript では job の標準機能として持つと良い。

```bash
forge job import-users --plan
forge job import-users --apply
```

`--plan` は副作用を実行せず、予定を表示する。

```text
Plan:
  Insert users: 120
  Update users: 34
  Skip invalid rows: 8
  Emit RowInvalid: 8
```

`--apply` で実行する。

Crucible と組み合わせることで、安全な DB 更新フローになる。

---

## 9. run id / event log / audit trail

ジョブ実行には必ず `run_id` を持たせる。

```text
runs/2026-04-23T10-22-31/
  events.jsonl
  result.json
  report.fnb
```

イベントログ例:

```json
{"event":"RowInvalid","row":12,"message":"email is invalid"}
{"event":"UserImported","id":"u_123"}
{"event":"ImportFinished","total":100,"failed":3}
```

標準で持ちたい情報:

```text
Run.id
Run.started_at
Run.env
Run.args
Run.events
Run.status
```

嬉しい点:

- 実行履歴を追える
- Notebook / dashboard / Web UI に流せる
- テストで検証できる
- retry / resume の土台になる
- 「いつ、誰が、何を、どの入力で実行したか」が残る

---

## 10. Notebook report

Forge Notebook を単なる実験環境ではなく、ジョブ実行結果の標準レポートにする。

```bash
forge job import-users --path users.csv --report notebook
```

出力:

```text
runs/latest/report.fnb
```

内容:

- 入力ファイル
- 検証エラー一覧
- イベントタイムライン
- DB 更新予定
- 実行ログ
- グラフや表

ForgeScript の強みである Notebook / WASM / CLI の横断に繋がる。

---

## 11. forge explain

規約アプリでは、コードからアプリ構造を説明できると強い。

```bash
forge explain
```

出力例:

```text
App: user-importer

Jobs:
  ImportUsers(path: string, dry_run: bool = true)
    emits RowInvalid, UserImported, ImportFinished
    uses Crucible::Connection, HtmlReport

Events:
  RowInvalid(row: int, message: string)
    handled by handlers/report_errors.forge

Services:
  report: HtmlReport
```

これは ForgeScript コードを「実行できる仕様書」に近づける。

---

## 12. テスト規約

### fixtures

```text
fixtures/users.valid.csv
fixtures/users.invalid.csv
fixtures/orders.json
```

```forge
test "ImportUsers rejects invalid users" {
    let result = run ImportUsers with fixture("users.invalid")
    assert result.failed == 12
}
```

### event contract test

```forge
test "ImportUsers emits invalid rows" {
    let run = run ImportUsers(path: fixture("invalid.csv"))

    expect_event(RowInvalid, where: fn(e) {
        e.row == 12 && e.field == "email"
    })

    expect_event(ImportFinished, where: fn(e) {
        e.failed > 0
    })
}
```

### snapshot

Notebook、JSON 出力、HTML レポート、イベントログを snapshot test できるとよい。

```forge
test "ImportUsers report" {
    let report = run ImportUsers(path: fixture("users.invalid"))
    expect_snapshot(report)
}
```

---

## 13. 危険操作の規約

本番更新や削除系ジョブには確認や権限を標準で付けられるとよい。

```forge
job DeleteOldUsers {
    require role: "admin"
    require env: "production"
    confirm "Delete users older than 2 years?"

    run {
        ...
    }
}
```

CLI では:

```text
Delete users older than 2 years?
Type "DeleteOldUsers" to continue:
```

ForgeScript が「安全な業務スクリプト」を目指すなら重要。

---

## 14. 既存言語・フレームワークとの位置づけ

### Rails / Laravel / Spring / .NET

これらは service / job / event / DI / handler をフレームワークとして持つ。

ただし言語本体ではなく、規約や annotation、container、framework の仕組み。

ForgeScript はこれを言語・CLI・規約として小さく持てる可能性がある。

### Ballerina

`service` が言語構文として存在し、ネットワークサービスに強い。

ForgeScript の service 構文を考える上で参考になる。

### Elixir / Erlang / Pony

メッセージ・Actor・イベント駆動が言語/VMに深い。

ForgeScript の event / handler / runtime 設計の参考になる。

### Temporal / Cloudflare Workers / Encore

job / workflow / queue / scheduled task / service を実行基盤として持つ。

ForgeScript の job 実行モデル、run id、event log、dashboard の参考になる。

---

## 15. ForgeScript の独自ポジション

Rust:

```text
堅牢な基盤・ランタイム・高速処理を書く
```

ForgeScript:

```text
その基盤の上で、検証・ジョブ・イベント・レポートを持つ小型アプリを書く
```

Rails:

```text
Web アプリの標準形
```

ForgeScript:

```text
検証・自動化・外部連携・ジョブ・レポートの標準形
```

言い換えると:

```text
ForgeScript は、規約優先の小型アプリ実行環境。
```

---

## 16. MVP 案

最初の実装順:

1. `job` 構文
2. `forge job <name>`
3. `event` / `emit` / `on`
4. event log / run id
5. `jobs/`, `events/`, `handlers/` の規約読み込み
6. `--plan` / `--apply` / `dry-run`
7. Notebook report
8. `service` は暗黙注入なしで小さく導入
9. `forge explain`
10. dashboard / WASM share

デモとして強い流れ:

```bash
forge new app user-importer --recipe csv-import
forge job import-users --path users.csv --dry-run
forge explain
forge dashboard
```

中身:

```forge
event RowInvalid {
    row: int
    message: string
}

event ImportFinished {
    total: int
    failed: int
}

job ImportUsers {
    input path: string
    input dry_run: bool = true

    run {
        let rows = csv.read(path)

        for row in rows {
            let result = UserSchema.validate(row)
            if !result.ok {
                emit RowInvalid {
                    row: row.index,
                    message: result.message,
                }
            }
        }

        emit ImportFinished {
            total: rows.len(),
            failed: Run.events.count(RowInvalid),
        }
    }
}

@service
struct RowInvalidReportHandler {
    report: Report
}

impl RowInvalidReportHandler {
    @on(RowInvalid)
    fn handle(self, e: RowInvalid) -> unit! {
        self.report.add_error(e.row, e.message)
    }
}
```

このデモが動くと、ForgeScript の個性が伝わる。

---

## 17. 共通部分とカスタマイズ部分の分離

ForgeScript が規約付きアプリ実行環境を目指す理由の一つは、顧客ごと・案件ごとに毎回ゼロから構築される無駄を減らすこと。

多くの業務アプリで本当に顧客ごとに違うのは、次のような部分に限られる。

```text
データの形
検証ルール
接続先
業務フローの細部
通知先
帳票・レポートの見た目
権限や承認ルール
```

一方で、毎回作り直すべきではない共通部分は多い。

```text
入力を受け取る
バリデーションする
dry-run する
plan/apply する
ログを残す
イベントを記録する
失敗時に止める
リトライする
実行履歴を残す
レポートを生成する
CLI から呼ぶ
Notebook で確認する
Web で共有する
テストする
```

ForgeScript では、この共通部分をライブラリとして提供するだけでなく、規約として固定する。

```text
共通:
  job runner
  event dispatcher
  validation engine
  report generator
  run log
  CLI
  Notebook
  dashboard
  plan/apply

カスタム:
  schema
  validator
  job body
  event handlers
  service config
  report template
```

この分離により、案件ごとの初期構築を薄くし、同じ地図で設計・レビュー・運用できるようにする。

```text
共通部分はランタイムと規約へ。
差分は validator / job / event handler / service config / report template へ。
```

これは「受託開発をプロダクト化する」方向に近い。個別要件には対応するが、毎回アーキテクチャを作り直さない。

---

## 18. recipe と extension point

テンプレートやボイラープレート生成だけでは、最初は楽でも案件ごとに分岐していく。

ForgeScript では、単なるテンプレートではなく、標準レシピと拡張ポイントを持つ方がよい。

```text
recipe:
  標準アプリの雛形。CSV 取込、API 同期、帳票生成、監視など。

extension point:
  顧客ごとに差し込んでよい場所。
```

例:

```bash
forge new app acme-importer --recipe csv-import
```

生成される構成:

```text
jobs/import.forge           共通寄り
validators/record.forge     カスタム前提
handlers/report.forge       カスタム前提
handlers/notify.forge       カスタム前提
reports/template.forge      カスタム前提
services/config.forge       カスタム前提
fixtures/sample.csv         テスト・デモ用
```

重要なのは、コピーして自由に改造するのではなく、標準ジョブや標準イベントに対して、顧客ごとのルールやハンドラを差し込むこと。

概念例:

```forge
extension CustomerRules for ImportUsers {
    validate row {
        row.customer_code required
        row.plan in ["basic", "pro", "enterprise"]
    }

    on imported(user) {
        CRM.sync(user)
    }
}
```

実際に `extension` 構文を導入するかは別として、思想としては以下を守る。

```text
標準処理をコピーして分岐させるのではなく、
標準処理が用意した拡張ポイントに差分を置く。
```

これにより、案件ごとに差分はあるが、ランタイム、実行履歴、テスト、レポート、CLI、イベントログは共通化できる。

---

## 19. 案件間で知識を持ち越す

規約の価値は、コード量を減らすことだけではない。

新しい案件でも同じ構造で読めることが大きい。

```text
この顧客のカスタム処理はどこか:
  jobs/ と validators/

通知はどこか:
  handlers/

DB 接続はどこか:
  app.forge または services/config.forge

実行履歴はどこか:
  runs/

テストデータはどこか:
  fixtures/
```

この一貫性があると、開発者だけでなく運用者も楽になる。

```bash
forge explain
forge job import-users --plan
forge dashboard
forge test
```

これらのコマンドが案件をまたいで同じ意味を持つことが重要。

最終的には、ForgeScript は次のような存在を目指せる。

```text
毎回作り直される業務アプリの骨格を規約として固定し、
顧客ごとの差分だけを安全に書くための言語。
```

---

## 20. Data-first という軸

ForgeScript は単なる汎用スクリプトではなく、data professional 向けの言語として育てると筋が通る。

データのプロが日常的に扱う流れは、次のようなもの。

```text
データの取得
データの検証
データの変換
データの保存
データの可視化
データ処理の実行履歴
データ品質の説明
```

ForgeScript が提供する機能は、この流れに自然に対応する。

```text
pipe:
  データ変換の流れ

validator:
  データ品質

Crucible:
  DB 入出力

job:
  実行単位

event:
  異常・進捗・完了通知

Notebook:
  探索・確認

WASM / report:
  共有
```

この方向では、ForgeScript は次のように定義できる。

```text
ForgeScript は、データ品質と業務フローを第一級に扱う、
Rust 基盤の規約優先アプリケーション言語。
```

---

## 21. パイプ演算子の位置づけ

データ処理では、値を段階的に変換していく流れが中心になる。

通常の関数呼び出しでは、処理順が内側から外側になり、読みづらくなる。

```forge
write_report(validate(clean(parse(read_csv("users.csv")))))
```

パイプ演算子では、思考の順序とコードの順序が一致する。

```forge
read_csv("users.csv")
    |> parse(UserRow)
    |> validate(UserSchema)
    |> clean()
    |> enrich()
    |> write_report("report.html")
```

これは単なる構文糖ではなく、data-first な ForgeScript の核になり得る。

job と組み合わせる例:

```forge
job ImportUsers {
    input path: string
    input dry_run: bool = true

    run {
        path
            |> csv.read()
            |> parse(UserRow)
            |> validate(UserSchema)
            |> emit_invalid(RowInvalid)
            |> filter_valid()
            |> upsert(db.users)
            |> report()
    }
}
```

よりデータ処理らしい例:

```forge
let result =
    from csv("users.csv")
    |> validate(UserSchema)
    |> reject_invalid emit RowInvalid
    |> normalize {
        email: lower(email),
        name: trim(name),
    }
    |> join db.departments on department_id
    |> upsert db.users
    |> summarize
```

パイプは、`job / validator / Crucible / event / report` を接続する主導線になる。

---

## 22. Databricks との関係

パイプ演算子や pipeline 的な構文が Databricks 的な発想に似るのは自然な収束。

データ処理を正面から考えると、次のような概念に寄っていく。

```text
notebook
pipeline
table
schema
validation
job
scheduling
lineage
dashboard
SQL / DataFrame
```

ForgeScript は Databricks をそのまま目指す必要はない。

差分は以下。

```text
Databricks:
  Spark / Lakehouse / 大規模分散 / クラウド

ForgeScript:
  Rust / ローカルファースト / 型付き / validator-first / 小型アプリ / WASM 共有
```

ForgeScript の狙いは「小さな Databricks」ではなく、より軽量なデータアプリ開発体験。

```text
ローカルから始められる、
型付き・検証付き・Rust 基盤のデータアプリ環境。
```

---

## 23. lineage と inspect

data professional 向けには lineage が重要になる。

知りたいこと:

```text
この report.html はどの入力 CSV から作られたか
どの validator を通ったか
どの変換を通ったか
どの DB テーブルに書いたか
どの行が落ちたか
```

`job + pipe + event log` があると、lineage を自然に記録できる。

```text
Run id
Input file hash
Pipeline steps
Validation result
Events
Output artifact
DB writes
```

また、Notebook や dashboard と連携する `inspect` も重要。

```forge
csv.read("users.csv")
    |> inspect("raw")
    |> validate(UserSchema)
    |> inspect("validated")
    |> normalize()
    |> inspect("normalized")
```

各段階のデータを Notebook / dashboard で確認できると、データ品質確認とデバッグがかなり楽になる。

標準オプション候補:

```bash
forge job import-users --sample 100
forge job import-users --limit 1000
forge job import-users --profile
forge job import-users --lineage
```

---

## 24. forge explain と pipeline

`forge explain` は pipeline を読めるべき。

例:

```text
Pipeline: ImportUsers

Steps:
  1. csv("users.csv")
  2. validate UserSchema
     emits RowInvalid
  3. normalize
  4. join db.departments
  5. upsert db.users
  6. summarize

Inputs:
  path: string

Outputs:
  ImportSummary

Events:
  RowInvalid
```

コードがそのままデータ処理仕様書になる。

---

## 25. 既存言語仕様との整合方針

この文書は規約付きアプリ構想のアイディア整理であり、既存の言語仕様をそのまま置き換えるものではない。

特に `lang/di/spec.md`、`lang/validator/spec.md`、`lang/tests/spec.md` には既に実装済みまたは実装対象として固定された構文があるため、今後の設計では以下を基本方針にする。

```text
既存仕様:
  破壊しない。実装済み構文を canonical syntax として扱う。

convention-apps:
  上位規約、ショートハンド、将来構文の候補として扱う。

新しい構文:
  既存構文の置き換えではなく、既存構文へ展開できる sugar として検討する。
```

現時点の優先判断:

```text
DI:
  container { bind X to Y } を維持する。
  app Production { ... } は container を包む上位 composition root として検討する。

event handler:
  @on(Event) は現行の canonical syntax。
  on Event as e { } は handler ファイル向けの将来 sugar として扱う。

validator:
  v.validate(x) / v.validate_all(x) を維持する。
  validate x with UserSchema は UserSchema.validate(x) への sugar として検討する。

test:
  test "..." { } を維持する。
  expect_event / expect_snapshot は追加 assertion として検討する。
```

---

## 26. DI の位置づけ

ForgeScript が `Crucible` の明示接続、`job` の明示 input、`service` の明示定義を重視するなら、DI は暗黙注入の主役ではなくなる。

ただし、DI が不要になるわけではない。

ForgeScript における DI は、次のように再定義すると自然。

```text
DI =
  実行時にどこからともなく依存を注入する仕組みではなく、
  環境ごとの依存グラフを明示的に組み立て、
  job / handler / service に渡すための配線レイヤー。
```

つまり、DI は `app.forge` における composition root の一部。

ただし、DI の下位プリミティブとしては既存仕様の `container { bind X to Y }` を維持する。

```forge
app Production {
    let db = Crucible::connect(env("DATABASE_URL"))
    let report = HtmlReport::new("target/report.html")

    container {
        bind Report to HtmlReport
    }

    wire ImportUsers {
        db: db
        report: report
    }
}
```

この形なら、依存は隠れない。

一方で、毎回 `run ImportUsers { db: db, report: report, ... }` を手で書く必要もない。

---

## 27. DI と service

`service` と DI は以下のように分担する。

```text
service:
  依存として使える名前付きリソースや外部機能の定義。

DI:
  service をどの環境でどう実装し、
  どの job / handler に渡すかの配線。
```

例:

```forge
service Notifier {
    fn send(message: string)
}

service SlackNotifier implements Notifier {
    config token: string

    fn send(message) {
        Slack.send(token, message)
    }
}

service NullNotifier implements Notifier {
    fn send(message) {}
}
```

環境ごとの配線:

```forge
app Production {
    container {
        bind Notifier to SlackNotifier::new(env("SLACK_TOKEN"))
    }
}

app Test {
    container {
        bind Notifier to NullNotifier::new()
    }
}
```

ジョブ側:

```forge
job ImportUsers {
    input notifier: Notifier

    run {
        notifier.send("import finished")
    }
}
```

ここで DI は、便利な隠蔽ではなく、環境ごとの差し替え規約。

---

## 28. DI と Crucible

DB 接続は副作用が大きいため、Crucible は明示的に扱う。

DI はこの方針と矛盾しない。

```forge
app Production {
    provide db = Crucible::connect(env("DATABASE_URL"))

    container {
        bind EventBus to InMemoryEventBus::new()
    }
}
```

ジョブ側にも依存を明示する。

```forge
job ImportUsers {
    input db: Crucible::Connection
    input path: string
}
```

実行:

```bash
forge job import-users --app production --path users.csv
```

このとき、`path` は CLI から、`db` は `app Production` から供給される。

`forge explain` は依存の供給元を表示する。

```text
Job: ImportUsers

Inputs:
  path: string                  <- CLI option
  db: Crucible::Connection      <- app Production provides db
```

これにより、DB がどこから来たか分からない問題を避けられる。

---

## 29. DI の主戦場

ForgeScript における DI が最も効くのは、次の領域。

```text
1. 環境差し替え
   Production / Staging / Test / DryRun

2. service 実装差し替え
   SlackNotifier / NullNotifier
   HtmlReport / MemoryReport
   RealClock / FixedClock

3. handler の依存配線
   on RowInvalid needs report
   on ImportFinished needs notifier

4. job 実行時の不足 input 補完
   path は CLI から
   db は app から
   report は service から

5. テスト
   本物 DB ではなく MemoryDb を渡す
   通知を NullNotifier にする

6. plan/apply
   apply 時だけ RealDb
   plan 時は PlanDb / DryRunDb
```

DI は主役ではない。

主役は以下。

```text
data pipeline
validator
typestate
job
event
Crucible
report
```

DI は、それらを本番・テスト・dry-run で安全に動かすための裏方。

```text
DI は、規約を実行可能にする配線盤。
```

---

## 30. DI と typestate

DI で提供する依存にも状態を付けられる。

```forge
provide db: Crucible::Connection<Connected>
provide tx: Transaction<Open>
provide client: ApiClient<Authenticated>
```

ジョブ側:

```forge
job ApplyImport {
    input db: Crucible::Connection<Connected>
    input plan: ImportPlan<Reviewed>
}
```

この形なら、DI は「正しい状態の依存だけを提供する」仕組みにもなる。

たとえば:

```text
未認証の API client は本番 job に渡せない
review 前の plan は apply job に渡せない
open でない transaction は commit できない
```

規約で決めた流れを、typestate と DI で実行時の配線にも反映できる。

---

## 31. イベントハンドラー構文の整理

現行仕様では、イベント購読は `@on(Event)` デコレータで `@service` のメソッドに付ける。

```forge
@service
struct NotificationService {
    mailer: EmailService
}

impl NotificationService {
    @on(UserCreated)
    fn handle_user_created(self, event: UserCreated) -> unit! {
        self.mailer.send(event.email, "ようこそ", "登録完了メール本文")?
    }
}
```

この構文は、依存が service のフィールドとして見えるため、DI 仕様と整合している。

一方、規約アプリ構想では `handlers/` ディレクトリに副作用を分離したい。

そのため、将来的に以下のトップレベル構文を sugar として検討できる。

```forge
on RowInvalid as e {
    report.add_error(e.row, e.message)
}
```

ただし、この sugar を採用する場合も、依存の供給元は `forge explain` で見える必要がある。

展開イメージ:

```text
handlers/report_errors.forge の on RowInvalid as e { ... }
  ↓
@service ReportErrorsHandler
impl ReportErrorsHandler {
    @on(RowInvalid)
    fn handle(self, e: RowInvalid) -> unit! { ... }
}
```

方針:

```text
canonical:
  @service + @on(Event)

future sugar:
  handlers/*.forge の top-level on Event as e { }
```

---

## 32. Validator 構文の整理

現行仕様では、validator はインスタンスメソッドで実行する。

```forge
let result = user_validator.validate(form)
let result = user_validator.validate_all(form)
```

規約アプリ構想の `validate row with UserSchema` は、以下への sugar として扱うのが自然。

```forge
validate row with UserSchema
// => UserSchema.validate(row)
```

パイプ内では、既存のパイプ構文と整合させるため、まずは関数呼び出し形式を優先する。

```forge
rows
    |> validate(UserSchema)
```

括弧なしの以下は、現時点では採用しない。

```forge
rows
    |> validate UserSchema
```

理由:

```text
既存パイプ構文と食い違いやすい
パーサーの曖昧性が増える
関数呼び出し形式で十分読める
```

---

## 33. テスト DSL の整理

現行仕様では、テスト名は文字列リテラルで書く。

```forge
test "ImportUsers rejects invalid users" {
    ...
}
```

この形式を維持する。

規約アプリ構想で出てきた `expect_event` / `expect_snapshot` は、テスト名構文の置き換えではなく、追加 assertion として扱う。

```forge
test "ImportUsers emits invalid rows" {
    let run = run ImportUsers with fixture("users.invalid")

    expect_event(RowInvalid, where: fn(e) {
        e.row == 12
    })

    expect_snapshot(run.report)
}
```

方針:

```text
test "..." { }:
  維持

expect_event:
  event log / run id が導入された後に追加検討

expect_snapshot:
  Notebook / JSON / HTML report の回帰テストとして追加検討
```

---

## 34. まとめ

validator が「データが正しいか」を扱う機能だとすると、service / job / event は「正しいデータを使って、外部世界とやり取りしながら、アプリとして動く」ための骨格。

ただし、ForgeScript は過剰な柔軟性ではなく規約を優先する。

最終的な方向性:

```text
ForgeScript は、検証・ジョブ・イベントを規約でまとめる、
小さなアプリのためのスクリプト言語。
```

または:

```text
データを検証し、ジョブとして実行し、イベントで反応する。
その標準形を最初から持った言語。
```
