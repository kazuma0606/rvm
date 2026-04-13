# forge/std/math — 数式解析・記号計算 構想

> 関連: `packages/notebook/spec.md`（ノートブック統合）
> 関連: `lang/std/plot/idea.md`（グラフ描画との連携）
> 関連: `forge/std/tex`（LaTeX 双方向変換、本ファイル末尾）

---

## 動機

Python の `sympy` に相当する記号計算ライブラリが Rust にはまだ存在しない。
これは ForgeScript の大きなチャンスである。

ForgeScript はコンパイラ（パーサー・AST）をすでに持っているため、
数式の構文解析器を自前で実装する基盤が揃っている。

```python
# Python / sympy でできること
from sympy import symbols, diff, integrate, solve, simplify
x = symbols('x')
f = x**2 + 3*x
diff(f, x)        # → 2*x + 3
integrate(f, x)   # → x**3/3 + 3*x**2/2
solve(f, x)       # → [0, -3]
```

これと同等以上の DX を ForgeScript + ノートブックで実現する。

---

## 設計方針

- **数式は文字列ではなく式木（Expr 型）で扱う**
- **ForgeScript の既存パーサー技術を流用**して数式パーサーを実装
- **ノートブックで LaTeX レンダリング**（`display::math`）
- **`forge/std/plot` と連携**して関数グラフを描画
- 数値計算（`eval`）と記号計算（`diff`, `integrate`）の両方をサポート

---

## Expr 型

数式の内部表現。ForgeScript の `data` 型として公開する。

```
Expr =
  | Num(float)               // 定数: 3.14
  | Var(string)              // 変数: x, y, t
  | Add(Expr, Expr)          // 加算: a + b
  | Sub(Expr, Expr)          // 減算: a - b
  | Mul(Expr, Expr)          // 乗算: a * b
  | Div(Expr, Expr)          // 除算: a / b
  | Pow(Expr, Expr)          // 冪乗: a ^ b
  | Neg(Expr)                // 単項マイナス: -a
  | Func(string, list<Expr>) // 関数: sin(x), cos(x), exp(x), ln(x)
```

---

## API 設計

### 数式の生成

```forge
use forge/std/math.*

// 文字列からパース
let f = expr("x^2 + 3*x")
let g = expr("sin(x) * exp(-x)")

// ForgeScript 式から直接構築（将来）
let h = x^2 + 3*x   // parse マクロ的な扱い
```

### 基本演算

```forge
let f = expr("x^2 + 3*x + 2")

// 微分
f.diff("x")           // → Expr: 2*x + 3
f.diff("x", order: 2) // → 2（二階微分）

// 積分（不定積分）
f.integrate("x")      // → x^3/3 + 3*x^2/2 + 2*x

// 定積分
f.integrate("x", from: 0.0, to: 2.0)  // → 16/3 ≈ 5.333...

// 数値評価
f.eval(x: 3.0)        // → 20.0
f.eval(x: -1.0)       // → 0.0

// 方程式を解く（f = 0 の解）
f.solve("x")          // → [-1.0, -2.0]
```

### 式変換

```forge
let f = expr("(x + 1)^2")

f.expand()            // → x^2 + 2*x + 1
f.simplify()          // → (x+1)^2（すでに簡潔）
f.factor()            // → (x+1)*(x+1)

expr("sin(x)^2 + cos(x)^2").simplify()  // → 1

// 代入
f.substitute(x: expr("t + 1"))  // → (t+2)^2
```

### 多変数

```forge
let f = expr("x^2 + x*y + y^2")

// 偏微分
f.diff("x")           // → 2*x + y
f.diff("y")           // → x + 2*y

// 勾配ベクトル
f.gradient(["x", "y"])  // → [2*x + y, x + 2*y]

// 数値評価（多変数）
f.eval(x: 1.0, y: 2.0)  // → 7.0
```

### 極限・テイラー展開

```forge
let f = expr("sin(x) / x")

// 極限
f.limit("x", to: 0.0)   // → 1.0

// テイラー展開（x=0 まわり、n次まで）
expr("sin(x)").taylor("x", around: 0.0, order: 5)
// → x - x^3/6 + x^5/120

expr("exp(x)").taylor("x", around: 0.0, order: 4)
// → 1 + x + x^2/2 + x^3/6 + x^4/24
```

