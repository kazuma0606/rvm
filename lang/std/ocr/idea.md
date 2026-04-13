# forge/std/ocr — 数式 OCR・論文検証 構想

> 関連: `lang/std/math/idea.md`（記号計算・検証）
> 関連: `lang/std/math/idea.md#forge/std/tex`（LaTeX 双方向変換）
> 関連: `packages/notebook/spec.md`（ノートブック統合）

---

## 動機

数学・科学論文の数式を手動入力せずに ForgeScript で検証できる環境を作る。

**現状の問題:**
- 論文の数式が正しいか確認するには手計算か手動入力が必要
- 科学界の「再現性の危機（Replication Crisis）」の一因に数式ミスがある
- Mathematica / sympy でも PDF から直接数式を取り込む標準的な方法がない

**目標:**
```
PDF をドロップするだけで数式の整合性チェックができる
```

---

## パイプライン全体像

```
論文（PDF / 画像 / スキャン）
    ↓  forge/std/ocr
LaTeX 数式文字列
    ↓  forge/std/tex
Expr 型（forge/std/math）
    ↓  diff / integrate / solve / equals
検証結果
    ↓  display::math + display::plot
ノートブック（.fnb）で視覚的に確認
```

すべてが ForgeScript 内で完結する。

---

## バックエンド設計：プラグイン方式

数式認識のバックエンドは用途・環境に応じて切り替えられる設計にする。
**ローカル完結を優先**し、クラウド API は任意で使用できる。

```forge
use forge/std/ocr.*

// バックエンドを明示指定
let eq = ocr::from_image("eq.png", backend: OcrBackend::Local)?
let eq = ocr::from_image("eq.png", backend: OcrBackend::ClaudeApi)?
let eq = ocr::from_image("eq.png", backend: OcrBackend::OpenAiApi)?

// 省略時は forge.toml の設定を使用
let eq = ocr::from_image("eq.png")?
```

`forge.toml` での設定：

```toml
[ocr]
backend = "local"         # local / claude / openai / codex
# model = "claude-opus-4" # バックエンドが claude / openai の場合
```

---

## バックエンド一覧

| バックエンド | 種別 | 精度 | コスト | オフライン |
|---|---|---|---|---|
| `Local` | pix2tex（ローカルモデル） | ★★☆ | 無料 | **◎** |
| `ClaudeApi` | Claude API（multimodal） | ★★★ | 有料 | × |
| `OpenAiApi` | GPT-4V | ★★★ | 有料 | × |
| `Codex` | Claude Code / Codex（ローカル起動中） | ★★★ | 無料※ | **◎** |
| `MathPix` | MathPix API | ★★★ | 有料 | × |

※ Claude Code / Codex がローカルで起動していれば API コストなしで使用可能

---

## ローカルバックエンド詳細

### pix2tex（デフォルトローカル）

オープンソースの数式認識モデル。画像 → LaTeX に変換。

```
初回: ~/.forge/tools/pix2tex モデルを自動ダウンロード（~200MB）
以降: ローカルで完全オフライン動作
```

```forge
// ローカルモデルで処理（ネットワーク不要）
let eq = ocr::from_image("equation.png", backend: OcrBackend::Local)?
```

### Claude Code / Codex ローカル連携

`forge dev` 起動中、または Claude Code が動いている環境では、
**MCP 経由でローカルの LLM に投げる**ことができる。

```forge
// MCP サーバー経由でローカルの Claude Code に投げる
let eq = ocr::from_image("eq.png", backend: OcrBackend::Codex)?
// → forge/mcp のエンドポイントを通じてリクエスト
// → クラウド API コスト不要・高精度
```

内部的には `forge-mcp` の既存インフラを再利用する：

```
forge/std/ocr
    ↓ MCP リクエスト（画像 + "LaTeX に変換してください"）
forge-mcp サーバー
    ↓
Claude Code / Codex（ローカル起動中）
    ↓ LaTeX 文字列
forge/std/tex → Expr 型
```

---

## API 設計

### 画像・PDF からの数式抽出

```forge
use forge/std/ocr.*
use forge/std/tex.*
use forge/std/math.*

// 単一画像から数式を認識
let latex = ocr::from_image("equation.png")?
// → "\\frac{d}{dx}x^2 = 2x"

let eq = tex::parse_expr(latex)?
display::math(eq)

// PDF から全数式を抽出
let doc = ocr::from_pdf("paper.pdf")?
doc.equation_count()          // → 23
doc.equation(1)               // → Expr
doc.equations()               // → list<Equation>

// ページ指定
let page3 = ocr::from_pdf("paper.pdf", pages: 3..5)?

// スキャン画像（複数ページ）
let scanned = ocr::from_images(["p1.png", "p2.png", "p3.png"])?
```

### 数式の検証

