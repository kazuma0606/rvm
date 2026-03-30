# ForgeScript 言語仕様 v0.0.1

> 対象バージョン: forge 0.0.1
> ファイル拡張子: `.forge`
> 設計思想の詳細: `dev/design-v2.md` / `dev/design-v3.md` を参照

---

## 1. 字句規則

### 1-1. コメント

```forge
// これは行コメント
let x = 1  // 行末コメントも可
```

ブロックコメント（`/* */`）は v0.0.1 では未対応。

### 1-2. リテラル

```forge
// 整数（i64）
42
-1
1_000_000   // アンダースコア区切り

// 浮動小数点（f64）
3.14
-0.5
1.0e10

// 文字列（UTF-8）
"hello"
"改行は\nエスケープ"
"タブは\t"

// 文字列補間（{式} を埋め込む）
"Hello, {name}!"
"合計: {a + b}"

// 真偽値
true
false
```

### 1-3. 識別子とキーワード

識別子: `[a-zA-Z_][a-zA-Z0-9_]*`

予約キーワード:

```
let     state   const   fn      return
if      else    for     in      while
match   true    false   none    some
ok      err     and     or      not
```

### 1-4. 演算子・区切り文字

```
算術:    +  -  *  /  %
比較:    ==  !=  <  >  <=  >=
論理:    &&  ||  !
代入:    =
型注釈:  :
戻り値:  ->
アロー:  =>
伝播:    ?
範囲:    ..=  ..
区切:    ( )  [ ]  { }  ,  ;
```

---

## 2. 型システム

### 2-1. 組み込み型

| ForgeScript | Rust変換先 | 説明 |
|---|---|---|
| `number` | `i64` | 符号付き64bit整数 |
| `float` | `f64` | 64bit浮動小数点 |
| `string` | `String` | UTF-8文字列 |
| `bool` | `bool` | 真偽値 |
| `list<T>` | `Vec<T>` | 可変長リスト |

### 2-2. Option / Result 型

```forge
T?        // Option<T>: 値があるかもしれない
T!        // Result<T, anyhow::Error>: 失敗するかもしれない
T![E]     // Result<T, E>: カスタムエラー型（v0.0.1では予約のみ）
```

```forge
// Option の値
let a: string? = some("hello")
let b: string? = none

// Result の値
let c: number! = ok(42)
let d: number! = err("失敗しました")
```

### 2-3. 型注釈（省略可能）

```forge
let x: number = 10      // 型注釈あり
let y = 10              // 型推論（number と推論）
let z: string? = none   // Option型は明示推奨
```

---

## 3. バインディング

```forge
// 不変バインディング（デフォルト）
let name: string = "Alice"
let count = 0

// 可変バインディング（変化する値）
state score: number = 0
state items: list<string> = []

// コンパイル時定数（モジュールスコープ）
const MAX_SIZE: number = 100
const PI: float = 3.14159
```

- `let` は再代入不可
- `state` は再代入可能（`let mut` に変換）
- `const` はコンパイル時に確定する値のみ代入可能

---

## 4. 式

ForgeScript では `if` / `for` / `match` / ブロック `{ }` はすべて**式**であり値を返す。

### 4-1. 算術・比較・論理

```forge
let a = 10 + 3 * 2       // 演算子優先順位は標準的
let b = (10 + 3) * 2
let c = a == b            // bool
let d = a > 0 && b < 100  // 論理演算
let e = !d
```

### 4-2. if 式

```forge
// if は値を返す
let label = if score > 90 { "A" } else { "B" }

// 文として使う場合も可
if x > 0 {
    print("正")
} else {
    print("非正")
}

// else if
let grade = if score >= 90 { "A" }
            else if score >= 70 { "B" }
            else if score >= 50 { "C" }
            else { "D" }
```

### 4-3. while

```forge
state i = 0
while i < 10 {
    print(i)
    i = i + 1
}
```

### 4-4. for 式

```forge
// for はイテレータを消費し、最後の式の値のリストを返す
let doubled: list<number> = for x in [1, 2, 3] { x * 2 }

// 値を使わない場合は文として扱う
for item in items {
    print(item)
}

// 範囲リテラル
for i in [1..=10] {
    print(i)
}
```

### 4-5. ブロック式

```forge
// ブロックの最後の式が値になる（セミコロンなし）
let result = {
    let a = compute_a()
    let b = compute_b()
    a + b       // この値が result に入る
}
```

### 4-6. 文字列補間

```forge
let name = "Alice"
let age = 30
let msg = "Hello, {name}! You are {age} years old."

// 式も埋め込める
let info = "Score: {if score > 90 { "A" } else { "B" }}"
```

---

## 5. 関数

### 5-1. 関数定義

```forge
// 基本
fn add(a: number, b: number) -> number {
    a + b   // 最後の式が戻り値（return省略可）
}

// return を使う場合
fn divide(a: number, b: number) -> number! {
    if b == 0 {
        return err("ゼロ除算")
    }
    ok(a / b)
}

// 戻り値なし（型省略）
fn greet(name: string) {
    print("Hello, {name}!")
}
```

### 5-2. 関数呼び出し

```forge
let sum = add(10, 20)
let result = divide(10, 0)?   // ? で Result を伝播
```

### 5-3. クロージャ（アロー記法）

