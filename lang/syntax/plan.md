# ForgeScript シンタックスハイライト 実装計画

---

## フェーズ構成

```
Phase S-1: VS Code 拡張（TextMate grammar）
Phase S-2: Tree-sitter grammar
Phase S-3: Marketplace 公開
```

---

## Phase S-1: VS Code 拡張

### 目標
`.forge` ファイルを VS Code で開いたときに適切な色分けが行われること。

### 成果物
```
forge-vscode/
├── package.json
├── syntaxes/forge.tmLanguage.json
└── language-configuration.json
```

### 実装ステップ

1. `forge-vscode/` ディレクトリ作成、`package.json` 雛形を書く
2. `language-configuration.json` を書く（括弧ペア・コメント）
3. `forge.tmLanguage.json` を書く（下記の順番で追加）
   1. コメント（`//`）
   2. 文字列リテラル（エスケープシーケンス含む）
   3. 文字列補間（`{expr}` の入れ子スコープ）
   4. 数値リテラル（整数・浮動小数点・アンダースコア区切り）
   5. キーワード（`let` / `fn` / `if` / `match` 等）
   6. 組み込み識別子（`some` / `none` / `print` 等）
   7. 型名（型注釈コンテキスト）
   8. 演算子・記号
   9. 関数定義名（`fn NAME`）
4. `.vsix` をビルドして動作確認
5. `cargo test --workspace` が引き続き通ることを確認（既存コードに影響しないため自明）

### インストール方法（開発中）
```bash
cd forge-vscode
npm install -g @vscode/vsce
vsce package          # forge-language-x.x.x.vsix を生成
code --install-extension forge-language-x.x.x.vsix
```

---

## Phase S-2: Tree-sitter grammar（将来）

### 目標
より正確な構文認識（ネスト・エラー耐性）。Neovim / Helix / Zed / GitHub での利用。

### 実装ステップ
1. `tree-sitter-forge/` ディレクトリ作成
2. `grammar.js` に ForgeScript の文法を定義
3. `tree-sitter generate` でパーサを生成
4. VS Code 拡張の grammar を TextMate から Tree-sitter に切り替え
5. Neovim / Helix 用の highlight クエリ（`.scm`）を追加

### 依存ツール
- Node.js + `tree-sitter-cli`

---

## Phase S-3: VS Code Marketplace 公開（将来）

### 前提条件
- ForgeScript 言語仕様が v1.0.0 に到達
- Microsoft publisher アカウント取得済み

### 手順
```bash
vsce publish
```

### 事前チェックリスト
- [ ] `package.json` にアイコン・説明・カテゴリを設定
- [ ] `CHANGELOG.md` を整備
- [ ] README に拡張のインストール手順を追記
- [ ] キーワード・スコープが最新仕様と一致しているか確認
