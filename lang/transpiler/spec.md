# ForgeScript トランスパイラ仕様（forge build）

> ステータス: B-0〜B-6 実装済み / B-7・B-8 設計済み
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
| `print(x)` | `println!("{}", x)` | ForgeScript の print は改行付き（インタープリタと同挙動） |
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

## 12. 実装済みフェーズ一覧

| Phase | 内容 | 状態 |
|---|---|---|
| B-0 | クレート準備・CLI | ✅ |
| B-1 | 基本変換（let/fn/if/for/while/match/補間/組み込み） | ✅ |
| B-2 | 型システム（T?/T!/Option/Result/?） | ✅ |
| B-3 | クロージャ（Fn推論・FnMut は TODO） | ✅ 一部 |
| B-4 | コレクション（Vec/イテレータチェーン） | ✅ |
| B-5 | 型定義（struct/data/enum/impl/trait/mixin） | ✅ |
| B-6 | モジュール（use/when/use raw/test ブロック） | ✅ |
| B-7 | async/await / tokio 自動挿入 | 📐 設計済み |
| B-8 | typestate 変換 | 📐 設計済み（制約あり） |

未対応（将来）:
- ジェネリクス（`<T>`）
- FnMut（state キャプチャ）/ FnOnce（消費キャプチャ）

---

## 13. Phase B-7: async / await

### 13-1. 基本方針

- ForgeScript は `async` キーワードを持たない。`.await` 式を検出した時点でコンパイラが自動的に `async fn` に昇格させる
- `forge run` では `.await` を no-op として評価する（組み込み関数はブロッキング同期実装を持つ）
- `forge build` では Rust の `async fn` + tokio に変換する

### 13-2. async 伝播ルール

```
1. 関数本体内に .await 式がある → その関数を async fn に昇格
2. async fn を呼び出して .await している関数 → 同様に async fn に昇格
3. 1〜2 を固定点に達するまで繰り返す（呼び出しグラフ全体で伝播）
4. main が async fn になる場合 → #[tokio::main] を付与
```

```forge
fn fetch(id: number) -> User! {
    let res = http.get("/users/{id}").await?
    res.json()
}

fn load() {
    let u = fetch(1).await?   // fetch が async → load も async に昇格
    println(u.name)
}
```

```rust
async fn fetch(id: i64) -> Result<User, anyhow::Error> {
    let res = http::get(&format!("/users/{}", id)).await?;
    res.json()
}

async fn load() -> Result<(), anyhow::Error> {
    let u = fetch(1).await?;
    println!("{}", u.name);
    Ok(())
}
```

### 13-3. main エントリーポイント

```forge
fn main() {
    let user = fetch(1).await?
    println(user.name)
}
```

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let user = fetch(1).await?;
    println!("{}", user.name);
    Ok(())
}
```

tokio が必要になった場合、`Cargo.toml` に自動追記：
```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

### 13-4. .await 式変換

```forge
expr.await      →  expr.await
expr.await?     →  expr.await?
```

### 13-5. async 再帰

async fn の再帰は Rust でそのままコンパイルできないため、`Box::pin` を自動挿入する：

```forge
fn fib(n: number) -> number! {
    if n <= 1 { ok(n) }
    else { ok(fib(n-1).await? + fib(n-2).await?) }
}
```

```rust
fn fib(n: i64) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i64, anyhow::Error>>>> {
    Box::pin(async move {
        if n <= 1 { Ok(n) }
        else { Ok(fib(n - 1).await? + fib(n - 2).await?) }
    })
}
```

### 13-6. test ブロック内の await

