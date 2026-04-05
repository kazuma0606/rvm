# ForgeScript

> **Pythonのように書けて、Rustとして動く。**

ForgeScript は「Rust の難しさを隠蔽し、Rust の強さを届ける」スクリプト言語です。

- `forge run` でインタープリタとしてすぐ動く（Pythonのような体験）
- `forge build` でネイティブバイナリになる（Rustの性能・安全性）
- ノートブック（`.fnb`）でインタラクティブに試せる（近日対応予定）
- AIと一緒に書きやすい（明示的な型・シンプルな構文）

Rust に興味はあるが学習コストで断念した人、Python や Go で書いているがパフォーマンスと安全性が欲しい人のための言語です。

---

## クイックスタート

```bash
# ビルド（Rust が必要）
cargo build --release

# forge コマンドとして使う
./target/release/forge run hello.forge
./target/release/forge repl
./target/release/forge new my-app
```

```forge
// hello.forge
println("Hello, ForgeScript!")
```

---

## 言語サンプル

```forge
// 変数（let: 不変 / state: 可変）
let name = "World"
state count = 0

// 関数
fn fib(n: number) -> number {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
println(fib(10))   // 55

// コレクション API
let result = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    |> filter(fn(x) { x % 2 == 0 })
    |> map(fn(x) { x * x })
    |> fold(0, fn(acc, x) { acc + x })
println(result)    // 220

// エラーハンドリング（T! = Result、? で早期リターン）
fn safe_div(a: number, b: number) -> number! {
    if b == 0 { err("division by zero") } else { ok(a / b) }
}

match safe_div(10, 2) {
    ok(v)  => println(v),
    err(e) => println("エラー: {e}"),
}

// data 型（バリデーション付き）
data User {
    name:  string
    email: string
    age:   number

    validate {
        name.len() >= 2,
        email.contains("@"),
        age >= 0,
    }
}

// typestate（状態遷移をコンパイル時に保証）
typestate Connection {
    Disconnected -> Connected -> Disconnected

    Disconnected {
        fn connect(self, host: string) -> Connected! { /* ... */ }
    }
    Connected {
        fn send(self, msg: string) -> Connected! { /* ... */ }
        fn disconnect(self) -> Disconnected { /* ... */ }
    }
}

// インラインテスト
test "fib: 基本ケース" {
    assert_eq(fib(0), 0)
    assert_eq(fib(1), 1)
    assert_eq(fib(10), 55)
}
```

---

## HTTP マイクロサービス（Anvil）

```forge
use anvil/anvil.*
use anvil/request.*
use anvil/response.*
use anvil/middleware.*

fn hello_handler(req: Request<string>) -> Response<string>! {
    ok(Response::text("Hello, {req.params.get("name") |> or("World")}"))
}

let APP = Anvil::new()
    .use(logger())
    .get("/hello/:name", hello_handler)

fn dispatch(raw: RawRequest) -> RawResponse! {
    APP.dispatch_async(raw).await?
}

fn main() -> number! {
    tcp_listen_async(8080, dispatch)?
    ok(0)
}
main()?
```

```bash
forge run src/main.forge
# → http://localhost:8080/hello/ForgeScript
```

---

## CLI コマンド

| コマンド | 説明 |
|---|---|
| `forge run <file>` | インタープリタで実行（即時・Rust不要） |
| `forge build <file>` | Rust にトランスパイルしてネイティブバイナリを生成 |
| `forge transpile <file>` | Rust コードを出力（コンパイルしない） |
| `forge check <file>` | 型チェックのみ（実行しない） |
| `forge test <file>` | インラインテストを収集・実行 |
| `forge test <file> --filter <pattern>` | テスト名でフィルタ |
| `forge repl` | 対話型 REPL を起動 |
| `forge new <name>` | プロジェクトを新規作成 |
| `forge help` | ヘルプを表示 |

---

## Rust と ForgeScript の対応

| ForgeScript | Rust | 説明 |
|---|---|---|
| `let` / `state` | `let` / `let mut` | 不変・可変変数 |
| `T?` | `Option<T>` | 値があるかもしれない |
| `T!` | `Result<T, E>` | 失敗するかもしれない |
| `?` 演算子 | `?` 演算子 | エラーの早期リターン |
| `data` 型 | `struct` + バリデーション手書き | バリデーション付きデータ型 |
| `typestate` | `PhantomData` パターン手書き | 状態遷移のコンパイル時保証 |
| `list<T>` | `Vec<T>` | 可変長リスト |
| `number` | `i64` | 整数 |
| `float` | `f64` | 浮動小数点数 |

