# ForgeScript 設計方針 v2

> MVP完了後の方向転換についての議論まとめ（2026-03-29）

---

## 1. 競合との差別化：Rhaiとの関係

### Rhaiとは

RhaiはRustアプリケーションに**埋め込む**スクリプトエンジンであり、ゲームロジック・設定・プラグインシステムなどに使われる。

### ForgeScriptの立ち位置

Rhaiとは**直交した関係**にある。競合しない。

| 観点 | Rhai | ForgeScript |
|---|---|---|
| 用途 | Rustアプリの内側で動く | スタンドアロンで動く |
| ホスト | Rustアプリが制御 | RVM / rustcが制御 |
| Cargo統合 | なし | `use serde` で自動インストール（構想） |
| 対象ユーザー | Rustエンジニア | Rustを書けない人がRustエコシステムを使う |

```
Rhai   = Rustの中に埋め込む
ForgeScript = Rustの外から使う
```

### ForgeScriptが補完するニッチ

1. **Cargoへの玄関口** — `use serde` だけでcrates.ioのクレートを使える唯一の言語
2. **Rustへの移行パス** — `forge transpile` でForgeScriptをRustコードに変換できる
3. **Rustネイティブなシェルスクリプト代替** — bash/Pythonの代わりにRustクレートを使ったスクリプトを書く

---

## 2. 型システムの方針

### 基本設計：段階的型付け（Gradual Typing）

TypeScriptが「JSに型注釈を乗せた」ように、ForgeScriptは**「動的VMの上に静的型チェックを乗せる」**設計とする。

```
ソース → Lexer → Parser → [型チェッカー] → Compiler → 出力
                               ↑
                         型はここで消費される（TypeScriptと同じ）
```

### `nil` を廃止する

Rustが解決した「nullの問題」をForgeScriptに持ち込まない。

| 目的 | ForgeScript型 | 内部変換（Rust） |
|---|---|---|
| 必ず値がある | `number` / `string` / `bool` | `i64` / `String` / `bool` |
| あるかもしれない | `number?` | `Option<i64>` |
| 失敗するかもしれない | `number!` | `Result<i64, Error>` |
| 戻り値なし | （省略） | `()` |
| ~~存在しない~~ | ~~`nil`~~ | ~~廃止~~ |

### `T?` / `T!` 記法

TypeScriptユーザーに馴染みやすく、かつRustの安全性を保つ独自記法。

```fs
let x: number  = 10          // i64、必ず存在
let y: string? = "Alice"     // Option<String>、自動でsome()にラップ
let z: number! = divide(a, b) // Result<i64, Error>
let w: string? = none         // Option::None
```

```fs
// match で両ケースを強制処理（ケース漏れはコンパイルエラー）
match z {
    ok(value) => print(value),
    err(msg)  => print("Error: " + msg),
}

// ? 演算子でエラー伝播
fn parse_and_double(s: string) -> number! {
    let n = parse_number(s)?  // errなら即return
    ok(n * 2)
}
```

### 暗黙ラップ

```fs
// ? 型への代入 → 自動的に some() でラップ
let name: string? = "Alice"    // some("Alice") と同等

// ! 型の関数 → 正常パスは ok() を省略可能
fn double(n: number) -> number! {
    n * 2   // 自動的に ok(n * 2) として返る
}
```

### Rustへのトランスパイル対応

記法が1対1で対応するため変換規則がシンプル。

```fs
// ForgeScript
let name: string? = "Alice"
let result: number! = divide(10, 2)
```

```rust
// 生成されたRust
let name: Option<String> = Some("Alice".to_string());
let result: Result<i64, Error> = divide(10, 2);
```

---

## 3. 実行モデルの方針

### VM最適化はrustcに任せる

| モード | コマンド | 仕組み | 用途 |
|---|---|---|---|
| インタプリタ | `forge run file.fs` | 既存RVM | スクリプト・REPL・開発 |
| トランスパイル | `forge build file.fs` | Rust変換 → rustc | 本番・パフォーマンスが必要 |
| コード出力 | `forge transpile file.fs` | Rustコードのみ出力 | 学習・既存Rustへの統合 |

TypeScriptの `ts-node`（開発）/ `tsc`（本番）と同じ構造。

### ディスパッチ戦略

- **型が確定している箇所** → Rustに変換した時点で静的ディスパッチ（rustcが最適化）
- **`any` 型** → `Box<dyn Any>` による動的ディスパッチ（ForgeScriptユーザーは意識しない）
- **JIT・AOT・VM最適化はやらない** → rustcに委譲することで実装コストを最小化

```
ForgeScript側の責務:  書きやすさ・型安全性・エラーの分かりやすさ
パフォーマンスの責務:  rustc（LLVM）に全委任
```

---

## 4. UI / Web対応の方針

### 基本方針：仮想DOMを使わない

仮想DOMはランタイムオーバーヘッドが大きく、依存関係も重い。
Svelteのように**コンパイル時リアクティビティ**を採用する。

### `bind` によるDOM束縛

```fs
// リアクティブな状態変数
state count: number = 0

// 宣言的にDOM要素と束縛（仮想DOMなし）
bind "#counter" { text: "Count: " + string(count) }
bind "#btn"     { on_click: || { count = count + 1 } }
```

`count` が変わると束縛されたDOM要素が自動更新される。
コンパイル時に依存関係を解析し、最小限のDOM更新コードを生成する。

### DOM APIの設計

```fs
// 要素の取得（失敗しうる → result型）
let btn: element! = dom.query("#btn")?

// プロパティ操作
btn.text = "Hello"
btn.style.color = "red"
btn.class.add("active")

// イベント
btn.on("click", || {
    print("clicked")
})

// 要素の生成
let div: element = dom.create("div")
dom.body.append(div)
```

### HTMLへの埋め込み

```html
<!-- ランタイム1ファイルのみ -->
<script src="forge.min.js"></script>

<script type="forgescript">
  state count: number = 0
  bind "#counter" { text: "Count: " + string(count) }
  bind "#btn"     { on_click: || { count = count + 1 } }
</script>
```

### WASMとDOMの関係

WASMはDOMを直接操作できないが、`wasm-bindgen` + `web-sys` 経由でJSブリッジを通して操作できる。

```
ForgeScript
    ↓
Rust (web-sys呼び出し)
    ↓
WASM + JSグルーコード（wasm-bindgen自動生成）
    ↓
DOM操作
```

ForgeScriptユーザーには透過的。`dom.query()` などのAPIを呼ぶだけでよい。

`bind` の設計により、WASM→JS境界の呼び出しをWASM内でまとめてから渡すため、オーバーヘッドを最小化できる。

### 将来対応

| 提案 | 内容 | 状態 |
|---|---|---|
| WebAssembly GC | WASMからGCオブジェクト操作 | Chrome/Firefox実装済み |
| Interface Types | JSグルーなしでDOM直接操作 | 策定中 |

Interface Typesが標準化された際も、`bind` による抽象化により**ForgeScriptの書き方は変わらない**。

---

## 5. ミュータビリティの方針

### 基本方針：`mut` キーワードを廃止する

Rustの `let mut` は安全性のために必要だが、ForgeScriptでユーザーにmutabilityを意識させることは「最大入で悪習」と判断する。
代わりに **`let` と `state` の2キーワード**で意図を表現する。

| キーワード | 意味 | Rust対応 | 使う場面 |
|---|---|---|---|
| `let` | 変わらない値（デフォルト） | `let` | 計算結果・定数的な値 |
| `state` | 変わりうる値（明示的） | `let mut` | UI状態・カウンター・イベント |

```fs
let   name: string = "Alice"   // 不変
state count: number = 0        // 可変
```

`mut` という単語を排除することで、Rustを知らないユーザーが概念に迷わない。
`state` は「変化する値」という意味が直感的に伝わり、ReactのuseStateと同じ感覚で使える。

### `for` / `if` を式にすることでmutationをなくす

文（statement）は副作用を前提とするためmutationが必要になる。
式（expression）は値を返すためmutationが不要になる。

```fs
// if は式 → 再代入不要
let label: string = if score > 90 { "A" } else { "B" }

// for は式 → 新しいリストを返す
let doubled: list<number> = for x in [1, 2, 3] { x * 2 }
```

### イテレータメソッドでmutationを吸収する

ループで `state` を使いたくなるパターンのほぼすべてはイテレータで書き直せる。

