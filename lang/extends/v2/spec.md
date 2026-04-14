# ForgeScript 言語拡張 v2 仕様書

> バージョン: 0.2.0-extends
> 前提: v0.1.0 コア言語・extends v1（E-1〜E-7）完成済み
> 背景: examples 追加作業で発見した実用上の制約を解消する

---

## 拡張一覧

| ID   | 機能                        | 優先度 | 前提         |
|------|-----------------------------|--------|--------------|
| E2-1 | 分割代入                    | 高     | なし         |
| E2-2 | Option 型メソッド拡充       | 高     | なし         |
| E2-3 | 匿名 struct                 | 中     | E2-1 推奨    |

---

## E2-1: 分割代入（Destructuring Assignment）

### 背景

`partition()` / `zip()` / `chunk()` などが 2〜n 要素のリストを返すが、
現状は `parts[0]` / `parts[1]` と添字アクセスするしかなく冗長。

```forge
// 現状（不便）
let parts = nums |> partition(n => n % 2 == 0)
let evens = parts[0]
let odds  = parts[1]

// E2-1 実装後
let (evens, odds) = nums |> partition(n => n % 2 == 0)
```

### 構文

```
let_stmt ::= "let" pattern "=" expr

pattern  ::= IDENT                          // 従来の単一束縛
           | "(" pattern_list ")"           // タプル風分割代入
           | "[" pattern_list "]"           // リストパターン（同義）

pattern_list ::= pattern ("," pattern)*
               | pattern ("," pattern)* "," ".." IDENT  // 残余パターン

pattern  ::= IDENT | "_"                   // ワイルドカード
```

### 使用例

```forge
// 2要素
let (a, b) = [1, 2]

// partition の典型パターン
let (evens, odds) = nums |> partition(n => n % 2 == 0)

// zip の典型パターン
let pairs = [1, 2, 3] |> zip(["a", "b", "c"])
for (n, s) in pairs {
    println("{n}: {s}")
}

// ワイルドカード（不要な要素を無視）
let (first, _) = chunk_result

// 3要素以上
let (x, y, z) = [10, 20, 30]

// 残余パターン（head + tail）
let (head, ..tail) = [1, 2, 3, 4, 5]
// head = 1, tail = [2, 3, 4, 5]

// ブラケット記法（同義）
let [a, b, c] = [1, 2, 3]
```

### 意味論

1. 右辺を評価してリスト値を得る
2. パターンの要素数 ≤ リストの長さ であればマッチ（余りは無視）
3. 残余パターン `..name` がある場合: 残りの要素をリストとして束縛
4. 要素数が足りない場合はランタイムエラー
5. `_` は束縛を作らない（無視）

### `for` ループへの拡張

```forge
// for ループでも分割代入を使えるようにする
for (i, v) in list |> enumerate() {
    println("{i}: {v}")
}

for (k, v) in map_value {
    println("{k} => {v}")
}
```

### Rust 変換

```rust
// let (a, b) = expr;
let _destructure = expr;
let a = _destructure[0].clone();
let b = _destructure[1].clone();

// let (head, ..tail) = expr;
let _destructure = expr;
let head = _destructure[0].clone();
let tail = _destructure[1..].to_vec();
```

### 制約

- 現フェーズではネストした分割代入は対象外（`let ((a, b), c) = ...`）
- 右辺がリスト以外（struct・map）への分割代入は E2-3 以降
- `for` ループでの分割代入は E2-1 と同時に実装する

---

## E2-2: Option 型メソッド拡充

### 背景

`max()` / `min()` / `find()` などが `Option` を返すが、
現状は `match` でしか処理できず冗長。

```forge
// 現状
match nums |> max() {
    some(v) => assert_eq(v, 9),
    none    => assert(false),
}

// E2-2 実装後
let max_val = nums |> max() |> unwrap_or(0)
let doubled = find_user(1) |> map(u => u.name)
```

### 追加メソッド一覧

#### `unwrap_or(default: T) -> T`

`some(v)` なら `v`、`none` なら `default` を返す。

