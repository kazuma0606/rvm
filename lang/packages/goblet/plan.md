# `forge-goblet` 実装計画

> 仕様: `lang/packages/goblet/spec.md`
> 前提: `forge-compiler` (parser / AST) が利用可能であること

---

## フェーズ構成

```
Phase G-0:   土台    — クレート作成・中間表現定義・renderer
Phase G-1:   正規化  — |>/methodchain 正規化・pipeline{}抽出・span対応
Phase G-2:   型      — builtin sig table・前向き型伝播 (Goblet独自実装)
Phase G-E2E: 検証    — 既存examples回帰 + examples/goblet 説明用サンプル
Phase G-3:   CLI     — forge goblet サブコマンド統合
Phase G-4:   高度化  — closure詳細解析・匿名struct・module跨ぎ
Phase G-5:   トレース — 実行時値観測 (将来フェーズ)
```

G-0 → G-1 → G-2 → G-E2E → G-3 の順に実施する。
**G-E2E は G-2 完成後、CLI 統合前に必ず実施する。**
ForgeScript 側の実装漏れは G-E2E で発見し、G-3 に進む前に修正する。
G-4 は G-3 完成後。G-5 は別途。

---

## Phase G-0: 土台

### 目標

`crates/forge-goblet` クレートを作成し、Goblet の中間表現と出力形式を定義する。
この段階では型情報は空 (`None`) のままでよい。

### 実装ステップ

1. **クレート作成**
   - `crates/forge-goblet/Cargo.toml` を新規作成
   - `forge-compiler` を dependency に追加（AST 参照用）
   - `serde`, `serde_json` を追加（JSON 出力用）

2. **`graph.rs` — 中間表現定義**
   - `PipelineGraph` 構造体
   - `PipelineNode` 構造体（id / label / kind / span / input_type / output_type / status / notes）
   - `PipelineEdge` 構造体
   - `NodeKind` enum（Source / MethodCall / FunctionCall / Closure / Filter / Map / Fold / Find / OptionOp / ResultOp / Unknown）
     - 注: `|>` は parser 済み AST 上で `Expr::MethodCall` に変換されるため `PipeCall` は廃止。
       由来情報（pipe_syntax / method_syntax）は `notes` フィールドで保持する（G-4 以降）
   - `TypeSummary` 構造体（display / nullable / fallible）
   - `NodeStatus` enum（Ok / Warning / Error / Unknown）
   - `Diagnostic` 構造体（node_id / code / message / span / expected / actual）

3. **`render/text.rs`** — テキスト出力
   - `render_text(graph: &PipelineGraph) -> String`
   - `[N] label    input: T   output: U` 形式

4. **`render/json.rs`** — JSON 出力
   - `render_json(graph: &PipelineGraph) -> String`
   - `serde` derive で実装

5. **`render/mermaid.rs`** — Mermaid 出力
   - `render_mermaid(graph: &PipelineGraph) -> String`
   - ノード色分け: Ok=緑 / Warning=黄 / Error=赤
   - `:::error` スタイル付与
   - **ラベルサニタイズ必須:** ForgeScript の型表記は Mermaid の予約記法と衝突する
     - `<`, `>` → `&lt;`, `&gt;`（`list<T>` 対策）
     - `{`, `}` → `&#123;`, `&#125;`（匿名 struct 対策）
     - `|` → `&#124;`（Union 型対策）
     - `"` → `#quot;`
     - すべてのノードラベルを `["..."]` 形式（ダブルクォート囲み）で出力
     - ノード ID は `N` + 連番のみ（数字始まり禁止）
   - エッジラベルにデータ形状を付与: `N1 -->|"list&lt;Student&gt;"| N2`

6. **`lib.rs`** — 公開 API
   - `analyze_source(src: &str) -> Result<Vec<PipelineGraph>, GobletError>`
     - 注: 1ファイルに複数パイプラインが存在するため単一グラフではなく Vec を返す
   - `OutputFormat` enum（Text / Json / Mermaid）
   - `save_pipeline(graph: &PipelineGraph, path: &Path, format: OutputFormat) -> Result<(), GobletError>`
     - Mermaid の場合はコードブロック付き `.md` として保存
   - `render_mermaid` / `render_json` / `render_text` を lib から再エクスポート

### テスト方針

- 空グラフのレンダリングが各形式でパニックしない
- ノード1個のシンプルなグラフが正しくレンダリングされる