```forge
use forge/std/ocr.*
use forge/std/tex.*
use forge/std/math.*

let doc = ocr::from_pdf("paper.pdf")?

// ── 両辺の等価検証 ──
let eq3 = doc.equation(3)
match eq3.lhs().equals(eq3.rhs()) {
    true  => display::text("式(3): 恒等式として検証 OK ✓"),
    false => {
        display::text("式(3): 両辺が一致しません")
        display::math(eq3.lhs().simplify())
        display::text("≠")
        display::math(eq3.rhs().simplify())
    }
}

// ── 微分の検証（論文の主張が正しいか）──
let f   = doc.equation(3).rhs()   // 元の関数
let df  = doc.equation(4).rhs()   // 論文が主張する導関数

let df_actual = f.diff("x")

match df.equals(df_actual) {
    true  => display::text("式(4)（微分）: 検証 OK ✓"),
    false => {
        display::text("式(4): 要確認")
        display::text("論文の主張:")
        display::math(df)
        display::text("実際の微分:")
        display::math(df_actual)
    }
}

// ── 積分の検証 ──
let integral_claimed  = doc.equation(7).rhs()
let integral_computed = doc.equation(6).rhs()
                           .integrate("x")
                           .simplify()

match integral_claimed.equals(integral_computed) {
    true  => display::text("式(7)（積分）: 検証 OK ✓"),
    false => display::text("式(7): 不一致の可能性があります"),
}
```

### 論文全体の一括検証

```forge
// 論文内の全数式を自動検証するレポートを生成
let report = ocr::verify_paper("paper.pdf")?

display::table(report.equations.map(eq => {
    "number":   eq.number,
    "status":   eq.status,    // OK / Warning / Error
    "note":     eq.note,
}))

// 要確認箇所だけ表示
for issue in report.issues() {
    display::text("--- 式({issue.number}) ---")
    display::math(issue.equation)
    display::text("問題: {issue.message}")
}
```

---

## ノートブックデモシナリオ

```forge
// 論文検証ノートブック
use forge/std/ocr.*
use forge/std/tex.*
use forge/std/math.*
use forge/std/plot.*

// 1. PDF を読み込む
let doc = ocr::from_pdf("arxiv_2401_12345.pdf")?
display::text("数式数: {doc.equation_count()}")

// 2. 数式一覧を表示
for eq in doc.equations() {
    display::text("式({eq.number}):")
    display::math(eq)
}

// 3. 主要な定理の数式を取り出して検証
let theorem = doc.equation(12)
display::text("定理 2.1 の数式:")
display::math(theorem)

// 4. 導関数を計算して比較
let f  = theorem.rhs()
let df = f.diff("x")
display::text("微分:")
display::math(df)

// 5. グラフで視覚確認
plot()
    .fn(f,  x: -3.0..3.0, name: "f(x)")
    .fn(df, x: -3.0..3.0, name: "f'(x)")
    .title("定理 2.1 の検証")
    .show()
```

---

## 精度向上の工夫

### アンサンブル認識

複数バックエンドの結果を比較して信頼度を上げる：

```forge
let eq = ocr::from_image("eq.png",
    backend: OcrBackend::Ensemble([
        OcrBackend::Local,
        OcrBackend::Codex,
    ])
)?
// → 両者が一致すれば高信頼・不一致なら警告
```

### 文脈補完

前後の数式の文脈から認識結果を補正する：

```forge
// doc 全体の文脈を使って認識精度を向上
let doc = ocr::from_pdf("paper.pdf", context_aware: true)?
// → 「この論文では x は実数、n は正整数」という文脈を保持して認識
```

---

## 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **O-0** | `ocr::from_image`（pix2tex ローカルバックエンド） |
| **O-1** | `forge/std/tex` との統合（LaTeX → Expr 変換） |
| **O-2** | Claude API / OpenAI API バックエンド |
| **O-3** | MCP 経由 Codex / Claude Code ローカル連携 |
| **O-4** | `ocr::from_pdf`（PDF → 数式リスト） |
| **O-5** | `Equation::equals` / `verify_paper` 自動検証 |
| **O-6** | アンサンブル認識・文脈補完 |

**O-1 完成 = 「画像の数式を微分できる」最初のデモ**

---

## 意義

### 再現性の危機への貢献

科学論文の数式ミスは意外と多く、査読でも見逃されることがある。
`forge/std/ocr` + `forge/std/math` の組み合わせは：

- **査読者**: PDF を投げるだけで数式の整合性チェック
- **研究者**: 自分の論文の数式を提出前に自動検証
- **学生**: 教科書の数式を実際に計算して理解を深める

### 未解決問題への入口

数値計算 + 記号計算 + 可視化が揃うことで：

```
既存論文の数式を取り込む
    ↓ 数値的に探索
    ↓ パターンを発見
    ↓ 仮説を立てる
    ↓ forge/std/math で検証
```

「きっかけで未解決問題が進む」ための道具として機能できる。

---

## 参考

- [pix2tex](https://github.com/lukas-blecher/LaTeX-OCR) — オープンソース数式 OCR
- [MathPix](https://mathpix.com/) — 高精度数式認識 API
- [latex2sympy](https://github.com/augustt198/latex2sympy) — LaTeX → 記号計算変換
- [Replication Crisis](https://en.wikipedia.org/wiki/Replication_crisis) — 科学の再現性問題
