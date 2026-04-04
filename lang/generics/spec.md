# ForgeScript ジェネリクス仕様

> バージョン対象: v0.3.0（コレクション型・ユーティリティ型含む）
> ステータス: 設計済み・未実装

---

## 1. 設計方針

- `<T>` 構文を採用（Rust と同じ）
- v1 はシンプルな型パラメータのみ（トレイト境界・where句・ライフタイムは将来）
- 既存の `list<T>` / `T?` / `T!` と一貫した表記
- `forge run`・`forge build` 両モードで動作

---

## 2. 構文

### 2-1. ジェネリック struct

```forge
struct Response<T> {
    status: number,
    body: T,
}

struct Pair<A, B> {
    first: A,
    second: B,
}
```

### 2-2. ジェネリック fn

```forge
fn wrap<T>(value: T) -> Response<T> {
    Response { status: 200, body: value }
}

fn swap<A, B>(pair: Pair<A, B>) -> Pair<B, A> {
    Pair { first: pair.second, second: pair.first }
}
```

### 2-3. ジェネリック impl

```forge
impl<T> Response<T> {
    fn map<U>(self, f: T => U) -> Response<U> {
        Response { status: self.status, body: f(self.body) }
    }

    fn is_ok(self) -> bool {
        self.status >= 200 && self.status < 300
    }
}
```

### 2-4. ジェネリック enum

```forge
enum Either<L, R> {
    Left(L),
    Right(R),
}
```

### 2-5. T? / T! との組み合わせ

```forge
fn find<T>(list: list<T>, pred: T => bool) -> T? {
    // ...
}

fn parse<T>(json: string) -> T! {
    // ...
}

// ネスト
fn fetch<T>() -> Response<T>! {
    // ...
}
```

---

## 3. 型推論

型引数は多くの場合省略できる：

```forge
let r = wrap(42)          // Response<number> と推論
let p = Pair { first: "hello", second: true }  // Pair<string, bool>
```

明示も可能：

```forge
let r = wrap::<number>(42)
let empty = Response::<string> { status: 204, body: "" }
```

---

## 4. 制約（v1 スコープ）

| 機能 | v1 | 将来 |
|---|---|---|
| 単一型パラメータ `<T>` | ✅ | |
| 複数型パラメータ `<T, U>` | ✅ | |
| 型推論 | ✅ | |
| トレイト境界 `<T: Clone>` | ❌ | ✅ |
| where 句 | ❌ | ✅ |
| 関連型 | ❌ | ✅ |
| const ジェネリクス | ❌ | ✅ |
| ライフタイム | ❌ | ❌（RVM 抽象化で不要） |

---

## 5. Rust 変換

```forge
struct Response<T> {
    status: number,
    body: T,
}

impl<T> Response<T> {
    fn is_ok(self) -> bool { self.status >= 200 }
}
```

```rust
struct Response<T> {
    status: i64,
    body: T,
}

impl<T> Response<T> {
    pub fn is_ok(self) -> bool {
        self.status >= 200
    }
}
```

型パラメータ `T` は変換時にそのまま Rust の型パラメータとして出力される。
トレイト境界なしのため、生成される Rust コードも境界なし。

---

## 6. 組み込みジェネリック型

### 6-1. 組み込み型一覧

| ForgeScript | Rust 変換 | 説明 |
|---|---|---|
| `list<T>` | `Vec<T>` | 可変長リスト（既存） |
| `T?` | `Option<T>` | Optional（既存） |
| `T!` | `Result<T, anyhow::Error>` | Result（既存） |
| `map<K, V>` | `HashMap<K, V>` | ハッシュマップ |
| `set<T>` | `HashSet<T>` | 重複なし集合 |
| `ordered_map<K, V>` | `BTreeMap<K, V>` | ソート済みマップ |
| `ordered_set<T>` | `BTreeSet<T>` | ソート済み集合 |

### 6-2. map<K, V>

```forge
let headers: map<string, string> = {}
headers["Content-Type"] = "application/json"

let scores: map<string, number> = {
    "Alice": 95,
    "Bob":   82,
}

// メソッド
scores.get("Alice")           // number?
scores.contains_key("Bob")    // bool
scores.keys()                 // list<string>
scores.values()               // list<number>
scores.entries()              // list<Pair<string, number>>
scores.len()                  // number
```

Rust 変換:
```rust
use std::collections::HashMap;
let mut headers: HashMap<String, String> = HashMap::new();
headers.insert("Content-Type".to_string(), "application/json".to_string());
```

### 6-3. set<T>

```forge
let tags: set<string> = {"rust", "forge", "http"}

tags.contains("rust")         // bool
tags.insert("async")          // set<string>（新しい set を返す）
tags.union(other_set)         // set<string>
tags.intersect(other_set)     // set<string>
tags.difference(other_set)    // set<string>
tags.len()                    // number
tags.to_list()                // list<string>
```

Rust 変換:
```rust
use std::collections::HashSet;
let tags: HashSet<String> = ["rust", "forge", "http"]
    .iter().map(|s| s.to_string()).collect();
```

---

## 7. ユーティリティ型

型レベルで定義された変換型。コンパイル時に新しい struct を生成する。

### 7-1. Partial\<T\>

全フィールドを `T?`（Optional）にした新しい型を生成する。
PATCH リクエストの部分更新ボディや、段階的な構築パターンで使用する。

```forge
data User {
    id:    number,
    name:  string,
    email: string,
    role:  string,
}

// Partial<User> は以下と等価な型として扱われる
// struct PartialUser {
//     id:    number?,
//     name:  string?,
//     email: string?,
//     role:  string?,
// }

fn update_user(id: number, patch: Partial<User>) -> User! {
    // patch.name は string?
    // patch.email は string?
    // ...
}
```