---

## Phase G-1: 正規化

### 目標

ForgeScript ソースを parse し、`|>` 記法と通常の method chain を
共通の pipeline node 列として抽出・正規化できること。

**注:** 現 parser では `|>` は専用 AST ノードではなく `Expr::MethodCall` 連鎖へ正規化される
（`parser/mod.rs:1652`）。一方 `pipeline { ... }` 構文は `Expr::Pipeline { steps, span }` として
別に存在する（`ast/mod.rs:434`）。Goblet はこの両者を区別して扱う。

```
// どちらも同じ PipelineGraph ノード列へ正規化される
students |> filter(...) |> map(...)       // → Expr::MethodCall 連鎖
students.filter(...).map(...)             // → Expr::MethodCall 連鎖

// 別系統として抽出する
pipeline { source students; filter ...; map ... }  // → Expr::Pipeline { steps }
```

### 実装ステップ

1. **`extractor.rs` — パイプライン正規化**
   - `extract_pipelines(stmts: &[Stmt]) -> Vec<PipelineGraph>`
   - `|>` 由来の `Expr::MethodCall` 連鎖を pipeline node 列へ正規化
   - 通常の `Expr::MethodCall` 連鎖も同じ内部表現へ正規化
   - `pipeline { ... }` 構文の `Expr::Pipeline { steps, span }` を別系統として抽出
   - 各ノードに `forge_compiler::lexer::Span` を付与（新規型は不要、既存 Span を流用）
   - 初期版では各ステップを `NodeKind::MethodCall` として登録

2. **クロージャ要約**
   - `map(s => s.name)` のクロージャパラメータと本体を `notes` に追記
   - closure 本体の field access を抽出して注記に含める

4. **`let` バインディングの収集**
   - `let names = expr` のバインド名を Source ノードのラベルに使う

### テスト方針

- `|>` 3段のパイプラインから 4ノード（Source + 3段）を抽出できる
- 同等の method chain からも同じノード列が得られる（正規化の確認）
- `pipeline { }` 構文は別系統として独立したグラフを生成する
- span は `forge_compiler::lexer::Span` そのまま使用（`start / end / line / col`）

---

## Phase G-2: 型推論

### 目標

各ノードの `input_type` / `output_type` を推定し、型不整合を `Diagnostic` として記録できること。

**注意:** `forge-compiler` の TypeChecker は `MethodCall` / `Call` / `Pipeline` / `AnonStruct`
に対してすべて `Type::Unknown` を返す。Goblet は TypeChecker に依存せず、
独自の builtin シグネチャ表と前向き型伝播で型情報を構築する。

### 実装ステップ

0. **`graph.rs` — DataShape / DataState 拡張**

   各パイプラインノードが「データの形状と状態」を持てるようにする。

   ```rust
   pub enum DataShape {
       Scalar(String),                           // "number", "string", "bool"
       List(Box<DataShape>),                     // list<Student>
       Option(Box<DataShape>),                   // T?
       Result(Box<DataShape>),                   // T!
       Struct(String, Vec<(String, DataShape)>), // Student { id: number, name: string }
       AnonStruct(Vec<(String, DataShape)>),     // { id: number, score: number }
       Tuple(Vec<DataShape>),                    // (list<T>, list<T>)
       Unknown,
   }

   pub enum DataState {
       Definite,    // 値が確定している
       MaybeNone,   // Option — None の可能性あり
       MaybeErr,    // Result — Err の可能性あり
       MaybeEmpty,  // list — 空の可能性あり
       Unknown,
   }

   pub struct NodeDataInfo {
       pub param_name: Option<String>, // closure のパラメータ名 (e.g. "s")
       pub shape: DataShape,           // 構造的な型
       pub state: DataState,           // 値の状態
   }
   ```

   `PipelineNode` に `data_info: Option<NodeDataInfo>` を追加する。

   Mermaid / Text 出力でのデータ形状表示例:

   ```
   N1["students\nlist&lt;Student&gt;\n{id:number, name:string, score:number}"]
   N1 -->|"list&lt;Student&gt;"| N2["filter(score &gt;= 80)"]
   N2 -->|"list&lt;Student&gt; (MaybeEmpty)"| N3["map(s.name)"]
   N3 -->|"list&lt;string&gt;"| N4["take(10)"]
   ```

   Text 出力での形状表示例:

   ```
   [1] students          list<Student>  {id:number, name:string, score:number}  state: Definite
   [2] filter(score>=80) list<Student>  shape: preserved                        state: MaybeEmpty
   [3] map(s => s.name)  list<string>   shape: string (from Student.name)       state: MaybeEmpty
   [4] take(10)          list<string>   shape: preserved                        state: MaybeEmpty
   ```

