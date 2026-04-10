# ForgeScript 言語拡張 仕様書

> バージョン: 0.1.0-extends
> 前提: v0.1.0 コア言語・モジュールシステム・トランスパイラ B-0〜B-8 が完成済み
> 参照: `lang/extend_idea.md`（アイデア原文）

---

## 拡張一覧

| ID | 機能 | 優先度 | 前提 |
|---|---|---|---|
| E-1 | `\|>` パイプ演算子 | 高 | なし |
| E-2 | `?.` オプショナルチェーン / `??` null 合体 | 高 | なし |
| E-3 | 演算子オーバーロード（`operator`） | 高 | struct / impl |
| E-4 | 非同期クロージャ完成（`spawn`） | 高 | B-7 async/await |
| E-5 | `const fn` / コンパイル時定数 | 中 | なし |
| E-6 | ジェネレータ / `yield` | 中 | E-4 |

---

## E-1: `|>` パイプ演算子

### 構文

```
pipe_expr ::= expr ("|>" call_suffix)+

call_suffix ::= IDENT "(" args ")"
             | IDENT            // 引数なしメソッドは "()" 省略可
```

### 意味論

`expr |> method(args...)` は `expr.method(args...)` と完全に等価なシンタックスシュガー。
パース時点で AST 上のメソッド呼び出しに変換される（インタープリタ・トランスパイラへの影響なし）。

```forge
// メソッドチェーン（引き続き有効）
let result = list.filter(x => x > 0).map(x => x * 2).fold(0, (acc, x) => acc + x)

// |> スタイル（推奨）
let result = list
    |> filter(x => x > 0)
    |> map(x => x * 2)
    |> fold(0, (acc, x) => acc + x)
```

### 優先度

`|>` は代入より高く、比較演算子より低い。左結合。

```forge
let x = a |> f() |> g()   // g(f(a)) と等価
```

### Rust 変換

パース時に `.method(args)` へ変換済みのため、トランスパイラへの追加実装不要。

### 制約

- `|>` の右辺はメソッド呼び出し形式のみ（変数・クロージャ直接は不可）
- 将来的に `|> free_fn(args)` 形式（自由関数へのパイプ）を E-1b として追加可能

---

## E-2: `?.` オプショナルチェーン / `??` null 合体演算子

### 構文

```
optional_chain ::= expr "?." IDENT ("(" args ")")?
                 | expr "?." "[" expr "]"

null_coalesce  ::= expr "??" expr
```

### 意味論

#### `?.` フィールドアクセス

`expr?.field` は `T?` 型の値に対して、`none` なら即座に `none` を返し、`some(v)` なら `v.field` を返す。

```forge
let city = user?.address?.city

// 展開後の意味
let city = match user {
    some(u) => match u.address {
        some(a) => some(a.city),
        none    => none,
    },
    none => none,
}
```

#### `?.` メソッド呼び出し

```forge
let len = name?.len()
// → match name { some(s) => some(s.len()), none => none }
```

#### `??` null 合体

`expr ?? default` は `expr` が `none` のとき `default` を返す。

```forge
let city = user?.address?.city ?? "unknown"
// 型: string（Option が剥がれる）
```

### 型規則

| 式 | 入力型 | 出力型 |
|---|---|---|
| `expr?.field` | `T?` | `FieldType?` |
| `expr?.method()` | `T?` | `ReturnType?` |
| `expr ?? default` | `T?` | `T`（Option が剥がれる） |

### Rust 変換

```rust
// expr?.field
expr.and_then(|v| Some(v.field))

// expr?.method()
expr.and_then(|v| Some(v.method()))

// expr ?? default
expr.unwrap_or(default)
```

### 制約

- `?.` は `T?` 型にのみ適用可能（`T!` には `?` 演算子を使う）
- `??` の左辺は `T?` 型のみ

---

## E-3: 演算子オーバーロード

### 構文

`impl` ブロック内で `operator <op>` として定義する。

```forge
impl TypeName {
    operator +(self, other: TypeName) -> TypeName { ... }
    operator -(self, other: TypeName) -> TypeName { ... }
    operator *(self, scalar: float)   -> TypeName { ... }
    operator ==(self, other: TypeName) -> bool    { ... }
    operator <(self, other: TypeName)  -> bool    { ... }
    operator [](self, index: number)   -> T       { ... }  // インデックス演算子
    operator unary-(self)              -> TypeName { ... }  // 単項マイナス
}
```

### サポートする演算子

