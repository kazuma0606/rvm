# ForgeScript 設計判断ログ

> 目的: 「なぜそう決めたか」を残す。次セッションの Codex や将来の開発者が誤った方向に進まないよう、判断の根拠を記録する。
>
> 形式: 判断 → 却下した選択肢 → 理由

---

## DD-1: `provide`（インフラ）と `bind`（pluggable）を分ける

### 判断

```forge
app Production {
    provide db    = Crucible::connect(env("DATABASE_URL"))  // インフラ
    provide queue = EventQueue::new()                        // インフラ

    container {
        bind Notifier to SlackNotifier::new(env("SLACK_TOKEN"))  // pluggable
        bind Report   to HtmlReport::new("target/report.html")   // pluggable
    }
}
```

`provide` されたものは job の `input` に書かなくても全 job から使える。
`bind` されたものは `input notifier: Notifier` のように明示的に受け取る。

### 却下した選択肢

**A. 全依存を `input` で明示する**
```forge
job CreateUser {
    input db: Crucible::Connection  // 毎回書く
    ...
}
```
→ job 定義が冗長になる。DB は「どの job でも使う」インフラであり、毎回宣言する意味が薄い。

**B. 全依存を暗黙注入する（Spring スタイル）**
→ どこから来たか分からない。`forge explain` で可視化できない。

### 理由

AWS SQS + Lambda のモデルが示すように、DB・キューは「インフラ設定」であり job の引数ではない。
一方 Notifier・Report は「テストで差し替えたい」pluggable なサービスであり、明示的 DI が有効。
`provide`（固定インフラ）と `bind`（差し替え可能）を分けることで両方の利点を得る。

---

## DD-2: `container { bind X to Y }` を維持し `app { bind X = Y }` にしない

### 判断

```forge
// canonical
container {
    bind Notifier to SlackNotifier::new(env("SLACK_TOKEN"))
}
```

### 却下した選択肢

```forge
// convention-apps.md 初稿の提案（却下）
app Production {
    bind Notifier = SlackNotifier { token: env("SLACK_TOKEN") }
}
```

### 理由

`container { bind X to Y }` は `lang/di/spec.md` で実装済みかつテスト済みの canonical syntax。
`app {}` は convention-apps の上位概念であり、内部で `container {}` を使う形にする方が整合的。
実装済みのものを破壊するより、`app {}` が `container {}` を包む階層にした方がコストが低い。

---

## DD-3: `@on(Event)` を canonical とし `on Event as e { }` はシュガー候補止まり

### 判断

```forge
// canonical
@service
struct RowInvalidReportHandler { report: Report }

impl RowInvalidReportHandler {
    @on(RowInvalid)
    fn handle(self, e: RowInvalid) -> unit! {
        self.report.add_error(e.row, e.message)
    }
}
```

### 却下した選択肢（将来シュガー候補）

```forge
// handlers/*.forge のトップレベルに書ける将来 sugar
on RowInvalid as e {
    report.add_error(e.row, e.message)
}
```

### 理由

`@service + @on(Event)` は DI 仕様と整合しており、依存（`report`）がフィールドとして見える。
`on Event as e { }` の方が書きやすいが、依存の供給元が見えなくなる。
まず canonical で実装し、`handlers/` ディレクトリ規約が定着したら sugar として展開する方式で検討する。

---

## DD-4: `|> validate UserSchema`（括弧なし）を採用しない

### 判断

```forge
// 採用
rows |> validate(UserSchema)

// 不採用
rows |> validate UserSchema
```

### 理由

現行パイプ構文は `expr |> fn_call(args)` の形式。括弧なしにするとパーサーが次のトークンを
引数と見るべきか次の文と見るべきか曖昧になる。読みやすさの向上より曖昧性のコストが大きい。

---

## DD-5: `validate x with Schema` は sugar 候補。canonical は `.validate(x)`

### 判断

```forge
// canonical
let result = user_validator.validate(form)
let result = user_validator.validate_all(form)

// 将来 sugar（job 構文が安定したら検討）
validate form with user_validator
// => user_validator.validate(form) に展開
```

### 理由

`lang/validator/spec.md` は実装済み（81/81 完了）。`validate x with Y` は自然言語に近いが、
既存 API を置き換えるのではなく展開形として定義することで後方互換を保てる。

---

## DD-6: ForgeScript の拡張として実装し、別プロジェクト化しない

### 判断

`job / event / emit / app` は ForgeScript 本体の言語拡張として実装する。
別リポジトリ・別フレームワークには切り出さない。

### 却下した選択肢

ForgeScript の上に乗る「Forge Framework」として別プロジェクト化する。

### 理由

- `forge job <name>` / `forge explain` が first-class で動くには CLI が AST の `job` 宣言を理解する必要がある
- `forge explain` が「実行可能な仕様書」になるのは、コンパイラが `job` の `input` / `emit` を静的に把握しているからこそ
- 「迷う場所を減らす規約」という価値は、言語・CLI・規約が三位一体であることに依存する
- `system`（Ember 用）/ `@validate`（ミドルウェア用）と同様に、ドメイン固有キーワードを言語に追加する precedent がすでにある

---

## DD-7: job の `input` は CLI オプションと DI 注入の両方を兼ねる

### 判断

```forge
job ImportUsers {
    input path:    string        // → --path users.csv （CLI）
    input dry_run: bool = true   // → --dry-run         （CLI、デフォルト true）
    input notifier: Notifier     // → wire / container から注入（DI）
}
```

`input` の型が primitive（string / bool / number）なら CLI オプション、
trait / struct 型なら DI 注入として自動判別する。

### 理由

CLI から実行される場合と、`app.forge` の `wire` で依存を注入される場合を統一的に記述できる。
`forge explain` は供給元（CLI か app か）を自動的に表示できる。

---

## DD-8: テスト名はクォート文字列 `test "..." { }` を維持

### 判断

```forge
// canonical（維持）
test "ImportUsers rejects invalid users" {
    ...
}

// 却下
test ImportUsers rejects invalid users {  // クォートなし
    ...
}
```

### 理由

クォートなし識別子 + 説明文のスタイルはパーサーの終端判定が困難。
既存の `test "..." { }` は実装済みでテストランナーと統合されている。
`expect_event` / `expect_snapshot` は test 名構文ではなく、ブロック内の assertion 関数として追加する。
