---
name: forge-phase
description: 引数で指定したエリアの全タスクを順番に実装する。例: /forge-phase lang/syntax でsyntaxタスクを全て実装する。
---

`$ARGUMENTS` で指定されたエリアの全タスクを実装してください。

## 手順

### 1. タスクファイルを特定する

引数の形式に応じて対象ファイルを特定する:
- `lang/syntax` → `lang/syntax/tasks.md`
- `lang/tests` → `lang/tests/tasks.md`
- `lang/transpiler` → `lang/transpiler/tasks.md`
- `packages/anvil` → `packages/anvil/tasks.md`
- `lang/packages/forge-time` → `lang/packages/forge-time/tasks.md`
- 数字のみ（例: `0`）→ 後方互換として `lang/v0.1.0/tasks.md` の Phase 0 セクション

### 2. コンテキストを読む

- 対象 `tasks.md` を全文読む
- 同ディレクトリの `spec.md` / `plan.md` を読む（存在する場合）
- `dev/design-v3.md` で設計方針を確認する

### 3. 依存関係順に実装する

- 未完了 `[ ]` タスクを上から順に処理する
- 各タスクの実装後、`cargo test` を実行して確認する
- 失敗したら修正してから次へ進む

### 4. 一括更新する

全タスク完了後、`tasks.md` の `[ ]` を `[x]` に一括更新する。
進捗サマリーテーブルも更新する。

### 5. 完了レポートを出力する

- 完了タスク数 / 総タスク数
- `cargo test --workspace` の結果サマリー

## 絶対に守ること

- `Value::Nil` は使わない（`Value::Unit`）
- `unwrap()` は使わない
- テスト名は tasks.md の定義と完全一致
- テストが失敗した場合は修正してから次のタスクへ進む
