# `forge-goblet` タスク一覧

> 仕様: `lang/packages/goblet/spec.md`
> 計画: `lang/packages/goblet/plan.md`
> 実装予定: `crates/forge-goblet/`

---

## Phase G-0: 土台（クレート・中間表現・renderer）

### G-0-A: クレート準備
- [x] `crates/forge-goblet/` ディレクトリ作成
- [x] `crates/forge-goblet/Cargo.toml` 作成（forge-compiler / serde / serde_json を依存に追加）
- [x] ワークスペース `Cargo.toml` に `forge-goblet` を追加
- [x] `crates/forge-goblet/src/lib.rs` 作成（公開 API の骨格）

### G-0-B: 中間表現定義（`graph.rs`）
- [x] `NodeId` 型定義（`usize` ラッパー）
- [x] `NodeKind` enum（Source / MethodCall / FunctionCall / Closure / Filter / Map / Fold / Find / OptionOp / ResultOp / Unknown）
  - 注: `PipeCall` は廃止（`|>` は AST 上 `Expr::MethodCall` に変換済みのため）。由来は `notes` で保持（G-4）
- [x] `NodeStatus` enum（Ok / Warning / Error / Unknown）
- [x] `TypeSummary` 構造体（display: String / nullable: bool / fallible: bool）
- [x] `DataShape` enum（Scalar / List / Option / Result / Struct / AnonStruct / Tuple / Unknown）
- [x] `DataState` enum（Definite / MaybeNone / MaybeErr / MaybeEmpty / Unknown）
- [x] `NodeDataInfo` 構造体（param_name: Option\<String\> / shape: DataShape / state: DataState）
- [x] `Diagnostic` 構造体（node_id / code / message / span / expected / actual）
- [x] `PipelineEdge` 構造体（from / to / label: Option\<String\>）
- [x] `PipelineNode` 構造体（id / label / kind / span / input_type / output_type / data_info / status / notes）
- [x] `PipelineGraph` 構造体（roots / nodes / edges / diagnostics / source_file）
- [x] `PipelineGraph::new()` コンストラクタ
- [x] `PipelineGraph::add_node()` / `add_edge()` / `add_diagnostic()` メソッド

### G-0-C: テキスト renderer（`render/text.rs`）
- [x] `render_text(graph: &PipelineGraph) -> String` 実装
- [x] `[N] label    type    shape    state` 形式で各ノードを出力
- [x] `data_info.shape` が Struct の場合フィールド一覧を `{field:type, ...}` 形式で表示
- [x] `data_info.state` を `(MaybeEmpty)` 等のサフィックスで追記
- [x] エラーノードに `error: <message>` を追記
- [x] テスト: 3ノードグラフの text 出力が期待フォーマットに一致する

### G-0-D: JSON renderer（`render/json.rs`）
- [x] `PipelineGraph` / `PipelineNode` / `Diagnostic` に `serde::Serialize` derive
- [x] `render_json(graph: &PipelineGraph) -> String` 実装
- [x] テスト: JSON 出力が valid JSON で `nodes` キーを持つ

### G-0-E: Mermaid renderer（`render/mermaid.rs`）
- [x] `sanitize_label(s: &str) -> String` 実装（`<>&{}|"` を Mermaid エンティティに変換）
- [x] `render_mermaid(graph: &PipelineGraph) -> String` 実装
- [x] `flowchart LR` ヘッダー出力
- [x] ノード定義を常に `["..."]` 形式（ダブルクォート囲み）で出力
- [x] ノード ID を `N` + 連番のみにする（数字始まり禁止）
- [x] エッジラベルにデータ型を付与: `N1 -->|"list&lt;Student&gt;"| N2`
- [x] ノードラベルに `data_info.shape` の概要を改行区切りで含める
- [x] エラーノードに `:::error` スタイル付与
- [x] `classDef` でノードステータス色定義（ok=緑 / warning=黄 / error=赤）
- [x] テスト: 出力が `flowchart LR` で始まる
- [x] テスト: `list<T>` を含むラベルに `<` `>` がそのまま出ない（エスケープ済み）
- [x] テスト: `{ field: type }` を含むラベルで `{` `}` がエスケープ済み
- [x] テスト: ノード ID がすべて `N` + 数字パターンに一致する
- [x] テスト: 型エラーノード（`NodeStatus::Error`）を含むグラフで `:::error` が付与され `classDef error` が出力される（壊れたパイプラインの図示）

