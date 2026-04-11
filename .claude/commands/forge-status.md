---
name: forge-status
description: 全 tasks.md の進捗を集計し、cargo test の結果と合わせて報告する。
---

リポジトリ内の全 `tasks.md` を集計して進捗レポートを出力してください。

## 手順

### 1. 全 tasks.md を集計する

以下のファイルを順番に読み、`[ ]`（未完了）と `[x]`（完了）の数を集計する:

```
lang/v0.1.0/tasks.md
lang/syntax/tasks.md
lang/tests/tasks.md
lang/transpiler/tasks.md
lang/std/v1/tasks.md
lang/extends/tasks.md
lang/generics/tasks.md
lang/modules/tasks.md
lang/package/tasks.md
lang/typedefs/tasks.md
lang/install/tasks.md
lang/packages/http/tasks.md
packages/anvil/tasks.md
```

### 2. cargo test を実行する

```
cargo test --workspace 2>&1
```

### 3. 以下の形式で報告する

```
## ForgeScript 実装進捗レポート

### エリア別進捗
| エリア | 完了 | 総数 | 進捗率 | 残り |
|---|---|---|---|---|
| lang/v0.1.0 コア言語 | X | Y | Z% | R件 |
| lang/syntax エディタ統合 | X | Y | Z% | R件 |
| lang/tests テストフレームワーク | X | Y | Z% | R件 |
| lang/transpiler トランスパイラ | X | Y | Z% | R件 |
| lang/std/v1 標準ライブラリ | X | Y | Z% | R件 |
| lang/extends 言語拡張 | X | Y | Z% | R件 |
| lang/generics ジェネリクス | X | Y | Z% | R件 |
| lang/modules モジュール | X | Y | Z% | R件 |
| lang/package パッケージ | X | Y | Z% | R件 |
| lang/typedefs 型定義 | X | Y | Z% | R件 |
| lang/install インストール/MCP | X | Y | Z% | R件 |
| lang/packages/http HTTPパッケージ | X | Y | Z% | R件 |
| packages/anvil Anvilサーバー | X | Y | Z% | R件 |
| **合計** | **X** | **Y** | **Z%** | **R件** |

### テスト通過状況
実行: X / 通過: X / 失敗: X

### 未完了タスク（上位5件）
1. [ ] タスク説明（ファイル名）
2. [ ] タスク説明（ファイル名）
...
```
