# ForgeScript トランスパイラ最適化アイデア

> 作成: 2026-04-06
> 「手書きの一般的な Rust コードより速くなりうるパターン」をトランスパイラに自動適用する。
> 「最高の Rust コード」には勝てないが、人間が書き忘れる最適化を確実に適用できることが強み。

---

## 背景

ForgeScript のトランスパイラはプログラム全体を俯瞰できる。局所的にしか見えない人間が書き忘れる
最適化を、生成コードに確実に埋め込める。`time_ms()` が実装されれば `forge run` vs `forge build`
vs 手書き Rust を定量的に比較できるようになる。

---

## 最適化パターン一覧

### 1. イテレータ融合（最優先・効果大）

`.filter().map().fold()` チェーンを中間 `Vec` なしの単一パス処理に変換する。

**問題となる Rust コード（人間が書きがち）**
```rust
// 中間 Vec を 2 回作る
let filtered: Vec<_> = list.iter().filter(|x| ...).collect();
let mapped:   Vec<_> = filtered.iter().map(|x| ...).collect();
let result            = mapped.iter().fold(0, |acc, x| acc + x);
```

**トランスパイラが出力すべきコード**
```rust
// 単一パス・中間アロケーションなし
let result = list.iter()
    .filter(|x| ...)
    .map(|x| ...)
    .fold(0, |acc, x| acc + x);
```

ForgeScript はチェーンの末端が `fold` / `sum` / `count` / `each`（終端操作）か
`map` / `filter`（中間操作）かをコンパイル時に判断できるため、常に正しい形を出力できる。

**優先度**: ◎（今すぐ実装可能・効果が最も大きい）

---

### 2. `Vec::with_capacity` の自動挿入

`map` は要素数が変わらないことが保証されているので、自動的に容量を確保できる。

**問題となるコード**
```rust
// リアロケーションが発生しうる
let result: Vec<_> = list.iter().map(|x| x * 2).collect();
```

**トランスパイラが出力すべきコード**
```rust
let mut result = Vec::with_capacity(list.len());
result.extend(list.iter().map(|x| x * 2));
```

`filter` は要素数が減るため適用不可。`map` のみ確実に適用できる。

**優先度**: ◎（今すぐ実装可能）

---

### 3. クロージャの静的展開（動的ディスパッチの回避）

ForgeScript のクロージャはすべてコンパイル時に型が決まるため、`Box<dyn Fn>` を使わず
常に `impl Fn`（モノモーフィズム）として展開できる。

**問題となるコード（慎重な Rust プログラマが書きがち）**
```rust
fn apply(f: Box<dyn Fn(i64) -> i64>, x: i64) -> i64 { f(x) }
```

**トランスパイラが出力すべきコード**
```rust
// インライン展開・仮想関数呼び出しなし
fn apply<F: Fn(i64) -> i64>(f: F, x: i64) -> i64 { f(x) }
```

仮想関数呼び出しのオーバーヘッドがゼロになる。コレクション API（map/filter/fold 等）の
すべてのクロージャ引数に適用できる。

**優先度**: ◎（今すぐ実装可能）

---

### 4. 文字列補間の容量事前確保

```forge
"{first_name} {last_name} ({age})"
```

各変数の長さを合算して `String::with_capacity` を使う。`format!()` マクロより速い場面がある。

**トランスパイラが出力すべきコード**
```rust
let mut s = String::with_capacity(
    first_name.len() + 1 + last_name.len() + 2 + age_str.len() + 1
);
s.push_str(&first_name);
s.push(' ');
s.push_str(&last_name);
// ...
```

**優先度**: ○（効果は中程度・実装コストやや高）

---

### 5. 小さい struct への `Copy` 自動付与

フィールドがすべて `Copy` な数値型で、フィールド数が少ない struct に対して
自動で `#[derive(Copy, Clone)]` を付与する。クローンコストが消える。

```forge
struct Point { x: number, y: number }
// → #[derive(Copy, Clone, ...)] を自動付与
```

閾値案: フィールド数 ≤ 4 かつ全フィールドが `number` / `float` / `bool` の場合。

**優先度**: ○（解析ロジックが必要だが効果は確実）

---

### 6. `group_by` の容量ヒント付き HashMap

`group_by` の内部実装で `HashMap::with_capacity` を使う。
キー数の上限を `distinct` 後の長さや `len()` から推定できる場合に適用。

**優先度**: △（`group_by` 実装時に組み込めばよい）

---

## 優先度まとめ

| 最適化 | 効果 | 実装コスト | 優先度 |
|---|---|---|---|
| イテレータ融合 | 大（中間 Vec 削減） | 低 | ◎ 最優先 |
| `Vec::with_capacity` 自動挿入 | 中（再アロケーション削減） | 低 | ◎ |
| クロージャ静的展開 | 中（仮想呼び出し削減） | 低 | ◎ |
| 文字列補間の事前確保 | 小〜中 | 中 | ○ |
| 小 struct への Copy 自動付与 | 小〜中 | 中 | ○ |
| `group_by` 容量ヒント | 小 | 低 | △ |

---

## 計測方針

`time_ms()` / `time_ns()` 組み込み関数（`extend_idea.md` §8 参照）が実装されたら、
以下の3パターンを比較する：

1. `forge run`（インタープリタ）
2. `forge build`（トランスパイル後バイナリ）
3. 手書き Rust（最適化なし）
4. 手書き Rust（`with_capacity` / イテレータ融合など手動適用）

3 vs 2 の差がトランスパイラ最適化の効果を示す。
