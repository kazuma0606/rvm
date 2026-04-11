---
name: forge-task
description: 引数で指定したタスク名を全 tasks.md から探して実装する。例: /forge-task test_lex_integer
---

全 `tasks.md` から **`$ARGUMENTS`** というタスクを見つけて実装してください。

## 手順

### 1. タスクを探す

以下のファイルを grep して `$ARGUMENTS` を含む行を探す:

```
lang/syntax/tasks.md
lang/tests/tasks.md
lang/transpiler/tasks.md
lang/std/v1/tasks.md
packages/anvil/tasks.md
lang/packages/*/tasks.md
lang/*/tasks.md
packages/*/tasks.md
```

### 2. コンテキストを把握する

- タスクが属するエリアと spec.md を読む
- 関連する実装ファイルを特定して読む
- `dev/design-v3.md` で設計方針を確認する

### 3. 実装する

- コードを書く（テストを含む）
- `cargo test $ARGUMENTS` でテストが通ることを確認する

### 4. tasks.md を更新する

- 該当 `[ ]` を `[x]` に変更する

### 5. 完了を報告する

## タスクが見つからない場合

類似するタスク名を提示してどれを実装するか確認する。

## 絶対に守ること

- `Value::Nil` は使わない（`Value::Unit`）
- `unwrap()` は使わない
- テスト関数名は tasks.md の定義と完全一致させる
