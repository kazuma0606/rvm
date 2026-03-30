# ForgeScript 設計視点 v3

> design-v2.md の議論を経て固まった新しい視点のまとめ（2026-03-30）
> 次のフェーズの詳細設計に入る前の出発点として使う

---

## 核心：ForgeScriptとは何か

```
ForgeScript = Kotlin の設計哲学 × Rust のエコシステム × ゼロ依存バイナリ
```

Kotlin が Java を置き換えようとせず「Java エコシステムの間口を広げた」ように、
ForgeScript は Rust を置き換えようとせず **「Rust エコシステムへの玄関口になる」**。

Rustacian は対象ユーザーではない。彼らはすでに Rust を書ける。
ForgeScript が対象とするのは：

- Python / JS で書いているが、もっと速く・安全にしたい人
- Rust に興味はあるが学習コストで断念した人
- データ処理・スクリプト用途に Rust エコシステムを使いたい人

---

## RVM の役割：2モードアーキテクチャ

RVM はこのプロジェクトの中核であり、2つの明確に異なるモードを持つ。

```
.forge ソース
    │
    ├─── forge run ──────→ [ インタプリタモード ]
    │                           Lexer → Parser → AST → Interpreter
    │                           ・Rc<RefCell<T>> で所有権を動的に処理
    │                           ・Fn/FnMut/FnOnce を判定しない
    │                           ・async は同期フォールバック
    │                           ・型注釈なしでも動く
    │                           ・目的：即時実行・REPL・開発イテレーション
    │
    └─── forge build ────→ [ トランスパイルモード ]
                                Lexer → Parser → AST → 型チェッカー → Rustコード生成
                                ・クロージャのキャプチャから Fn/FnMut/FnOnce を推論
                                ・async fn に #[tokio::main] を自動挿入
                                ・String / &str の最適化
                                ・rustc（LLVM）に最適化を全委任
                                ・目的：本番・配布・ゼロ依存バイナリ
```

**RVM がある理由の本質：**
「Rust の制約（所有権・ライフタイム・Fn トレイト）をユーザーの前から完全に消す」

`forge check` はこの2モードの橋渡しをする：
「`forge run` では動くが `forge build` では問題になる」箇所を事前警告する。

---

## 確定した記法の決定事項

### クロージャ：`=>` アロー記法に統一

```fs
// 引数なし
btn.on("click", () => { count = count + 1 })

// 単引数（括弧省略可）
items.map(x => x * 2)
items.filter(x => x > 0)

// 複数引数
items.fold(0, (acc, x) => acc + x)

// 複数行ブロック
items.filter(x => {
    let threshold = compute_threshold()
    x > threshold
})
```

`|x|` Rust記法は **受け付けるが非推奨**（`forge fmt` が自動変換）。
`lambda` キーワードは採用しない。

### バインディングの3キーワード

```fs
const PI: float   = 3.14159    // コンパイル時定数（モジュール/グローバルスコープ）
let   x: number   = compute()  // 実行時不変束縛（ローカルスコープ）
state count: number = 0         // 実行時可変束縛
```

| キーワード | Rust変換先 | 値の決定 | スコープ |
|---|---|---|---|
| `const` | `const` | コンパイル時 | モジュール・グローバル |
| `let` | `let` | 実行時 | ローカル |
| `state` | `let mut` | 実行時 | ローカル |

### 型システム

```fs
number    → i64    （整数・最大サイズに統一）
float     → f64    （浮動小数点・最大サイズに統一）
string    → String （所有権はコンパイラが管理）
bool      → bool

T?        → Option<T>
T!        → Result<T, anyhow::Error>
T![E]     → Result<T, E>           （カスタムエラー型）
```

数値型は **最大サイズに統一**（i64/f64）。
Rust クレートが u32/usize を要求する場面はコンパイラが自動変換を挿入。
低レベル操作が必要なら `use raw` エスケープハッチで生 Rust に降りる。

---

## 解決済みの課題

| 課題 | 採用方針 |
|---|---|
| 数値型 | i64/f64 に統一、自動キャスト挿入 |
| 文字列型 | ForgeScript内は `String`、トランスパイル時に最適化 |
| 所有権・クロージャ | RVMは Rc<RefCell>で動的処理、forge build で静的推論 |
| async/await | RVMは同期フォールバック、forge build で tokio 自動挿入 |
| エラー型 | デフォルト anyhow、明示時 `T![MyError]` で thiserror |
| ディレクトリ構成 | 11クレート → 5クレートに統合（Phase 0） |
| バージョン依存地獄 | editionシステム + バイトコード非配布 + ゼロ依存バイナリ |
| Rustacian との関係 | 対象ユーザーではない・`|x|` を受け付けることで摩擦最小化 |

