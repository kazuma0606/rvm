# ForgeScript 型定義仕様

> バージョン対象: v0.1.0
> 関連ファイル: `dev/design-v3.md`, `forge/v0.1.0/plan.md` Phase 5

---

## 1. struct

### 1-A: 基本構文

```forge
struct Point {
    x: number
    y: number
}

let p = Point { x: 1, y: 2 }
println(p.x)   // フィールドアクセス
```

### 1-B: メソッド（impl ブロック）

```forge
struct Rectangle {
    width: number
    height: number
}

impl Rectangle {
    fn area() -> number {
        self.width * self.height
    }

    fn scale(factor: number) -> Rectangle {
        Rectangle { width: self.width * factor, height: self.height * factor }
    }
}

let r = Rectangle { width: 3, height: 4 }
println(r.area())   // 12
```

`self` はメソッド内で暗黙的に使用可能。変更を伴うメソッドは `state self` で宣言。

```forge
impl Counter {
    fn increment(state self) {
        self.count = self.count + 1
    }
}
```

### 1-C: @derive

```forge
@derive(Debug, Clone, Eq, Accessor, Singleton)
struct AppConfig {
    db_url: string
    port:   number
}
```

| derive | 効果 |
|---|---|
| `Debug` | `"{:?}"` 形式の文字列化（println デバッグ用） |
| `Clone` | `.clone()` によるディープコピー |
| `Eq` | `==` / `!=` 比較 |
| `Hash` | `map` のキーとして使用可能 |
| `Ord` | `<` / `>` 比較・`order_by` 対応 |
| `Default` | フィールドをゼロ値で初期化する `Default::new()` |
| `Accessor` | getter / setter を自動生成（下記参照） |
| `Singleton` | インスタンスを1つに制限（下記参照） |

### 1-D: Accessor

`@derive(Accessor)` を付与すると、各フィールドに対して getter / setter が自動生成される。

```forge
@derive(Accessor)
struct User {
    name: string
    age:  number
}

let u = User { name: "Alice", age: 30 }
u.get_name()         // "Alice"
u.set_name("Bob")    // name を更新
u.get_age()          // 30
```

手動で `impl` すれば上書き可能。

```forge
impl User {
    fn get_name() -> string {
        "[{self.name}]"    // カスタム getter
    }
}
```

### 1-E: Singleton

`@derive(Singleton)` を付与すると、インスタンスが1つに制限される。

```forge
@derive(Singleton)
struct AppConfig {
    db_url: string
    port:   number
}

let config = AppConfig::instance()   // 初回は生成、2回目以降は同一インスタンスを返す
```

Rust 側では `once_cell::sync::Lazy` または `std::sync::OnceLock` を使って実装される。

---

## 2. enum

### 2-A: データなしバリアント

```forge
enum Direction {
    North
    South
    East
    West
}

let d = Direction::North

match d {
    North => println("up")
    South => println("down")
    _     => println("other")
}
```

### 2-B: データありバリアント（タプル形式）

```forge
enum Shape {
    Circle(number)
    Rectangle(number, number)
}

match shape {
    Circle(r)    => println("radius={r}")
    Rectangle(w, h) => println("{w}x{h}")
}
```

### 2-C: データありバリアント（名前付きフィールド）

```forge
enum Message {
    Quit
    Move { x: number, y: number }
    Write(string)
}

match msg {
    Quit            => println("quit")
    Move { x, y }   => println("move {x},{y}")
    Write(text)     => println(text)
    _               => println("unknown")
}
```

### 2-D: @derive

enum にも同様の `@derive` が使用可能。

```forge
@derive(Debug, Clone, Eq)
enum Status {
    Active
    Inactive
    Pending(string)
}
```

---

## 3. trait

### 3-A: 定義（純粋な契約）

```forge
trait Printable {
    fn display() -> string
}

trait Serializable {
    fn to_json() -> string
    fn from_json(json: string) -> Self!
}
```

`Self` は実装型を指す。

### 3-B: impl

```forge
struct User {
    name: string
}

impl Printable for User {
    fn display() -> string {
        "User: {self.name}"
    }
}
```

### 3-C: 複数 trait の impl

```forge
impl Printable for User { ... }
impl Serializable for User { ... }
```

### 3-D: デフォルト実装

