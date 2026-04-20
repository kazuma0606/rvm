# `forge/validator` 仕様書

> バージョン: 0.2.0
> 作成: 2026-04-08
> 更新: 2026-04-19

---

## 概要

`data` 型の `validate` ブロックを補完する高機能バリデーション DSL。
クロスフィールド検証・全エラー収集・カスタムメッセージ・条件付き検証・Anvil ミドルウェア統合をサポートする。

---

## `data` validate との違い

| 機能 | `data` validate | `forge/validator` |
|---|---|---|
| 1フィールド制約 | ✅ | ✅ |
| クロスフィールド | ❌ | ✅ |
| 真の正規表現 | ❌（`contains()` のみ） | ✅ |
| 全エラー収集 | ❌（fail-fast） | ✅ |
| カスタムメッセージ | ❌ | ✅ |
| 条件付きバリデーション | ❌ | ✅ |
| リスト要素の検証 | ❌ | ✅ |
| ネスト struct の検証 | ❌ | ✅ |
| 共通ルールの再利用 | ❌ | ✅ |
| Anvil ミドルウェア統合 | ❌ | ✅ |

### `data validate` との併用ルール

`data` に `validate` ブロックがある型に対して `Validator` を追加した場合、**両方のバリデーションが順番に実行される**。`data validate` が先に走り、通過した場合のみ `Validator` が実行される。どちらか片方だけで十分な場合は混在させないことを推奨する。

---

## API

### `Validator` の構築

```forge
use forge/validator.{ Validator }

let v = Validator::new(PasswordForm)
    // 単フィールドルール
    .rule("password", fn(r) { r.min_length(8) },
          "8文字以上で入力してください")
    .rule("email",    fn(r) { r.matches("^[^@]+@[^@]+\\.[^@]+$") },
          "メールアドレスの形式が正しくありません")

    // クロスフィールドルール
    .rule_cross(fn(f) { f.password == f.confirm_password },
                ["password", "confirm_password"],
                "パスワードが一致しません")
    .rule_cross(fn(f) { f.end_date > f.start_date },
                ["start_date", "end_date"],
                "終了日は開始日より後にしてください")

    // 条件付きルール
    .rule_when(
        fn(f) { f.payment_method == "credit" },
        "card_number",
        fn(r) { r.matches("^\\d{16}$") },
        "カード番号は16桁の数字で入力してください"
    )

    // リスト要素の検証
    .rule_each("tags", fn(r) { r.max_length(20) },
               "タグは20文字以内で入力してください")

    // ネスト struct の検証
    .rule_nested("address", address_validator)
```

### バリデーション実行

`validate()` の戻り値は `Result<(), ValidationError>`（fail-fast）または `Result<(), list<ValidationError>>`（全収集）。

```forge
// 最初のエラーで止まる（fail-fast）
match v.validate(form) {
    ok(_)  => proceed()
    err(e) => println("{e.fields}: {e.message}")
}

// 全エラーを収集
match v.validate_all(form) {
    ok(_)       => proceed()
    err(errors) => errors.each(fn(e) { println("{e.fields}: {e.message}") })
}
```

### ルールビルダー（`fn(r) { ... }` の `r` で使えるメソッド）

| メソッド | 説明 |
|---|---|
| `r.required()` | `none` / 空文字列を禁止 |
| `r.min_length(n)` | 最小文字数 |
| `r.max_length(n)` | 最大文字数 |
| `r.min(n)` | 最小値（number） |
| `r.max(n)` | 最大値（number） |
| `r.matches(pattern)` | 正規表現マッチ（`forge/std/regex` を内部使用） |
| `r.one_of(list)` | 列挙値チェック |
| `r.custom(fn)` | カスタム検証関数 `(value) -> Result<(), string>` |
| `r.ok()` | 常に成功（条件付きルールで「この場合はスキップ」を表現） |

`r.custom()` は `bool` ではなく `Result<(), string>` を返す。エラーメッセージを動的に生成したい場合はこちらを使う。

```forge
.rule("username", fn(r) {
    r.custom(fn(v) {
        if v.starts_with("admin") {
            err("'admin' で始まるユーザー名は使用できません")
        } else {
            ok(())
        }
    })
}, "ユーザー名が不正です")
```

---

## エラー型

```forge
// バリデーションエラー 1 件
data ValidationError {
    fields:  list<string>   // 関連フィールド名のリスト（クロスフィールドは複数）
    message: string         // エラーメッセージ
    value:   any?           // 入力値（デバッグ用）
}
```

`fields` をリストにすることで、クロスフィールドエラー時にフロントエンド側で複数フィールドをハイライトできる。単フィールドルールの場合は 1 要素のリスト `["email"]` になる。

---

## 共通ルールの名前付き定義

プロジェクト全体で使い回すルールを定数として定義できる。