| ForgeScript | Rust trait | 備考 |
|---|---|---|
| `+` | `std::ops::Add` | |
| `-`（二項） | `std::ops::Sub` | |
| `*` | `std::ops::Mul` | |
| `/` | `std::ops::Div` | |
| `%` | `std::ops::Rem` | |
| `==` | `PartialEq` | `!=` も自動で使用可能 |
| `<` | `PartialOrd` | `>` `<=` `>=` も自動 |
| `[]` | `std::ops::Index` | |
| `unary-` | `std::ops::Neg` | |

### 意味論

演算子オーバーロードが定義された型に対し、通常の演算子式が使用可能になる。

```forge
struct Vector2 { x: float, y: float }

impl Vector2 {
    operator +(self, other: Vector2) -> Vector2 {
        Vector2 { x: self.x + other.x, y: self.y + other.y }
    }
    operator *(self, scalar: float) -> Vector2 {
        Vector2 { x: self.x * scalar, y: self.y * scalar }
    }
    operator ==(self, other: Vector2) -> bool {
        self.x == other.x && self.y == other.y
    }
    operator [](self, index: number) -> float {
        if index == 0 { self.x } else { self.y }
    }
}

let v1 = Vector2 { x: 1.0, y: 2.0 }
let v2 = Vector2 { x: 3.0, y: 4.0 }
let v3 = v1 + v2          // Vector2 { x: 4.0, y: 6.0 }
let v4 = v3 * 2.0         // Vector2 { x: 8.0, y: 12.0 }
let eq = v1 == v2         // false
let x  = v4[0]            // 8.0
```

### インタープリタでの解決順序

1. 組み込み型（number / float / string）の演算子 → 既存処理
2. struct 型の演算子 → impl ブロックに `operator <op>` メソッドを探す
3. 見つからない場合 → ランタイムエラー

### Rust 変換

```rust
// operator + の定義
impl std::ops::Add for Vector2 {
    type Output = Vector2;
    fn add(self, other: Vector2) -> Vector2 {
        Vector2 { x: self.x + other.x, y: self.y + other.y }
    }
}

// operator == の定義
impl PartialEq for Vector2 {
    fn eq(&self, other: &Vector2) -> bool {
        self.x == other.x && self.y == other.y
    }
}
```

### 制約

- `@derive(Eq)` と `operator ==` の併用は禁止（競合）
- `@derive(Ord)` と `operator <` の併用は禁止（競合）
- 右辺の型が self と異なる場合（`* scalar`）は、その型を明示する必要がある

---

## E-4: 非同期クロージャ完成 / `spawn`

### 背景

B-3（クロージャのトランスパイル）は tail position の `FnOnce` 判定に留まっている。
`spawn` が未実装のため、非同期クロージャの完全なストーリーが閉じていない。

### `spawn` 構文

```forge
// 非同期タスクを起動（戻り値は handle）
let handle = spawn {
    let result = fetch("https://api.example.com/data").await?
    result
}

// await で結果を待つ
let data = handle.await?
```

### 非同期クロージャの自動昇格

`await` を含むクロージャは自動的に `async` クロージャに昇格する。

```forge
// 書き方（async キーワード不要）
let handler = req => fetch(req.url).await?

// トランスパイラが生成する Rust
let handler = |req| async move { fetch(req.url).await };
```

### `spawn` の Rust 変換

```rust
// spawn { ... }
tokio::spawn(async move { ... })

// handle.await?
handle.await?
```

### 制約

- `spawn` ブロックはキャプチャした変数を `move` する
- `spawn` は `async fn` コンテキストまたはトップレベルの `main` 内でのみ使用可能
- `forge run`（インタープリタ）では `spawn` はシングルスレッド逐次実行として動作

---

## E-5: `const fn` / コンパイル時定数

### 構文

```forge
// コンパイル時定数（既に const は実装済み・式評価の強化）
const MAX_BUFFER = 1024 * 4
const PI         = 3.14159265

// コンパイル時関数
const fn clamp(value: number, min: number, max: number) -> number {
    if value < min { min } else if value > max { max } else { value }
}

const CLAMPED = clamp(150, 0, 100)   // コンパイル時に 100 に評価される
```

### 意味論

`const fn` はコンパイル時に評価可能な関数。制約：

- 副作用なし（`state` への書き込み禁止・`println` 禁止）
- 再帰は許可（末尾再帰のみ推奨）
- 引数・戻り値はすべて `number` / `float` / `bool` / `string` のプリミティブ型のみ
- ループ（`for` / `while`）は許可

`forge run` では通常の関数として実行。`forge build` では Rust の `const fn` に変換。

### Rust 変換

```rust
const fn clamp(value: i64, min: i64, max: i64) -> i64 {
    if value < min { min } else if value > max { max } else { value }
}
const CLAMPED: i64 = clamp(150, 0, 100);
```

### 制約