### G-0-F: 公開 API（`lib.rs`）
- [x] `analyze_source(src: &str) -> Result<Vec<PipelineGraph>, GobletError>` シグネチャ定義（中身は stub）
  - 注: 1ファイルに複数パイプラインが存在しうるため Vec を返す（`extract_pipelines` と整合）
- [x] `GobletError` enum 定義（ParseError / ExtractionError / InternalError）
- [x] `render_mermaid` / `render_json` / `render_text` を lib から再エクスポート
- [x] `OutputFormat` enum 定義（Text / Json / Mermaid）
- [x] `save_pipeline(graph: &PipelineGraph, path: &Path, format: OutputFormat) -> Result<(), GobletError>` 実装（Mermaid の場合はコードブロック付き `.md` として保存）
- [x] テスト: `save_pipeline` がファイルを生成し、内容が空でない

---

## Phase G-1: 抽出（`extractor.rs`）

### G-1-A: パイプライン正規化
- [x] `extract_pipelines(stmts: &[Stmt]) -> Vec<PipelineGraph>` 実装
- [x] `|>` 由来の `Expr::MethodCall` 連鎖を pipeline node 列へ正規化
- [x] 通常の `Expr::MethodCall` 連鎖も同じ内部表現へ正規化（両者を同一 path で処理）
- [x] `pipeline { ... }` 構文の `Expr::Pipeline { steps, span }` を別系統として抽出
- [x] Source ノード（最初の被演算子）を `NodeKind::Source` として登録
- [x] 各ステップを初期版では `NodeKind::MethodCall` として登録
- [x] `let <name> = <pipeline>` のバインド名を Source ノードラベルに使用

### G-1-B: span 対応
- [x] AST の span 情報を `PipelineNode::span` に格納
- [x] `forge_compiler::lexer::Span` をそのまま使用（`start / end / line / col`）
  - 注: `Span` は `Eq` / `Serialize` を持たないため、`SourceSpan` を薄いラッパーとして保持。フィールドは Span と同一。`file` が必要になった時点で `source_file` と組み合わせて解決
- [x] parser が span を持つ場合は抽出、なければ `None`

### G-1-C: method chain 抽出
- [x] `Expr::MethodCall` の連鎖を再帰的に検出
- [x] `students.filter(...).map(...).take(10)` → 4ノード列に変換
- [x] `NodeKind::MethodCall` として登録し、メソッド名をラベルに使用

### G-1-D: クロージャ要約
- [x] `map(s => s.name)` のクロージャを検出し `NodeKind::Closure` 注記として追加
- [x] closure パラメータ名・本体の最上位 field access を `notes` に追記
- [x] `filter(s => s.score >= 80)` の条件式を 1行サマリとして `notes` に追記

### G-1-E: `analyze_source` 実装
- [x] `analyze_source` の中身を実装（parse → extract）
- [x] `forge-compiler::parser::parse(src)` を呼び出して AST を取得
- [x] AST を `extract_pipelines` に渡して `Vec<PipelineGraph>` を返す

### G-1-F: 抽出テスト
- [x] `test_extract_pipe_3_steps` — `a |> f() |> g() |> h()` から 4ノードを抽出
- [x] `test_extract_method_chain` — `xs.filter(...).map(...)` から 3ノードを抽出
- [x] `test_extract_pipe_equals_method_chain` — `|>` 記法と同等の method chain が同じノード列を生成する（正規化の確認）
- [x] `test_extract_pipeline_block` — `pipeline { source xs; filter ... }` が別系統として抽出される
- [x] `test_extract_let_binding_label` — `let names = ...` の `names` がルートラベルに反映される
- [x] `test_extract_closure_notes` — closure パラメータが notes に含まれる

---

## Phase G-2: 型推論（`typing.rs`）