trait 内にデフォルト実装を持てる。impl 側で上書き可能。

```forge
trait Loggable {
    fn label() -> string    // 必須（デフォルトなし）

    fn log() {              // デフォルト実装
        println(self.label())
    }
    fn warn() {
        println("[WARN] {self.label()}")
    }
}
```

---

## 4. mixin

trait のうち「デフォルト実装の束」を簡潔に宣言するためのシュガー構文。
`trait` との違い: 抽象メソッド（契約）を持たない。

```forge
mixin Timestamped {
    fn created_label() -> string {
        "created: {self.created_at}"
    }
    fn updated_label() -> string {
        "updated: {self.updated_at}"
    }
}

struct Post {
    title:      string
    created_at: string
    updated_at: string
}

impl Timestamped for Post   // デフォルト実装をそのまま使う
```

複数 mixin の組み合わせ:

```forge
impl Timestamped for Post
impl Loggable for Post
```

---

## 5. data（純粋データモデル）

`struct` の特化形。フィールド宣言だけで以下が自動付与される：

- `Debug`, `Clone`, `Eq`, `Hash`
- `Serialize`, `Deserialize`（serde 経由）
- `Accessor`（getter/setter）

```forge
data UserProfile {
    id:    number
    name:  string
    email: string?
}
```

### validate ブロック（オプション）

```forge
data UserRegistration {
    username: string
    email:    string
    password: string
} validate {
    username: length(3..20), alphanumeric
    email:    email_format
    password: length(min: 8), contains_digit, contains_uppercase
}
```

`validate` は書き込み前にアプリ側で実行されるバリデーション。
DB の unique / pk 制約とは分離（DBレイヤーの責務）。

### data vs struct

| 観点 | struct | data |
|---|---|---|
| 用途 | ロジックを持つ型 | 純粋なデータ転送・保存 |
| impl | 可 | 不可（ロジックは別の struct に委譲） |
| derive | 明示的に指定 | 全て自動 |
| トランスパイル先 | Rust struct | Rust struct + serde derive |

---

## 6. typestate

型によって「現在の状態」を表し、状態遷移の正当性をコンパイル時に検証する。

### 6-A: 構文

```forge
typestate Connection {
    states: [Disconnected, Connected, Authenticated]

    Disconnected {
        fn connect(url: string) -> Connected!
    }

    Connected {
        fn auth(token: string) -> Authenticated!
        fn disconnect() -> Disconnected
    }

    Authenticated {
        fn query(sql: string) -> string!
        fn disconnect() -> Disconnected
    }
}
```

### 6-B: 使用

```forge
let conn  = Connection::new<Disconnected>()
let conn2 = conn.connect("localhost")?    // Connected
let conn3 = conn2.auth("token")?          // Authenticated
let rows  = conn3.query("SELECT 1")?

// conn2.query(...) → コンパイルエラー（Authenticated でのみ使用可）
```

### 6-C: Rust へのトランスパイル（B-8 フェーズ）

各状態を型パラメータとしてエンコードする zero-cost 型状態パターン。

```rust
struct Connection<S> { inner: InnerConn, _state: PhantomData<S> }
struct Disconnected;
struct Connected;
struct Authenticated;

impl Connection<Disconnected> {
    fn connect(self, url: &str) -> Result<Connection<Connected>, anyhow::Error> { ... }
}
```

---

## 7. 型定義の組み合わせ例

```forge
trait Animal {
    fn sound() -> string
}

mixin Walker {
    fn walk() { println("{self.name} is walking") }
}

@derive(Debug, Clone, Accessor)
struct Dog {
    name: string
    age:  number
}

impl Animal for Dog {
    fn sound() -> string { "woof" }
}

impl Walker for Dog

let dog = Dog { name: "Rex", age: 3 }
println(dog.sound())   // "woof"
dog.walk()             // "Rex is walking"
println(dog.get_name()) // "Rex"
```

---

## 8. 制約・未サポート

- ジェネリクス `<T>` は v0.1.0 では未サポート（将来対応）
- `impl` ブロック内での他 trait のデフォルト実装への委譲は未サポート
- `mixin` の多重継承でのメソッド名衝突はコンパイルエラー（明示的 impl で解決）
- `data` に `impl` ブロックは付与不可
