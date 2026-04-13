# forge/std/math — 数式解析・記号計算 構想

> 関連: `packages/notebook/spec.md`（ノートブック統合）
> 関連: `lang/std/plot/idea.md`（グラフ描画との連携）

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

## 参考

- [SymPy](https://www.sympy.org/) — Python 記号計算の標準
- [nalgebra](https://nalgebra.org/) — Rust 線形代数
- [KaTeX](https://katex.org/) — 高速 LaTeX レンダリング（ノートブック WebView 用）
- [roots](https://docs.rs/roots/) — Rust 方程式の数値解
