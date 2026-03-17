# サンプルプログラム

ForgeScript の実用的なサンプルコード集です。

## 基本

### 変数宣言と参照

```fs
let x = 10
let y = 20
let sum = x + y
print(sum)
```

出力: `30`

### 四則演算

```fs
let a = 100
let b = 7

let sum = a + b
let diff = a - b
let product = a * b
let quotient = a / b

print(sum)
print(diff)
print(product)
print(quotient)
```

出力:
```text
107
93
700
14
```

### 演算子の優先順位

```fs
let result = 2 + 3 * 4
print(result)
```

出力: `14` （`3 * 4` が先に評価される）

```fs
let with_parens = (2 + 3) * 4
print(with_parens)
```

出力: `20`

---

## 文字列

### 文字列リテラル

```fs
let greeting = "Hello, World"
print(greeting)
```

出力: `Hello, World`

### 文字列連結

```fs
let first = "Hello"
let space = " "
let second = "World"
let greeting = first + space + second
print(greeting)
```

出力: `Hello World`

### 変数と文字列の組み合わせ

```fs
let name = "Alice"
let greeting = "Hello, " + name
print(greeting)
```

出力: `Hello, Alice`

---

## 計算例

### 簡単な計算機

```fs
let a = 10
let b = 20
let sum = a + b
let diff = a - b
let product = a * b
let quotient = b / a

print(sum)
print(diff)
print(product)
print(quotient)
```

### 複合式

```fs
let a = 10
let b = 20
let c = 30
let result = a + b * c
print(result)
```

出力: `610` （`b * c` が先に評価され、`20 * 30 = 600`、`10 + 600 = 610`）

### 括弧を使った計算

```fs
let a = 10
let b = 20
let c = 30
let result = (a + b) * c
print(result)
```

出力: `900` （`(10 + 20) * 30 = 900`）

---

## 実用的な例

### 挨拶メッセージ

```fs
let name = "World"
let greeting = "Hello, " + name + "!"
print(greeting)
```

出力: `Hello, World!`

### 数値の表示

```fs
let value = 42
print(value)
let doubled = value * 2
print(doubled)
```

出力:
```text
42
84
```

### 複数変数の連鎖

```fs
let x = 1
let y = x + 1
let z = y + 1
let total = x + y + z
print(total)
```

出力: `6` （1 + 2 + 3）

---

## エラーになる例（参考）

### 未定義変数

```fs
let y = x + 1
```

→ `Compile error: Undefined variable 'x'`

### 0 除算

```fs
let x = 0
let result = 10 / x
```

→ `Runtime error: Division by zero`

### 構文エラー

```fs
let x =
```

→ `Parse error: Unexpected end of input`

---

## ファイルの場所

プロジェクト内のサンプルファイル:

| ファイル | 説明 |
|---------|------|
| `fixtures/hello.fs` | 基本的な挨拶 |
| `fixtures/print_test.fs` | print の使用例 |
| `fixtures/demo_calc.fs` | 四則演算のデモ |
| `fixtures/e2e/arithmetic.fs` | 算術演算の E2E テスト用 |
| `fixtures/e2e/variables.fs` | 変数の E2E テスト用 |
| `fixtures/e2e/string_concat.fs` | 文字列連結の E2E テスト用 |

実行例:

```bash
forge run fixtures/demo_calc.fs
forge run fixtures/e2e/arithmetic.fs
```

---

## 関連ドキュメント

- [言語リファレンス](language-reference.md) — 文法の詳細
- [クイックスタート](quick-start.md) — 実行方法
- [エラーとトラブルシューティング](errors.md) — エラー対処法
