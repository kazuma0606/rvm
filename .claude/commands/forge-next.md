---
name: forge-next
description: 全 tasks.md から最初の未完了タスク（[ ]）を1つ選んで実装する。実装・テスト・tasks.md更新まで一貫して行う。
---

リポジトリ内の全 `tasks.md` から最初の未完了タスク（`[ ]`）を1つ見つけて実装してください。

## 手順

### 1. 未完了タスクを探す

以下の優先順でファイルを確認し、`[ ]` が残っている最初のファイルを選ぶ:

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

- タスクが属するセクション・フェーズを確認する
- 同ディレクトリの `spec.md` / `plan.md` を読む（存在する場合）
- 関連する実装ファイル（`crates/` 以下）を特定して読む

### 3. 仕様を確認する

- コア言語仕様: `lang/v0.1.0/spec_v0.0.1.md`
- 設計方針: `dev/design-v3.md`
- パッケージ固有仕様: タスクと同ディレクトリの `spec.md`

### 4. 実装する

- コードを書く（テストを含む）
- `cargo test` で関連テストが通ることを確認する

### 5. tasks.md を更新する

- 完了したタスクの `[ ]` を `[x]` に変更する
- 進捗サマリーテーブルがあれば更新する

### 6. 完了を報告して停止する

次のタスクには自動で進まない。ユーザーの確認を待つ。

## 絶対に守ること

- `Value::Nil` は使わない → `Value::Unit` を使う
- `unwrap()` は使わない → `?` か `match` を使う
- テスト関数名は tasks.md の定義と完全一致させる
- 1タスク完了したら必ず確認を求める