```fs
// mutが必要に見えるパターン
state sum = 0
for i in [1, 2, 3, 4, 5] {
    sum = sum + i
}

// イテレータで書き直す（let で十分）
let sum = [1, 2, 3, 4, 5].sum()
let sum = [1, 2, 3, 4, 5].fold(0, |acc, x| acc + x)

// リスト変換
let doubled  = [1, 2, 3].map(|x| x * 2)
let evens    = [1, 2, 3, 4].filter(|x| x % 2 == 0)
let filtered = items.filter(|x| x > 0).map(|x| string(x))
```

### `state` が本当に必要な場面

イテレータで表現できない「時間とともに変化する値」に限定する。

```fs
// UI状態・イベント駆動
state count: number = 0
state todos: list<string> = []

btn.on("click", || {
    count = count + 1
    todos = todos + ["new item"]
})

// インクリメンタルな構築（条件が複雑でイテレータに乗らない場合）
state results: list<string> = []
for x in items {
    if x > 0 {
        results = results + [string(x)]
    }
}
// ↑ これも可能なら以下で書く
let results = items.filter(|x| x > 0).map(|x| string(x))
```

### 判断基準

```
値が変化するか？
  No → let（デフォルト）
  Yes → イテレータで表現できるか？
          Yes → let + .map() / .filter() / .fold()
          No  → state
```

### `state` に制限することの副次効果

- コードを読むとき `state` を探せばどこで値が変化するかが一目でわかる
- バグの原因箇所が特定しやすくなる
- Rustへのトランスパイル時に `let mut` に確実に変換できる

---

## 6. rinq の吸収：コレクションAPIの方針

### 背景

