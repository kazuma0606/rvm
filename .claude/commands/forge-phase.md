---
name: forge-phase
description: 引数で指定したPhaseの全タスクを順番に実装する。例: /forge-phase 0 でPhase 0を全て実装する。
---

forge/tasks.md と forge/plan.md を読んで、**Phase $ARGUMENTS** の全タスクを実装してください。

## 手順

1. `forge/tasks.md` から `## Phase $ARGUMENTS` セクションの全 `[ ]` タスクを抽出する
2. `forge/plan.md` で Phase $ARGUMENTS の実装計画を確認する
3. `forge/spec_v0.0.1.md` で該当する仕様を確認する
4. 依存関係順に実装する（例: トークン定義 → Lexer → テスト）
5. 各タスク実装後に `cargo test` を実行して確認する
6. 全タスク完了後に `forge/tasks.md` を一括更新する
7. 最終的に `/forge-status` 相当の報告を行う

## フェーズ別の実装順序の目安

- Phase 0: ディレクトリ・Cargo.toml → テスト基盤
- Phase 1: tokens.rs → ast/mod.rs → parser/mod.rs → テスト
- Phase 2: value.rs → interpreter/mod.rs → stdlib → forge-cli
- Phase 3: collections/ の各メソッド → テスト
- Phase 4: typechecker/types.rs → 型推論 → forge check コマンド

## 重要なルール

- `Value::Nil` は使わない（`Value::Unit`）
- `unwrap()` は使わない
- テスト名は tasks.md の定義と完全一致
- テストが失敗した場合は修正してから次のタスクへ進む