```forge
// 単引数（括弧省略可）
let double = x => x * 2

// 複数引数
let add = (a, b) => a + b

// 引数なし
let greet = () => print("Hello!")

// 複数行ブロック（最後の式が戻り値）
let process = x => {
    let y = x * 2
    y + 1
}

// 高階関数への渡し方
let nums = [1, 2, 3, 4, 5]
let evens = nums.filter(x => x % 2 == 0)
let doubled = nums.map(x => x * 2)
```

- Rust の `|x|` 記法も受け付けるが非推奨（`forge fmt` が `x =>` に変換）

---

## 6. match 式

```forge
// 基本
match value {
    0 => print("ゼロ"),
    1 => print("イチ"),
    _ => print("その他"),     // ワイルドカード（必須）
}

// Option の分解
let name: string? = find_name()
match name {
    some(n) => print("Found: {n}"),
    none    => print("Not found"),
}

// Result の分解
let result: number! = parse_int("42")
match result {
    ok(n)    => print("Value: {n}"),
    err(msg) => print("Error: {msg}"),
}

// match は式
let label = match score {
    90..=100 => "A",
    70..=89  => "B",
    50..=69  => "C",
    _        => "D",
}

// if let（match の糖衣構文）
if let some(n) = find_name() {
    print("Found: {n}")
}
```

---

## 7. Option / Result の操作

### 7-1. ? 演算子（エラー伝播）

```forge
fn read_and_parse(s: string) -> number! {
    let trimmed = trim(s)?    // T! → エラーなら即return
    let n = parse_int(trimmed)?
    ok(n * 2)
}
```

### 7-2. 組み込みメソッド

```forge
let x: number? = some(42)

x.is_some()          // bool
x.is_none()          // bool
x.unwrap_or(0)       // T（none なら 0）
x.map(v => v * 2)    // number?（some の中を変換）

let r: number! = ok(42)
r.is_ok()            // bool
r.is_err()           // bool
r.unwrap_or(0)       // T（err なら 0）
r.map(v => v * 2)    // number!
```

---

## 8. コレクション

### 8-1. list<T>

```forge
// リテラル
let nums: list<number> = [1, 2, 3, 4, 5]
let empty: list<string> = []

// 範囲リテラル
let range = [1..=100]        // [1, 2, ..., 100]
let range2 = [0..10]         // [0, 1, ..., 9]（末端除外）

// インデックスアクセス
let first = nums[0]           // number
let last = nums[nums.len() - 1]
```

### 8-2. イテレータメソッド（v0.0.1 実装範囲）

**変換**
```forge
nums.map(x => x * 2)          // list<number>
nums.filter(x => x > 0)       // list<number>
nums.flat_map(x => [x, x*2])  // list<number>
```

**集計**
```forge
nums.sum()                     // number
nums.count()                   // number
nums.any(x => x > 0)           // bool
nums.all(x => x > 0)           // bool
nums.fold(0, (acc, x) => acc + x)  // number
nums.first()                   // number?
nums.last()                    // number?
```

**ソート・その他**
```forge
nums.order_by(x => x)          // list<number>
nums.order_by_descending(x => x)
nums.take(3)                   // list<number>
nums.skip(2)                   // list<number>
nums.distinct()                // list<number>
nums.reverse()                 // list<number>
```

**収集**
```forge
nums.collect()                 // list<number>（チェーンの終端）
```

---

## 9. 組み込み関数

```forge
print(value)         // 任意の値を出力
println(value)       // 改行付き出力（print と同じ動作）
string(value)        // 任意の値を string に変換
number(value)        // string/float を number に変換（失敗時 number!）
float(value)         // string/number を float に変換（失敗時 float!）
len(value)           // string/list の長さ
type_of(value)       // 型名を string で返す（デバッグ用）
```

---

## 10. エラーハンドリング

```forge
// 関数内で ? を使うには戻り値が T! である必要がある
fn process(input: string) -> string! {
    let n = number(input)?          // 変換失敗でreturn err(...)
    let doubled = n * 2
    ok(string(doubled))
}

// カスタムエラーメッセージ
fn validate(age: number) -> number! {
    if age < 0 {
        err("年齢は0以上である必要があります: {age}")
    } else {
        ok(age)
    }
}

// 呼び出し元での処理
match process("42") {
    ok(result) => print(result),
    err(msg)   => print("エラー: {msg}"),
}
```

---

## 11. スコープとシャドーイング

```forge
let x = 10
{
    let x = 20    // 内側のスコープでシャドーイング可能
    print(x)      // 20
}
print(x)          // 10

// let は同名で再宣言可能（シャドーイング）
let result = compute()
let result = result * 2   // 前の result をシャドーイング
```

---

## 12. 未対応（v0.0.1 スコープ外）

以下は v0.0.1 では実装しない。設計方針は `dev/design-v2.md` / `dev/design-v3.md` 参照。

- `struct` / `enum` / `trait` / `impl`
- `typestate` キーワード
- `derive` キーワード
- ジェネリクス（`<T>`）
- `async` / `await`
- `use`（モジュール・クレートインポート）
- `forge build`（Rustトランスパイラ）
- クエリ式（`from ... where ... select`）
- `use raw`（エスケープハッチ）
- 名前付き引数・デフォルト引数
