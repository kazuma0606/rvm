# ForgeScript シンタックスハイライト タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `~/.vscode/extensions/forge-language/` に配置してローカルでハイライトが動くこと
> **方針**: Node.js・vsce・Marketplace 不要。2ファイルを置くだけで完結させる

---

## S-1-A: ファイル雛形

- [x] `forge-vscode/` ディレクトリ作成
- [x] `forge-vscode/package.json` 作成
  - [x] 言語 ID `forge`、拡張子 `.forge` の登録
  - [x] grammar パス・scopeName の設定
- [x] `forge-vscode/language-configuration.json` 作成
  - [x] 括弧ペア `{}` `[]` `()`
  - [x] 行コメント `//`
  - [x] auto-closing pairs（`"`, `{`, `[`, `(`）

## S-1-B: TextMate grammar 本体

- [x] `forge-vscode/syntaxes/forge.tmLanguage.json` 作成
- [x] コメント（`//`）
- [x] 文字列リテラル（エスケープ `\n` `\t` `\"` `\\` 含む）
- [x] 文字列補間（`{expr}` を別スコープで着色）
- [x] 数値リテラル（整数 `42` / アンダースコア区切り `1_000` / 浮動小数点 `3.14`）
- [x] 真偽値（`true` / `false`）
- [x] バインディングキーワード（`let` / `state` / `const`）
- [x] 関数キーワード（`fn` / `return`）
- [x] 制御フローキーワード（`if` / `else` / `for` / `in` / `while` / `match`）
- [x] Option / Result コンストラクタ（`some` / `none` / `ok` / `err`）
- [x] 組み込み関数（`print` / `println` / `string` / `number` / `float` / `len` / `type_of`）
- [x] 型名（`number` / `float` / `string` / `bool` / `list`）
- [x] 演算子（`=>` / `->` / `?` / `..` / `..=` / 算術 / 比較 / 論理）
- [x] 関数定義名（`fn NAME(...)` の `NAME` を強調）

## S-1-C: ローカル配置・動作確認

- [x] `forge-vscode/` を `~/.vscode/extensions/forge-language/` にコピー（またはシンボリックリンク）
- [x] VS Code を再起動
- [x] `fixtures/hello.forge` を開いて目視確認
  - [x] キーワードが着色されている
  - [x] 文字列補間 `{name}` が別色になっている
  - [x] `//` コメントが着色されている
  - [x] 型注釈（`: number`、`-> string`）が着色されている

---

## オプション（仕様安定後）

- [ ] **S-2**: Tree-sitter grammar（Neovim / Helix / Zed 対応）
- [ ] **S-3**: `.vsix` パッケージング（`vsce`）
- [ ] **S-4**: VS Code Marketplace 公開
- [ ] **S-5**: GitHub Linguist 登録（`.forge` をリポジトリで自動認識）
- [ ] **S-6**: `test` キーワードのハイライト追加（ForgeScript テスト構文が固まり次第）
