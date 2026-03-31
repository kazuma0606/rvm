# ForgeScript トランスパイラ仕様（forge build）

> ステータス: 設計中（未実装）
> 対象: forge 0.2.0 以降
> クレート: `forge-transpiler`

---

## 概要

`forge build` は ForgeScript のソースコードを Rust コードに変換し、`rustc` でネイティブバイナリを生成する。

```
.forge ソース
  → forge-compiler（Lexer → Parser → AST → TypeChecker）
  → forge-transpiler（CodeGenerator）
  → .rs ファイル
  → rustfmt（整形）
  → rustc（コンパイル）
  → ゼロ依存ネイティブバイナリ
```

`forge run`（インタープリタ）との出力は**完全に等価**でなければならない。

---

## CLI インターフェース

```bash
forge build src/main.forge           # バイナリ生成（デフォルト: target/forge/main）
forge build src/main.forge -o myapp  # 出力先指定
forge transpile src/main.forge       # Rust コード出力のみ（コンパイルしない）
```

---

## 1. 型変換マッピング

| ForgeScript | Rust | 備考 |
|---|---|---|
| `number` | `i64` | 符号付き64bit整数に統一 |
| `float` | `f64` | 64bit浮動小数点に統一 |
| `string` | `String` | 所有権付き文字列 |
| `bool` | `bool` | |
| `list<T>` | `Vec<T>` | |
| `T?` | `Option<T>` | |
| `T!` | `Result<T, anyhow::Error>` | |
| `T![E]` | `Result<T, E>` | カスタムエラー型 |

---

## 2. バインディング変換

```forge
let x = 42                  →  let x: i64 = 42;
let x: number = 42          →  let x: i64 = 42;
state count = 0             →  let mut count: i64 = 0;
const PI: float = 3.14159   →  const PI: f64 = 3.14159;
```

---

## 3. 関数変換

```forge
fn add(a: number, b: number) -> number {
    a + b
}
```
```rust
fn add(a: i64, b: i64) -> i64 {
    a + b
}
```

```forge
fn divide(a: number, b: number) -> number! {
    if b == 0 { return err("ゼロ除算") }
    ok(a / b)
}
```
```rust
fn divide(a: i64, b: i64) -> Result<i64, anyhow::Error> {
    if b == 0 { return Err(anyhow::anyhow!("ゼロ除算")); }
    Ok(a / b)
}
```

### エントリーポイント

`fn main()` は `Result` を返すように変換される。

```forge
fn main() {
    println("Hello!")
}
```
```rust
fn main() -> Result<(), anyhow::Error> {
    println!("Hello!");
    Ok(())
}
```

---

## 4. 制御フロー変換

### if 式

```forge
let label = if score > 90 { "A" } else { "B" }
```
```rust
let label = if score > 90 { "A" } else { "B" };
```

### for

```forge
for x in items { println(x) }
```
```rust
for x in &items { println!("{}", x); }
```

### while

```forge
while i < 10 { i = i + 1 }
```
```rust
while i < 10 { i += 1; }
```

### match

```forge
match result {
    ok(v)  => println(v),
    err(e) => println(e),
}
```
```rust
match result {
    Ok(v)  => println!("{}", v),
    Err(e) => println!("{}", e),
}
```

---

## 5. 文字列補間

```forge
"Hello, {name}!"
"sum = {a + b}"
"grade = {if score > 90 { "A" } else { "B" }}"
```
```rust
format!("Hello, {}!", name)
format!("sum = {}", a + b)
format!("grade = {}", if score > 90 { "A" } else { "B" })
```

---

## 6. クロージャ変換

```forge
let double = x => x * 2
let add    = (a, b) => a + b
```
```rust
let double = |x: i64| -> i64 { x * 2 };
let add    = |a: i64, b: i64| -> i64 { a + b };
```

### Fn / FnMut / FnOnce 推論

| キャプチャの種類 | 推論結果 | 変換 |
|---|---|---|
| `let` / `const` を読むだけ | `Fn` | `\|x\| ...` |
| `state` 変数を変更する | `FnMut` | `move \|x\| ...` |
| 変数を消費する（1回限り） | `FnOnce` | `move \|x\| ...` |

---

## 7. 組み込み関数変換

| ForgeScript | Rust |
|---|---|
| `print(x)` | `print!("{}", x)` |
| `println(x)` | `println!("{}", x)` |
| `string(x)` | `x.to_string()` |
| `number(x)` | `x.to_string().parse::<i64>()?` |
| `float(x)` | `x.to_string().parse::<f64>()?` |
| `len(x)` | `x.len()` |
| `type_of(x)` | `std::any::type_name_of_val(&x)` |

---

## 8. Option / Result 変換

```forge
some("hello")   →  Some("hello".to_string())
none            →  None
ok(42)          →  Ok(42_i64)
err("失敗")     →  Err(anyhow::anyhow!("失敗"))

x?              →  x?
x.is_some()     →  x.is_some()
x.is_none()     →  x.is_none()
x.unwrap_or(0)  →  x.unwrap_or(0)
x.map(f)        →  x.map(f)
```

---

## 9. コレクション変換

```forge
[1, 2, 3]                         →  vec![1_i64, 2, 3]
nums.map(x => x * 2)              →  nums.iter().map(|x| x * 2).collect::<Vec<_>>()
nums.filter(x => x > 0)           →  nums.iter().filter(|x| **x > 0).collect::<Vec<_>>()
nums.fold(0, (acc, x) => acc + x) →  nums.iter().fold(0, |acc, x| acc + x)
nums.sum()                        →  nums.iter().sum::<i64>()
nums.count()                      →  nums.len()
nums.any(x => x > 0)              →  nums.iter().any(|x| *x > 0)
nums.all(x => x > 0)              →  nums.iter().all(|x| *x > 0)
nums.first()                      →  nums.first().copied()
nums.last()                       →  nums.last().copied()
nums.reverse()                    →  { let mut v = nums.clone(); v.reverse(); v }
nums.distinct()                   →  { let mut v = nums.clone(); v.dedup(); v }
```

---

## 10. 生成コードの構造

```rust
// forge build が生成する Rust ファイルのテンプレート
use anyhow;  // T! に使用する場合のみ

// ユーザー定義関数
fn greet(name: String) -> String { ... }

// エントリーポイント
fn main() -> Result<(), anyhow::Error> {
    // ユーザーのトップレベルコード
    Ok(())
}
```

---

## 11. テスト戦略

### Level 1: スナップショットテスト
ForgeScript → 期待する Rust コードの文字列比較。

### Level 2: ラウンドトリップテスト
`forge run` と `forge build → 実行` の標準出力が完全一致することを検証。
既存の E2E テスト（`forge-cli/tests/e2e.rs`）をラウンドトリップテストとして転用できる。

---

## 12. 未対応（Phase B-5 以降）

- `struct` / `data` / `enum` / `impl` / `trait`
- モジュールシステム（`use ./module`）
- `use raw {}` ブロック
- 外部クレートのインポート
- `async` / `await`
- ジェネリクス（`<T>`）
- `typestate` / `mixin` / `when`
