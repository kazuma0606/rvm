# ForgeScript / RVM

> Rust エコシステムへの玄関口となるスクリプト言語

ForgeScript は「Kotlin の設計哲学 × Rust のエコシステム × ゼロ依存バイナリ」を目指した言語です。
Python や JavaScript で書いているが速さ・安全性を求める人、Rust に興味はあるが学習コストで断念した人をターゲットにしています。

---

## クイックスタート

```bash
# ビルド
cargo build --release

# ファイルを実行
cargo run --bin forge-new -- run fixtures/hello.forge

# Rust にトランスパイルしてネイティブバイナリを生成
cargo run --bin forge-new -- build fixtures/hello.forge

# 型チェック（実行しない）
cargo run --bin forge-new -- check myfile.forge

# インラインテストを実行
cargo run --bin forge-new -- test myfile.forge

# 対話型 REPL
cargo run --bin forge-new -- repl
```

---

## 言語サンプル

```forge
// 変数・ミュータブル変数
let name = "World"
state count = 0

// 関数
fn fib(n: number) -> number {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
println(fib(10))   // 55

// コレクション API（LINQ スタイル）
let result = [1..=10]
    .filter(x => x % 2 == 0)
    .map(x => x * x)
    .sum()
println(result)    // 220

// Option / Result
fn safe_div(a: number, b: number) -> number! {
    if b == 0 { err("division by zero") } else { ok(a / b) }
}

match safe_div(10, 2) {
    ok(v)  => println(v),
    err(e) => println("エラー: {e}")
}

// struct + impl
struct Point { x: float, y: float }

impl Point {
    fn distance(self) -> float {
        (self.x * self.x + self.y * self.y)
    }
}

// typestate（状態遷移をコンパイル時に保証）
typestate Connection {
    states: [Disconnected, Connected]

    Disconnected {
        fn connect(self, host: string) -> Connected { /* ... */ }
    }

    Connected {
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

## 型システム

| ForgeScript | Rust 変換 | 説明 |
|---|---|---|
| `number` | `i64` | 整数 |
| `float` | `f64` | 浮動小数点数 |
| `string` | `String` | UTF-8 文字列 |
| `bool` | `bool` | 真偽値 |
| `list<T>` | `Vec<T>` | 可変長リスト |
| `T?` | `Option<T>` | 値があるかもしれない |
| `T!` | `Result<T, anyhow::Error>` | 失敗するかもしれない |

---

## CLI コマンド

| コマンド | 説明 |
|---|---|
| `forge run <file>` | インタープリタで実行 |
| `forge build <file>` | Rust にトランスパイルしてネイティブバイナリを生成 |
| `forge transpile <file>` | Rust コードを出力（コンパイルしない） |
| `forge check <file>` | 型チェックのみ（実行しない） |
| `forge test <file>` | インラインテストブロックを収集・実行 |
| `forge test <file> --filter <pattern>` | テスト名でフィルタ |
| `forge repl` | 対話型 REPL を起動 |
| `forge help` | ヘルプを表示 |

---

## 実装済み機能

### コア言語（forge run）

| 機能 | 詳細 |
|---|---|
| 変数 | `let` / `state`（ミュータブル）/ `const` |
| 関数 | `fn` / クロージャ / 再帰 |
| 制御フロー | `if` / `while` / `for` / `match` |
| 型 | `T?` / `T!` / `list<T>` / 型推論 |
| 文字列補間 | `"Hello, {name}!"` |
| `?` 演算子 | Result の早期リターン |
| コレクション API | 30 種（map / filter / fold / order_by 等） |

### 型定義（forge run）

| 機能 | 詳細 |
|---|---|
| `struct` | 定義・`impl`・`@derive` |
| `enum` | Unit / Tuple / Struct バリアント・パターンマッチ |
| `trait` / `mixin` | 純粋契約・デフォルト実装 |
| `data` | 全 derive 自動付与・`validate` ブロック |
| `typestate` | 状態遷移のコンパイル時保証（PhantomData パターン） |

### モジュールシステム（forge run）

`use ./path/module.symbol` / `pub` 可視性 / `mod.forge` ファサード /
外部クレート自動検出 / 循環参照検出 / `when` 条件付きコンパイル / `use raw` 生 Rust 埋め込み

### テストシステム（forge test）

`test "名前" { }` インラインテスト / `assert` / `assert_eq` / `assert_ne` / `assert_ok` / `assert_err` /
テストスコープ分離 / `--filter` オプション

### トランスパイラ（forge build）

| Phase | 内容 |
|---|---|
| B-1〜B-4 | 基本変換・型システム・クロージャ・コレクション |
| B-5 | struct / data / enum / impl / trait / mixin → Rust |
| B-6 | モジュール / when / use raw / test ブロック → Rust |
| B-7 | async/await → async fn 自動昇格・tokio 統合 |
| B-8 | typestate → PhantomData パターン（制約付き） |

---

## アーキテクチャ

```
.forge ソース
    │
    ├─ forge run ────→ Lexer → Parser → AST → Interpreter
    │                                         (Rc<RefCell<T>> で動的所有権)
    │
    ├─ forge check ──→ Lexer → Parser → AST → TypeChecker
    │                                         (型推論・網羅性チェック)
    │
    ├─ forge test ───→ Lexer → Parser → AST → Interpreter（テストモード）
    │
    └─ forge build ──→ Lexer → Parser → AST → TypeChecker
                                              → CodeGenerator → .rs
                                              → rustc → ネイティブバイナリ