---

## パラダイムの優先順位

| パラダイム | 位置づけ |
|---|---|
| 関数型（Functional-first） | ★★★ デフォルト |
| 型駆動（Type-driven） | ★★★ デフォルト |
| OOP-lite（コンポジション） | ★★ サポート |
| 手続き型 | ★ `state` が必要な場面のみ |
| クラス継承 | ✗ 非対応 |

`state` よりイテレータを、クラスより `struct + trait` を自然に選ぶ設計にする。

---

## 課題の解決状況（更新）

### 優先度 A：解決済み

**① クロージャキャプチャ推論ルール（確定）**

`forge build` 時のトレイト決定ルール：

| クロージャの振る舞い | 推論結果 | Rust変換 |
|---|---|---|
| `let` / `const` を読むだけ | `Fn` | `\|x\| ...` |
| `state` 変数を変更する | `FnMut` | `move \|x\| ...` |
| 変数を消費する（1回限り） | `FnOnce` | `move \|x\| ...` |

```fs
// Fn: threshold は let → 読むだけ
let threshold = 10
items.filter(x => x > threshold)

// FnMut: count は state → 変更する
state count = 0
btn.on("click", () => { count = count + 1 })

// FnOnce: name を消費（moveセマンティクス）
let name = get_name()
spawn(() => { send(name) })   // name がムーブされる
```

`forge run`（RVM）では `Rc<RefCell<T>>` が全パターンを吸収するため、
ユーザーはこの区別を意識しない。

**② async/await 設計（確定）**

`.await` を検出したら**コンパイラが自動的に async に昇格**させる。

```fs
// ユーザーが書くもの（async キーワード任意）
fn fetch_user(id: number) -> User! {
    let res = http.get("/users/{id}").await?   // .await がある
    res.json()
}

fn main() {
    let user = fetch_user(1).await?
    print(user.name)
}
```

```
forge run:
  .await → ブロッキング実行（単純な executor、tokio 不要）

forge build:
  .await を含む関数 → 自動で async fn に昇格
  main に .await → #[tokio::main] を自動挿入
  forge.toml に tokio を自動追加
```

生成されるRust：
```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let user = fetch_user(1).await?;
    println!("{}", user.name);
    Ok(())
}
```

---

### 優先度 B：設計方針確定

**③ 名前付き引数・デフォルト引数（方針確定）**

構文はシンプル。Rust への変換は Builder パターン自動生成で解決する。

```fs
fn greet(name: string, prefix: string = "Hello") -> string {
    "{prefix}, {name}!"
}

greet("Alice")                   // → "Hello, Alice!"
greet("Alice", prefix: "Hi")     // → "Hi, Alice!"
```

```rust
// forge build が生成するRust（Builderパターン）
struct GreetArgs {
    name: String,
    prefix: String,
}
impl GreetArgs {
    fn new(name: String) -> Self {
        Self { name, prefix: "Hello".to_string() }
    }
    fn prefix(mut self, v: String) -> Self { self.prefix = v; self }
}
fn greet(args: GreetArgs) -> String {
    format!("{}, {}!", args.prefix, args.name)
}
```

ユーザーには Builder パターンは見えない。`forge fmt` が補完。

**④ スマートキャスト（方針確定）**

`match` / `if let` 後のスコープで型が自動確定する。

```fs
let val: number? = find()

// match 内で型確定
match val {
    some(v) => print(v * 2)   // v は number として扱える
    none    => print("なし")
}

// if let パターン（Kotlin の if-let 相当）
if let some(v) = val {
    print(v * 2)              // この中で v は number
}

// for ループの型確定
let items: list<number> = [1, 2, 3]
for item in items { ... }     // item は number(i64) 確定

// any 型はmatchで分岐
let mixed: list<any> = [1, "hello", true]
for item in mixed {
    match item {
        number(n) => print("数値: {n}"),
        string(s) => print("文字列: {s}"),
        bool(b)   => print("真偽値: {b}"),
    }
}
```

**⑤ `use raw` エスケープハッチ（方針確定）**