### ベクトル・行列演算

```forge
use forge/std/math/linalg.*

let A = Matrix::from([[1.0, 2.0], [3.0, 4.0]])
let b = Vector::from([5.0, 6.0])

// 基本演算
A.det()                  // → -2.0
A.inv()                  // → [[-2, 1], [1.5, -0.5]]
A.transpose()
A.eigenvalues()          // → [-0.37, 5.37]

// 連立方程式 Ax = b
let x = A.solve(b)?      // → [-4.0, 4.5]

// 特異値分解
let (U, S, V) = A.svd()
```

---

## ノートブックとの統合

### display::math（LaTeX レンダリング）

`.fnb` ノートブック内で数式を美しく表示する。

```forge
use forge/std/math.*

let f = expr("x^2 + 3*x + 2")

display::math(f)
// ノートブック内で LaTeX レンダリング:
// x² + 3x + 2

display::math(f.diff("x"))
// 2x + 3

display::math(f.integrate("x"))
// x³/3 + 3x²/2 + 2x + C
```

VS Code の WebView で KaTeX を使って LaTeX をレンダリングする。

### 数式 + グラフの組み合わせ

```forge
use forge/std/math.*
use forge/std/plot.*

let f  = expr("x^2 - 2*x - 3")
let df = f.diff("x")

// 数式を表示
display::math(f)    // x² - 2x - 3
display::math(df)   // 2x - 3

// 関数とその導関数をグラフに重ねて描画
plot()
    .fn(f,  x: -2.0..4.0, name: "f(x)")
    .fn(df, x: -2.0..4.0, name: "f'(x)")
    .title("f(x) とその導関数")
    .x_label("x")
    .y_label("y")
    .show()

// 根（解）を求めてグラフにマーク
let roots = f.solve("x")
display::text("解: {roots}")   // → 解: [-1.0, 3.0]
```

---

## 実装アプローチ

### パーサー

ForgeScript の既存パーサー（`crates/forge-compiler`）の技術を流用して
数式専用の再帰降下パーサーを実装する。

```
"x^2 + 3*x + 2"
    ↓ Lexer
[Var("x"), Pow, Num(2), Add, Num(3), Mul, Var("x"), Add, Num(2)]
    ↓ Parser（再帰降下・演算子優先順位）
Add(
  Add(Pow(Var("x"), Num(2)), Mul(Num(3), Var("x"))),
  Num(2)
)
```

### 微分（記号微分）

微分のルールを式木の再帰的変換として実装する。

```
diff(Num(c), x)       = Num(0)
diff(Var(x), x)       = Num(1)
diff(Var(y), x)       = Num(0)  // y ≠ x
diff(Add(f, g), x)    = Add(diff(f,x), diff(g,x))
diff(Mul(f, g), x)    = Add(Mul(diff(f,x), g), Mul(f, diff(g,x)))  // 積の微分
diff(Pow(f, Num(n)),x)= Mul(Mul(Num(n), Pow(f, Num(n-1))), diff(f,x))  // 冪の微分
diff(Func("sin",f),x) = Mul(Func("cos",f), diff(f,x))  // 合成関数
diff(Func("cos",f),x) = Mul(Neg(Func("sin",f)), diff(f,x))
diff(Func("exp",f),x) = Mul(Func("exp",f), diff(f,x))
diff(Func("ln",f),x)  = Div(diff(f,x), f)
```

ForgeScript の `match` を使って自然に書ける。

### 積分（記号積分）

微分より複雑。段階的に実装する。

```
Phase M-0: 基本公式
  ∫ x^n dx = x^(n+1)/(n+1)
  ∫ sin(x) dx = -cos(x)
  ∫ cos(x) dx = sin(x)
  ∫ exp(x) dx = exp(x)
  ∫ 1/x dx = ln(x)

Phase M-1: 線形性
  ∫ (f + g) dx = ∫f dx + ∫g dx
  ∫ c*f dx = c * ∫f dx

Phase M-2: 置換積分・部分積分
Phase M-3: 有理関数の積分（部分分数分解）
```

### 簡約（simplify）

式木に対して変換ルールを繰り返し適用する。