- 現フェーズでは型レベル変換（`type Nullable<T> = T?` 等）は対象外
- `const fn` 内での struct 生成・メソッド呼び出しは将来拡張

---

## E-6: ジェネレータ / `yield`

### 構文

```forge
fn fibonacci() -> generate<number> {
    state a = 0
    state b = 1
    loop {
        yield a
        let next = a + b
        a = b
        b = next
    }
}

fibonacci()
    |> take(10)
    |> each(n => println(n))
```

### 意味論

`generate<T>` は遅延評価のシーケンスを表す型。`yield` で値を1つずつ生産する。
コレクション API（`map` / `filter` / `take` / `fold` 等）と接続できる。

### Rust 変換方針

Rust の `Generator` は unstable のため、`async fn` + `tokio::sync::mpsc` チャネルによる変換か、
`std::iter::from_fn` クロージャへの変換を採用する。

```rust
// fibonacci() の変換例（from_fn 方式）
fn fibonacci() -> impl Iterator<Item = i64> {
    let mut a = 0i64;
    let mut b = 1i64;
    std::iter::from_fn(move || {
        let val = a;
        let next = a + b;
        a = b;
        b = next;
        Some(val)
    })
}
```

### コレクション API との接続

`generate<T>` はコレクション API のすべてのメソッドを使用可能（遅延評価）。
`take(n)` で有限化してから `each` / `fold` で消費する。

### 制約

- E-6 は E-4 の `spawn` 完成後に着手（非同期ストリームとの統合のため）
- `forge run` では `Iterator` を模倣した内部実装で対応
- `yield` はジェネレータ関数内でのみ使用可能

---

## E-7: `defer` — スコープ終了時の確実な実行

### 背景

Rust の RAII（Drop trait）はリソース解放を保証するが、暗黙的で読みにくい。
Go の `defer` は「ここで後片付けを宣言する」という意図を明示できる。
ForgeScript では言語キーワードとして `defer` を追加し、ファイル・DB 接続・ロック等のクリーンアップを明示的に書けるようにする。

### 構文

```forge
defer expr
defer { block }
```

`defer` はスコープ（関数・ブロック）終了時に実行される。
複数の `defer` がある場合は LIFO（後入れ先出し）順で実行される。
正常終了・エラー終了（`?` による早期リターン）どちらでも実行が保証される。

### 使用例

```forge
fn process_file(path: string) -> unit! {
    let f = open_file(path)?
    defer f.close()               // スコープ終了時に必ず実行

    let data = f.read_all()?
    transform(data)?
    // f.close() はここで自動実行（エラーで抜けても）
}

fn with_transaction(db: DbConnection) -> unit! {
    db.begin()?
    defer db.rollback()           // コミット前にエラーが起きたら rollback

    db.execute("INSERT ...")?
    db.execute("UPDATE ...")?
    db.commit()?                  // 成功時は commit → defer の rollback は no-op
}

fn acquire_locks() -> unit! {
    lock_a.acquire()?
    defer lock_a.release()        // 後入れ先出しで解放される順序が明確

    lock_b.acquire()?
    defer lock_b.release()        // lock_b → lock_a の順で解放
}
```

### Rust 変換

```rust
// defer f.close()  →  let _guard = scopeguard::defer(|| f.close());
```

内部実装は `scopeguard` クレートの `defer!` マクロに変換する。
`forge build` では `scopeguard::defer` を使用。`forge run` ではインタープリタがスコープ終了フックとして管理する。

### `@defer` デコレータ（メソッドへの自動適用）

```forge
// メソッド呼び出し後に自動で cleanup を defer する
@defer(cleanup: "close")
fn open_file(path: string) -> File! { ... }

// 使う側は defer 不要
fn read_config(path: string) -> Config! {
    let f = open_file(path)?   // close() が自動 defer される
    parse_config(f.read_all()?)
}
```

### 制約

- `defer` はブロック内の式のみ（`defer let x = ...` は不可）
- `defer` ブロック内でエラーが発生した場合は無視される（ログ出力推奨）
- `defer` は関数スコープではなくブロックスコープで有効（`if` ブロック内の `defer` はそのブロック終了時に実行）

---

## 付録: 実装の依存関係

```
E-1 |> パイプ演算子
    └─ 独立（✅ 実装済み）

E-2 ?. / ??
    └─ 独立（✅ 実装済み）

E-3 演算子オーバーロード
    └─ struct / impl（✅ 実装済み）

E-4 非同期クロージャ / spawn
    └─ B-7 async/await（✅ 実装済み）

E-5 const fn
    └─ 独立（✅ 実装済み）

E-6 ジェネレータ / yield
    └─ E-4（✅ 実装済み）

E-7 defer
    └─ 独立（今すぐ実装可能）
```