```forge
test "fetch works" {
    let u = fetch(1).await?
    assert_eq(u.name, "Alice")
}
```

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_works() -> Result<(), anyhow::Error> {
        let u = fetch(1).await?;
        assert_eq!(u.name, "Alice");
        Ok(())
    }
}
```

### 13-7. 禁止事項

**クロージャ内の `.await` は禁止**（コンパイルエラー）：

```forge
// ❌ エラー: クロージャ内での .await は未サポート
let f = () => { http.get("/api").await }
```

Rust の async closure は nightly 機能のため、安定化されるまでサポートしない。

### 13-8. forge run フォールバック

インタープリタでは `.await` を no-op として扱う：
- `expr.await` → `expr` を同期評価してそのまま返す
- 組み込み非同期関数（`http.get` 等）はブロッキング同期実装を持つ（reqwest の blocking など）

---

## 14. Phase B-8: typestate 変換

### 14-1. 制約条件（重要）

B-8 でサポートする typestate には以下の制約を設ける。制約に違反した場合はコンパイルエラー。

| 制約 | 内容 |
|---|---|
| **Unit 状態のみ** | `states:` に列挙する状態はフィールドを持たない Unit 型のみ |
| **ジェネリクスなし** | `typestate Query<T> { ... }` は未サポート |
| **@derive なし** | typestate への `@derive` は未サポート |
| **any の制限** | `any { }` ブロックは1つのみ・メソッド定義のみ（フィールドアクセス可） |
| **初期状態** | コンストラクタは `states:` の最初の状態で生成される |

### 14-2. 変換パターン

```forge
typestate Connection {
    states: [Disconnected, Connected, Authenticated]

    Disconnected {
        fn connect(self, host: string) -> Connection<Connected> {
            // ...
        }
    }

    Connected {
        fn login(self, user: string) -> Connection<Authenticated> {
            // ...
        }
        fn disconnect(self) -> Connection<Disconnected> {
            // ...
        }
    }

    Authenticated {
        fn query(self, sql: string) -> string {
            // ...
        }
    }

    any {
        fn host(self) -> string { self.host }
    }
}
```

生成される Rust：

```rust
use std::marker::PhantomData;

// 状態マーカー型
struct Disconnected;
struct Connected;
struct Authenticated;

// 本体 struct
struct Connection<S> {
    host: String,
    _state: PhantomData<S>,
}

// 状態別 impl
impl Connection<Disconnected> {
    pub fn new(host: String) -> Self {
        Connection { host, _state: PhantomData }
    }

    pub fn connect(self, host: String) -> Connection<Connected> {
        Connection { host, _state: PhantomData }
    }
}

impl Connection<Connected> {
    pub fn login(self, user: String) -> Connection<Authenticated> {
        Connection { host: self.host, _state: PhantomData }
    }

    pub fn disconnect(self) -> Connection<Disconnected> {
        Connection { host: self.host, _state: PhantomData }
    }
}

impl Connection<Authenticated> {
    pub fn query(&self, sql: String) -> String {
        // ...
    }
}

// any ブロック → 各状態に同一 impl を生成
impl Connection<Disconnected> {
    pub fn host(&self) -> String { self.host.clone() }
}
impl Connection<Connected> {
    pub fn host(&self) -> String { self.host.clone() }
}
impl Connection<Authenticated> {
    pub fn host(&self) -> String { self.host.clone() }
}
```

### 14-3. コンストラクタ生成

`typestate Name { ... }` のコンストラクタは `states:` の**最初の状態**で自動生成：

```forge
let conn = Connection::new(host: "localhost")
// → Connection<Disconnected> が生成される
```

### 14-4. 遷移メソッドの self 変換

| ForgeScript | Rust |
|---|---|
| `fn method(self)` | `pub fn method(self)` |
| `fn method(self) -> NextState` | `pub fn method(self) -> TypeName<NextState>` |
| `fn getter(self) -> T` | `pub fn getter(&self) -> T`（値を返すだけなら `&self`） |

遷移メソッド（戻り値が別状態）は `self` を消費する（所有権移動）。
参照のみのメソッド（戻り値が同型 or プリミティブ）は `&self` に変換する。