```forge
let name = find_user(99) |> unwrap_or("ゲスト")       // "ゲスト"
let val  = nums |> max() |> unwrap_or(0)              // 0
```

#### `unwrap() -> T`

`some(v)` なら `v`、`none` なら実行時エラー（panic）。
確実に値がある場合のみ使用する。

```forge
let first = non_empty_list |> find(n => n > 0) |> unwrap()
```

#### `map(fn: T -> U) -> U?`

`some(v)` なら `fn(v)` を `some` に包んで返す。`none` はそのまま `none`。

```forge
let upper_name = find_user(1) |> map(u => u.name.to_upper())
// some("ALICE") または none
```

#### `and_then(fn: T -> U?) -> U?`

`some(v)` なら `fn(v)`（`U?` を返す関数）を呼ぶ。`none` はそのまま `none`。
`flat_map` とも呼ばれる。

```forge
fn parse_id(s: string) -> number? { ... }
let user = get_param("id") |> and_then(s => parse_id(s)) |> and_then(id => find_user(id))
```

#### `is_some() -> bool`

値があるかを返す。

```forge
if find_user(1) |> is_some() {
    println("ユーザーが存在します")
}
```

#### `is_none() -> bool`

値がないかを返す。

```forge
if find_user(99) |> is_none() {
    println("ユーザーが見つかりません")
}
```

#### `or(default: T?) -> T?`

`none` のとき代替の `Option` を返す。

```forge
let result = find_by_email(email) |> or(find_by_name(name))
```

#### `filter(fn: T -> bool) -> T?`

`some(v)` で `fn(v)` が `true` なら `some(v)`、`false` なら `none`。

```forge
let adult = find_user(1) |> filter(u => u.age >= 18)
```

### `|>` との組み合わせ

Option メソッドはパイプ演算子と組み合わせて使うのが基本スタイル：

```forge
// Option チェーン
let result = get_config("timeout")
    |> map(s => number(s))       // string -> number?
    |> and_then(n => n)          // number? のまま通過
    |> unwrap_or(30)             // デフォルト 30 秒

// find + map + unwrap_or
let top_scorer = students
    |> find(s => s.score >= 90)
    |> map(s => s.name)
    |> unwrap_or("該当者なし")
```

### インタープリタでの実装方針

`Value::Option(Option<Box<Value>>)` に対してメソッドディスパッチを追加する。
`interpreter.rs` の `call_method` 内に `"unwrap_or" | "map" | ...` のアームを追加。

### Rust 変換

| ForgeScript                    | Rust                            |
|--------------------------------|---------------------------------|
| `opt \|> unwrap_or(default)`   | `opt.unwrap_or(default)`        |
| `opt \|> unwrap()`             | `opt.unwrap()`                  |
| `opt \|> map(f)`               | `opt.map(f)`                    |
| `opt \|> and_then(f)`          | `opt.and_then(f)`               |
| `opt \|> is_some()`            | `opt.is_some()`                 |
| `opt \|> is_none()`            | `opt.is_none()`                 |
| `opt \|> or(other)`            | `opt.or(other)`                 |
| `opt \|> filter(f)`            | `opt.filter(f)`                 |

### 制約

- `map` の `fn` の戻り値が `T?` の場合、`map` ではなく `and_then` を使う（フラット化しない）
- `unwrap()` は本番コードでは避けることを推奨（ドキュメントで注意喚起）
- Result 型への同様の拡充は E2-2 の後続タスクとして別途対応

---

## E2-3: 匿名 struct

### 背景

関数の戻り値や一時的なデータ構造に、名前付き struct を毎回定義するのは冗長。
TypeScript / Go のインラインオブジェクト型のような記法を追加する。

```forge
// 現状（毎回 struct を定義する必要がある）
struct UserSummary { name: string, score: number }
fn summarize(s: Student) -> UserSummary {
    UserSummary { name: s.name, score: s.score }
}

// E2-3 実装後
fn summarize(s: Student) -> { name: string, score: number } {
    { name: s.name, score: s.score }
}
```

### 構文

#### 匿名 struct 型