### G-2-A: builtin シグネチャ表（list 系 core）
> 初期版スコープ: 既存 `examples/collections` / `examples/pipe` で使われるメソッドに限定
- [x] `BuiltinSig` 構造体定義（type_name / method_name / input_type_param / output_type）
- [x] `list<T>.map(fn(T)->U) -> list<U>` 登録
- [x] `list<T>.filter(fn(T)->bool) -> list<T>` 登録
- [x] `list<T>.find(fn(T)->bool) -> T?` 登録
- [x] `list<T>.take(number) -> list<T>` 登録
- [x] `list<T>.skip(number) -> list<T>` 登録
- [x] `list<T>.fold(U, fn(U,T)->U) -> U` 登録
- [x] `list<T>.zip(list<U>) -> list<(T,U)>` 登録
- [x] `list<T>.partition(fn(T)->bool) -> (list<T>, list<T>)` 登録
- [x] `list<T>.group_by(fn(T)->K) -> map<K, list<T>>` 登録
- [x] `list<T>.len() -> number` 登録
- [x] `list<T>.first() -> T?` 登録
- [x] `list<T>.any(fn(T)->bool) -> bool` 登録
- [x] `list<T>.all(fn(T)->bool) -> bool` 登録

### G-2-A-ext: builtin シグネチャ表（list 系 extended）
> G-2 後半または G-4 で追加。初期スコープ外
- [x] `list<T>.find_index(fn(T)->bool) -> number?` 登録
- [x] `list<T>.last() -> T?` 登録
- [x] `list<T>.sort(fn(T,T)->number) -> list<T>` 登録
- [x] `list<T>.flat_map(fn(T)->list<U>) -> list<U>` 登録

### G-2-B: builtin シグネチャ表（Option 系 core）
- [x] `T?.map(fn(T)->U) -> U?` 登録
- [x] `T?.and_then(fn(T)->U?) -> U?` 登録
- [x] `T?.unwrap_or(T) -> T` 登録
- [x] `T?.is_some() -> bool` 登録
- [x] `T?.is_none() -> bool` 登録

### G-2-B-ext: builtin シグネチャ表（Option 系 extended）
> G-2 後半または G-4 で追加
- [x] `T?.unwrap() -> T` 登録
- [x] `T?.or(T?) -> T?` 登録
- [x] `T?.filter(fn(T)->bool) -> T?` 登録

### G-2-C: builtin シグネチャ表（Result 系）
> extended 扱い。G-E2E-F の Result 伝播検証が必要になった時点で実装
- [x] `Result<T>.map(fn(T)->U) -> Result<U>` 登録
- [x] `Result<T>.and_then(fn(T)->Result<U>) -> Result<U>` 登録
- [x] `Result<T>.unwrap_or(T) -> T` 登録
- [x] `Result<T>.ok() -> T?` 登録

### G-2-D: 前向き型・DataShape 伝播（`type_propagate`）
- [x] `type_propagate(graph: &mut PipelineGraph, annotations: &TypeAnnotations)` 実装
- [x] ローカル変数の型アノテーションを収集する `TypeAnnotations` 構造体
- [x] Source ノードの型・shape をアノテーションから解決
- [x] `list<T>` → `filter` → 出力型が `list<T>`、state が `MaybeEmpty` になる
- [x] `list<T>` → `map(fn(T)->U)` → 出力型が `list<U>` になる
- [x] `list<T>` → `find(...)` → 出力型が `T?`、state が `MaybeNone` になる
- [x] `T?` → `unwrap_or(x)` → 出力型が `T`、state が `Definite` になる
- [x] closure の入力パラメータ名を `NodeDataInfo::param_name` に格納する
- [x] closure の入力 `T` を `list<T>` から抽出するロジック
- [x] `DataShape::Struct` のフィールド情報を struct 定義 AST から収集する
- [x] `map(s => { id: s.id })` の出力 shape を `DataShape::AnonStruct` として推定する
- [x] エッジラベルに出力 shape の display 文字列を設定する

### G-2-E: 型エラー検出
- [x] シグネチャ表にないメソッドを `UnknownMethod` Diagnostic として記録
- [x] field access が non-struct 型に対して行われる場合 `InvalidFieldAccess` を記録
- [x] 推論不能な場合を `InferenceFailed` (Warning) として記録
- [x] エラーノードの `NodeStatus` を `Error` / `Warning` に設定
- [x] 未定義シンボルを参照した場合 `UnknownSymbol` (Error) として記録
- [x] `and_then` クロージャが `Option<T>` / `Result<T>` 以外を返す場合 `TypeMismatch` (Error) として記録
- [x] `Tuple` / `AnonStruct` / `Struct` 等のサポート外 shape にメソッドが呼ばれた場合 `UnsupportedPipelineShape` (Error) として記録
- [x] `filter` クロージャが `bool` 以外の型を返す場合 `InvalidClosureReturn` (Error) として記録

