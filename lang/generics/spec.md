# ForgeScript ジェネリクス仕様

> バージョン対象: v0.3.0
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

## 6. 組み込み型との関係

| ForgeScript | 内部表現 |
|---|---|
| `list<T>` | ジェネリック型（既存） |
| `T?` | `Option<T>` のシンタックスシュガー |
| `T!` | `Result<T, anyhow::Error>` のシンタックスシュガー |
| `Response<T>` | ユーザー定義ジェネリック型 |
| `Response<T>!` | `Result<Response<T>, anyhow::Error>` |
