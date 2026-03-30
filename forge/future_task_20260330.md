# 将来タスク（2026-03-30）

---

## 1. シンタックスハイライト

最速の実現経路は **VS Code 拡張（TextMate grammar）**。

```
forge-vscode/
├── package.json        ← 言語登録（.forge の関連付け）
└── syntaxes/
    └── forge.tmLanguage.json
```

定義するトークン：

| カテゴリ | 対象 |
|---|---|
| キーワード | `let` `state` `const` `fn` `return` `if` `else` `for` `in` `while` `match` `some` `none` `ok` `err` |
| 型名 | `number` `float` `string` `bool` `list` |
| 演算子 | `=>` `->` `?` `..` `..=` `&&` `\|\|` |
| 文字列補間 | `"Hello, {name}!"` の `{...}` 部分を別スコープに |
| コメント | `//` |

TextMate grammar は JSON で書けて実装コストが低く、VS Code・Zed・GitHub が共通で使える。より高精度にしたい場合は後から **Tree-sitter grammar** に移行するのが自然な流れ。

---

## 2. ForgeScript ネイティブテスト

### A: インラインブロック（モジュールシステム不要・今すぐ実装可）

```forge
fn add(a: number, b: number) -> number {
    a + b
}

test "add: 基本" {
    assert_eq(add(1, 2), 3)
    assert_eq(add(0, 0), 0)
}

test "add: 負の数" {
    assert(add(-1, 1) == 0)
}
```

- `forge run` では `test` ブロックをスキップ
- `forge test <file>` では `test` ブロックを収集して実行
- Zig のスタイルに近い

### B: `.test.forge` コンパニオンファイル（モジュール実装後）

```forge
// math.test.forge
test "add" {
    assert_eq(add(1, 2), 3)
}
```

- Go の `_test.go` に近い
- `forge test` がディレクトリを走査して `*.test.forge` を自動収集
- 本番コードとテストコードの完全分離

### 方針

**まず A（インライン）、後から B（コンパニオン）** の順序が現実的。現時点ではモジュールシステムがないため B はまだ作れない。A が実装されていれば、B は「ファイルを分けるだけ」の拡張になる。最終的には両方共存させて選べるようにする（Rust がインラインと `tests/` の両方を持つように）。

「より簡潔に」という観点では B を推奨。テストと実装が混在すると長いファイルになりがちで、ファイルが分かれている方がレビューも CI も扱いやすい。

---

## 3. Playground サーバ

### パターン A: WASM（推奨・サーバーレス）

```
forge-wasm/ (新クレート)
└── src/lib.rs
    #[wasm_bindgen]
    pub fn eval(source: &str) -> String { ... }
```

- `forge-vm` を `wasm32-unknown-unknown` でビルド
- `wasm-bindgen` で JS から `eval(source)` を呼ぶだけ
- 静的サイトにデプロイ可能（GitHub Pages 等）
- サーバーコストゼロ、レイテンシーゼロ

### パターン B: サーバー型（Axum）

```
POST /eval
  { "source": "print(42)" }
→ { "stdout": "42\n", "errors": [] }
```

- 将来的に重い処理（コンパイル、crate 解決）が必要になったとき必要
- 今は WASM の方がシンプル

**推奨**: まず WASM で Playground を作り、`forge build`（Rust トランスパイラ）が完成したタイミングでサーバー型を検討する。

### セキュリティ考察（Azure ブログへの埋め込みを想定）

#### WASM 方式が安全な理由

WASM はブラウザのサンドボックス内で完結するため、ネットワークアクセス・ファイルシステム・DB への経路が物理的に存在しない。
どんなコードを入力されても Azure 側のリソースには届かない。

```
ブラウザ
  └─ ForgeScript WASM モジュール（サンドボックス）
       ├─ ネットワークアクセス: 不可
       ├─ ファイルシステム: 不可
       └─ DB: 物理的に届かない
```

| 観点 | WASM | サーバー型（Axum 等） |
|---|---|---|
| DB 越境リスク | **なし**（ブラウザ完結） | あり（要サンドボックス） |
| インジェクション | **なし** | 要対策（入力検証・タイムアウト） |
| 無限ループ攻撃 | ブラウザタブが固まるだけ | サーバーリソース枯渇リスク |
| コスト | **ゼロ** | 実行コスト発生 |
| レイテンシ | **ゼロ**（ローカル実行） | ネットワーク往復あり |

#### サーバー型が必要になった場合の Azure 構成

`forge build`（Rust トランスパイル）等の重い処理が必要になったタイミングで検討する。
その場合は Azure Functions + VNet でアウトバウンドを遮断する構成が安全。

