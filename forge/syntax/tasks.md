# ForgeScript シンタックスハイライト タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `~/.vscode/extensions/forge-language/` に配置してローカルでハイライトが動くこと
> **方針**: Node.js・vsce・Marketplace 不要。2ファイルを置くだけで完結させる

---

## S-1-A: ファイル雛形

- [ ] `forge-vscode/` ディレクトリ作成
- [ ] `forge-vscode/package.json` 作成
  - [ ] 言語 ID `forge`、拡張子 `.forge` の登録
  - [ ] grammar パス・scopeName の設定
- [ ] `forge-vscode/language-configuration.json` 作成
  - [ ] 括弧ペア `{}` `[]` `()`
  - [ ] 行コメント `//`
  - [ ] auto-closing pairs（`"`, `{`, `[`, `(`）

## S-1-B: TextMate grammar 本体

- [ ] `forge-vscode/syntaxes/forge.tmLanguage.json` 作成
- [ ] コメント（`//`）
- [ ] 文字列リテラル（エスケープ `\n` `\t` `\"` `\\` 含む）
- [ ] 文字列補間（`{expr}` を別スコープで着色）
- [ ] 数値リテラル（整数 `42` / アンダースコア区切り `1_000` / 浮動小数点 `3.14`）
- [ ] 真偽値（`true` / `false`）
- [ ] バインディングキーワード（`let` / `state` / `const`）
- [ ] 関数キーワード（`fn` / `return`）
- [ ] 制御フローキーワード（`if` / `else` / `for` / `in` / `while` / `match`）
- [ ] Option / Result コンストラクタ（`some` / `none` / `ok` / `err`）
- [ ] 組み込み関数（`print` / `println` / `string` / `number` / `float` / `len` / `type_of`）
- [ ] 型名（`number` / `float` / `string` / `bool` / `list`）
- [ ] 演算子（`=>` / `->` / `?` / `..` / `..=` / 算術 / 比較 / 論理）
- [ ] 関数定義名（`fn NAME(...)` の `NAME` を強調）

## S-1-C: ローカル配置・動作確認

- [ ] `forge-vscode/` を `~/.vscode/extensions/forge-language/` にコピー（またはシンボリックリンク）
- [ ] VS Code を再起動
- [ ] `fixtures/hello.forge` を開いて目視確認
  - [ ] キーワードが着色されている
  - [ ] 文字列補間 `{name}` が別色になっている
  - [ ] `//` コメントが着色されている
  - [ ] 型注釈（`: number`、`-> string`）が着色されている

---

## オプション（仕様安定後）

- [ ] **S-2**: Tree-sitter grammar（Neovim / Helix / Zed 対応）
- [ ] **S-3**: `.vsix` パッケージング（`vsce`）
- [ ] **S-4**: VS Code Marketplace 公開
- [ ] **S-5**: GitHub Linguist 登録（`.forge` をリポジトリで自動認識）
- [ ] **S-6**: `test` キーワードのハイライト追加（ForgeScript テスト構文が固まり次第）
