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

## 優先順位

```
今すぐ作れる
  │
  ├─ [1] シンタックスハイライト（TextMate grammar）
  ├─ [2] forge test + test "..." インラインブロック
  │
  モジュールシステム実装後
  │
  ├─ [3] 言語サーバー（LSP）
  ├─ [4] Playground（WASM ビルド）
  └─ [5] .test.forge コンパニオンスタイル
```
