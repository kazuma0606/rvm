# forge/std/plot + forge/std/ml 構想

> 関連: `packages/notebook/spec.md`（ノートブック可視化統合）

---

## 動機

ForgeScript ノートブック（`.fnb`）で Python の matplotlib / seaborn / sklearn に相当する
データ可視化・次元削減が使えるようにする。

Python でよくあるワークフロー:

```python
from sklearn.decomposition import PCA
from sklearn.manifold import TSNE
import matplotlib.pyplot as plt
import seaborn as sns

# 次元削減
reduced = TSNE(n_components=2).fit_transform(embeddings)

# 可視化
sns.scatterplot(x=reduced[:,0], y=reduced[:,1], hue=labels)
plt.show()
```

これと同等の DX を ForgeScript で実現する。

---

## forge/std/ml — 次元削減・機械学習アルゴリズム

### 依存クレート

| アルゴリズム | Rustクレート | Pythonの対応 |
|---|---|---|
| PCA | `linfa-reduction` | `sklearn.decomposition.PCA` |
| t-SNE | `bhtsne` | `sklearn.manifold.TSNE` |
| K-means | `linfa-clustering` | `sklearn.cluster.KMeans` |
| 正規化 | `linfa-preprocessing` | `sklearn.preprocessing` |

`linfa` は Rust の scikit-learn 相当。API 設計も近い。

### API 設計

```forge
use forge/std/ml.*

// PCA（主成分分析）
let result = pca(data, dims: 2)?
// result: Matrix  shape: [n_samples, 2]

// t-SNE（非線形次元削減）
let result = tsne(data, dims: 2, perplexity: 30.0, max_iter: 1000)?

// UMAP（将来対応）
let result = umap(data, dims: 2, n_neighbors: 15)?

// K-means クラスタリング
let clusters = kmeans(data, k: 5)?
// clusters.labels: list<number>   各サンプルのクラスタ番号
// clusters.centers: Matrix        クラスタ中心

// 正規化
let normalized = normalize(data, method: NormalizeMethod::ZScore)?
let scaled     = min_max_scale(data)?
```

### DataFrame との統合

```forge
use forge/std/ml.*
use forge/std/dataframe.*  // 将来モジュール

let df = DataFrame::from_csv("embeddings.csv")?

let features = df.columns(["x1", "x2", "x3", "x4"])?  // 特徴量行列
let labels   = df.column("label")?

let reduced = tsne(features, dims: 2)?

// reduced と labels を結合して可視化へ
```

---

## forge/std/plot — 可視化

### 方針

**バックエンドは plotly（Rust バインディング）を使用。**

理由:
- 出力が Plotly.js 互換 HTML → ノートブックに `display::plot` で埋め込み可能
- インタラクティブ（ズーム・パン・ホバー）
- PNG エクスポートも可能（静的出力が必要な場合）
- Node.js / npm 不要

静的グラフが必要な場合は `plotters`（PNG/SVG）を補完的に使用。

### 依存クレート

| 用途 | クレート |
|---|---|
| インタラクティブグラフ（メイン） | `plotly`（Rust） |
| 静的グラフ PNG/SVG | `plotters` |

### API 設計

```forge
use forge/std/plot.*

// 散布図
scatter(x: list<float>, y: list<float>)
    .color_by(labels: list<string>)      // ラベルで色分け
    .title("t-SNE visualization")
    .x_label("dim 1")
    .y_label("dim 2")
    .size(800, 600)
    .show()                              // ノートブックに display::plot として出力

// 折れ線グラフ
line(y: list<float>)
    .x(x: list<float>)                  // x 軸（省略時はインデックス）
    .title("Training Loss")
    .show()

// 複数系列
plot()
    .line(train_loss, name: "train")
    .line(val_loss,   name: "val")
    .title("Loss Curve")
    .show()

// ヒストグラム
histogram(data: list<float>)
    .bins(30)
    .title("Distribution")
    .show()

// ヒートマップ（相関行列など）
heatmap(matrix: Matrix)
    .labels(columns)
    .color_scale(ColorScale::Viridis)
    .show()

// バーチャート
bar(labels: list<string>, values: list<float>)
    .title("Category Counts")
    .show()
```

### 典型的なワークフロー（ノートブック）

```forge
use forge/std/ml.*
use forge/std/plot.*

// 1. データ読み込み
let data   = load_csv("embeddings.csv")?   // list<list<float>>
let labels = load_csv("labels.csv")?       // list<string>

// 2. 次元削減
let reduced = tsne(data, dims: 2, perplexity: 30.0)?

// 3. 可視化（ノートブックにインタラクティブ散布図が表示）
scatter(reduced.col(0), reduced.col(1))
    .color_by(labels)
    .title("t-SNE — 埋め込み空間")
    .show()

// 4. 損失の推移
let loss_history = [0.9, 0.7, 0.5, 0.35, 0.28, 0.22, 0.18]
line(loss_history)
    .title("Training Loss")
    .x_label("Epoch")
    .y_label("Loss")
    .show()
```

---

## ノートブックとの統合

### display::plot（新 output タイプ）

既存の `display::html` で Plotly HTML を渡すことも可能だが、
専用の `display::plot` を用意することで VS Code 側が適切にレンダリングできる。

```forge
// 内部的には Plotly.js HTML を生成して display::plot として出力
scatter(x, y).show()
// ↓ 等価
display::plot(scatter(x, y).to_plotly_json())
```

`.fnb.out.json` への格納形式:

```json
{
  "type": "plot",
  "backend": "plotly",
  "spec": { ...Plotly JSON spec... }
}
```

VS Code 拡張側は Plotly.js を WebView に埋め込み、JSON spec を渡してレンダリング。

### forge run でのフォールバック

ノートブック外（`forge run`）では:

```
scatter(x, y).show()
→ "[scatter plot: 100 points, x: [-2.3..3.1], y: [-1.8..2.7]]" と println
```

テキスト要約を出力する。PNG が必要な場合は `.save("output.png")` を使う。

---

## Candle との関係

Candle（HuggingFace の Rust ML フレームワーク）はテンソル演算・モデル推論が専門。
可視化機能は持たない。

```
Candle              → モデルの実行・推論（forge/std/candle として将来追加）
forge/std/ml        → 次元削減・クラスタリング（scikit-learn 相当）
forge/std/plot      → 可視化（matplotlib/plotly 相当）
```

Candle から出力した埋め込みベクトルを `forge/std/ml` で t-SNE し、
`forge/std/plot` で可視化、というパイプラインが自然に繋がる。

```forge
use forge/std/candle.*   // 将来
use forge/std/ml.*
use forge/std/plot.*

let model      = Model::load("model.safetensors")?
let embeddings = model.encode(texts)?         // Tensor → Matrix

let reduced    = tsne(embeddings, dims: 2)?
scatter(reduced.col(0), reduced.col(1))
    .color_by(labels)
    .show()
```

---

## 実装フェーズ案

| フェーズ | 内容 | 依存クレート |
|---|---|---|
| **P-0** | `forge/std/plot` 基本グラフ（scatter / line / bar / histogram） | `plotly` |
| **P-1** | `display::plot` ノートブック統合 + VS Code WebView | `plotly` |
| **P-2** | `forge/std/ml` PCA + t-SNE | `linfa-reduction`, `bhtsne` |
| **P-3** | K-means + 正規化 | `linfa-clustering`, `linfa-preprocessing` |
| **P-4** | heatmap / 相関行列 + `plotters` 静的 PNG 出力 | `plotters`, `plotly` |
| **P-5** | `forge/std/candle` 統合（モデル推論 → 可視化パイプライン） | `candle-core` |