### G-2-F: 型推論テスト
- [x] `test_type_list_filter_map` — `list<Student> |> filter |> map(s=>s.name)` の各ノード型が正しい
- [x] `test_type_find_returns_option` — `find(...)` の後が `T?` になる
- [x] `test_type_option_unwrap_or` — `T?.unwrap_or(x)` の後が `T` になる
- [x] `test_type_mismatch_field_on_number` — `list<number> |> map(s=>s.name)` でエラーが出る
- [x] `test_type_unknown_method` — 未知メソッドが `UnknownMethod` Diagnostic になる
- [x] `test_broken_pipeline_mermaid` — 型エラーを含むパイプラインを `render_mermaid` に渡し、エラーノードに `:::error` が付き `classDef error fill:#f88` が含まれることを確認する（壊れたパイプラインの視覚的図示）
- [x] `test_broken_pipeline_save` — 上記グラフを `save_pipeline(..., Mermaid)` でファイル保存し、保存された `.md` が Mermaid コードブロック（` ```mermaid `）を含むことを確認する
- [x] `test_diag_unknown_symbol` — 未定義変数が `UnknownSymbol` Diagnostic になる
- [x] `test_diag_type_mismatch_and_then_option` — `and_then` クロージャが非 Option を返すと `TypeMismatch` Diagnostic になる
- [x] `test_diag_unsupported_pipeline_shape` — Tuple shape にメソッドを呼ぶと `UnsupportedPipelineShape` Diagnostic になる
- [x] `test_diag_invalid_closure_return_filter` — `filter` クロージャが非 bool を返すと `InvalidClosureReturn` Diagnostic になる

---

## Phase G-E2E: ForgeScript 検証例（`examples/goblet/`）

G-2 完成後、G-3 に進む前に必ず実施する。
ForgeScript 側の実装漏れはここで発見し修正する。

### G-E2E-A: 既存 examples による回帰確認（優先）
> 新規 examples を書く前に、既存コードが Goblet で解析できることを先に確認する
- [x] `forge goblet graph examples/collections/src/main.forge` がエラーなく完了する
- [x] `forge goblet graph examples/pipe/src/main.forge` がエラーなく完了する
- [x] 上記2ファイルから少なくとも1本以上のパイプラインが抽出される
- [x] 発見した実装漏れを修正する

### G-E2E-B: 説明用サンプル準備（最小限）
> examples/goblet は Goblet 固有の説明用に限定。既存 examples でカバーできるパターンは重複させない
- [x] `examples/goblet/forge.toml` 作成（name = "goblet-example"）
- [x] `examples/goblet/src/main.forge` 作成（Goblet 固有パターンのみ: 匿名struct変換・Result伝播）
- [x] `examples/goblet/tests/goblet.test.forge` 作成（骨格のみ）

### G-E2E-C: list パイプライン検証
> `examples/collections` で回帰確認済みのパターン。`examples/goblet` でも最低限確認する
- [x] `filter → map → take` の連鎖が `forge run` で正しく動く
- [x] `fold` による集計（合計・最大値など）が動く
- [x] `find` が `Option` を返し `unwrap_or` でデフォルト値を返す
- [x] `zip` による2リストの結合が動く
- [x] `partition` による分割が動く
- [x] `any` / `all` による真偽確認が動く
- [x] テスト: 上記パターンの期待値テストが `forge test` で通過する（examples/collections 30テスト全通過）

### G-E2E-D: Option チェーン検証
- [x] `find → map → unwrap_or` の連鎖が動く
- [x] `and_then` によるネストした Option 解決が動く
- [x] `is_some` / `is_none` による分岐が動く
- [x] `filter` による条件付き Option が動く
- [x] テスト: Option チェーンの各パターンテストが通過する

### G-E2E-E: 匿名 struct 変換検証
- [x] `map(e => { id: e.id, name: e.name })` でフィールド選択できる
- [x] 匿名 struct の list に `filter` をかけられる
- [x] `|>` と組み合わせた匿名 struct 変換パイプラインが動く
- [x] テスト: 匿名 struct 変換結果の型・値テストが通過する

### G-E2E-F: 分割代入検証
- [x] `let (a, b) = tuple_expr` が動く
- [x] `for (i, v) in enumerate(list)` が動く
- [x] rest パターン `let (head, ..tail) = list` が動く（実装済みの場合）
- [x] テスト: 分割代入の各パターンテストが通過する

### G-E2E-G: Result 伝播検証
- [x] `?` 演算子による早期リターンが動く
- [x] `unwrap_or` によるフォールバックが動く
- [x] `-> T!` 戻り値の関数から `?` で伝播できる
- [x] テスト: Result 伝播パターンのテストが通過する

### G-E2E-H: Mermaid 視認性確認
- [x] `forge goblet graph examples/goblet/src/main.forge --format mermaid` を実行
- [x] 出力を `examples/goblet/pipeline.md` にコードブロック付きで保存
- [x] `mmdc -i pipeline.md -o pipeline.png` で PNG にレンダリング（または Mermaid Live Editor で確認）
- [x] `list<T>` 含むラベルがノード崩れなく表示される（`&lt;` `&gt;` 変換済み）
- [x] `{ field: type }` 含むラベルがサブグラフと混同されない
- [x] エッジラベルの型表記が読める
- [x] エラーノードが赤く色分けされている
- [x] 5段以上のパイプラインが左右に正しく流れる
- [x] 問題があれば `render/mermaid.rs` の sanitize ロジックを修正して再確認する

### G-E2E-I: 実装漏れ修正・最終確認
- [x] `forge run examples/goblet/src/main.forge` がエラーなく完了する
- [x] `forge test examples/goblet/` が全テスト通過する
- [x] 発見した ForgeScript 実装漏れをすべて修正済みにする
- [x] `cargo test -p forge-vm` で既存テストが壊れていないことを確認する

---

## Phase G-3: CLI 統合

### G-3-A: `forge-cli` への統合
- [x] `crates/forge-cli/Cargo.toml` に `forge-goblet` を依存として追加
- [x] `forge goblet` サブコマンドを登録（graph / explain / dump）
- [x] `forge goblet graph <file> [--format text|json|mermaid] [--output <file>]`
- [x] `forge goblet explain <file> [--line N] [--function <name>]`
- [x] `forge goblet dump <file>`

### G-3-B: 出力制御
- [x] `--format` 未指定時のデフォルトを `text` にする
- [x] `--output <file>` 指定時はファイルへ書き込む
- [x] 終了コード: 型エラー検出時は 1、解析失敗時は 2、正常時は 0

### G-3-C: CLI テスト
- [x] `test_cli_graph_text` — `forge goblet graph` が text 出力で 0 終了する
- [x] `test_cli_graph_mermaid` — `--format mermaid` で Mermaid 出力が得られる
- [x] `test_cli_graph_json` — `--format json` で valid JSON が得られる

---

## Phase G-4: 高度化

### G-4-A: closure 詳細解析
- [x] `--include-closures` フラグ追加
- [x] closure 本体をサブグラフ化（field access ノードを子ノードとして展開）
- [x] closure 内の条件式・変換式を個別ノードとして表示
- [x] closure 条件式ノードに `bool` 型を付与する
- [x] closure field access ノードに推定型を付与する

### G-4-B: 匿名 struct 表示
- [x] `map(e => { id: e.id, score: e.meta.score })` の出力型を `{ id: T, score: U }` として推定
- [x] `AnonStruct` フィールド型を notes に含める
- [x] `TypeSummary::display` に `{ field: type, ... }` 形式で出力
- [x] `if` / `block` を含む closure 本体でも field access を再帰収集する
- [x] ネストした field path（`e.meta.score`）を実型で解決する
- [x] closure detail ノードが親ノードの出力型を継承できる

### G-4-C: 複数パイプライン表示
- [x] 1ファイルに複数の `|>` がある場合、すべてを列挙して連番で表示
- [x] `--function <name>` 指定で特定関数内のパイプラインのみ表示

---

## Phase G-5: 実行時トレース（将来）

- [x] `forge-vm` にパイプラインノード ID 対応のトレースフックを追加
- [x] 各ステップの要素数をトレース記録
- [x] `find` が `none` になった箇所を記録
- [x] `Result` が `err` に落ちた箇所を記録
- [x] 静的 DAG と動的 trace をマージして表示

---

## 進捗サマリ

| Phase | タスク数 | 完了 |
|---|---|---|
| G-0 | 46 | 46 |
| G-1 | 25 | 25 |
| G-2 | 65 | 65 |
| G-E2E | 44 | 44 |
| G-3 | 11 | 11 |
| G-4 | 13 | 13 |
| G-5 | 5 | 5 |
| **合計** | **209** | **209** |