```

`forge run` と `forge build` の出力は完全に等価（ラウンドトリップテストで検証済み）。

---

## プロジェクト構成

```
rvm/
├── Cargo.toml               # workspace 定義
│
├── crates/                  # RVM 実装（Rust クレート群）
│   ├── forge-compiler/      # Lexer → Parser → AST → 型チェッカー
│   ├── forge-vm/            # ツリーウォーキングインタープリタ
│   ├── forge-stdlib/        # 標準ライブラリ（コレクション API）
│   ├── forge-transpiler/    # AST → Rust コード生成器
│   └── forge-cli/           # CLI バイナリ（forge-new）+ E2E テスト
│
├── lang/                    # ForgeScript 言語仕様・ドキュメント
│   ├── ROADMAP.md           # 実装状況・ロードマップ
│   ├── v0.1.0/              # 言語仕様書 (spec_v0.0.1.md)
│   ├── typedefs/            # 型定義仕様（struct/enum/trait/data/typestate）
│   ├── modules/             # モジュールシステム仕様
│   ├── transpiler/          # トランスパイラ仕様・計画・タスク
│   └── tests/               # テストシステム仕様
│
├── ext/                     # VS Code シンタックスハイライト拡張
├── dev/                     # 設計ドキュメント（design-v2.md / design-v3.md）
├── fixtures/                # 動作確認用サンプル
└── UAT/                     # ユーザー受け入れテスト
```

---

## テスト

```bash
# ワークスペース全テスト（293 本）
cargo test --workspace

# E2E テストのみ
cargo test -p forge-cli --test e2e
```

| クレート | テスト数 | 内容 |
|---|---|---|
| forge-compiler | 76 本 | Lexer・Parser・型チェッカー・統合テスト |
| forge-vm | 90 本 | インタープリタ・モジュール・型定義 |
| forge-transpiler | 40 本 | スナップショットテスト（各フェーズ） |
| forge-stdlib | 14 本 | コレクション API |
| forge-cli (E2E) | 73 本 | run / build / check / test / ラウンドトリップ |

---

## 開発

```bash
cargo check --workspace   # 型チェック
cargo fmt --all           # フォーマット
cargo clippy --workspace  # Lint
```

**言語仕様**: [`lang/v0.1.0/spec_v0.0.1.md`](lang/v0.1.0/spec_v0.0.1.md)
**ロードマップ**: [`lang/ROADMAP.md`](lang/ROADMAP.md)
**設計方針**: [`dev/design-v3.md`](dev/design-v3.md)