```
x + 0 → x
x * 1 → x
x * 0 → 0
x^1   → x
x^0   → 1
-(-x) → x
sin²x + cos²x → 1
```

---

## 依存クレート

| 用途 | クレート | 備考 |
|---|---|---|
| 数値積分（定積分の数値近似） | `gauss-quad` | Gauss-Legendre 数値積分 |
| 方程式の数値解 | `roots` | Newton法・二分法 |
| 行列演算 | `nalgebra` | 線形代数（高性能） |
| 記号計算 | **自作**（forge-math） | sympy相当・Rustに成熟したものがない |

記号計算部分（微分・積分・簡約）は完全自作。
数値計算の補助として既存クレートを組み合わせる。

---

## 実装フェーズ

| フェーズ | 内容 | 達成基準 |
|---|---|---|
| **M-0** | 数式パーサー + Expr 型 | `expr("x^2 + 3*x")` が Expr に変換される |
| **M-1** | 数値評価（`eval`） | `f.eval(x: 2.0)` が正しい値を返す |
| **M-2** | 記号微分（`diff`） | `expr("x^2").diff("x")` → `2*x` |
| **M-3** | 簡約（`simplify` / `expand`） | `expr("(x+1)^2").expand()` → `x^2+2*x+1` |
| **M-4** | 基本積分（`integrate`） | 多項式・三角関数・指数関数の不定積分 |
| **M-5** | 定積分 + 方程式を解く | `f.integrate("x", from, to)` / `f.solve("x")` |
| **M-6** | 多変数・偏微分・勾配 | `f.diff("x")` for multivariate expr |
| **M-7** | ノートブック統合（`display::math`） | LaTeX レンダリング（KaTeX） |
| **M-8** | テイラー展開・極限 | `expr("sin(x)").taylor(order: 5)` |
| **M-9** | 線形代数（`linalg`） | 行列・固有値・SVD・連立方程式 |

**M-2 完成 = 「ForgeScript で x^2 を微分できる」最初のデモ**

---

## ノートブックデモシナリオ

```forge
// 微積分入門ノートブック
use forge/std/math.*
use forge/std/plot.*

// ── 1. 関数の定義 ──
let f = expr("x^3 - 3*x^2 + 2")
display::math(f)          // x³ - 3x² + 2

// ── 2. 微分 ──
let df = f.diff("x")
display::math(df)         // 3x² - 6x

// ── 3. 極値を求める（df = 0 の解）──
let critical = df.solve("x")
display::text("極値の候補: {critical}")  // [0.0, 2.0]

// ── 4. グラフで確認 ──
plot()
    .fn(f,  x: -1.0..3.5, name: "f(x)")
    .fn(df, x: -1.0..3.5, name: "f'(x)")
    .vline(critical[0], style: "dashed")
    .vline(critical[1], style: "dashed")
    .title("f(x) = x³ - 3x² + 2 とその導関数")
    .show()

// ── 5. 定積分 ──
let area = f.integrate("x", from: 0.0, to: 2.0)
display::text("0 から 2 の定積分: {area}")  // → 0.0
display::math(expr("\\int_0^2 f(x)\\,dx = {area}"))
```

---

---

## forge/std/tex — LaTeX 双方向変換

### 動機

論文・技術文書を `.tex` で書いている人が、数式をそのまま ForgeScript で計算・検算できる。
完全な LaTeX パーサーは不要で、**数式環境だけを抽出・変換する**限定的な実装で十分。

### 対象とする LaTeX 数式環境

```latex
% インライン数式
$f(x) = x^2 + 3x$

% ディスプレイ数式
$$\int_0^1 x^2\,dx$$

% equation 環境
\begin{equation}
  f(x) = x^3 - 3x^2 + 2
\end{equation}

% align 環境（複数式）
\begin{align}
  f(x)  &= x^3 - 3x^2 + 2 \\
  f'(x) &= 3x^2 - 6x
\end{align}
```

### LaTeX → Expr 変換ルール

