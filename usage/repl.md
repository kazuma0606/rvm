# REPL ガイド

ForgeScript の対話型インタプリタ (REPL) の使い方です。

## 概要

REPL (Read-Eval-Print Loop) は、1行ずつコードを入力して即座に評価・実行できる対話環境です。Python の `python`、Ruby の `irb` と同様の機能です。

## 起動方法

```bash
forge
```

または

```bash
forge repl
```

## プロンプト

起動すると次のように表示されます:

```text
ForgeScript REPL v0.1.0
Type 'exit' or 'quit' to exit, 'help' for help

>>>
```

`>>>` が入力待ちのプロンプトです。

## REPL 専用コマンド

REPL 内で以下のコマンドが使えます（ForgeScript の文法とは別です）:

| コマンド | 説明 |
|---------|------|
| `exit` | REPL を終了 |
| `quit` | REPL を終了（`exit` と同様） |
| `help` | ヘルプメッセージを表示 |
| `clear` | 定義した変数をすべてクリアし、環境をリセット |

**例:**

```text
>>> help
ForgeScript REPL Commands:
  exit, quit    Exit the REPL
  help          Show this help message
  clear         Clear the environment

Supported ForgeScript syntax:
  let x = 1              Variable declaration
  let y = x + 2          Arithmetic operations
  let s = "Hello"        String literals
  let t = s + " World"   String concatenation

>>> clear
Environment cleared
>>> exit
Goodbye!
```

## 変数の永続性

REPL では、一度定義した変数は次の入力まで保持されます。

```text
>>> let x = 10
>>> let y = 20
>>> let sum = x + y
>>> print(sum)
30
>>> print(x)
10
```

`clear` を実行するまで、変数は保持されます。

## エラー時の動作

構文エラーや実行時エラーが発生しても、REPL は終了せずにエラーメッセージを表示して次の入力を待ちます。

```text
>>> let x = undefined_var
Error: Undefined variable 'undefined_var' at 8:21

>>> let x =
Error: Unexpected end of input at 7:7

>>> let x = 10
>>> print(x)
10
```

## 使用例

### 四則演算の確認

```text
>>> let a = 10
>>> let b = 20
>>> print(a + b)
30
>>> print(a * b)
200
>>> print(b / a)
2
```

### 演算子の優先順位の確認

```text
>>> let result = 2 + 3 * 4
>>> print(result)
14
>>> let with_parens = (2 + 3) * 4
>>> print(with_parens)
20
```

### 文字列操作

```text
>>> let hello = "Hello"
>>> let world = " World"
>>> let greeting = hello + world
>>> print(greeting)
Hello World
```

### ネイティブ関数の使用

```text
>>> print(42)
42
>>> print("Hello")
Hello
>>> let x = 100
>>> print(x)
100
```

### 環境のリセット

```text
>>> let x = 1
>>> let y = 2
>>> clear
Environment cleared
>>> print(x)
Error: Undefined variable 'x' at 6:6
```

## 制限事項

- **1行入力**: 複数行にわたる入力には対応していません
- **式の結果表示**: `let` 文の評価結果は表示されません（`print` で明示的に出力する必要があります）
- **履歴**: コマンド履歴（上下キー）は未実装です
- **補完**: オートコンプリートは未実装です

## 終了方法

- `exit` または `quit` を入力
- 標準入力の終端 (Ctrl+D / Ctrl+Z) を送信

```text
>>> exit
Goodbye!
```

## 関連ドキュメント

- [CLI リファレンス](cli.md) — `forge` コマンドの詳細
- [言語リファレンス](language-reference.md) — ForgeScript の文法
- [サンプルプログラム](examples.md) — コード例
