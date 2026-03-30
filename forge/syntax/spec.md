# ForgeScript シンタックスハイライト仕様

> 対象: VS Code 拡張（TextMate grammar）
> ファイル拡張子: `.forge`
> 方式: JSON TextMate grammar（`forge.tmLanguage.json`）

---

## 1. トークン定義

### 1-1. キーワード

| トークン | 種別 | TextMate スコープ |
|---|---|---|
| `let` `state` `const` | バインディング | `keyword.other.binding.forge` |
| `fn` `return` | 関数 | `keyword.other.fn.forge` |
| `if` `else` | 条件分岐 | `keyword.control.conditional.forge` |
| `for` `in` `while` | ループ | `keyword.control.loop.forge` |
| `match` | パターンマッチ | `keyword.control.match.forge` |
| `test` | テストブロック | `keyword.other.test.forge` |

### 1-2. 組み込み識別子

| トークン | 種別 | TextMate スコープ |
|---|---|---|
| `some` `none` `ok` `err` | Option / Result コンストラクタ | `support.function.builtin.forge` |
| `print` `println` `string` `number` `float` `len` `type_of` | 組み込み関数 | `support.function.builtin.forge` |
| `true` `false` | 真偽値リテラル | `constant.language.boolean.forge` |

### 1-3. 型名

| トークン | TextMate スコープ |
|---|---|
| `number` `float` `string` `bool` `list` | `storage.type.forge` |

型注釈のコンテキスト（`: T`、`-> T`）で検出する。

### 1-4. リテラル

| 種別 | パターン例 | TextMate スコープ |
|---|---|---|
| 整数 | `42` `1_000` | `constant.numeric.integer.forge` |
| 浮動小数点 | `3.14` `1.0e10` | `constant.numeric.float.forge` |
| 文字列（通常） | `"hello"` | `string.quoted.double.forge` |
| 文字列補間 | `"Hello, {name}!"` | 下記参照 |

### 1-5. 文字列補間

`"..."` の中の `{expr}` を別スコープで着色する。

```
string.quoted.double.forge
  └─ punctuation.definition.string.begin.forge   "
  ├─ string.quoted.double.forge                  Hello,
  ├─ meta.interpolation.forge
  │    ├─ punctuation.section.interpolation.begin.forge   {
  │    ├─ source.forge.embedded                           name
  │    └─ punctuation.section.interpolation.end.forge     }
  ├─ string.quoted.double.forge                  !
  └─ punctuation.definition.string.end.forge     "
```

### 1-6. 演算子・記号

| 種別 | トークン | TextMate スコープ |
|---|---|---|
| クロージャアロー | `=>` | `keyword.operator.arrow.forge` |
| 戻り値型 | `->` | `keyword.operator.return-type.forge` |
| エラー伝播 | `?` | `keyword.operator.question.forge` |
| 範囲 | `..` `..=` | `keyword.operator.range.forge` |
| 算術 | `+` `-` `*` `/` `%` | `keyword.operator.arithmetic.forge` |
| 比較 | `==` `!=` `<` `>` `<=` `>=` | `keyword.operator.comparison.forge` |
| 論理 | `&&` `\|\|` `!` | `keyword.operator.logical.forge` |
| 代入 | `=` | `keyword.operator.assignment.forge` |
| 型注釈 | `:` | `punctuation.separator.type.forge` |

### 1-7. コメント

```
// これはコメント
```

スコープ: `comment.line.double-slash.forge`

### 1-8. 関数定義

`fn name(...)` の `name` 部分を強調。

スコープ: `entity.name.function.forge`

### 1-9. 変数宣言

`let x = ...` / `state x = ...` の `x` 部分。

スコープ: `variable.other.declaration.forge`

---

## 2. ファイル構成

```
forge-vscode/
├── package.json                ← VS Code 拡張マニフェスト
├── syntaxes/
│   └── forge.tmLanguage.json   ← TextMate grammar 本体
├── language-configuration.json ← 括弧ペア・コメント設定
└── CHANGELOG.md
```

### package.json（抜粋）

```json
{
  "name": "forge-language",
  "contributes": {
    "languages": [{
      "id": "forge",
      "aliases": ["ForgeScript", "forge"],
      "extensions": [".forge"],
      "configuration": "./language-configuration.json"
    }],
    "grammars": [{
      "language": "forge",
      "scopeName": "source.forge",
      "path": "./syntaxes/forge.tmLanguage.json"
    }]
  }
}
```

### language-configuration.json（括弧ペアとコメント）

```json
{
  "comments": {
    "lineComment": "//"
  },
  "brackets": [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"]
  ],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"" }
  ]
}
```

---

## 3. 将来拡張

| 優先度 | 内容 |
|---|---|
| ★★☆ | Tree-sitter grammar（より高精度なハイライト） |
| ★★☆ | Zed エディタ対応 |
| ★☆☆ | GitHub Linguist 登録（`.forge` をリポジトリで自動認識） |
| ★☆☆ | Neovim / Helix 対応（Tree-sitter 経由） |

---

## 4. 公開方針

| フェーズ | 配布方法 |
|---|---|
| 開発中（現在） | 同リポジトリに含め、`.vsix` でローカルインストール |
| 言語仕様安定後 | VS Code Marketplace に公開（`vsce publish`） |

Marketplace 公開には Microsoft publisher アカウントが必要。言語仕様が変わり続ける間は公開しない。
