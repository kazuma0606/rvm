# ForgeScript モジュールシステム仕様

> ステータス: 設計中（未実装）
> 対象: forge 0.2.0 以降

---

## 設計方針

### Rust の `mod` 宣言を廃止する

Rust はファイルを分割するために2段階の操作が必要。

```rust
// Rust: mod 宣言（ファイルが存在することをコンパイラに伝える）
mod utils;        // utils.rs または utils/mod.rs を探す
mod models;

// そのあと use でインポート
use utils::helper::add;
use models::User;
```

この「宣言してからインポート」という二段階が初学者には不要な複雑さ。

**ForgeScript では `mod` 宣言を廃止し、ディレクトリ構造がそのままモジュール構造になる。**

### パス解決の基準

`use` パスは `forge.toml` が存在するディレクトリ直下の `src/` を起点とする（Rust と同じ）。

```
myproject/
  forge.toml        ← プロジェクトルート
  src/
    main.forge      ← エントリーポイント
    utils/
      helper.forge
```

```forge
// src/main.forge から
use utils/helper.add    // → src/utils/helper.forge を探す
```

### モジュールトップレベルの制約

モジュールトップレベルには `const`（コンパイル時定数）のみ記述可能。
副作用のあるコード（関数呼び出し・`let` 宣言）はすべて関数内に書く。

```forge
// OK: コンパイル時定数
const MAX_SIZE = 100
const PI: float = 3.14159

// ❌ エラー: 副作用のある式はトップレベルに書けない
let db = connect("localhost")
```

これにより初期化順序の問題を根本から防ぐ。ランタイムの初期化は `fn main()` 内で行う。

---

## 1. ディレクトリ構造

```
src/
  main.forge          ← エントリーポイント
  utils/
    mod.forge         ← モジュールの公開APIを定義（省略可）
    helper.forge
    formatter.forge
  models/
    user.forge
    product.forge
```

- ディレクトリ = モジュール
- `mod.forge` = モジュールの入口（省略可。`index.ts` / `__init__.py` に相当）
- `mod.forge` がない場合はディレクトリ内の全 `.forge` ファイルが直接アクセス可能

---

## 2. `use` 構文

### 2-1. ローカルモジュール（`./` プレフィックス必須）

ローカルモジュールは必ず `./` で始める。TypeScript / Node.js と同じ規則。

```forge
// 単一インポート
use ./utils/helper.add

// 複数インポート（波括弧）
use ./utils/helper.{add, subtract}
use ./models/user.{User, UserRole}

// ワイルドカード
use ./utils/helper.*

// エイリアス
use ./utils/helper.add as add_numbers
use ./models/user.User as UserModel
```

### 2-2. 外部クレート（裸の識別子 → Cargo 名前解決をトリガー）

`./` を持たない裸の識別子は必ず外部クレートとして扱う。

```forge
// 外部クレート（forge.toml の [dependencies] に自動追記）
use serde
use tokio
use reqwest.{Client, Response}
```

### 2-3. ForgeScript 標準ライブラリ（`forge/std/` プレフィックス）

```forge
use forge/std/collections.{HashMap, HashSet}
use forge/std/io.{read_file, write_file}
use forge/std/http.Client
```

### 2-4. ローカル vs 外部の区別ルール

| パターン | 判定 | 例 |
|---|---|---|
| `./` で始まるパス | ローカルモジュール（確定） | `use ./utils/helper.add` |
| `forge/std/` 始まり | 標準ライブラリ | `use forge/std/io.read_file` |
| 裸の識別子（`./` なし） | 外部クレート（確定） | `use serde` / `use reqwest.{Client}` |

`./` の有無が判定の唯一の基準。パーサーが一意に決定できる。

### 2-5. ローカルディレクトリの命名規則

`./` プレフィックスによりローカルと外部クレートは構文で区別できるが、
可読性と混乱防止のため以下を推奨する。

**推奨**: `snake_case` のプロジェクト固有の名前を使う

```
✅ 推奨
  src/user_service/
  src/auth_helpers/
  src/api_client/

⚠️ 非推奨（有名Rustクレートと同名）
  src/reqwest/    ← reqwest クレートと紛らわしい
  src/serde/      ← serde クレートと紛らわしい
  src/tokio/      ← tokio クレートと紛らわしい
```

同名でもコンパイルエラーにはならないが（`./` で区別されるため）、
コードレビュー時の混乱を避けるため公式リファレンスで一覧を提示する。