| LaTeX 記法 | Expr 変換 |
|---|---|
| `x^{n}` / `x^n` | `Pow(x, n)` |
| `\frac{a}{b}` | `Div(a, b)` |
| `\sqrt{x}` | `Pow(x, 0.5)` |
| `\sin`, `\cos`, `\tan` | `Func("sin", ...)` 等 |
| `\exp`, `\ln`, `\log` | `Func("exp", ...)` 等 |
| `\cdot`, `\times` | `Mul` |
| `\int_a^b` | `integrate(from: a, to: b)` |
| `\sum_{i=0}^{n}` | `Sum(i, from: 0, to: n)` |
| `\left(`, `\right)` | グループ化（読み飛ばし） |
| `\,`, `\!`, `\quad` | 空白（読み飛ばし） |

### API 設計

```forge
use forge/std/tex.*
use forge/std/math.*

// ── .tex ファイルから数式を抽出して計算 ──

let doc = tex::parse("paper.tex")?

// 番号付き equation 環境を取得
let eq1 = doc.equation(1)        // \begin{equation} ... \end{equation} の1番目
display::math(eq1)               // f(x) = x³ - 3x² + 2

// 右辺だけ取り出す（f(x) = ... の右辺）
let f = eq1.rhs()

// 微分・積分
let df = f.diff("x")
display::math(df)                // 3x² - 6x

let integral = f.integrate("x", from: 0.0, to: 2.0)
display::text("定積分: {integral}")

// 結果を .tex に追記して保存
doc.append_equation(df, label: "deriv")
doc.save("paper_annotated.tex")?

// ── 文字列として LaTeX を直接パース ──
let expr = tex::parse_expr("\\frac{x^2 + 1}{x - 1}")?
display::math(expr.diff("x"))
```

### 逆方向：Expr → LaTeX

```forge
use forge/std/math.*

let f = expr("x^2 + 3*x")

// Expr を LaTeX 文字列に変換
f.to_latex()                   // "x^{2} + 3x"
f.diff("x").to_latex()         // "2x + 3"
f.integrate("x").to_latex()    // "\\frac{x^{3}}{3} + \\frac{3x^{2}}{2} + C"

// display::math は内部で to_latex() を使う
display::math(f.diff("x"))     // ノートブックで LaTeX レンダリング
```

### 実用ワークフロー例

```forge
// 論文の数式を検算するノートブック
use forge/std/tex.*
use forge/std/math.*
use forge/std/plot.*

let doc = tex::parse("thesis.tex")?

// 論文の数式(3)を取り出す
let eq = doc.equation(3)
display::text("論文の数式(3):")
display::math(eq)

// 微分して極値を確認
let f  = eq.rhs()
let df = f.diff("x")
display::text("導関数:")
display::math(df)

let roots = df.solve("x")
display::text("極値の候補: x = {roots}")

// グラフで確認
plot()
    .fn(f,  x: -3.0..3.0, name: "f(x)")
    .fn(df, x: -3.0..3.0, name: "f'(x)")
    .points(roots.map(r => [r, 0.0]), name: "極値")
    .title("論文 式(3) の検証")
    .show()
```

### 実装アプローチ

完全な LaTeX パーサーは不要。数式環境の抽出は正規表現ベースで十分。

```
Step 1: 数式環境の抽出
  - $...$, $$...$$, \begin{equation}...\end{equation} を正規表現で切り出す

Step 2: LaTeX 数式 → Expr 変換
  - 上記の変換ルールテーブルを再帰的に適用
  - `forge-compiler` の Lexer を参考に LaTeX トークナイザーを実装

Step 3: Expr → LaTeX 変換（逆方向）
  - Expr を再帰的にたどって LaTeX 文字列を生成
```

### 依存クレート

| 用途 | クレート |
|---|---|
| 正規表現（数式環境抽出） | `regex`（forge-stdlib に既存） |
| LaTeX パーサー | **自作**（数式環境限定） |

### 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **T-0** | `.tex` から数式環境の抽出（`$...$` / `equation` / `align`） |
| **T-1** | LaTeX 数式 → Expr 変換（基本四則・冪・分数） |
| **T-2** | 三角関数・指数・対数の変換 |
| **T-3** | Expr → LaTeX 逆変換（`to_latex()`） |
| **T-4** | `\int` / `\sum` の変換 |
| **T-5** | `doc.equation(n)` / `doc.append_equation()` / `doc.save()` |