1. **`typing.rs` — builtin シグネチャ表（core）**

   初期版は既存 `examples/collections` / `examples/pipe` で使われるメソッドに限定する。

   list 系 core:
   - `list<T>.map(fn(T)->U) -> list<U>`
   - `list<T>.filter(fn(T)->bool) -> list<T>`
   - `list<T>.find(fn(T)->bool) -> T?`
   - `list<T>.take(number) -> list<T>`
   - `list<T>.skip(number) -> list<T>`
   - `list<T>.fold(U, fn(U,T)->U) -> U`
   - `list<T>.zip(list<U>) -> list<(T,U)>`
   - `list<T>.partition(fn(T)->bool) -> (list<T>, list<T>)`
   - `list<T>.group_by(fn(T)->K) -> map<K, list<T>>`
   - `list<T>.len() -> number`
   - `list<T>.first() -> T?`
   - `list<T>.any(fn(T)->bool) -> bool`
   - `list<T>.all(fn(T)->bool) -> bool`

   Option 系 core:
   - `T?.map(fn(T)->U) -> U?`
   - `T?.and_then(fn(T)->U?) -> U?`
   - `T?.unwrap_or(T) -> T`
   - `T?.is_some() -> bool`
   - `T?.is_none() -> bool`

   **extended（G-2 後半 or G-4 で追加）:**
   - list: `find_index` / `last` / `sort` / `flat_map`
   - Option: `unwrap` / `or` / `filter`
   - Result 系: `Result<T>.map` / `and_then` / `unwrap_or` / `ok`

2. **前向き型伝播ロジック**
   - ノード列を先頭から処理し、出力型を次ノードの入力型として伝播
   - ローカル変数の型アノテーション (`let x: list<User> = ...`) を初期型として使用
   - closure `s => s.name` に対して、入力型 T のフィールド型を推定

3. **型エラー検出**
   - シグネチャ表にないメソッドを `UnknownMethod` として記録
   - closure の return 型と expected 型が合わない場合 `TypeMismatch`
   - field access が non-struct 型に対して行われる場合 `InvalidFieldAccess`
   - 推論不能な場合は `InferenceFailed` (エラーではなく Warning)

### テスト方針

- `list<Student> |> filter(...) |> map(s => s.name)` で各ノード型が正しく伝播する
- `list<number> |> map(s => s.name)` でエラーノードが生成される
- `find(...)` の後が `T?` になる

---

## Phase G-E2E: ForgeScript 検証例

### 目標

既存の `examples/collections` と `examples/pipe` を回帰入力として使い、
Goblet の抽出・推論が実コードで正しく動くことを確認する。
`examples/goblet/` は最小の説明用サンプルに限定する。

Goblet が解析対象とするコードを実際に動かすことで、
**ForgeScript 側の実装漏れ・パーサーバグ・VM の挙動ずれ**を
CLI 統合前に発見・修正する。

過去の経験（crucible-crud・anvil-crud・pattern）では、
examples を書く段階で未実装機能やパーサーバグが複数発見されている。
Goblet は特にパイプライン・Option・匿名 struct を多用するため、
この検証フェーズが重要である。

**E2E 対象ファイル（優先順）:**
1. `examples/collections/src/main.forge` — list パイプライン全般の回帰入力
2. `examples/pipe/src/main.forge` — `|>` 記法の回帰入力
3. `examples/goblet/src/main.forge` — Goblet 固有の最小説明用サンプル（新規作成）

### `examples/goblet/src/main.forge` の内容

以下のパターンをすべて網羅した ForgeScript プログラムを書く。

1. **list パイプライン基本**
   - `filter` → `map` → `take` の連鎖
   - `fold` で集計
   - `find` で `Option` を返し `unwrap_or` で解決

2. **Option チェーン**
   - `find` → `map` → `and_then` → `unwrap_or`
   - `is_some` / `is_none` による分岐

3. **匿名 struct 変換**
   - `map(e => { id: e.id, name: e.name })` でフィールド選択
   - 変換後の list を `filter` にかける

