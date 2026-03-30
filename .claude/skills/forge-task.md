---
name: forge-task
description: 引数で指定したタスク名をforge/tasks.mdから探して実装する。例: /forge-task test_lex_integer
---

`forge/tasks.md` から **`$ARGUMENTS`** というタスクを見つけて実装してください。

## 手順

1. `forge/tasks.md` で `$ARGUMENTS` を検索して該当タスクを特定する
2. そのタスクが属するPhaseとコンポーネント（Lexer/Parser/VM等）を確認する
3. `forge/spec_v0.0.1.md` の関連仕様を読む
4. 実装対象のファイルを特定して読む（存在する場合）
5. タスクを実装する（コード + テスト）
6. `cargo test $ARGUMENTS` でテストが通ることを確認する
7. `forge/tasks.md` の該当 `[ ]` を `[x]` に更新する
8. 完了を報告する

## タスクが見つからない場合

`$ARGUMENTS` が tasks.md に存在しない場合は、
類似するタスク名を提示してどれを実装するか確認する。

## 重要なルール

- `Value::Nil` は使わない（`Value::Unit`）
- `unwrap()` は使わない
- テスト関数名は tasks.md の定義と完全一致させる