**T-1 完成 = 「.tex の数式を微分できる」最初のデモ**

---

---

## 数学的な正しさ — 本物の math クレートに向けて

ほとんどの数値計算ライブラリが手を抜いている部分。
ForgeScript の型システム（`T!` / typestate）と組み合わせることで、
**数学的な不確かさを型で表現できる唯一のライブラリ**を目指す。

---

### 設計原則

```
原則1: 数学的に不可能なことは「不可能」と返す
原則2: 結果に「有効な条件（定義域・収束条件）」を添付する
原則3: 解析解と数値解を型で区別する
原則4: 特異点・不連続点を自動検出する
原則5: 「答えが出た」と「正しい答えが出た」を区別する
```

---

### 五次方程式と Abel-Ruffini の定理

**アーベル＝ルフィニの定理**: 五次以上の方程式には代数的な一般解の公式が存在しない。

多くのライブラリはこれを無視して数値近似を返すだけだが、
forge/std/math は数学的に正直に設計する。

```forge
// 解析解が存在する場合 → Exact を返す
expr("x^2 - 2").solve("x")
// → SolveResult::Exact(["-√2", "√2"])

// 四次以下で解の公式がある場合
expr("x^3 - 6*x - 9").solve("x")
// → SolveResult::Exact(["3.0", ...])  // カルダノの公式

// 五次以上 → 解析解なし・数値近似を返す
expr("x^5 + x + 1").solve("x")
// → SolveResult::Numerical([-0.7549...], precision: 1e-10)
//   + note: "Abel-Ruffini: 5次以上の方程式に代数的一般解は存在しません"
```

`SolveResult` は `T!` と同様に match で強制的に処理させる：

```forge
match f.solve("x") {
    SolveResult::Exact(roots)          => println("解析解: {roots}"),
    SolveResult::Numerical(roots, eps) => println("数値解（誤差 {eps}）: {roots}"),
    SolveResult::NoRealRoots           => println("実数解なし"),
    SolveResult::InfiniteRoots         => println("恒等式（すべての x が解）"),
}
```

---

### 定義域の型表現

`Expr` は計算結果とともに**定義域**を持つ。

```forge
data Domain {
    All                              // すべての実数 (-∞, +∞)
    GreaterThan(float)               // x > a
    GreaterOrEqual(float)            // x ≥ a
    Interval(float, float)           // a < x < b
    ClosedInterval(float, float)     // a ≤ x ≤ b
    Union(Domain, Domain)            // 和集合
    Except(Domain, list<float>)      // 特異点を除外
}

data Expr {
    tree:   ExprTree
    domain: Domain                   // 有効な定義域
}
```

関数の定義域は自動付与される：

```forge
expr("ln(x)").domain            // → Domain::GreaterThan(0.0)
expr("sqrt(x)").domain          // → Domain::GreaterOrEqual(0.0)
expr("1/x").domain              // → Domain::Except(All, [0.0])
expr("tan(x)").domain           // → Domain::Except(All, [π/2, 3π/2, ...])
expr("x^2").domain              // → Domain::All

// 合成関数の定義域は自動的に交差を取る
expr("ln(x^2 - 1)").domain      // → Domain::Union(LessThan(-1.0), GreaterThan(1.0))
//  (x^2 - 1 > 0 → x < -1 または x > 1)
```

定義域外で eval しようとするとエラー：

```forge
let f = expr("ln(x)")
f.eval(x: -1.0)
// → Err("DomainError: x = -1.0 は定義域 x > 0 の外です")

f.eval(x: 2.0)
// → Ok(0.693...)
```

---

### 広義積分の収束判定

```forge
// 発散する積分
expr("1/x").integrate("x", from: -1.0, to: 1.0)
// → IntegrateResult::Diverges
//   reason: "x = 0.0 に非可積分特異点があります"

expr("1/x").integrate("x", from: 1.0, to: Inf)
// → IntegrateResult::Diverges
//   reason: "上限が ∞ で被積分関数が 1/x オーダー（収束には 1/x^p, p>1 が必要）"

// 収束する積分
expr("1/x^2").integrate("x", from: 1.0, to: Inf)
// → IntegrateResult::Converges(1.0)

expr("exp(-x^2)").integrate("x", from: Neg::Inf, to: Inf)
// → IntegrateResult::Converges(√π)  // ガウス積分

// 数値積分（解析解がない場合）
expr("sin(x)/x").integrate("x", from: 0.0, to: 10.0)
// → IntegrateResult::Numerical(1.6583..., error: 1e-8)
```