**主要な予約推奨回避リスト（抜粋）:**
`serde`, `tokio`, `reqwest`, `axum`, `hyper`, `tonic`, `sqlx`,
`diesel`, `clap`, `tracing`, `anyhow`, `thiserror`, `rayon`,
`regex`, `uuid`, `chrono`, `rand`, `log`, `env_logger`

---

## 3. 可視性（`pub`）

デフォルトはモジュール内プライベート。外部に公開するには `pub` を付ける。

```forge
// utils/helper.forge

pub fn add(a: number, b: number) -> number { a + b }   // 公開
pub fn subtract(a: number, b: number) -> number { a - b }

fn internal_helper() -> number { 42 }   // プライベート（外部から使えない）
```

---

## 4. `mod.forge` — モジュールの公開 API 定義

ディレクトリに `mod.forge` を置くことで、モジュールとして外部に公開するものを明示できる。

```forge
// utils/mod.forge

// helper.forge の一部だけを re-export
pub use helper.{add, subtract}

// formatter.forge の全公開シンボルを re-export
pub use formatter.*

// internal_helper は re-export しない → utils の外からは見えない
```

`mod.forge` がない場合はディレクトリ内の `pub` なシンボルがすべてアクセス可能。

---

## 5. 使用例

### ディレクトリ構成

```
src/
  main.forge
  math/
    mod.forge
    basic.forge
    advanced.forge
  models/
    user.forge
```

### `math/basic.forge`

```forge
pub fn add(a: number, b: number) -> number { a + b }
pub fn multiply(a: number, b: number) -> number { a * b }

fn internal() -> number { 0 }   // プライベート
```

### `math/mod.forge`

```forge
pub use basic.{add, multiply}
// advanced は一部だけ公開
pub use advanced.fast_pow
```

### `models/user.forge`

```forge
pub data User {
    name: string
    age: number
}

pub fn new_user(name: string, age: number) -> User {
    User { name, age }
}
```

### `main.forge`

```forge
use ./math.{add, multiply}       // mod.forge 経由で re-export されたもの
use ./models/user.{User, new_user}

let result = add(1, 2)
let u = new_user("Alice", 30)
println("{u.name}: {result}")
```

---

## 6. `forge build` での Rust 変換

ForgeScript のモジュール構造は Rust の `mod` ツリーに変換される。ユーザーには見えない。

```
ForgeScript                    →  生成される Rust
─────────────────────────────────────────────────
src/utils/helper.forge         →  src/utils/helper.rs
src/utils/mod.forge            →  src/utils/mod.rs
use ./utils/helper.add         →  use crate::utils::helper::add;
pub use helper.{add, subtract} →  pub use helper::{add, subtract};
```

---

## 7. 未使用インポートの検出

`main.forge` を起点に依存グラフを辿り、到達しないモジュールを未使用として警告する。

```
main.forge → use utils/helper ✔
           → use models/user  ✔
           → use legacy/old   ← どこからも呼ばれていない

⚠️ 警告: `legacy/old` はインポートされていますが使用されていません
```

---

## 8. 循環参照の検出

ForgeScript コンパイラが Rust に渡す前に検出する（二重安全網）。

```
1. 全 use 文を収集して有向グラフを構築
2. トポロジカルソートで閉路（サイクル）を検出
3. 閉路があれば ForgeScript レベルでエラーを出す
4. 仮に抜けても rustc が最終的に検出する
```

ForgeScript 側で止めることで読みやすいエラーメッセージを出せる。

```
循環参照エラー: utils/helper → models/user → utils/helper
  utils/helper.forge:3  use models/user.User
  models/user.forge:1   use utils/helper.format
```

---

## 8. 同名シンボルの衝突

### 外部クレートの重複

`use` をトリガーとした Cargo 名前解決はインメモリの依存処理層で重複を除外してから Cargo.toml に書き込む。べき等に動作する。

```
use serde        ← 依存リストに追加（既にあれば skip）
use serde        ← skip（同一クレート名）
↓
Cargo.toml への書き込みは1回だけ
```

### ローカルモジュール同士の衝突

異なるモジュールから同名のシンボルをインポートした場合はエイリアスで解決。

```forge
use utils/math.add as utils_add
use core/math.add as core_add

let result = utils_add(1, 2)
```

エイリアスなしで衝突した場合はコンパイルエラー。