[rinq](https://github.com/kazuma0606/rinq) は型安全なLINQライクのクエリエンジンとしてcrates.ioに公開済み（v0.1.0）だが、
「Rust標準に `.iter().map()` があるのになぜ作るのか」というフィードバックを受けた。

**この批判はForgeScriptに吸収することで完全に解消できる。**

| 状況 | 批判 | 回答 |
|---|---|---|
| rinq単体クレート | 「Rust標準で足りる」 | 反論しにくい |
| ForgeScript内蔵 | 「Rustを書かずにこれが使えるのか」 | 「それがForgeScriptです」 |

TypeScriptに「なぜJS標準で書かないのか」と言わないのと同じ構造。

### rinq をForgeScriptに吸収した姿

**2つの記法を両立させる。**

#### メソッドチェーン記法

```fs
// rinq単体（Rustの制約が残る）
QueryBuilder::from(vec![1,2,3,4,5])
    .where_(|x| x % 2 == 0)   // where_ はRustキーワード回避
    .order_by(|x| *x)
    .collect::<Vec<_>>()       // 型注釈が必要

// ForgeScript（制約がすべて消える）
[1, 2, 3, 4, 5]
    .where(|x| x % 2 == 0)    // where で済む
    .order_by(|x| x)
    .collect()
```

#### クエリ式記法（rinq-syntaxを言語に昇格）

rinqには実験的な `query!` マクロ（rinq-syntax crate）が存在する。
ForgeScriptではこれをマクロではなく**言語の文法そのもの**にする。

```fs
// rinq-syntax（現状はRustマクロ構文）
let adults = query! {
    from user in users
    where user.age >= 18
    order_by user.age
    select user.name
};

// ForgeScript（マクロ不要、ネイティブ構文）
let adults = from user in users
    where user.age >= 18
    order_by user.age
    select user.name
```

C# LINQのクエリ式と同じ感覚で書ける。SQLを知っているユーザーにも即座に馴染む。

#### 型ステートマシンが型チェッカーに対応

rinqの `Initial → Filtered → Sorted → Projected` という操作順序の制約を、ForgeScriptの型チェッカーがコンパイル時にチェックする。

```fs
items
    .where(|x| x > 0)    // OK: フィルタ
    .order_by(|x| x)     // OK: ソート
    .select(|x| x * 2)   // OK: 変換
    .where(|x| x > 5)    // コンパイルエラー: select後にwhereは不可
```

#### `*OrDefault` 系が `T?` に自然に対応

```fs
// rinq: first_or_default() → T（Default trait が必要で冗長）
// ForgeScript: first() → T?（Option型として自然に返る）

let first: number? = items.first()
match first {
    some(v) => print(v),
    none    => print("空でした"),
}
```

`QueryBuilder::from()` も `collect::<Vec<_>>()` も消える。
これがForgeScriptという言語の文法であるため、ボイラープレートの概念自体がなくなる。

### 吸収するAPI全体像

**フィルタ・変換**
```fs
items
    .where(|x| x > 0)                              // where_  → where
    .select(|x| x * 2)                             // 変換
    .skip(2).take(5)
    .take_while(|x| x < 100)                       // rinqギャップ → ForgeScriptで実装
    .skip_while(|x| x < 0)                         // rinqギャップ → ForgeScriptで実装
    .flat_map(|x| [x, x * 2])
    .filter_map(|x| if x > 0 { some(x) } else { none })
```

**ソート**
```fs
items
    .order_by(|x| x.score)
    .then_by(|x| x.name)
    .order_by_descending(|x| x.date)               // rinqギャップ → ForgeScriptで実装
    .then_by_descending(|x| x.id)                  // rinqギャップ → ForgeScriptで実装
```

**スカラー集計**
```fs
let total   = items.sum()
let average = items.average()
let maximum = items.max()
let minimum = items.min()
let count   = items.count()
let exists  = items.any(|x| x > 0)
let all_pos = items.all(|x| x > 0)
let total2  = items.aggregate(0, |acc, x| acc + x) // rinqギャップ → ForgeScriptで実装
let found   = items.contains(42)                   // rinqギャップ → ForgeScriptで実装
```

**グループ・セット操作**
```fs
let groups  = users.group_by(|u| u.team)
let unique  = items.distinct()
let merged  = list_a.concat(list_b)                // rinqギャップ → ForgeScriptで実装
let union_  = list_a.union(list_b)                 // rinqギャップ → ForgeScriptで実装
let common  = list_a.intersect(list_b)             // rinqギャップ → ForgeScriptで実装
let diff    = list_a.except(list_b)                // rinqギャップ → ForgeScriptで実装
let paired  = list_a.zip(list_b)
let map_    = items.to_hashmap(|x| x.id)           // rinqギャップ → ForgeScriptで実装
```

**JOIN（rinqに実装済み・そのまま吸収）**
```fs
let result = orders.inner_join(users, |o| o.user_id, |u| u.id)
let result = orders.left_join(users,  |o| o.user_id, |u| u.id)
let matrix = list_a.cross_join(list_b)
```

**ウィンドウ解析（rinqの独自価値）**
```fs
let prices = [100.0, 102.0, 98.0, 105.0, 103.0]

let running = prices.running_sum()
let ma3     = prices.moving_average(3)
let ranked  = prices.rank_by(|x| x)
let lagged  = prices.lag(1)
let leading = prices.lead(1)
```

**生成**
```fs
let range   = [1..=100]                            // 範囲リテラルとして自然に表現
let repeated = number.repeat(5)                    // rinqギャップ → ForgeScriptで実装
```

**統計（rinq-statsの吸収）**
```fs
let data = [1.0, 2.0, 3.0, 4.0, 5.0]

let std_dev    = data.std_dev()
let median     = data.median()
let hist       = data.histogram(5)
let corr       = data.zip(other).pearson_correlation()
let regression = data.zip(other).linear_regression()
```

### 開く新しいニッチ

ウィンドウ解析・統計機能を持つことで**データ処理スクリプト言語**というポジションが加わる。

```fs
// PythonのPandas的な使い方がForgeScriptで書ける
let sales = load_csv("sales.csv")

let report = sales
    .group_by(|r| r.month)
    .select(|g| {
        month: g.key,
        total: g.sum(|r| r.amount),
        average: g.average(|r| r.amount),
    })
    .order_by(|r| r.month)

print(report)
```

```
Python pandas:   重い・型なし
Rust polars:     高速だが記述量が多い
ForgeScript:     型安全・軽量・LINQライクで書ける ← 独自ポジション
```

### 実装方針

- **rusted-ca/rinq** を `rvm-stdlib` crate として取り込む（ロジックの再利用）
- **rusted-ca/rinq-stats** の統計関数も同様に吸収
- **rusted-ca/rinq-syntax** の `query!` マクロの構文をForgeScriptのパーサーに昇格させる
- `linq-gap-analysis.md` で特定された未実装項目をForgeScriptで最初から実装する
- `QueryBuilder` の型ステートマシン（`Initial→Filtered→Sorted→Projected`）はForgeScriptの型チェッカーが担当
- 内部実装はRustへのトランスパイル時に rinq の型安全なコードが生成される
- `*OrDefault` 系メソッドは `T?`（Option型）として自然に返す設計に統一する

---

## 7. JITコンパイラの方針

### 結論：現時点では不要

ForgeScriptの2モード設計がJITの需要を吸収している。

```
forge run file.fs   → RVM（起動速度優先、スクリプト用途）
forge build file.fs → rustc → ネイティブバイナリ（速度優先）
ブラウザ           → WASM（ブラウザのJITエンジンがWASMを最適化）
```

JITが必要になるのは「インタプリタは遅いが、コンパイルもできない状況」だが、ForgeScriptはrustcとWASMがその中間を埋める。
Phase 11以降、RVMインタプリタのホットパスがボトルネックになった時点で初めて検討する。

---

## 8. DB アクセスの方針

### 基本方針：クエリ式構文をDBに伸ばす

**すでに設計したクエリ式構文がそのままDBクエリに使える。**
C#のLINQ to SQL / Entity Frameworkと同じ発想。

```fs
// in-memoryコレクション（Phase 7-A++で設計済み）
let adults = from user in users
    where user.age >= 18
    select user.name

// DBクエリ（同じ構文でDBに向ける）
use db from "sqlite:./mydb.db"

let adults = from user in db.users
    where user.age >= 18
    select user.name
```

書き方が変わらない。対象がメモリかDBかだけが変わる。

### 内部での変換

```
from user in users（list）  → rinqメソッドチェーン → Rustコード
from user in db.users（DB） → sqlxクエリ         → Rustコード
```

```rust
// ForgeScriptが生成するRust（DB向け）
sqlx::query_as!(User, "SELECT name FROM users WHERE age >= ?", 18)
    .fetch_all(&pool)
    .await
```

### ORMは作らない

```
JS:          生のDB操作不可 → Prismaなどが必要
ForgeScript: use sqlx で生SQL + クエリ式構文で安全に書ける
```

Prismaのような重いスキーマ定義ファイルを必要とするORMを作る必要はない。
sqlxをバックエンドにしたクエリ式構文で十分な独自性がある。
複雑なケースは `use sqlx` 経由で生SQLを書けばよい。

### 非同期対応

DBアクセスは非同期が必須になるため、`async/await` も合わせて設計する。

```fs
// 非同期関数
async fn get_adults(db: connection) -> list<string>! {
    from user in db.users
        where user.age >= 18
        select user.name
}

// 呼び出し
let names = get_adults(db).await?
```

---

## 9. VSCode拡張の方針

### 優先度：高（早期に着手）

シンタックスハイライトがないと開発体験が著しく低下する。
言語仕様が固まる前でも、キーワードベースのハイライトは実装できる。

### 拡張子の決定

`.fs` はF#が使用しているため競合する。

```
.forge  ← 推奨（言語名と一致・直感的）
.fgs    ← 代替案
```

### 必要なファイル構成

```
forgescript-vscode/
  package.json                      ← 拡張機能メタデータ・言語登録
  language-configuration.json       ← 括弧対応・コメント・インデント
  syntaxes/
    forgescript.tmLanguage.json     ← ハイライトルール（TextMate文法）
```

### ハイライト対象

```json
{
  "keywords":     ["let", "state", "fn", "if", "else", "for", "while",
                   "match", "return", "use", "from", "where", "select",
                   "order_by", "component", "bind", "async", "await"],
  "typeKeywords": ["number", "string", "bool", "any", "list"],
  "constants":    ["some", "none", "ok", "err", "true", "false"],
  "operators":    ["?", "!", "|>", "=>", "->"]
}
```

### 将来の拡張

| 機能 | 優先度 |
|---|---|
| シンタックスハイライト | 今すぐ |
| スニペット（`let`, `fn`, `component` など） | 次 |
| 言語サーバー（LSP）によるエラー表示・補完 | Phase 7-B以降 |
| Marketplace公開 | シンタックスハイライト完成後 |

---

## 10. ジェネリクスと演算子オーバーロードの方針

### ジェネリクス

型パラメータを関数・構造体に持たせる。`list<T>` / `option<T>` / `result<T, E>` として
すでに組み込み型で使っている構文をユーザー定義に開放する。

```fs
// 関数ジェネリクス
fn identity<T>(x: T) -> T { x }
fn first<T>(items: list<T>) -> T? { items.first() }
fn zip_with<T, U, V>(a: list<T>, b: list<U>, f: fn(T, U) -> V) -> list<V> {
    a.zip(b).map(|(x, y)| f(x, y))
}

// 構造体ジェネリクス
struct Pair<T, U> {
    first: T,
    second: U,
}

let p: Pair<number, string> = Pair { first: 10, second: "hello" }
```

#### 型制約（トレイト境界に相当）

Rustの `T: Add + Copy` のような複雑な記述をForgeScript側で隠蔽する。

```fs
// ForgeScript
fn sum<T: addable>(items: list<T>) -> T {
    items.fold(items[0], |acc, x| acc + x)
}

fn sort<T: comparable>(items: list<T>) -> list<T> {
    items.order_by(|x| x)
}
```

```rust
// 生成されるRust
fn sum<T: std::ops::Add<Output = T> + Copy>(items: &[T]) -> T {
    items.iter().copied().fold(items[0], |acc, x| acc + x)
}
```

組み込み制約として `addable` / `comparable` / `displayable` / `cloneable` を用意し、
複雑なトレイト境界の記述をユーザーから隠す。

### 演算子オーバーロード

Rustの `impl std::ops::Add for T` を `op` キーワードで簡潔に表現する。

```fs
struct Vec2 {
    x: number,
    y: number,
}

impl Vec2 {
    op +(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
    op -(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x - other.x, y: self.y - other.y }
    }
    op *(self, scalar: number) -> Vec2 {
        Vec2 { x: self.x * scalar, y: self.y * scalar }
    }
    op ==(self, other: Vec2) -> bool {
        self.x == other.x && self.y == other.y
    }
}

let a = Vec2 { x: 1.0, y: 2.0 }
let b = Vec2 { x: 3.0, y: 4.0 }
let c = a + b         // Vec2 { x: 4.0, y: 6.0 }
let d = c * 2.0       // Vec2 { x: 8.0, y: 12.0 }
let eq = a == a       // true
```

生成されるRust：

```rust
impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 { x: self.x + other.x, y: self.y + other.y }
    }
}
impl std::ops::Mul<i64> for Vec2 { ... }
impl PartialEq for Vec2 { ... }
```

Rustの `type Output = ...` や `impl PartialEq` などのボイラープレートはすべて自動生成する。

#### オーバーロード可能な演算子一覧

| ForgeScript | Rustトレイト | 意味 |
|---|---|---|
| `op +` | `Add` | 加算 |
| `op -` | `Sub` | 減算 |
| `op *` | `Mul` | 乗算 |
| `op /` | `Div` | 除算 |
| `op %` | `Rem` | 剰余 |
| `op ==` | `PartialEq` | 等値比較 |
| `op <` | `PartialOrd` | 順序比較 |
| `op []` | `Index` | インデックスアクセス |
| `op !` | `Not` | 否定 |

---

## 11. Cargo依存解決の自動化方針

### 基本方針：Cargoに委譲する

Cargoは既に優秀な依存解決エンジンを持っている。
ForgeScriptはCargoが解決している問題を再発明せず、**薄いラッパーとして機能する**。

```
use serde_json  ← ForgeScriptが検出

↓ ForgeScriptの処理（3ステップ）

1. cargo add serde_json を実行
   → バージョン解決・競合検出はCargoが担当

2. ForgeScriptのCargo.tomlを更新
   → edition = "2021" に固定（ユーザーは意識しない）

3. 生成したRustコードを cargo build
   → コンパイル・リンクはrustcが担当
```

### 失敗ケースと対策

| ケース | 原因 | 対策 |
|---|---|---|
| クレートが見つからない | タイポ・非公開 | `cargo add` のエラー + 候補をサジェスト |
| バージョン競合 | 推移的依存の不整合 | Cargoのエラーをそのまま表示（Cargoが詳細説明） |
| ネイティブ依存（OpenSSL等） | システムライブラリ不足 | 「システム設定が必要です」と案内メッセージ |
| feature flag が必要 | デフォルトでは機能不足 | よく使うクレートはfeatureを事前定義 |
| Rust Edition差異 | 2021/2024の挙動違い | 生成コードのeditionを2021に固定 |

### クレートバインディングの段階的展開

任意のRustクレートをForgeScriptから呼ぶには、**Rust APIをForgeScript型に橋渡しする情報**が必要。
TypeScriptの `@types` に相当する仕組みを段階的に構築する。

```
Phase A（すぐ）: よく使う20クレートに事前定義バインディング
  serde_json / tokio / reqwest / sqlx / chrono / regex / uuid
  rand / log / clap / anyhow / thiserror / itertools ...

Phase B（次）:   バインディングがないクレートはエスケープハッチで対応
  use raw "some_crate"  // Rustコードを直接書く

Phase C（将来）: コミュニティがバインディングを追加できる仕組み
  forge add serde_json  // ForgeScriptバインディングをインストール
```

#### `forge add` コマンドの設計

```bash
# Cargo統合（内部でcargo addを実行）
forge add serde_json          # バインディング付きで追加
forge add reqwest --async     # 非同期バインディング付き
forge add sqlx --feature postgres  # feature指定
```

### Rust Editionの扱い

ForgeScriptが生成するコードは常に `edition = "2021"` に固定する。
Rust 2024への移行はForgeScript側でテストしたタイミングで更新し、ユーザーは意識しない。

### 現実的な見通し

```
高い信頼性（すぐ）:  よく使うクレート20種 → バインディング事前定義で確実に動く
中程度（段階的）:    cargo add委譲 → Cargoが解決できる範囲で動く
課題（将来）:        任意クレートの完全自動バインディング → コミュニティ主導
```

「ロジックが組みやすくなるだけでも価値がある」という判断は正しく、
Cargo統合は**段階的に信頼性を上げていく機能**として位置づける。
最初からすべてのクレートに対応しようとせず、実績を積みながら範囲を広げる。

---

## 12. ロードマップ（更新版）

```
Phase 0（リファクタリング）: ディレクトリ再編
  - 11クレート → 5クレートに統合
    - forge-compiler（fs-lexer + fs-ast + fs-parser → 内部 mod 分割）
    - forge-vm（fs-compiler + fs-bytecode + rvm-core + rvm-runtime + rvm-host → 内部 mod 分割）
    - forge-stdlib（将来の rinq 吸収先）
    - forge-transpiler（将来の Rust コード生成）
    - forge-cli（fs-cli + fs-repl → コマンド別ファイル分割）
  - テスト（e2e-tests + test-utils）は forge-cli/tests/ 以下に統合
  - 既存テスト125件をすべて通過させてからリネーム
  - forge.toml の edition フィールドを定義

Phase 7-A: 言語基盤
  - コメント構文 (//)
  - if / else（式として値を返す）
  - for（式として値を返す）
  - while
  - 関数定義 (fn)
  - state キーワード（可変バインディング）
  - let は完全不変に固定・mut キーワード廃止

Phase 7-A+: コレクションAPI（rinq吸収）
  - list<T> 型・範囲リテラル [1..=100]
  - メソッドチェーン記法
  - フィルタ: where / take / skip / take_while / skip_while / filter_map / step_by
  - 変換: select / map / flat_map / flatten / scan / zip / enumerate / inspect
  - ソート: order_by / order_by_descending / then_by / then_by_descending
  - 集計: sum / average / min / max / count / any / all / aggregate / contains
  - グループ・セット: group_by / distinct / concat / union / intersect / except / zip
  - JOIN: inner_join / left_join / cross_join
  - シーケンス: reverse / chunk / window / pairwise / intersperse / tee / cycle
  - ウィンドウ解析: running_sum / moving_average / rank_by / lag / lead
  - 統計（rinq-stats）: std_dev / median / histogram / pearson_correlation / linear_regression
  - Terminal: first / last / nth / single / contains / collect / to_hashmap / to_lookup
  - *OrDefault 系 → T? として返す（Option型に統一）
  - for 式との統合
  - 内部実装: rusted-ca/rinq を rvm-stdlib crate として取り込む

Phase 7-A++: クエリ式記法（rinq-syntax を言語に昇格）
  - from / where / order_by / select のネイティブ構文
  - メソッドチェーンと等価なコードに変換
  - パーサー拡張: rinq-syntax の query! マクロ構文を参考に実装

Phase 7-B: 型システム
  - 型注釈パーサー (number / string / bool)
  - T? / T! 記法
  - 型チェッカー (fs-typechecker crate)
  - match の網羅性チェック
  - ジェネリクス（型パラメータ T / U / V）
  - 型制約: addable / comparable / displayable / cloneable
  - 演算子オーバーロード（op + / - / * / == / [] など）
  - struct 定義 + impl ブロック

Phase 8-A: Rustトランスパイラ
  - ForgeScript → Rust コード生成
  - forge transpile コマンド
  - forge build コマンド（rustc呼び出し）

Phase 8-B: Web対応
  - forge.min.js（JSトランスパイラのブラウザ版）
  - dom API
  - bind / state キーワード
  - <script type="forgescript"> サポート

Phase 9: WASMターゲット
  - forge build --target web → WASM出力
  - wasm-bindgen統合
  - web-sys DOM バインディング

Phase 7-Z（並行・早期着手）: VSCode拡張
  - .forge 拡張子の決定
  - tmLanguage.json シンタックスハイライト
  - language-configuration.json（括弧・コメント対応）
  - スニペット（let / fn / component / from...select）
  - VSCode Marketplace 公開

Phase 10-A: Cargo統合（Phase A：事前定義バインディング）
  - use <crate> 構文
  - forge add コマンド（内部で cargo add を実行）
  - 事前定義バインディング20種
    （serde_json / tokio / reqwest / sqlx / chrono / regex / uuid 等）
  - edition = "2021" 固定・ユーザーはEditionを意識しない
  - エスケープハッチ: use raw "crate" で生Rustコードを書く

Phase 10-B: Cargo統合（Phase B：バインディング拡充）
  - forge add による ForgeScriptバインディングのインストール
  - feature flag 指定（forge add reqwest --async）
  - ネイティブ依存クレートの案内メッセージ整備
  - コミュニティ主導のバインディング追加の仕組み

Phase 10+: DB対応
  - use db from "<connection-string>" 構文
  - from...where...select → sqlxクエリ変換
  - async/await 構文
  - 非同期関数定義（async fn）
  - 複雑なケースは use sqlx で生SQL対応

Phase 7-B+: 型定義（struct / enum / trait）
  - struct 定義 + impl ブロック（フィールド・メソッド・関連関数）
  - enum（シンプル列挙 + データを持つ代数的データ型）
  - trait 定義 + デフォルト実装
  - impl Trait for Type 構文
  - derive キーワード（debug / clone / eq / ord / hash / display / serialize / deserialize / default / copy）
  - derive ショートハンド（data / serde / full）
  - match でのデストラクチャリング（enum / struct フィールド展開）

Phase 7-C: 言語の完成度向上
  - float 型（f64）+ number/float 間の明示キャスト
  - 文字列補間（"Hello, {name}!"）→ format! に変換
  - デストラクチャリング（タプル・struct・enum）
  - union 型 / 型エイリアス（type Id = number | string → enum に変換）
  - モジュールシステム（use "./utils.forge" + pub キーワード）
  - テスト構文（test / test_group / assert / assert_err）

Phase 11: forgeツールチェーン
  - forge test（テスト実行）
  - forge fmt（コード整形）
  - forge check（型チェックのみ）
  - forge doc（ドキュメント生成）
  - forge generate（スキャフォールディング）
    - forge generate struct <Name> [fields...]
    - forge generate enum <Name> [variants...]
    - forge generate trait <Name> [methods...]
    - forge generate impl <Trait> for <Type>
    - forge generate test <name>
    - インタラクティブモード（対話型入力）
    - カスタムテンプレート（.forge/templates/*.hbs）
  - forge new（プロジェクトテンプレート生成）
    - forge new <name> --template [script/web/lib/cli/data]
    - forge.toml（プロジェクト設定ファイル）
    - インタラクティブモード（template選択・依存追加・git初期化）

Phase 12（将来）: JIT
  - RVMインタプリタがボトルネックになった時点で検討
  - Interface Types標準化後に再評価
  - Cranelift JIT PoC
```

---

## 13. 設計上の原則（更新）

1. **nilを持ち込まない** — Option/Resultで型レベルの安全性を保証
2. **mutを意識させない** — `let`（不変）と `state`（可変）の2キーワードに集約、`mut` 廃止
3. **for/if は式** — 値を返すことでmutationの必要性を排除
4. **イテレータを優先する** — `.map()/.filter()/.fold()` でループのmutationを吸収
5. **型変換はrustcに委ねる** — ForgeScript自体はVM最適化をしない
6. **依存関係を最小に** — ブラウザランタイムは1ファイル目標
7. **書き方を変えない** — 内部実装（JSグルー/Interface Types）が変わっても構文は変わらない
8. **rinqを言語に吸収する** — 単体クレートへの批判を「言語の文法」として昇華させる
9. **クエリ構文はメモリとDBで統一する** — 同じ `from...where...select` がin-memoryとDB両方に使える
10. **JITは後回し** — rustcとWASMがカバーする範囲でJITは不要、需要が生まれた時点で検討
11. **拡張子は `.forge`** — `.fs`はF#と競合するため
12. **ジェネリクスはRust genericsに直接対応** — `list<T>` などの組み込みと統一した構文
13. **演算子オーバーロードは `op` キーワードで統一** — Rustのトレイトボイラープレートを自動生成
14. **Cargo統合は段階的に** — まず20クレートのバインディングで確実に動かし、範囲を広げる
15. **Rustのeditionはユーザーに意識させない** — 生成コードは edition = "2021" に固定
16. **Rhaiと競合しない** — 「埋め込み」ではなく「スタンドアロン」に特化
17. **float と number を明確に分ける** — 暗黙の型変換は行わず、明示キャストを要求
18. **文字列補間は `{式}` 記法** — バッククォートは使わず、ダブルクォート内に直接埋め込む
19. **デストラクチャリングは構文に組み込む** — `let { name, age } = user` をネイティブサポート
20. **union型はenumに変換する** — 動的ディスパッチではなくコンパイル時に網羅チェック
21. **モジュールは use で統一** — Cargoクレートのインポートと同じキーワード・同じ感覚
22. **テストは言語組み込み** — `test` ブロックを言語仕様に含め、外部フレームワーク不要
23. **derive はキーワードとして定義直前に書く** — `#[derive]` アトリビュートではなく `derive()` を型の上に配置
24. **forge generate でボイラープレートを排除** — struct/trait の骨組み生成をCLIで提供
25. **ツールチェーンはforgeコマンドに統一** — test/fmt/check/doc/generate/new を1つのCLIに集約
26. **関数型ファースト** — `state` より `let` + イテレータを自然に導く設計
27. **継承を持ち込まない** — Rustと同様にコンポジション（struct + trait）のみ
28. **型が文書である** — `T?` / `T!` を見れば関数の振る舞いがわかる設計
29. **Rustへの玄関口であり代替ではない** — Rustエコシステムを使わせる・Rustを学ばせる・Rustへ誘う
30. **forge new でゼロから正しい構造を導く** — テンプレートが「最初の1ファイル」から設計を体現する
31. **クレートはフェーズ境界で分割する** — Cargo クレートの粒度は「外部APIの境界」、内部分割は `mod` で行う
32. **RVM バイトコードは内部表現・配布しない** — 配布形式は .forge ソースと forge build のネイティブバイナリのみ
33. **edition システムで後方互換を保証する** — 破壊的変更は新 edition にのみ入れ、forge migrate で自動移行
34. **forge build の成果物はゼロ依存バイナリ** — 実行先に forge / Rust / RVM は不要

---

## 14. 不足している言語機能の方針

### 14-1. float型

現状の `number` は `i64` にマッピングされているが、小数点数も扱う必要がある。

```
number   → i64   （整数）
float    → f64   （浮動小数点）
```

```fs
let x: float  = 3.14
let y: float  = 2.0 / 3.0
let z: number = 42

// 自動昇格はしない（明示的なキャスト）
let n: number = x.to_int()    // float → number（切り捨て）
let f: float  = z.to_float()  // number → float
```

`number?` / `float!` など `T?` / `T!` 記法との組み合わせも完全対応する。

---

### 14-2. 文字列補間

テンプレートリテラルは開発体験に直結する。TypeScriptのバッククォートではなく、
Swiftや最近の言語に習い **`{式}` をダブルクォート内に埋め込む** 記法を採用する。

```fs
let name = "Alice"
let age  = 30

let msg = "Hello, {name}! You are {age} years old."
// → "Hello, Alice! You are 30 years old."

// 式も埋め込み可能
let label = "Result: {if score > 90 { "A" } else { "B" }}"

// 型変換は自動（Displayトレイトがある型なら何でも）
let pos = Vec2 { x: 1.0, y: 2.0 }
let msg = "Position: {pos}"   // Vec2 が display を実装していれば動く
```

Rustへのトランスパイル：

```rust
format!("Hello, {}! You are {} years old.", name, age)
```

---

### 14-3. デストラクチャリング

Rust / TypeScriptの `let (x, y) = ...` に対応する構文を持つ。

```fs
// タプルのデストラクチャリング
let (x, y) = (10, 20)
let (first, rest..) = items    // スプレッド

// 構造体のデストラクチャリング
let User { name, age } = user
let Vec2 { x, y } = position

// match でのデストラクチャリング（すでに設計済みのmatchと統合）
match result {
    ok(User { name, age }) => print("{name} is {age}"),
    err(msg) => print("Error: {msg}"),
}

// for でのデストラクチャリング
for (key, value) in map {
    print("{key}: {value}")
}
for Vec2 { x, y } in points {
    print("({x}, {y})")
}
```

---

### 14-4. union型 / 型エイリアス

TypeScriptの `string | number` に相当するが、Rustの型システム上では **enum として表現** する。

```fs
// 型エイリアス（シンプルなケース）
type Id = number | string

// 実質的にenumに変換される
let id: Id = 42
let id: Id = "user-abc"

match id {
    number(n) => print("数値ID: {n}"),
    string(s) => print("文字列ID: {s}"),
}
```

```rust
// 生成されるRust
enum Id {
    Number(i64),
    String(String),
}
```

`any` 型とは異なり、コンパイル時に網羅チェックが効く安全なunionとして機能する。

---

### 14-5. モジュールシステム

`use` キーワードをモジュールインポートにも使う（Cargoクレートと統一した構文）。

```fs
// 同一プロジェクト内のファイルをインポート
use "./utils.forge"
use "./models/user.forge" as user_module

// 名前付きエクスポート
pub fn greet(name: string) -> string {
    "Hello, {name}!"
}

pub struct Config {
    host: string,
    port: number,
}

// インポートして使う
use "./config.forge" { Config }

let cfg: Config = Config { host: "localhost", port: 8080 }
```

Rustへのトランスパイル時は `mod` + `pub use` に変換される。

---

### 14-6. forgeツールチェーン（forge test / fmt / check / doc）

TypeScriptが `tsc` / `eslint` / `prettier` / `jest` をバラバラに持つのに対し、
ForgeScriptは **Rustと同様にワンコマンドで揃える**。

```bash
forge run file.forge        # 実行（RVM）
forge build file.forge      # Rustにトランスパイル → rustcでビルド
forge transpile file.forge  # Rustコード出力のみ
forge repl                  # 対話型REPL

forge test                  # テスト実行（後述）
forge fmt                   # コード整形
forge check                 # 型チェックのみ（コンパイルなし）
forge doc                   # ドキュメント生成
forge generate <template>   # スキャフォールディング（後述 §17）
forge add <crate>           # Cargoクレートのバインディング追加
```

---

### 14-7. テスト構文

Rustの `#[test]` に対応する、言語組み込みのテスト記法。

```fs
// テスト定義
test "2 + 2 equals 4" {
    assert 2 + 2 == 4
}

test "greet returns correct string" {
    let result = greet("Alice")
    assert result == "Hello, Alice!"
}

// 期待エラーのテスト
test "division by zero returns error" {
    let result = divide(10, 0)
    assert_err result
}

// グループ化
test_group "User model" {
    test "can be created" {
        let u = User { name: "Alice", age: 30 }
        assert u.name == "Alice"
    }

    test "age must be positive" {
        assert_err User::new("Alice", -1)
    }
}
```

```bash
forge test                      # 全テスト実行
forge test --filter "User model" # グループ指定
```

---

## 15. struct / enum / trait の正式仕様

### 15-1. struct

Rustの `struct` をそのままForgeScriptに持ち込む。
`let`（不変）/ `state`（可変）との統合も自然に動く。

```fs
// フィールド定義
struct User {
    name: string,
    age:  number,
    email: string?,   // Option型フィールド
}

// コンストラクタ（フィールド名 = 値）
let alice: User = User { name: "Alice", age: 30, email: none }

// フィールドアクセス
print(alice.name)
print(alice.age)

// メソッド定義（impl ブロック）
impl User {
    fn greet(self) -> string {
        "Hello, I'm {self.name}!"
    }

    fn with_email(self, email: string) -> User {
        User { name: self.name, age: self.age, email: some(email) }
    }

    // 関連関数（Rustの fn new() に相当）
    fn new(name: string, age: number) -> User! {
        if age < 0 {
            err("age must be non-negative")
        } else {
            ok(User { name, age, email: none })
        }
    }
}

let user = User::new("Bob", 25)?
let msg  = user.greet()
```

```rust
// 生成されるRust
struct User {
    name: String,
    age: i64,
    email: Option<String>,
}
impl User {
    fn greet(&self) -> String {
        format!("Hello, I'm {}!", self.name)
    }
    fn new(name: String, age: i64) -> Result<User, Error> { ... }
}
```

---

### 15-2. enum（代数的データ型）

Rustの `enum` を完全サポートする。ただし記法はForgeScriptらしくシンプルにする。

```fs
// シンプルな列挙
enum Direction {
    North,
    South,
    East,
    West,
}

// データを持つ列挙（代数的データ型）
enum Shape {
    Circle { radius: float },
    Rectangle { width: float, height: float },
    Triangle { base: float, height: float },
}

// 使い方
let s: Shape = Shape::Circle { radius: 5.0 }

// match で分岐（網羅性チェックあり）
let area: float = match s {
    Shape::Circle { radius }           => 3.14159 * radius * radius,
    Shape::Rectangle { width, height } => width * height,
    Shape::Triangle { base, height }   => 0.5 * base * height,
}
```

```fs
// メソッド定義
impl Shape {
    fn area(self) -> float {
        match self {
            Shape::Circle { radius }           => 3.14159 * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height }   => 0.5 * base * height,
        }
    }

    fn is_circle(self) -> bool {
        match self {
            Shape::Circle { .. } => true,
            _                    => false,
        }
    }
}
```

---

### 15-3. trait

Rustの `trait` をForgeScriptで定義・実装する構文。

```fs
// trait定義（インターフェースに相当）
trait Describable {
    fn describe(self) -> string
}

trait Area {
    fn area(self) -> float
    fn perimeter(self) -> float

    // デフォルト実装（Rustと同様）
    fn is_large(self) -> bool {
        self.area() > 100.0
    }
}

// impl Trait for Type
impl Describable for User {
    fn describe(self) -> string {
        "User({self.name}, age={self.age})"
    }
}

impl Area for Shape {
    fn area(self) -> float {
        match self {
            Shape::Circle { radius }           => 3.14159 * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height }   => 0.5 * base * height,
        }
    }

    fn perimeter(self) -> float {
        match self {
            Shape::Circle { radius }           => 2.0 * 3.14159 * radius,
            Shape::Rectangle { width, height } => 2.0 * (width + height),
            Shape::Triangle { base, height }   => base + height + (base * base + height * height).sqrt(),
        }
    }
}
```

```fs
// トレイト境界（ジェネリクスと組み合わせ）
fn print_area<T: Area>(shape: T) {
    print("Area: {shape.area()}")
}

fn describe_all<T: Describable>(items: list<T>) {
    for item in items {
        print(item.describe())
    }
}
```

---

## 16. derive キーワードの方針

### Rustの `#[derive]` をForgeScriptに取り込む

Rustの `#[derive(Debug, Clone, PartialEq)]` はボイラープレートを自動生成する最強の仕組み。
ForgeScriptでは `derive` キーワードとして **型定義の直前に書く** 形式で統一する。

```fs
// Rustの記法（参考）
#[derive(Debug, Clone, PartialEq)]
struct User { ... }

// ForgeScriptの記法
derive(debug, clone, eq)
struct User {
    name: string,
    age:  number,
}
```

### 組み込みderiveの一覧

| ForgeScript | Rustのderive | 自動生成されるもの |
|---|---|---|
| `debug` | `Debug` | `print(user)` でデバッグ出力 |
| `clone` | `Clone` | `.clone()` メソッド |
| `eq` | `PartialEq, Eq` | `==` `!=` 演算子 |
| `ord` | `PartialOrd, Ord` | `<` `>` `<=` `>=` 演算子 + sort対応 |
| `hash` | `Hash` | HashMapのキーとして使用可能 |
| `display` | `Display` | 文字列補間 `"{user}"` で使用可能 |
| `serialize` | `Serialize` (serde) | JSON変換（use serde_json と連動） |
| `deserialize` | `Deserialize` (serde) | JSON変換（use serde_json と連動） |
| `default` | `Default` | `.default()` コンストラクタ |
| `copy` | `Copy` | 値コピー（小さな型向け） |

```fs
// よく使う組み合わせ
derive(debug, clone, eq)
struct Point {
    x: float,
    y: float,
}

derive(debug, clone, eq, serialize, deserialize)
struct User {
    name:  string,
    age:   number,
    email: string?,
}

// enum にも使える
derive(debug, clone, eq)
enum Status {
    Active,
    Inactive,
    Pending { reason: string },
}
```

### derive のショートハンド

よく使う組み合わせには別名を用意する。

```fs
derive(data)      // = debug + clone + eq（最もよく使う組み合わせ）
derive(serde)     // = serialize + deserialize
derive(full)      // = debug + clone + eq + hash + display

derive(data)
struct Config {
    host: string,
    port: number,
}
```

### カスタムderive（Phase B）

コミュニティが独自の derive を定義できる仕組み。Rustの proc-macro に相当するが、
ForgeScript側では `forge-derive` クレートとして提供する。

```fs
// 将来: カスタムderiveの定義（forge-derive crate）
derive(Builder)   // Builderパターンを自動生成
struct Config { ... }

// → Config::builder().host("localhost").port(8080).build()?
```

---

## 17. forge generate — CLIスキャフォールディング

### 背景と目的

struct / trait / enum を1から書くのは定型作業が多い。
`forge generate` コマンドで骨組みを生成し、編集コストを削減する。
RustのCargo Generateや、Next.jsの `create-next-app` に相当する機能。

### 基本コマンド

```bash
# struct の骨組みを生成
forge generate struct User name:string age:number email:string?

# enum の骨組みを生成
forge generate enum Shape Circle Rectangle Triangle

# trait の骨組みを生成
forge generate trait Describable describe serialize

# impl Trait for Type の骨組みを生成
forge generate impl Describable for User

# テストファイルの生成
forge generate test user_model
```

### 生成されるコード例

```bash
$ forge generate struct User name:string age:number email:string?
```

```fs
// 生成: src/models/user.forge

derive(data)
struct User {
    name:  string,
    age:   number,
    email: string?,
}

impl User {
    fn new(name: string, age: number) -> User! {
        ok(User {
            name,
            age,
            email: none,
        })
    }
}
```

```bash
$ forge generate trait Describable describe
```

```fs
// 生成: src/traits/describable.forge

trait Describable {
    fn describe(self) -> string
}
```

```bash
$ forge generate impl Describable for User
```

```fs
// 生成: 既存の user.forge に追記 (または src/impls/describable_user.forge)

impl Describable for User {
    fn describe(self) -> string {
        // TODO: implement
        todo!()
    }
}
```

### インタラクティブモード

```bash
$ forge generate struct
? Struct name: User
? Fields (name:type, ...): name:string, age:number, email:string?
? Add derive? [data/serde/full/custom]: data
? Generate tests? [Y/n]: Y

✔ Created src/models/user.forge
✔ Created tests/user_test.forge
```

### テンプレートの仕組み

```
.forge/templates/         ← カスタムテンプレート（省略可）
  struct.forge.hbs
  trait.forge.hbs
  impl.forge.hbs
```

デフォルトテンプレートはForgeScript本体に内蔵する。
カスタムテンプレートはHandlebars形式（`.hbs`）でプロジェクト内に置けばオーバーライドできる。

---

## 18. プログラミングパラダイムのポジショニング

### Rustはマルチパラダイム — ForgeScriptはどこに立つか

Rustは手続き型・オブジェクト指向・関数型・ジェネリックプログラミングをすべてサポートする。
「なんでも書ける」ことは自由だが、初学者には「何で書くべきか」の判断基準がない。

ForgeScriptは**パラダイムに優先順位をつけることで、判断基準をデザインに組み込む**。

| パラダイム | ForgeScriptでの位置づけ | 具体的な設計決定 |
|---|---|---|
| **関数型（Functional-first）** | ★★★ 第一優先 | `let` デフォルト不変・`for`/`if` が式・イテレータ優先・rinq吸収 |
| **型駆動（Type-driven）** | ★★★ 第一優先 | `T?`/`T!`・代数的データ型（enum）・match の網羅性チェック |
| **OOP-lite（構成ベース）** | ★★ サポート | struct + impl + trait（継承なし、コンポジション） |
| **手続き型** | ★ 必要最小限 | `state` を使う場面のみ許可・デフォルトは推奨しない |
| **クラス継承OOP** | ✗ 非対応 | Rust自体が継承を持たないためForgeScriptにも持ち込まない |

```
TypeScript: クラスOOPも関数型も両方OK（混在しがち）
ForgeScript: 関数型と型駆動をデフォルトに据えて「正しい書き方」を自然に導く
```

### 関数型ファーストが意味すること

```fs
// 推奨: 関数型スタイル（let + イテレータ）
let result = users
    .where(|u| u.age >= 18)
    .select(|u| u.name)
    .order_by(|n| n)

// 許可されるが推奨しない: 手続き型スタイル（state + for）
state result: list<string> = []
for u in users {
    if u.age >= 18 {
        result = result + [u.name]
    }
}
```

linterが「`state` を使わなくても書けます」と提案する設計にする。
強制はしないが、**関数型スタイルの方が書きやすい設計**になっている。

### OOP-lite：継承なし・コンポジションあり

Rust（そしてForgeScript）のOOPはGoやHaskellに近い。

```fs
// ❌ クラス継承（ForgeScriptには存在しない）
// class Animal { ... }
// class Dog extends Animal { ... }

// ✅ コンポジション + トレイト
trait Animal {
    fn sound(self) -> string
    fn name(self) -> string
}

struct Dog { name: string }
struct Cat { name: string }

impl Animal for Dog {
    fn sound(self) -> string { "Woof" }
    fn name(self) -> string  { self.name }
}

impl Animal for Cat {
    fn sound(self) -> string { "Meow" }
    fn name(self) -> string  { self.name }
}

fn describe<T: Animal>(a: T) -> string {
    "{a.name()} says {a.sound()}"
}
```

「継承がない不便さ」ではなく、「継承がないシンプルさ」として設計する。
TypeScriptでもReactコミュニティが「クラスよりフック」に移行したように、
ForgeScriptは最初から**クラス継承の概念を持ち込まない**。

### 型駆動開発（Type-driven Development）

エラーや欠損値を型で表現し、コンパイル時に処理を強制する。

```fs
// 型が「何が起こりうるか」を文書化する
fn find_user(id: number) -> User?       // 見つからないかもしれない
fn parse_age(s: string) -> number!      // 失敗するかもしれない
fn create_user(name: string) -> User!   // バリデーションエラーがあるかもしれない

// 型を見るだけでエラーハンドリングが必要かわかる
let user = find_user(42)         // User? → match が必要
let age  = parse_age("30")?      // number! → ? で伝播
```

「型はドキュメントである」——型を書くだけで、関数の振る舞いが自明になる。

---

## 19. forge new — プロジェクトテンプレート生成

### 目的

`cargo new` に相当する ForgeScript の入口コマンド。
用途別テンプレートで「最初の1ファイル」から正しい構造を導く。

### 基本コマンド

```bash
forge new my-project                    # デフォルト（スクリプト用途）
forge new my-app   --template web       # Web / WASM アプリ
forge new my-lib   --template lib       # ライブラリ（他プロジェクトからuse可能）
forge new my-cli   --template cli       # CLIツール（clap バインディング付き）
forge new my-data  --template data      # データ処理スクリプト（rinq+統計）
```

### 生成されるプロジェクト構造

```bash
$ forge new my-app --template cli
```

```
my-app/
  forge.toml            ← プロジェクト設定（Cargo.tomlに相当）
  src/
    main.forge          ← エントリーポイント
    models/             ← struct / enum
    commands/           ← CLI コマンド定義
  tests/
    main_test.forge
  .forge/
    templates/          ← カスタムテンプレート置き場（省略可）
  README.md
```

```toml
# forge.toml
[package]
name    = "my-app"
version = "0.1.0"
edition = "forge-2026"    # ForgeScript版のedition

[dependencies]
# forge add コマンドで自動追加される
```

### テンプレート別の初期コード

```bash
$ forge new hello --template script
```

```fs
// src/main.forge（scriptテンプレート）
fn main() {
    print("Hello, World!")
}
```

```bash
$ forge new my-cli --template cli
```

```fs
// src/main.forge（cliテンプレート）
use clap

fn main() {
    let args = clap::parse([
        clap::arg("name", "Your name", type: string),
        clap::flag("verbose", "Enable verbose output"),
    ])

    let greeting = if args.verbose {
        "Hello, {args.name}! (verbose mode)"
    } else {
        "Hello, {args.name}!"
    }

    print(greeting)
}
```

```bash
$ forge new my-web --template web
```

```fs
// src/main.forge（webテンプレート）
state count: number = 0

bind "#counter" { text: "Count: {count}" }
bind "#btn"     { on_click: || { count = count + 1 } }
```

```html
<!-- index.html -->
<script src="forge.min.js"></script>
<h1 id="counter">Count: 0</h1>
<button id="btn">Increment</button>
<script type="forgescript" src="src/main.forge"></script>
```

### インタラクティブモード

```bash
$ forge new

? Project name: my-project
? Template: [script / web / lib / cli / data]: cli
? Add dependencies? (serde_json, tokio, ...): serde_json
? Initialize git? [Y/n]: Y

✔ Created my-project/
✔ Initialized git repository
✔ Running forge check...
✔ All good! Start with: cd my-project && forge run src/main.forge
```

---

## 20. ForgeScript のポジション宣言：「Rustへの玄関口」

### TypeScriptとの対比

TypeScript が「JavaScript の上に型と構造を乗せた」ように、
ForgeScript は「Rust のエコシステムの上に書きやすさを乗せた」。

```
TypeScript  = JavaScript + 型安全性
            = better JavaScript（JSを置き換える意志がある）

ForgeScript = Rust + 書きやすさ
            = Rustへの玄関口（Rustを置き換えない・Rustへ誘う）
```

**重要な違い：** TypeScript は JavaScript の代替を目指しているが、
ForgeScript は **Rustの補完** であり、Rustへの **移行パス** でもある。

```
Rustを知らない人    → ForgeScript で書いて Rust エコを使う
Rustを学んでいる人  → forge transpile でコード変換して Rust を学ぶ
Rust エンジニア     → ForgeScript を内部で動かす（Rhai と同じ用途で検討可）
```

### 「TSがよりよいJSを生成する」に相当するもの

TypeScript コンパイラは、型情報を使ってより安全な JavaScript を生成する。
ForgeScript に相当するのは：

```
forge build → Rust コードを生成 → rustc が最適化 → ネイティブバイナリ

「Rust を書けない人でも、rustc の最適化の恩恵を完全に受けられる」
```

手書きの Rust より生成された Rust が悪い理由はない。
むしろ `derive` の自動展開・イテレータチェーンの最適化・ライフタイム推論は、
rustc が得意とすることであり、ForgeScript はそれを最大限引き出す設計にする。

```
手書きRust:  所有権・ライフタイム・型変換を全部自分で書く
ForgeScript: 「意図」だけ書いて、rustcに最適化を任せる
```

### 3つのターゲット・ユーザー

```
1. Rustエコシステムを使いたい非Rustエンジニア
   → serde_json / reqwest / tokio を「use するだけ」で使える
   → Python/Node.js の代替スクリプト言語として

2. Rustを学んでいるエンジニア
   → forge transpile でForgeScriptをRustに変換して写経
   → T? / T! が Option<T> / Result<T,E> に対応することで型システムを体得

3. データサイエンティスト・分析エンジニア
   → rinq吸収による LINQ/Pandas ライクなコレクション操作
   → Rust の高速処理を Python より簡単に書ける
```

### 設計上の制約として守ること

```
✅ 「書きやすさ」のために型安全性を犠牲にしない
✅ 「便利さ」のために Rust エコシステムとの互換を犠牲にしない
✅ 「シンプルさ」のために将来の拡張可能性を犠牲にしない
❌ GC を持ち込まない（Rust の所有権モデルを壊さない）
❌ 継承を持ち込まない（Rust の設計哲学を壊さない）
❌ nil を持ち込まない（Rust が解決した問題を再発させない）
```

---

## 21. ディレクトリ構成の方針

### 現状の問題

現在の `crates/` 配下は11クレートが並んでおり、それぞれ `lib.rs` 1ファイルだけという状態。

```
crates/
  fs-ast/src/lib.rs         184行
  fs-bytecode/src/lib.rs    313行
  fs-compiler/src/lib.rs    286行
  fs-lexer/src/lib.rs       344行
  fs-parser/src/lib.rs      496行
  fs-repl/src/lib.rs        165行
  rvm-core/src/lib.rs       215行
  rvm-host/src/lib.rs       184行
  rvm-runtime/src/lib.rs    511行
  e2e-tests/src/lib.rs      113行
  test-utils/src/lib.rs     197行
```

**問題点：**
- Cargo クレートの分割 ≠ 可読性の向上（Cargo.toml が11個）
- 言語実装として見た時に「フロントエンド・バックエンド・ランタイム」というフェーズの境界が見えない
- 各クレートが小さすぎて、モジュール間の関係が Cargo.toml の依存関係グラフに散らばっている

### 目標構成：5クレート + 内部モジュール分割

Cargo のクレート境界は「外部に公開するAPIの境界」として使い、
内部の分割は Rust の `mod` で行う。

```
forge/
  Cargo.toml                    ← workspace（5クレートのみ）
  crates/
    forge-compiler/             ← フロントエンド（字句解析〜型チェック）
      Cargo.toml
      src/
        lib.rs
        lexer/
          mod.rs
          tokens.rs
        parser/
          mod.rs
          expr.rs
          stmt.rs
        ast/
          mod.rs
          types.rs
        typechecker/            ← Phase 7-B 以降に追加
          mod.rs
    forge-vm/                   ← RVM（バイトコード・コンパイラ・ランタイム）
      Cargo.toml
      src/
        lib.rs
        bytecode/
          mod.rs
          opcodes.rs
        compiler/               ← AST → バイトコード
          mod.rs
        runtime/                ← スタックマシン実行
          mod.rs
        value.rs                ← Value enum, VmError, CallFrame
    forge-stdlib/               ← 標準ライブラリ（rinq吸収・将来追加）
      Cargo.toml
      src/
        lib.rs
        collections/
          mod.rs
        query/                  ← クエリ式（from...where...select）
          mod.rs
        stats/                  ← rinq-stats の吸収
          mod.rs
    forge-transpiler/           ← forge build（ForgeScript → Rust コード生成）
      Cargo.toml
      src/
        lib.rs
        codegen/
          mod.rs
        emit/
          mod.rs
    forge-cli/                  ← forge コマンド群（エントリーポイント）
      Cargo.toml
      src/
        main.rs
        commands/
          run.rs
          build.rs
          new.rs
          generate.rs
          test.rs
          fmt.rs
          check.rs
```

### クレートの責務

| クレート | 責務 | 外部公開API |
|---|---|---|
| `forge-compiler` | `.forge` → AST → （型チェック） | `parse(src) -> Ast`, `typecheck(ast) -> TypedAst` |
| `forge-vm` | AST → バイトコード → 実行 | `compile(ast) -> Bytecode`, `execute(bc) -> Value` |
| `forge-stdlib` | rinq コレクションAPI・統計 | `list`, `query`, `stats` モジュール |
| `forge-transpiler` | AST → Rust コード生成 | `transpile(ast) -> String` |
| `forge-cli` | `forge` コマンド全体 | バイナリエントリーポイント |

### 移行方針

現在の11クレートは以下のように統合する。

| 現在 | 移行先 |
|---|---|
| `fs-lexer`, `fs-ast`, `fs-parser` | `forge-compiler/src/lexer/`, `ast/`, `parser/` |
| `fs-compiler` | `forge-vm/src/compiler/` |
| `fs-bytecode` | `forge-vm/src/bytecode/` |
| `rvm-core`, `rvm-runtime`, `rvm-host` | `forge-vm/src/value.rs`, `runtime/` |
| `fs-repl` | `forge-cli/src/commands/repl.rs` |
| `e2e-tests`, `test-utils` | `forge-cli/tests/` または workspace 直下の `tests/` |

---

## 22. バージョン依存地獄の回避方針

### JVMとの対比

JVM（Java）が引き起こした「バージョン依存地獄」の根本原因を分析する。

```
JVM地獄の原因：
  1. バイトコード（.class）を配布形式にした
     → JVM のバージョンによって動くバイトコード・動かないバイトコードが生まれた
  2. ランタイムのバージョンが複数共存する
     → Java 8 / 11 / 17 / 21 を管理するツール（jenv等）が必要に
  3. 後方互換を破る変更を大型リリースで入れた
     → Java 9 のモジュールシステムで大量の既存コードが動かなくなった
  4. 実行にランタイムが必須
     → JARを配布しても相手にJVMがなければ動かない
```

### ForgeScript が地獄にならない理由

```
ForgeScript の設計：
  1. RVM バイトコードは内部表現・配布しない
     → .forge ソースを配布する（TypeScript が .ts を配布するのと同じ）
     → バイトコードは forge run 実行時の一時的な中間表現

  2. forge build の成果物はゼロ依存のネイティブバイナリ
     → 実行先に forge も Rust も RVM も不要
     → 「動かない環境」がない

  3. edition システムで後方互換を保証
     → 破壊的変更は新 edition にのみ入れる
     → 古い edition のコードは古いコンパイラでも動く（Rust と同じモデル）

  4. ランタイムを持ち込まない（バイナリ配布の場合）
     → スクリプト用途は forge run だが、配布物は forge build のバイナリのみ
```

### edition システムの設計

```toml
# forge.toml
[package]
name    = "my-project"
edition = "forge-2026"    # このコードが準拠するForgeScript仕様のバージョン
```

```
forge-2026: 初回リリース仕様（T?/T!/let/state/struct/enum/trait）
forge-2027: 追加機能（async/await、DB対応 等）
forge-2028: 将来の変更
```

- **マイナー追加**（新メソッド・新構文糖衣）→ 同edition内で後方互換
- **破壊的変更** → 新editionに入れ、古いeditionのコンパイルは引き続き動く
- **edition移行ツール** → `forge migrate` で自動変換

```bash
forge migrate --from forge-2026 --to forge-2027
# → .forge ファイルを新 edition の構文に自動変換
```

### 「バイトコードを配布しない」設計の徹底

```
❌ 配布すべきでないもの
  - RVM バイトコード（.fbc など）
  - ASTのシリアライズ（.forge.ast など）
  - コンパイラキャッシュ（他マシンへの持ち出し禁止）

✅ 配布するもの
  - .forge ソースファイル（常に人間可読）
  - forge build の成果物（ネイティブバイナリ）
  - WASM バイナリ（forge build --target web）
```

### Go / Deno との比較

| 観点 | Java/JVM | Go | Deno | ForgeScript |
|---|---|---|---|---|
| 配布形式 | JARバイトコード | ネイティブバイナリ | JSバンドル | ソース or ネイティブバイナリ |
| ランタイム依存 | JVM必須 | なし | Denoランタイム必須 | forge build ならなし |
| バージョン管理 | JVM版数管理が必要 | Go自体のバージョン | 安定 | editionシステム |
| 後方互換 | 壊れることがある | 強く保証 | 一部破壊的 | edition で保証 |

Go と Deno が現代的に設計されて「バージョン地獄なし」と評価されているのと同じ方向性を取る。