---

### 特異点の自動検出

```forge
let f = expr("(x^2 - 1) / (x - 1)")

f.singularities()
// → [Singularity { at: 1.0, kind: SingularityKind::Removable }]
//   ※ x→1 の極限は 2（除去可能特異点）

f.simplify()
// → expr("x + 1")  // 除去可能特異点を simplify で除去
//   domain: Except(All, [1.0])  // 定義域は保持

let g = expr("1 / (x - 1)^2")
g.singularities()
// → [Singularity { at: 1.0, kind: SingularityKind::Pole(order: 2) }]
```

---

### 代数構造（群・環・体）

抽象代数の基本構造をサポートする。

```forge
use forge/std/math/algebra.*

// ── 群（Group）──

// 巡回群 Z/nZ
let Z6 = CyclicGroup::new(6)
Z6.order()                      // → 6
Z6.is_abelian()                 // → true
Z6.subgroups()                  // → [Z1, Z2, Z3, Z6]
Z6.generators()                 // → [1, 5]（生成元）

// 対称群 S_n（n次置換群）
let S3 = SymmetricGroup::new(3)
S3.order()                      // → 6
S3.is_abelian()                 // → false（n ≥ 3 で非可換）
S3.cayley_table()               // → 乗積表（6×6）

// 任意の群を定義
let G = Group::from_cayley_table([
    [0, 1, 2],
    [1, 2, 0],
    [2, 0, 1],
])
G.is_cyclic()                   // → true
G.is_isomorphic(Z6.subgroup(3)) // → true

// ── 環・体（Ring / Field）──

// 有限体 GF(p)
let GF7 = GaloisField::new(7)
GF7.add(3, 5)                   // → 1  (mod 7)
GF7.mul(3, 5)                   // → 1  (3*5=15≡1)
GF7.inverse(3)                  // → 5
GF7.is_prime_field()            // → true

// 多項式環 Z[x]
let Zx = PolynomialRing::integer()
let p  = Zx.poly([1, 0, -1])   // x^2 - 1
let q  = Zx.poly([1, -1])      // x - 1
Zx.gcd(p, q)                    // → x - 1
Zx.factor(p)                    // → (x-1)(x+1)
```

---

### フェーズ更新（数学的正しさ対応）

| フェーズ | 内容 |
|---|---|
| M-0 | 数式パーサー + Expr 型 |
| M-1 | 数値評価（`eval`） |
| M-2 | 記号微分（`diff`） |
| M-3 | 簡約（`simplify` / `expand`） |
| M-4 | 基本積分（不定積分） |
| M-5 | 定積分 + `SolveResult` 型（解析解 / 数値解 / 解なし） |
| M-6 | **定義域の自動付与**（`Domain` 型・`DomainError`） |
| M-7 | **広義積分の収束判定**（`IntegrateResult::Diverges / Converges`） |
| M-8 | **特異点の自動検出**（除去可能・極・真性） |
| M-9 | テイラー展開・極限 |
| M-10 | 多変数・偏微分・勾配 |
| M-11 | ノートブック統合（`display::math`・KaTeX） |
| M-12 | **代数構造**（群・環・体） |
| M-13 | 線形代数（`linalg`・行列・固有値・SVD） |

---

## 参考

- [SymPy](https://www.sympy.org/) — Python 記号計算の標準
- [nalgebra](https://nalgebra.org/) — Rust 線形代数
- [KaTeX](https://katex.org/) — 高速 LaTeX レンダリング（ノートブック WebView 用）
- [roots](https://docs.rs/roots/) — Rust 方程式の数値解
- [latex2sympy](https://github.com/augustt198/latex2sympy) — LaTeX → sympy 変換の参考実装
- [Abstract Algebra（Dummit & Foote）](https://www.wiley.com/en-us/Abstract+Algebra%2C+3rd+Edition-p-9780471433347) — 群・環・体の参考文献