```fs
// ブロック全体を生 Rust にする
use raw {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    map.entry(key).or_insert_with(Vec::new).push(value);
    // ForgeScript の変数はここから参照できる（コンパイラが橋渡し）
}

// バインディングなしのクレートを直接使う（Cargo 依存は自動追加）
use raw "bitvec"

// ForgeScript の変数との境界
let threshold: number = 10
use raw {
    // threshold は i64 として使える
    let doubled = threshold * 2_i64;
}
```

`use raw` ブロック内の変数は ForgeScript のスコープへ持ち出せない
（`let result = use raw { ... }` のような形は不可）。
持ち出しが必要なら `use raw` 内で ForgeScript 関数を呼ぶ設計にする。

---

### 優先度 C：方針確定

**⑥ `forge.toml` の最小仕様（確定）**

Cargo.toml のワークスペース機能をそのまま踏襲する。

```toml
[package]
name    = "my-project"
version = "0.1.0"
edition = "forge-2026"

# ワークスペース（Cargoのworkspaceに1:1対応）
# forge new --template workspace で複数パッケージ管理
[workspace]
members = ["packages/*"]

[dependencies]
# use 検出チェッカーが自動管理（手書き不要）
# serde_json = "1.0"  ← forge add / use検出で自動追記

[dev-dependencies]
# forge test 時のみ使うクレート

[forge]
lint      = "strict"    # off / normal / strict
fmt-width = 100
```

**`use` 検出チェッカーの動作（重要）：**

```
1. ソース解析時に use 文を検出
   use serde_json  → [dependencies] に未登録なら追記
   use tokio       → [dependencies] に未登録なら追記

2. cargo add <crate> を自動実行（バージョン解決はCargoに委任）

3. 解決済みバージョンを forge.toml に書き戻す

4. 次回は変更なし（べき等）
```

`forge new` のテンプレート展開時に `.forge/setup.sh` を同梱：

```bash
#!/bin/bash
# forge new 実行時に自動で走るセットアップスクリプト
rustup target add wasm32-unknown-unknown 2>/dev/null || true   # web テンプレートのみ
cargo fetch                                                      # 依存関係の事前取得
echo "✔ Setup complete. Run: forge run src/main.forge"
```

**⑦ 数値型の境界ケース（方針確定）**

| ケース | 対処 |
|---|---|
| 配列インデックス（usize 必要） | コンパイラが `as usize` を自動挿入、負数は実行時エラー |
| for 制御変数 | i64 に統一（範囲で十分） |
| i64 を超える整数 | `use raw` エスケープハッチ（`u128` 等を直接使う） |
| u32 / u64 を要求するRustクレート | バインディング層で変換、無理なら `use raw` |
| Vec 要素の型確定 | 型注釈から静的に決定、`any` なら動的dispatch |

「数値型を増やす」方向には進まない。複雑さが増えるだけで ForgeScript の価値（シンプルさ）が薄れるため。

---

## 新設計：`typestate` キーワード

今回の議論で浮上した **ForgeScript 独自の最強機能候補**。

### 背景

Rust の型状態パターンは PhantomData を手書きするため冗長で読みにくい。

```rust
// Rust（手書き）: 意図が読みにくい・ボイラープレートが多い
struct Connection<S> { _state: PhantomData<S> }
struct Disconnected; struct Connected; struct Authenticated;
impl Connection<Disconnected> {
    fn connect(self) -> Connection<Connected> { ... }
}
// ... impl を状態数分繰り返す
```

### ForgeScript の `typestate` キーワード

```fs
typestate Connection {
    states: [Disconnected, Connected, Authenticated]

    Disconnected {
        fn connect(host: string) -> Connected!
    }

    Connected {
        fn authenticate(creds: Credentials) -> Authenticated!
        fn disconnect() -> Disconnected
    }

    Authenticated {
        fn query(sql: string) -> list<Row>!
        fn logout() -> Connected
    }

    // 全状態で使えるメソッド
    any {
        fn status(self) -> string
    }
}
```

```fs
// 無効な遷移はコンパイルエラー（エラーメッセージも明確）
let conn = Connection::new()           // Disconnected
let conn = conn.connect("localhost")?  // Connected
let conn = conn.authenticate(creds)?   // Authenticated
let rows = conn.query("SELECT ...")?   // ✅ OK

let conn = Connection::new()
conn.query("SELECT ...")               // ❌ コンパイルエラー
// "Connection は Disconnected 状態です。query() は Authenticated 状態でのみ呼べます。"
```

### Rust より優れている点