```
ブログ (Azure Static Web Apps)
  └─ POST /eval → Azure Functions（使い捨てコンテナ）
                    ├─ タイムアウト: 3秒
                    ├─ ネットワーク: アウトバウンド遮断（VNet）
                    └─ DB 接続情報: Functions の環境変数に置かない
```

---

## 4. 言語サーバー（LSP）

Phase 4 で作った型チェッカーが直接活かせる。

```
forge-lsp/ (新クレート)
├── Cargo.toml         ← tower-lsp + tokio
└── src/
    ├── main.rs        ← LSP サーバー起動
    └── backend.rs     ← Backend トレイト実装
```

実装ロードマップ：

| 優先度 | 機能 | 使うもの |
|---|---|---|
| ★★★ | Diagnostics（型エラー表示） | `type_check_source()` |
| ★★☆ | Hover（変数の型表示） | `TypeChecker::lookup()` |
| ★★☆ | Semantic tokens（ハイライト精度向上） | Parser の AST |
| ★☆☆ | Completion（キーワード・メソッド補完） | 静的リスト + スコープ情報 |
| ★☆☆ | Go to definition | スパン情報（既に Span あり） |

---

## 5. インストール・配布

### 前提作業

- `forge-cli/Cargo.toml` の `[[bin]] name` を `"forge-new"` → `"forge"` に変更する
- E2E テスト内の `CARGO_BIN_EXE_forge-new` も合わせて修正する

### 方法A: Vagrant + Ubuntu でのローカル検証（推奨・開発中）

```ruby
# Vagrantfile
Vagrant.configure("2") do |config|
  config.vm.box = "ubuntu/jammy64"
  config.vm.synced_folder ".", "/vagrant"
end
```

```bash
# Ubuntu 内での手順
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
cd /vagrant
cargo build --release
sudo cp target/release/forge /usr/local/bin/
forge run UAT/hello.forge
```

### 方法B: cross でクロスコンパイル（Windows → Linux バイナリ生成）

```bash
cargo install cross
cross build --release --target x86_64-unknown-linux-gnu
# → target/x86_64-unknown-linux-gnu/release/forge
```

Docker が必要。生成したバイナリを Vagrant 共有フォルダ経由で Ubuntu に渡してそのまま実行できる。

### 方法C: GitHub Actions（CI/CD で自動生成）

Linux / macOS / Windows の各バイナリを Releases に自動アップロード。言語仕様が安定したタイミングで整備する。

---

## 6. セルフホスティング

### 概要

「ForgeScript で ForgeScript のコンパイラを書く」こと。C が C コンパイラを持つように、言語が成熟した証として自然に目指すゴール。

### rustc への依存とは別問題

セルフホスティング（コンパイラを何の言語で書くか）と、実行基盤（何の上で動くか）は独立した軸。

| 軸 | 選択肢 |
|---|---|
| コンパイラを書く言語 | Rust → ForgeScript（セルフホスティング） |
| 実行基盤 | rustc / LLVM への依存は**維持してよい** |

Kotlin がセルフホスティングでありながら JVM に依存し続けているように、ForgeScript も rustc に最適化を委任しながらコンパイラ自体を ForgeScript で書ける。「何で作られているか」と「何の上で動くか」は別の話。

### セルフホスティングのメリット

1. **言語仕様のバグが自然に発見される** — コンパイラという大規模実用プログラムを ForgeScript で書くことで、言語の穴・不便な箇所が露呈する
2. **ドッグフーディング** — 作者が最も ForgeScript を使い込む状況になる
3. **最良のサンプルコードになる** — コンパイラのソースが大規模実用プログラムのショーケースになる
4. **言語としての完全性の証明** — 外部から「実用レベルに達した」と認識される

### 現実的な進め方

```
Stage 1: forge build 完成
          ForgeScript → Rust コード → rustc → バイナリ

Stage 2: forge-compiler の中枢（パーサー・型チェッカー）を ForgeScript で書き直す
          コード生成・最適化は引き続き rustc に委任

Stage 3: セルフホスティング完成
          ForgeScript で書いた ForgeScript コンパイラを ForgeScript 自身がコンパイルできる
```

### 優先度

言語仕様が変わり続けている段階では着手しない。`forge build` 完成・仕様安定後に自然に目指すゴール。

---

## 優先順位

```
今すぐ作れる
  │
  ├─ [1] シンタックスハイライト（TextMate grammar）✅
  ├─ [2] forge test + test "..." インラインブロック
  ├─ [3] バイナリ名を forge に変更 + Vagrant 検証
  │
  モジュールシステム実装後
  │
  ├─ [4] 言語サーバー（LSP）
  ├─ [5] Playground（WASM ビルド）
  └─ [6] .test.forge コンパニオンスタイル

  言語仕様安定・forge build 完成後
  │
  └─ [7] セルフホスティング（ForgeScript で ForgeScript コンパイラを書く）
```
