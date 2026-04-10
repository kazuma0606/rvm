# `forge/validator` 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-08

---

## 概要

`data` 型の `validate` ブロックを補完する高機能バリデーション DSL。
クロスフィールド検証・全エラー収集・カスタムメッセージ・条件付き検証をサポートする。

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

---

## API

### `Validator` の構築

```forge
use forge/validator.{ Validator, all_errors }

let v = Validator::for(PasswordForm)
    // 単フィールドルール
    .rule("password", r => r.min_length(8), "8文字以上で入力してください")
    .rule("email",    r => r.matches("^[^@]+@[^@]+\\.[^@]+$"), "メールアドレスの形式が正しくありません")

    // クロスフィールドルール
    .rule_cross(f => f.password == f.confirm_password, "パスワードが一致しません")
    .rule_cross(f => f.end_date > f.start_date, "終了日は開始日より後にしてください")

    // 条件付きルール
    .rule_when(
        f => f.payment_method == "credit",
        "card_number",
        r => r.matches("^\\d{16}$"),
        "カード番号は16桁の数字で入力してください"
    )

    // リスト要素の検証
    .rule_each("tags", r => r.max_length(20), "タグは20文字以内で入力してください")
```

### バリデーション実行

```forge
// 最初のエラーで止まる（デフォルト）
match v.validate(form) {
    ok(_)       => proceed(),
    err(e)      => println("{e.field}: {e.message}"),
}

// 全エラーを収集
match v.validate(form) |> all_errors() {
    ok(_)       => proceed(),
    err(errors) => errors.each(e => println("{e.field}: {e.message}")),
}
```

### ルールビルダー（`r =>` の `r` で使えるメソッド）

| メソッド | 説明 |
|---|---|
| `r.required()` | `none` / 空文字列を禁止 |
| `r.min_length(n)` | 最小文字数 |
| `r.max_length(n)` | 最大文字数 |
| `r.min(n)` | 最小値（number） |
| `r.max(n)` | 最大値（number） |
| `r.matches(pattern)` | 正規表現マッチ（`forge/std/regex` を内部使用） |
| `r.one_of(list)` | 列挙値チェック |
| `r.custom(fn)` | カスタム検証関数 `(value) -> bool` |
| `r.ok()` | 常に成功（条件付きルールで「この場合はスキップ」を表現） |

---

## エラー型

```forge
// バリデーションエラー1件
data ValidationError {
    field:   string   // フィールド名（クロスフィールドは "_"）
    message: string   // エラーメッセージ
    value:   any?     // 入力値（デバッグ用）
}
```

---

## 使用例（完全版）

```forge
use forge/validator.{ Validator, all_errors }

data RegistrationForm {
    username:        string
    email:           string
    password:        string
    confirm_password: string
    age:             number
    tags:            list<string>
}

let v = Validator::for(RegistrationForm)
    .rule("username", r => r.required().min_length(3).max_length(20),
          "ユーザー名は3〜20文字で入力してください")
    .rule("email", r => r.required().matches("^[^@]+@[^@]+\\.[^@]+$"),
          "メールアドレスの形式が正しくありません")
    .rule("password", r => r.required().min_length(8),
          "パスワードは8文字以上で入力してください")
    .rule_cross(f => f.password == f.confirm_password,
                "パスワードと確認用パスワードが一致しません")
    .rule("age", r => r.min(18).max(120),
          "年齢は18〜120の範囲で入力してください")
    .rule_each("tags", r => r.max_length(20),
               "タグは20文字以内で入力してください")

let result = v.validate(form) |> all_errors()
match result {
    ok(_)       => register_user(form)
    err(errors) => {
        errors.each(e => println("❌ {e.field}: {e.message}"))
        err("入力内容に誤りがあります")
    }
}
```

---

## Rust 変換方針

- `Validator` は `validator` クレートまたはカスタム実装
- `matches()` は内部で `regex` クレートを使用
- `all_errors()` は `Result<T, Vec<ValidationError>>` に変換