Rust 変換:
```rust
// forge build 時にコンパイラが自動生成
#[derive(Debug, Clone, serde::Deserialize)]
struct PartialUser {
    id:    Option<i64>,
    name:  Option<String>,
    email: Option<String>,
    role:  Option<String>,
}
```

### 7-2. Required\<T\>

全フィールドの `?` を除去して必須にした新しい型を生成する。

```forge
data Config {
    host:    string?,
    port:    number?,
    timeout: number?,
}

fn connect(cfg: Required<Config>) -> Connection! {
    // cfg.host は string（非 Optional）
    // cfg.port は number
}
```

### 7-3. Readonly\<T\>

全フィールドを immutable にした型を生成する。
`forge run` では通常の struct と同様に扱い、`forge build` では `&T` 参照として変換する。

```forge
fn process(req: Readonly<Request<string>>) -> Response<string>! {
    // req のフィールドへの代入はコンパイルエラー
    // req.method = "POST"  // ❌
    ok(Response::text(req.path))
}
```

Rust 変換:
```rust
// Readonly<T> の引数は &T として生成
fn process(req: &Request<String>) -> Result<Response<String>, anyhow::Error> {
    // ...
}
```

### 7-4. Pick\<T, Keys\>

指定したフィールドのみを持つ新しい型を生成する。
`Keys` はフィールド名の文字列リテラル列（`|` で区切る）。

```forge
data User {
    id:       number,
    name:     string,
    email:    string,
    password: string,
    role:     string,
}

// id と name と email だけを持つ型
fn public_info(user: User) -> Pick<User, "id" | "name" | "email"> {
    Pick::from(user)
}
```

Rust 変換:
```rust
// forge build 時に自動生成
#[derive(Debug, Clone, serde::Serialize)]
struct UserPick_id_name_email {
    id:    i64,
    name:  String,
    email: String,
}
```

### 7-5. Omit\<T, Keys\>

指定したフィールドを除いた新しい型を生成する。`Pick` の逆。

```forge
// password を除いたすべてのフィールド
fn safe_response(user: User) -> Response<Omit<User, "password">>! {
    ok(Response::json(Omit::from(user)))
}
```

Rust 変換:
```rust
// forge build 時に自動生成
#[derive(Debug, Clone, serde::Serialize)]
struct UserOmit_password {
    id:    i64,
    name:  String,
    email: String,
    role:  String,
}
```

### 7-6. NonNullable\<T\>

`T?` から `?` を除去して必須値として扱う。`T? → T` の強制変換。

```forge
fn require_name(name: string?) -> NonNullable<string?> {
    // None の場合はランタイムエラー（forge run）または
    // コンパイル時の unwrap として変換（forge build）
    name
}
```

Rust 変換:
```rust
// NonNullable<T?> は T として変換（unwrap）
fn require_name(name: Option<String>) -> String {
    name.expect("NonNullable: value is None")
}
```

### 7-7. Record\<K, V\>

`map<K, V>` のエイリアス。TypeScript との親和性のために提供。

```forge
let env: Record<string, string> = {
    "HOST": "localhost",
    "PORT": "3000",
}
// map<string, string> と完全に同一
```

---

## 8. ユーティリティ型の制約（v1 スコープ）

| 型 | v1 | 備考 |
|---|---|---|
| `Partial<T>` | ✅ | struct / data のみ（enum は将来） |
| `Required<T>` | ✅ | struct / data のみ |
| `Readonly<T>` | ✅ | `forge build` では `&T` 参照に変換 |
| `Pick<T, Keys>` | ✅ | フィールド名の文字列リテラル |
| `Omit<T, Keys>` | ✅ | フィールド名の文字列リテラル |
| `NonNullable<T>` | ✅ | `T?` にのみ適用可能 |
| `Record<K, V>` | ✅ | `map<K, V>` のエイリアス |
| ネスト `Partial<Omit<T, K>>` | ✅ | 組み合わせ可能 |
| `ReturnType<fn>` | ❌ | 将来（型推論強化後） |
| `Parameters<fn>` | ❌ | 将来 |
| `Awaited<T>` | ❌ | 将来（async 強化後） |

---

## 9. Rust 変換（更新版）

### 組み込みコレクション型

| ForgeScript | Rust |
|---|---|
| `map<K, V>` | `std::collections::HashMap<K, V>` |
| `set<T>` | `std::collections::HashSet<T>` |
| `ordered_map<K, V>` | `std::collections::BTreeMap<K, V>` |
| `ordered_set<T>` | `std::collections::BTreeSet<T>` |

### ユーティリティ型の命名規則

`forge build` 時に生成される Rust 型の名前：

| ForgeScript | 生成 Rust 型名 |
|---|---|
| `Partial<User>` | `PartialUser` |
| `Required<Config>` | `RequiredConfig` |
| `Pick<User, "id" \| "name">` | `UserPick_id_name` |
| `Omit<User, "password">` | `UserOmit_password` |

同一の型引数に対しては重複生成しない（コンパイラが同一性を検出）。

---

## 10. 旧セクション6（組み込み型との関係）

| ForgeScript | 内部表現 |
|---|---|
| `list<T>` | `Vec<T>`（既存） |
| `T?` | `Option<T>` のシンタックスシュガー |
| `T!` | `Result<T, anyhow::Error>` のシンタックスシュガー |
| `map<K, V>` | `HashMap<K, V>`（新規追加） |
| `set<T>` | `HashSet<T>`（新規追加） |
| `Response<T>` | ユーザー定義ジェネリック型 |
| `Response<T>!` | `Result<Response<T>, anyhow::Error>` |
| `Partial<User>` | コンパイル時生成型 `PartialUser` |
