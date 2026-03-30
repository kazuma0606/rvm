---
name: forge-status
description: forge/tasks.mdの現在の進捗を表示し、cargo testを実行してテスト通過状況も合わせて報告する。
---

以下の手順で現在の進捗状況を報告してください。

## 手順

1. `forge/tasks.md` を読む
2. `[ ]`（未完了）と `[x]`（完了）の数を Phase ごとに集計する
3. `cargo test --workspace 2>&1` を実行する
4. 以下の形式で報告する

## 報告形式

```
## ForgeScript 実装進捗レポート

### Phase別進捗
| Phase | 完了 | 総数 | 進捗率 |
|---|---|---|---|
| Phase 0 基盤整備 | X | Y | Z% |
| Phase 1-A Lexer | X | Y | Z% |
| Phase 1-B AST | X | Y | Z% |
| Phase 1-C Parser | X | Y | Z% |
| Phase 2-A Value | X | Y | Z% |
| Phase 2-B Interpreter | X | Y | Z% |
| Phase 2-C Stdlib | X | Y | Z% |
| Phase 2-D CLI | X | Y | Z% |
| Phase 3 Collections | X | Y | Z% |
| Phase 4 TypeChecker | X | Y | Z% |

### テスト通過状況
実行: X / 通過: X / 失敗: X

### 次のアクション（未完了タスク 上位3件）
1. [ ] タスク名（Phase X）
2. [ ] タスク名（Phase X）
3. [ ] タスク名（Phase X）
```