```
シンボル衝突エラー: `add` が複数のモジュールからインポートされています
  utils/math.add
  core/math.add
解決策: エイリアスを使用してください（use utils/math.add as utils_add）
```

### `use *` のスコープ汚染と警告

ワイルドカードインポートで同名シンボルが衝突した場合、**インポート時点で警告**を出す。

```forge
use utils/math.*   // add, subtract, multiply
use core/math.*    // add, format ← add が衝突！

// ⚠️ 警告: `add` が複数のモジュールから `use *` でインポートされています
//   utils/math.add
//   core/math.add
// 予期しない関数が呼ばれる可能性があります。
// 明示的なインポートまたはエイリアスを推奨します。
```

呼び出し時にどのモジュールの `add` かを IDE が表示する（LSP 連携）。

### 生 Rust 関数の呼び出し形式

`use raw` ブロック内で Rust の関数を呼ぶ場合は `::` プレフィックスで ForgeScript 関数と明示的に区別する。

```forge
use raw {
    // Rust の標準ライブラリは :: で始める
    let map = ::std::collections::HashMap::new();
    let val = ::std::env::var("HOME");
}
```

これにより「ForgeScript の関数」と「生 Rust の関数」が構文レベルで区別できる。

### 再エクスポートチェーンの深さ制限

再エクスポートは **3段階を上限の目安** とする。それ以上は保守性が著しく下がるため非推奨。

```
1段: utils/helper               → OK
2段: utils/mod → helper         → OK
3段: lib/mod → utils/mod → helper → OK（上限目安）
4段以上                         → ⚠️ 警告（非推奨）
```

深いチェーンが必要な場合は、カスタム Result 型でエラー発生レイヤーを明示する設計を推奨。

```forge
data LayeredError {
    layer: string        // "db" / "service" / "api" など
    message: string
    source: LayeredError?
}

fn deep_operation() -> string![LayeredError] { ... }
```

---

## 9. 条件付きコンパイル — `when` キーワード

Rust の `#[cfg(...)]` は属性マクロ構文で TS/Python ユーザーには異質。
ForgeScript では **`when` キーワード** を採用する。コンパイル時の `if` として直感的に読める。

```forge
// プラットフォーム分岐
when platform.linux {
    fn config_path() -> string { "/etc/forge" }
}
when platform.windows {
    fn config_path() -> string { "C:/ProgramData/forge" }
}

// フィーチャーフラグ
when feature.debug {
    fn log(msg: string) { println("[DEBUG] {msg}") }
}
when not feature.debug {
    fn log(msg: string) { }   // リリース時は no-op
}

// 環境分岐
when env.dev  { const API_URL = "http://localhost:3000" }
when env.prod { const API_URL = "https://api.example.com" }
```

### `forge build` での変換

```
when platform.linux { ... }  →  #[cfg(target_os = "linux")]
when feature.debug  { ... }  →  #[cfg(feature = "debug")]
when env.dev        { ... }  →  #[cfg(debug_assertions)]
when test           { ... }  →  #[cfg(test)]
```

---

## 10. テストモジュール

`test "..." { }` ブロックは暗黙的に `when test` として扱われる。

```forge
// これは
test "add works" { assert add(1, 2) == 3 }

// 内部的にはこれと同等
when test {
    test "add works" { assert add(1, 2) == 3 }
}
```

- `forge run` → test ブロックをスキップ
- `forge test` → test ブロックのみ実行
- `forge build` → `#[cfg(test)]` に変換

モジュールをまたいだテストも `when test` ブロック内で他モジュールを use できる。

```forge
// models/user.forge
pub data User { name: string, age: number }

when test {
    use forge/std/assert.*

    test "User が作れる" {
        let u = User { name: "Alice", age: 30 }
        assert u.name == "Alice"
    }
}
```

---

## 11. REPL でのローカルモジュールインポート

Python の対話モードに倣い、カレントディレクトリから相対パスで解決する。

```forge
> use utils/helper.{add, subtract}
✔ utils/helper をロード済み

> add(1, 2)
3

> use models/user.User
✔ models/user をロード済み
```

### REPL 専用コマンド

```
:modules              # ロード済みモジュール一覧
:reload utils/helper  # モジュールの再読み込み（ファイル変更後に使う）
:unload utils/helper  # アンロード
```

モジュールファイルを変更した後 `:reload` することで、REPL セッションを維持しながら最新のコードを反映できる。