Rustの難しい部分（borrow checker・lifetime・`Box<dyn Trait>`・`Pin`・`Send`境界）は言語が隠蔽し、`forge build` 時に適切なRustコードとして生成されます。

---

## 実装済み機能

### コア言語
- 変数（`let` / `state` / `const`）・関数・クロージャ・再帰
- 制御フロー（`if` / `while` / `for` / `match`）
- 型システム（`T?` / `T!` / ジェネリクス / 型推論 / match 網羅性チェック）
- 文字列補間（`"Hello, {name}!"`）・`?` 演算子
- コレクション API 30種（`map` / `filter` / `fold` / `order_by` / `group_by` 等）
- 組み込み関数（`print` / `println` / `string` / `number` / `len` / `type_of` 等）

### 型定義
- `struct` + `impl` + `@derive`（Debug / Clone / Eq / Hash / Ord / Default 等）
- `enum`（Unit / Tuple / Struct バリアント）
- `trait` / `mixin`（純粋契約・デフォルト実装）
- `data`（全 derive 自動付与・`validate` ブロック）
- `typestate`（状態遷移のコンパイル時保証）

### モジュールシステム
- `use ./path/module.symbol` / `pub` 可視性 / `mod.forge` ファサード
- `forge.toml` でローカルパス依存（`dep = { path = "..." }`）
- 循環参照検出 / 未使用インポート警告

### トランスパイラ（forge build）
- 基本変換・型システム・クロージャ・コレクション（B-1〜B-4）
- 型定義変換 struct / data / enum / impl / trait / mixin（B-5）
- モジュール / when / use raw / test（B-6）
- async/await 自動昇格・tokio 統合（B-7）
- typestate → PhantomData パターン（B-8）

### パッケージ
- **Anvil**（`packages/anvil/`）: Express スタイルの HTTP マイクロフレームワーク
  - ルーティング・ミドルウェア・CORS・JSON パーサー・logger

---

## アーキテクチャ

```
.forge ソース
    │
    ├─ forge run ────→ Lexer → Parser → AST → Interpreter
    │                                         （即時実行・Rust不要）
    │
    ├─ forge check ──→ Lexer → Parser → AST → TypeChecker
    │
    ├─ forge test ───→ Lexer → Parser → AST → Interpreter（テストモード）
    │
    └─ forge build ──→ Lexer → Parser → AST → TypeChecker
                                              → CodeGenerator → .rs
                                              → cargo build → ネイティブバイナリ
```

`forge run` と `forge build` の出力は完全に等価（ラウンドトリップテストで検証済み）。

---

## プロジェクト構成

```
rvm/
├── crates/
│   ├── forge-compiler/      # Lexer → Parser → AST → 型チェッカー
│   ├── forge-vm/            # ツリーウォーキングインタープリタ
│   ├── forge-stdlib/        # 標準ライブラリ（fs / json / net）
│   ├── forge-transpiler/    # AST → Rust コード生成器
│   └── forge-cli/           # CLI バイナリ（forge）
│
├── packages/
│   └── anvil/               # HTTP マイクロフレームワーク
│
├── examples/
│   └── anvil/               # Anvil を使ったサンプルサーバ
│
├── lang/                    # 言語仕様・ロードマップ・設計ドキュメント
│   ├── ROADMAP.md
│   ├── app_ideas.md         # ForgeScript で作ると有用なアプリ素案
│   └── extend_idea.md       # 言語拡張アイデア（?.・yield・非同期クロージャ等）
│
├── ext/                     # VS Code シンタックスハイライト拡張
└── dev/                     # 設計ドキュメント（design-v3.md）
```

---

## テスト

```bash
# ワークスペース全テスト
cargo test --workspace

# E2E テストのみ
cargo test -p forge-cli --test e2e
```

---

## ロードマップ

詳細は [`lang/ROADMAP.md`](lang/ROADMAP.md) を参照。

**近日対応予定**:
- `forge/std` 拡充（`env()` / `args()` / `stringify()` 等）
- `forge-http` パッケージ（HTTP クライアント）
- Linux バイナリ配布（`cargo install` 対応）
- ノートブック形式（`.fnb`）+ VS Code Notebook 拡張

---

**言語仕様**: [`lang/v0.1.0/spec_v0.0.1.md`](lang/v0.1.0/spec_v0.0.1.md)
**ロードマップ**: [`lang/ROADMAP.md`](lang/ROADMAP.md)
**設計方針**: [`dev/design-v3.md`](dev/design-v3.md)
