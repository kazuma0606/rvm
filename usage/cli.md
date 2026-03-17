# CLI リファレンス

`forge` コマンドの完全なリファレンスです。

## 概要

```text
forge [command] [args]
```

`forge` は ForgeScript のコマンドラインツールです。引数なしで起動すると対話型 REPL が開始されます。

## コマンド一覧

| コマンド | 説明 |
|---------|------|
| *(引数なし)* | 対話型 REPL を起動 |
| `run <file>` | ForgeScript ファイルを実行 |
| `repl` | 対話型 REPL を起動（明示的） |
| `help` | ヘルプメッセージを表示 |

## コマンド詳細

### 引数なし / `repl`

対話型 REPL (Read-Eval-Print Loop) を起動します。

```bash
forge
# または
forge repl
```

**動作:**
- 標準入力から1行ずつ読み取り、評価して結果を表示
- `exit` または `quit` で終了
- `help` でヘルプ表示
- `clear` で環境をリセット

**例:**

```bash
$ forge
ForgeScript REPL v0.1.0
Type 'exit' or 'quit' to exit, 'help' for help

>>> let x = 1
>>> let y = 2
>>> print(x + y)
3
>>> exit
Goodbye!
```

詳細は [REPL ガイド](repl.md) を参照してください。

---

### `run <file>`

指定した ForgeScript ファイルを実行します。

```bash
forge run <file>
```

**引数:**
- `file` — 実行する `.fs` ファイルのパス（相対パスまたは絶対パス）

**動作:**
1. ファイルを UTF-8 として読み込み
2. 構文解析 → コンパイル → 実行
3. エラー時は標準エラーに診断を出力して終了コード 1 で終了

**例:**

```bash
# カレントディレクトリのファイル
forge run program.fs

# 相対パス
forge run fixtures/e2e/arithmetic.fs

# 絶対パス (Windows)
forge run C:\Users\me\scripts\calc.fs
```

**エラー例:**

```bash
$ forge run missing.fs
Error reading file 'missing.fs': 指定されたパスが見つかりません。 (os error 2)

$ forge run syntax_error.fs
Parse error: Unexpected token at 3:5
```

---

### `help`

ヘルプメッセージを表示します。

```bash
forge help
# または
forge --help
forge -h
```

**出力例:**

```text
ForgeScript (forge) v0.1.0

Usage: forge [command] [args]

Commands:
  (no args)         Start interactive REPL
  run <file>        Run a ForgeScript file
  repl              Start interactive REPL
  help              Show this help message

Examples:
  forge                        # Start REPL
  forge run program.fs         # Run a file
  forge repl                   # Start REPL explicitly
```

## 終了コード

| コード | 意味 |
|-------|------|
| `0` | 正常終了 |
| `1` | エラー（ファイル読み込み失敗、構文エラー、コンパイルエラー、実行時エラー） |

## 環境変数

現バージョン (v0.1.0) では、`forge` は環境変数に依存しません。

## ファイル形式

- **拡張子**: `.fs` を推奨
- **エンコーディング**: UTF-8
- **改行**: LF または CRLF に対応

## 使用例

### 開発中の試行錯誤

```bash
forge
>>> let result = 2 + 3 * 4
>>> print(result)
14
>>> exit
```

### スクリプトの実行

```bash
forge run scripts/daily_report.fs
```

### ヘルプの確認

```bash
forge help
```

## 関連ドキュメント

- [REPL ガイド](repl.md) — 対話モードの詳細
- [エラーとトラブルシューティング](errors.md) — エラー対処法
