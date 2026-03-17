# クイックスタート

5分で ForgeScript を始めるためのガイドです。

## 前提条件

- **Rust** 1.75 以上がインストールされていること
- プロジェクトのルートディレクトリにいること

## ビルド

```bash
cargo build --release
```

ビルド後、`target/release/forge.exe` (Windows) または `target/release/forge` (Unix) が生成されます。

## 方法1: REPL で試す（推奨）

対話型インタプリタで即座にコードを試せます。

```bash
forge
```

または

```bash
forge repl
```

起動後、プロンプト `>>>` が表示されます。

```
ForgeScript REPL v0.1.0
Type 'exit' or 'quit' to exit, 'help' for help

>>> let x = 10
>>> let y = 20
>>> print(x + y)
30
>>> exit
Goodbye!
```

詳細は [REPL ガイド](repl.md) を参照してください。

## 方法2: ファイルを実行する

`.fs` ファイルを作成して実行します。

**hello.fs** を作成:

```fs
let greeting = "Hello, World"
print(greeting)
```

実行:

```bash
forge run hello.fs
```

出力:

```
Hello, World
```

## 基本的な構文

### 変数宣言

```fs
let x = 42
let name = "Alice"
```

### 四則演算

```fs
let a = 10
let b = 20
let sum = a + b
let product = a * b
let quotient = b / a
```

### 文字列連結

```fs
let first = "Hello"
let second = " World"
let greeting = first + second
print(greeting)
```

### 出力

```fs
print(42)
print("Hello")
print(x + y)
```

## 次のステップ

- [CLI リファレンス](cli.md) — コマンドの詳細
- [言語リファレンス](language-reference.md) — 文法の完全な仕様
- [サンプルプログラム](examples.md) — 実用的な例