| 観点 | Rust（手書き） | ForgeScript `typestate` |
|---|---|---|
| 記述量 | 状態数 × impl ブロック | `typestate` ブロック1つ |
| 遷移の可視性 | 複数 impl に散在 | 1箇所に全遷移が集約 |
| エラーメッセージ | 「型が合わない」 | 「X状態では Y() は使えません」 |
| 状態の一覧 | コードを読み解く必要がある | `states:` で明示 |
| 全状態メソッド | 各 impl に重複記述 | `any` ブロックに1回だけ書く |

### rinq の型状態マシンとの統合

rinq の `Initial → Filtered → Sorted → Projected` は
`typestate` として自然に表現できる。

```fs
typestate Query<T> {
    states: [Initial, Filtered, Sorted, Projected]

    Initial {
        fn where(pred: T => bool) -> Filtered<T>
        fn order_by<K>(key: T => K) -> Sorted<T>
        fn select<U>(proj: T => U) -> Projected<U>
        fn collect() -> list<T>        // terminal
    }

    Filtered<T> {
        fn where(pred: T => bool) -> Filtered<T>   // 重ねられる
        fn order_by<K>(key: T => K) -> Sorted<T>
        fn select<U>(proj: T => U) -> Projected<U>
        fn collect() -> list<T>
    }

    Sorted<T> {
        fn then_by<K>(key: T => K) -> Sorted<T>
        fn select<U>(proj: T => U) -> Projected<U>
        fn collect() -> list<T>
        // ※ Sorted 後に where は不可（コンパイルエラー）
    }

    Projected<U> {
        fn collect() -> list<U>        // terminal ops のみ
        fn first() -> U?
        fn count() -> number
    }
}
```

```fs
// 正しい順序
items.where(x => x > 0).order_by(x => x).select(x => x * 2).collect()  // ✅

// 無効な順序 → コンパイルエラー
items.select(x => x * 2).where(x => x > 0)
// "Query は Projected 状態です。where() は Initial または Filtered 状態でのみ呼べます。"
```

### 独自ポジション

```
「型安全なステートマシンを言語組み込みで書ける
  唯一のスクリプト言語」
```

Rust で可能なことを、ForgeScript では**より少ない記述・より明確なエラーで**実現する。
これは「Kotlin が Java より便利」という次元を超えて、
「ForgeScript でしか書けない設計パターン」になりえる。

---

## design-v2.md との関係

`design-v2.md` は各機能の詳細仕様書として維持する。
`design-v3.md`（本文書）は **設計の視点・優先順位・未解決課題の一覧** として使う。

新しい設計判断が固まったら：
- 詳細仕様 → `design-v2.md` に追記
- 視点・方針の変化 → `design-v3.md` を更新

---

## 現在のForgeScript全体像（スナップショット 2026-03-30）

```
言語:       関数型ファースト・型駆動・OOP-lite（継承なし）
記法:       TS風（x => x * 2 / let / state / const / T? / T!）
実行:       forge run（RVM・動的所有権）/ forge build（Rust・静的解析）
配布:       .forge ソース or ゼロ依存ネイティブバイナリ
互換:       |x| 記法を受け付け・use raw でRustに降りられる
生態系:     Cargo / crates.io をそのまま使う
学習:       forge transpile で自分の書いたコードのRust版を確認できる
競合:       Rhai（埋め込み）とは直交・Rustacianは対象外
ポジション: 「RustにとってのKotlin」

独自機能:
  typestate キーワード  → 型状態パターンを言語組み込みで・Rustより可読性高く
  use検出チェッカー    → use serde → forge.toml を自動更新 → cargo が解決
  forge transpile       → 書いたコードのRust版を確認できる学習ツール
  rinq吸収             → from...where...select がネイティブ構文
  T? / T!              → Option/Result をTS風に表現
```

## 未解決・次のフェーズで詰める事項

| 項目 | 状態 | 次のアクション |
|---|---|---|
| `typestate` の詳細仕様 | 方針確定 | design-v2.md に章を追加 |
| async 自動昇格のエッジケース | 方針確定 | トランスパイラ実装時に詳細化 |
| `use raw` の変数橋渡し仕様 | 方針確定 | パーサー拡張時に詳細化 |
| 名前付き引数のBuilder変換 | 方針確定 | Phase 7-B+ で実装 |
| use 検出チェッカーの実装 | 方針確定 | forge-cli に組み込み |
| `forge.toml` の完全スキーマ | 方針確定 | Phase 11 前に文書化 |