```forge
use forge/validator.{ Rule }

// ルールを定数として定義
const EMAIL_RULE = Rule::new()
    .required()
    .matches("^[^@]+@[^@]+\\.[^@]+$")
    .message("メールアドレスの形式が正しくありません")

const PASSWORD_RULE = Rule::new()
    .required()
    .min_length(8)
    .message("パスワードは8文字以上で入力してください")

// 複数の Validator で使い回す
let login_validator = Validator::new(LoginForm)
    .rule("email",    EMAIL_RULE)
    .rule("password", PASSWORD_RULE)

let register_validator = Validator::new(RegistrationForm)
    .rule("email",    EMAIL_RULE)
    .rule("password", PASSWORD_RULE)
    .rule_cross(fn(f) { f.password == f.confirm_password },
                ["password", "confirm_password"],
                "パスワードが一致しません")
```

---

## ネストした struct の検証

入れ子構造の `data` 型を再帰的に検証する。

```forge
let address_validator = Validator::new(Address)
    .rule("zip",  fn(r) { r.matches("^\\d{3}-\\d{4}$") }, "郵便番号の形式が正しくありません")
    .rule("city", fn(r) { r.required() },                  "市区町村は必須です")

let user_validator = Validator::new(UserProfile)
    .rule("name", fn(r) { r.required().max_length(50) }, "名前は50文字以内です")
    .rule_nested("address", address_validator)
```

ネストしたフィールドのエラーは `fields: ["address.zip"]` のようにドット区切りで返る。

---

## Anvil ミドルウェア統合

`@validate` デコレータをハンドラに付与することで、リクエストボディのバリデーションを自動化できる。

```forge
use forge/validator.{ Validator }
use ./validators.{ registration_validator }

// バリデーション通過後のみハンドラが呼ばれる
@validate(RegistrationForm, using: registration_validator)
fn register_handler(req: Request<RegistrationForm>) -> Response<User>! {
    let user = register_usecase.execute(req.body)?
    ok(Response::json(user).status(201))
}
```

バリデーション失敗時は自動で **HTTP 422** を返す。レスポンスボディは以下の形式。

```json
{
  "errors": [
    { "fields": ["email"],               "message": "メールアドレスの形式が正しくありません" },
    { "fields": ["password", "confirm_password"], "message": "パスワードが一致しません" }
  ]
}
```

`@validate` なしで手動バリデーションする場合は `validate_all()` を使う。

```forge
fn register_handler(req: Request<RegistrationForm>) -> Response<User>! {
    match registration_validator.validate_all(req.body) {
        ok(_)       => { /* 続行 */ }
        err(errors) => return ok(Response::json({ errors: errors }).status(422))
    }
    let user = register_usecase.execute(req.body)?
    ok(Response::json(user).status(201))
}
```

---

## 使用例（完全版）

```forge
use forge/validator.{ Validator, Rule }

data RegistrationForm {
    username:         string
    email:            string
    password:         string
    confirm_password: string
    age:              number
    tags:             list<string>
    address:          Address
}

data Address {
    zip:  string
    city: string
}

const EMAIL_RULE = Rule::new()
    .required()
    .matches("^[^@]+@[^@]+\\.[^@]+$")
    .message("メールアドレスの形式が正しくありません")

let address_validator = Validator::new(Address)
    .rule("zip",  fn(r) { r.matches("^\\d{3}-\\d{4}$") }, "郵便番号の形式が正しくありません")
    .rule("city", fn(r) { r.required() },                  "市区町村は必須です")

let registration_validator = Validator::new(RegistrationForm)
    .rule("username", fn(r) { r.required().min_length(3).max_length(20) },
          "ユーザー名は3〜20文字で入力してください")
    .rule("email",    EMAIL_RULE)
    .rule("password", fn(r) { r.required().min_length(8) },
          "パスワードは8文字以上で入力してください")
    .rule_cross(fn(f) { f.password == f.confirm_password },
                ["password", "confirm_password"],
                "パスワードと確認用パスワードが一致しません")
    .rule("age",  fn(r) { r.min(18).max(120) },
          "年齢は18〜120の範囲で入力してください")
    .rule_each("tags", fn(r) { r.max_length(20) },
               "タグは20文字以内で入力してください")
    .rule_nested("address", address_validator)

match registration_validator.validate_all(form) {
    ok(_)       => register_user(form)
    err(errors) => {
        errors.each(fn(e) { println("{e.fields}: {e.message}") })
        err("入力内容に誤りがあります")
    }
}
```

---

## 将来の拡張候補

### 非同期バリデーション

DB の unique チェックなど IO が必要なケース。`v0.3.0` 以降で検討。

```forge
let v = Validator::new(RegistrationForm)
    .rule_async("email", async fn(r) {
        let exists = db.query("SELECT 1 FROM users WHERE email = ?", [r.value])?
        if exists.is_empty() { ok(()) } else { err("このメールアドレスは既に登録されています") }
    })
```

### i18n 対応

エラーメッセージをキー化してロケール別に切り替える。`v0.3.0` 以降で検討。

```forge
let v = Validator::new(RegistrationForm)
    .rule("email", fn(r) { r.required() }, message_key: "validation.email.required")

Validator::set_locale("ja")
```

---

## Rust 変換方針

- `Validator` はカスタム実装（`validator` クレートをベースに ForgeScript の型に合わせてラップ）
- `matches()` は内部で `regex` クレートを使用
- `validate_all()` は `Result<(), Vec<ValidationError>>` に変換
- `@validate` デコレータは Anvil のミドルウェアチェーンとして展開（`forge-compiler` が生成）
