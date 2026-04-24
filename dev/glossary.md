# ForgeScript 用語集

> 概念が増えてきたため、用語のブレを防ぐための一元管理。
> 実装済み・計画中・将来候補を区別して記載する。

---

## 言語キーワード

### `job` 🔲 計画中
意味のある実行単位。`fn` より上位の概念。CLI・イベント駆動・スケジューラから呼べる。
```forge
job ImportUsers {
    input path: string
    run { ... }
}
```
→ `forge job import-users --path users.csv`

### `input` 🔲 計画中（`job` と同時）
job が受け取る引数の宣言。型によって CLI オプション（primitive）か DI 注入（trait/struct）かを自動判別。
```forge
input path: string       // → --path（CLI）
input notifier: Notifier // → container / wire から注入（DI）
```

### `run { }` 🔲 計画中（`job` と同時）
job の実行ブロック。`fn` の body に相当するが、`run_id` / event log と統合される。

### `event` 🔲 計画中
型付きイベントの宣言。`data` に近い構造。何かが起きたことを表す名詞で命名する。
```forge
event RowInvalid {
    row: int
    message: string
}
```

### `emit` 🔲 計画中
イベントをランタイムの EventQueue に投入する文。副作用の分離に使う。
```forge
emit RowInvalid { row: 42, message: "invalid email" }
```

### `app` 🔲 計画中
アプリ全体の composition root。`app.forge` に置く。`load` / `provide` / `container` / `wire` を含む。
```forge
app Production {
    load jobs/*
    provide db = Crucible::connect(env("DATABASE_URL"))
    container { bind Notifier to SlackNotifier::new(...) }
}
```

### `provide` 🔲 計画中
インフラサービスをアプリ全体に供給する宣言。`provide` されたものは全 job から `input` なしで使える。
→ **`bind` との違い**: `provide` はインフラ固定、`bind` は差し替え可能。

### `wire` 🔲 計画中
特定の job/handler に対して pluggable サービスを明示的に接続する宣言。
```forge
wire ImportUsers {
    notifier: slack_notifier
    report:   html_report
}
```

### `load` 🔲 計画中
glob パターンで複数ファイルを一括インポートする宣言（`app {}` の中で使う）。
```forge
load jobs/*
load handlers/*
```

---

## 実装済みキーワード

### `container { bind X to Y }` ✅ 実装済み
差し替え可能なサービスを trait → 実装クラスにバインドする宣言。DI の中核。
```forge
container {
    bind Notifier to SlackNotifier
    bind Logger   to JsonLogger::new()
}
```

### `@service` ✅ 実装済み
UseCase・サービス層の struct に付与するデコレータ。`container` が配線対象として認識する。

### `@repository` ✅ 実装済み
Infrastructure 層のリポジトリ実装に付与するデコレータ。

### `@on(Event)` ✅ 実装済み
`@service` のメソッドに付与するイベント購読デコレータ。EventBus と統合。
```forge
@on(UserCreated)
fn handle_user_created(self, event: UserCreated) -> unit! { ... }
```

### `@timed` ✅ 実装済み
メソッドの実行時間を MetricsBackend に記録するデコレータ。

### `@validate(Type, using: validator)` ✅ 実装済み
Anvil ハンドラーにバリデーションミドルウェアを適用するデコレータ。失敗時 HTTP 422。

### `system name(params) { }` ✅ 実装済み（Ember 用）
ECS の System 定義。`World::query()` に展開されるシンタックスシュガー。

### `typestate` ✅ 実装済み
状態遷移を型で表現する構文。`Configured → Running → Stopped` のように遷移を強制できる。

---

## ランタイム概念

### EventQueue 🔲 計画中
`emit` で投入されたイベントを保持するメモリ上の FIFO キュー。job 実行後にフラッシュされ、`@on(Event)` ハンドラーが順に呼ばれる。

### run_id 🔲 計画中
job 実行のたびに生成される一意な ID（`YYYY-MM-DDTHH-MM-SS` または UUID）。`runs/<run_id>/` ディレクトリに実行ログを書き出す。

### event log 🔲 計画中
`runs/<run_id>/events.jsonl` に書き出される JSON Lines 形式のイベント記録。lineage・監査に使う。

---

## CLI コマンド

### `forge job <name>` 🔲 計画中
`jobs/*.forge` を自動スキャンし、`input` 宣言から CLI オプションを生成して job を実行する。

### `forge job <name> --plan` 🔲 計画中
副作用を実行せず、実行予定（DB 更新件数・emit されるイベント）を表示する。

### `forge job <name> --apply` 🔲 計画中
`--plan` で確認した内容を実際に実行する。

### `forge explain` 🔲 計画中
`app.forge` を解析し、Jobs / Events / Services / 依存の供給元を人間が読める形で出力する。

### `forge explain --json` 🔲 計画中
`forge explain` の機械可読形式出力。CI / dashboard との統合用。

### `forge job <name> --lineage` 🔲 将来
実行の lineage（入力ファイルハッシュ・パイプラインステップ・出力アーティファクト）を表示する。

---

## アーキテクチャ概念

### infrastructure（インフラ）
アプリが前提とする固定サービス。DB（Crucible）・EventQueue など。`provide` で供給し、job の `input` に書かない。テスト時は Crucible のモードを切り替えることで対応。

### pluggable（差し替え可能）
テスト・環境によって実装を切り替えるサービス。Notifier・Report など。`container { bind }` で宣言し、job の `input` で明示的に受け取る。

### composition root
アプリの配線を一か所で定義する場所。ForgeScript では `app.forge` がこれに相当する。`provide` / `container` / `wire` を置く。

### lineage
「この出力がどの入力から作られたか」を追跡できる情報。`job + pipe + event log` が揃うと自然に記録できる。

---

## シュガー候補（将来・未確定）

### `validate x with Schema`
`Schema.validate(x)` への展開形として検討中。canonical は `.validate()` メソッド呼び出し。

### `on Event as e { }` トップレベル文
`handlers/*.forge` の中で使えるシュガーとして検討中。`@service + @on(Event)` への展開形。canonical は `@on` デコレータ。

### `expect_event(E, where: fn)` / `expect_snapshot(x)`
`test "..." { }` ブロック内で使える assertion 関数。event log / run_id が実装された後に追加検討。
