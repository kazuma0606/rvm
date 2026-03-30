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

# 型チェック（実行しない）
cargo run --bin forge-new -- check myfile.forge

# 対話型 REPL
cargo run --bin forge-new -- repl
```

---

## 言語サンプル

```forge
// 変数
let name = "World"
print("Hello, {name}!")

// 関数
fn fib(n: number) -> number {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
print(fib(10))   // 55

// コレクション API
let nums = [1..=10]
let result = nums.filter(x => x % 2 == 0)
                 .map(x => x * x)
                 .sum()
print(result)    // 220

// Option / Result
fn safe_div(a: number, b: number) -> number! {
    if b == 0 { err("division by zero") } else { ok(a / b) }
}

match safe_div(10, 2) {
    ok(v)  => print(v),
    err(e) => print("エラー: {e}")
}
```

---

## 型システム

| ForgeScript | 説明 |
|---|---|
| `number` | 整数 (i64) |
| `float` | 浮動小数点 (f64) |
| `string` | UTF-8 文字列 |
| `bool` | 真偽値 |
| `list<T>` | 可変長リスト |
| `T?` | Option（値があるかもしれない） |
| `T!` | Result（失敗するかもしれない） |

---

## プロジェクト構成

```
rvm/
├── forge-compiler/          # Lexer → Parser → AST → 型チェッカー
│   └── src/
│       ├── lexer/           # トークナイザ
│       ├── parser/          # 再帰下降パーサ
│       ├── ast/             # 構文木定義
│       └── typechecker/     # 型推論・型検査・網羅性チェック
│
├── forge-vm/                # ツリーウォーキングインタープリタ
│   └── src/
│       ├── value.rs         # Value 型（Int/Float/String/Bool/Unit/Option/Result/List/Closure）
│       └── interpreter.rs   # 評価器 + コレクション API (30 メソッド)
│
├── forge-stdlib/            # 標準ライブラリ
│   └── src/collections/     # コレクション API モジュール
│
├── forge-cli/               # CLI バイナリ（forge-new）
│   ├── src/main.rs          # run / check / repl / help
│   └── tests/e2e.rs         # E2E テスト (29 本)
│
├── forge/                   # 言語仕様・実装計画
│   ├── spec_v0.0.1.md       # 言語仕様書
│   ├── plan.md              # ロードマップ
│   └── tasks.md             # Phase 別タスク管理（進捗トラッキング）
│
├── dev/                     # 設計ドキュメント
│   ├── design-v3.md         # 最新設計方針（2モードアーキテクチャ）
│   └── design-v2.md         # 設計議論の記録
│
└── fixtures/
    └── hello.forge          # 動作確認用サンプル
```

---

## CLI コマンド

| コマンド | 説明 |
|---|---|
| `forge run <file.forge>` | ファイルを読み込んで実行 |
| `forge check <file.forge>` | 型チェックのみ（実行しない） |
| `forge repl` | 対話型 REPL を起動 |
| `forge help` | ヘルプを表示 |

---

## コレクション API

リストに対してメソッドチェーンで操作できます。

```forge
[1..=100]
    .filter(x => x % 3 == 0)
    .map(x => x * 2)
    .take(5)
    .fold(0, (acc, x) => acc + x)
```

実装済みメソッド: `map` / `filter` / `flat_map` / `filter_map` / `take` / `skip` /
`take_while` / `skip_while` / `enumerate` / `zip` / `sum` / `count` / `fold` /
`any` / `all` / `none` / `first` / `last` / `nth` / `min` / `max` /
`min_by` / `max_by` / `order_by` / `order_by_descending` / `then_by` /
`then_by_descending` / `reverse` / `distinct` / `collect`

---

## 実装状況

| Phase | 内容 | 状態 |
|---|---|---|
| 0 | 基盤整備・クレート構成 | ✅ 完了 |
| 1 | Lexer / Parser / AST | ✅ 完了 |
| 2 | インタープリタ / stdlib / CLI | ✅ 完了 |
| 3 | コレクション API (30 メソッド) | ✅ 完了 |
| 4 | 型チェッカー / `forge check` | ✅ 完了 |
| 5 | struct / enum / trait | 未着手 |
| 6 | Rust トランスパイラ (`forge build`) | 未着手 |

---

## テスト

```bash
# ワークスペース全テスト
cargo test --workspace

# E2E テストのみ
cargo test --package forge-cli --test e2e
```

テスト内訳:

| クレート | テスト数 | 内容 |
|---|---|---|
| forge-compiler | 44 本 | Lexer・Parser・型チェッカー |
| forge-vm | 26 本 | インタープリタ |
| forge-stdlib | 13 本 | コレクション API |
| forge-cli (E2E) | 29 本 | run / check / repl |

---

## アーキテクチャ

```
.forge ソース
    │
    ├─ forge run ──→ Lexer → Parser → AST → Interpreter
    │                                        (Rc<RefCell<T>> で動的所有権)
    │
    └─ forge check → Lexer → Parser → AST → TypeChecker
                                             (型推論 + 網羅性チェック)
```

Phase 6 では `forge build` による Rust トランスパイルモードを追加予定。

---

## 開発

```bash
cargo check --workspace   # 型チェック
cargo fmt --all           # フォーマット
cargo clippy --workspace  # Lint
```

**言語仕様**: [`forge/spec_v0.0.1.md`](forge/spec_v0.0.1.md)
**設計方針**: [`dev/design-v3.md`](dev/design-v3.md)
**タスク管理**: [`forge/tasks.md`](forge/tasks.md)