```
anon_struct_type ::= "{" field_type_list "}"
field_type_list  ::= field_type ("," field_type)* ","?
                   | field_type (NEWLINE field_type)*
field_type       ::= IDENT ":" type_expr
```

#### 匿名 struct リテラル

```
anon_struct_lit ::= "{" field_value_list "}"
field_value_list ::= field_value ("," field_value)* ","?
field_value      ::= IDENT ":" expr
                   | IDENT          // ショートハンド: { x } = { x: x }
```

### 使用例

```forge
// 戻り値型に使う
fn make_point(x: number, y: number) -> { x: number, y: number } {
    { x: x, y: y }
}

// ショートハンド（変数名 = フィールド名のとき省略可）
let x = 10
let y = 20
let p = { x, y }   // { x: x, y: y } と同義

// 型注釈なしのリテラル（型推論）
let user = { name: "Alice", role: "admin" }
println(user.name)   // "Alice"

// リストの要素型として
state users: list<{ id: number, name: string }> = []

// 関数引数として
fn greet(user: { name: string }) -> string {
    "Hello, {user.name}!"
}

// パイプと組み合わせ
let summaries = students
    |> map(s => { name: s.name, score: s.score })
    |> filter(s => s.score >= 80)
```

### 意味論

1. **構造的型付け（Structural Typing）**:
   匿名 struct は「フィールド名と型が一致すれば互換」と見なす。
   名前付き struct と匿名 struct も、フィールドが一致すれば代入可能。

   ```forge
   struct Point { x: number, y: number }
   let p: { x: number, y: number } = Point { x: 1, y: 2 }  // OK
   ```

2. **フィールドアクセス**:
   通常の struct と同様に `.field` でアクセス。

3. **ショートハンド**:
   `{ x }` は `{ x: x }` の糖衣構文。スコープ内の変数 `x` を使う。

4. **インタープリタでの表現**:
   内部的には `Value::Struct(HashMap<String, Value>)` として扱う（既存の名前付き struct と同じ）。
   型名を持たない匿名 struct は型名を `"<anon>"` とする。

### 型チェック（現フェーズの制約）

現フェーズではランタイム型チェックのみ。
コンパイル時の構造的型チェックは将来の型システム強化フェーズで対応。

### Rust 変換

```rust
// { name: "Alice", role: "admin" }
// → HashMap ベースの動的値（インタープリタ）
// → Rust トランスパイル時は struct を自動生成

// 自動生成される struct（フィールドのハッシュでユニーク名を生成）
#[derive(Debug, Clone)]
struct AnonStruct_name_role {
    name: String,
    role: String,
}
```

### 制約

- 匿名 struct の再帰型は対象外（`{ node: { node: ... } }`）
- メソッド定義（`impl`）は匿名 struct に対してはできない（名前付き struct に変換してから行う）
- ショートハンド `{ x }` は変数 `x` がスコープに存在する場合のみ有効

---

## 付録: 実装の依存関係

```
E2-1 分割代入
    └─ 独立（パーサー・評価器への追加）
    └─ for ループ拡張も同時対応

E2-2 Option メソッド
    └─ 独立（インタープリタの call_method 追加のみ）

E2-3 匿名 struct
    └─ E2-1 のショートハンド `{ x, y }` と構文が重なるため E2-1 推奨
    └─ レキサー・パーサー・評価器すべてに変更が必要
```

## 実装推奨順序

1. **E2-2** — インタープリタのみ、リスクが最小
2. **E2-1** — パーサー + 評価器、影響範囲が限定的
3. **E2-3** — レキサー〜評価器まで全層、最も広い変更

---

## 変更ファイル一覧（予定）

| ファイル | E2-1 | E2-2 | E2-3 |
|----------|------|------|------|
| `crates/forge-lexer/src/lib.rs` | — | — | ○ |
| `crates/forge-parser/src/lib.rs` | ○ | — | ○ |
| `crates/forge-ast/src/lib.rs` | ○ | — | ○ |
| `crates/forge-vm/src/interpreter.rs` | ○ | ○ | ○ |
| `crates/forge-transpiler/src/lib.rs` | ○ | — | ○ |