4. **分割代入**
   - `let (first, rest) = ...` 形式の使用
   - `for (i, v) in enumerate(list)` の使用

5. **Result 伝播**
   - `?` 演算子を使った早期リターン
   - `unwrap_or` によるフォールバック

### `examples/goblet/tests/goblet.test.forge` の内容

- 上記の各パターンについて期待値を持つテストケースを書く
- `forge test` で全テスト通過を確認する

### Mermaid 視認性確認

G-E2E では ForgeScript の動作確認に加え、**Goblet が出力した Mermaid の視認性**も確認する。

Mermaid は `list<T>` や `{ field: type }` のような型表記と構文が衝突しやすく、
サニタイズが不十分だとグラフが壊れて何も表示されなくなる。

確認手順:
1. `forge goblet graph examples/goblet/src/main.forge --format mermaid` を実行
2. 出力を `examples/goblet/pipeline.md` に保存（コードブロック付き）
3. `mmdc -i pipeline.md -o pipeline.png` で PNG にレンダリング
4. 画像を目視確認: ノード・エッジ・ラベルが正しく表示されているか
5. 壊れている場合は `render/mermaid.rs` の sanitize ロジックを修正

`mmdc` が使えない環境では [Mermaid Live Editor](https://mermaid.live/) への貼り付けで代替する。

確認すべき視認性チェックリスト:
- `list<Student>` が `list&lt;Student&gt;` として表示される（タグ崩れなし）
- `{ id: number }` を含むラベルがサブグラフと混同されない
- エッジラベルの型表記が読める
- エラーノードが赤く色分けされている
- ノード数が多い（5段以上）場合でも左右に流れる

### 実装漏れ発見時の対応

- ForgeScript VM / parser の修正が必要な場合は該当クレートを修正する
- 修正後に `cargo test -p forge-vm` を実行して既存テストが壊れていないことを確認する
- 発見した問題と修正内容を commit メッセージに記録する

---

## Phase G-3: CLI 統合

### 目標

`forge goblet` サブコマンドで graph / explain / dump を呼び出せること。

### 実装ステップ

1. **`crates/forge-cli/src/main.rs` への統合**
   - `forge goblet graph <file> [--format text|json|mermaid]`
   - `forge goblet explain <file> [--line N] [--column N]`
   - `forge goblet dump <file>`

2. **`cli.rs` — 引数解析**
   - `clap` サブコマンド追加
   - `--format` オプション (デフォルト: text)
   - `--output <file>` オプション
   - `--function <name>` オプション (特定関数のみ解析)

3. **出力先制御**
   - `--output` 指定時はファイルへ書き込み
   - 未指定時は stdout へ

### テスト方針

- `forge goblet graph examples/collections/src/main.forge` が終了コード 0 で完了する
- Mermaid 出力が `flowchart LR` で始まる
- JSON 出力が valid JSON

---

## Phase G-4: 高度化

### 目標

closure の詳細解析、匿名 struct の表示、複数パイプラインの同時表示。

### 実装ステップ

1. **closure 詳細 (`--include-closures` 時)**
   - closure 本体をサブグラフ化
   - field access チェーンの表示

2. **匿名 struct 表示**
   - `map(e => { id: e.id, score: e.meta.score })` の出力型を `{ id: T, score: U }` として推定
   - `AnonStruct` ノードのフィールド型を notes に含める

3. **複数パイプライン**
   - 1ファイルに複数の `|>` 式がある場合すべてを列挙

---

## Phase G-5: 実行時トレース（将来）

- `forge-vm` にトレースフックを追加
- パイプラインノード ID と実行時イベントを対応づける
- Goblet で静的 DAG と動的 trace をマージ
- 各ステップの要素数・サンプル値を表示

---

## 依存クレート

| クレート | バージョン | 用途 |
|---|---|---|
| `forge-compiler` | workspace | AST / parser 参照 |
| `serde` | 1 | JSON シリアライズ |
| `serde_json` | 1 | JSON 出力 |
| `clap` | 4 | CLI 引数解析（forge-cli 側） |

---

## 実装後の確認

```bash
cargo test -p forge-goblet          # 全ユニットテスト通過
forge goblet graph examples/collections/src/main.forge
forge goblet graph examples/pattern/src/main.forge --format mermaid
forge goblet explain examples/pattern/src/main.forge
```
