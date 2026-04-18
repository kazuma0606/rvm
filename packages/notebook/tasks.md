# `forge-notebook` タスク一覧

> 仕様: `packages/notebook/spec.md`
> 計画: `packages/notebook/plan.md`
> 実装予定: `crates/forge-notebook/`

---

## 進捗サマリー

- Phase N-0: 25/25 完了
- Phase N-1: 26/26 完了
- Phase N-2: 21/21 完了
- Phase N-3: 14/14 完了
- Phase N-G: 24/24 完了（Goblet パイプライントレース出力）
- Phase N-4: 9/9 完了（オプション）
- **合計: 121/121 完了**

---

## Phase N-0: 基盤（クレート・パーサー・run コマンド）

### N-0-A: クレート準備

- [x] `crates/forge-notebook/` ディレクトリ作成
- [x] `crates/forge-notebook/Cargo.toml` 作成（`forge-compiler` / `forge-vm` / `serde` / `serde_json` / `chrono` 依存）
- [x] ワークスペース `Cargo.toml` に `forge-notebook` を追加
- [x] `crates/forge-notebook/src/lib.rs` 作成（公開 API の骨格）

### N-0-B: `.fnb` パーサー（`parser.rs`）

- [x] `Cell` enum 定義（`Code(CodeCell)` / `Markdown(MarkdownCell)`）
- [x] `CodeCell` 構造体定義（`index` / `name` / `hidden` / `skip` / `source` / `start_line`）
- [x] `MarkdownCell` 構造体定義（`index` / `content`）
- [x] `parse_notebook(src: &str) -> Vec<Cell>` 実装
  - ` ```forge ` フェンスをコードセルとして抽出
  - それ以外をMarkdownセルとして抽出
- [x] フェンス属性パース（`name="..."` / `hidden=true` / `skip=true`）
- [x] `name` 未指定時に `cell_<n>` を自動付与

### N-0-C: セル実行（`runner.rs`）

- [x] `RunOptions` 構造体定義（`cell_filter` / `stop_on_error`）
- [x] `CellResult` 構造体定義（`index` / `name` / `status` / `stdout` / `error` / `duration_ms`）
- [x] `run_notebook(cells: &[Cell], opts: RunOptions) -> Vec<CellResult>` 実装
  - `forge-vm` の `Interpreter` を使い回してセル間スコープを共有
  - `skip=true` のセルをスキップ
  - `--cell <name>` 指定時は指定セルのみ実行
  - `--stop-on-error` 時は最初のエラーで停止
  - エラーはセル境界で止め次のセルへ継続

### N-0-D: CLI 統合

- [x] `forge-cli` に `notebook` サブコマンドを追加（手書き引数パーサ）
- [x] `forge notebook run <file.fnb>` 実装
- [x] `forge notebook run <file.fnb> --cell <name>` 実装
- [x] `forge notebook run <file.fnb> --stop-on-error` 実装

### N-0-E: テスト

- [x] `test_parse_empty_notebook` — セルなしの `.fnb` をパースしてもパニックしない
- [x] `test_parse_code_cells` — コードセル 3 個を正しく抽出できる
- [x] `test_parse_fence_attrs` — `name` / `hidden` / `skip` 属性が正しくパースされる
- [x] `test_parse_default_name` — `name` なしセルに `cell_0`, `cell_1` が付与される
- [x] `test_run_shared_scope` — セル間で変数が共有される
- [x] `test_run_skip_cell` — `skip=true` のセルがスキップされる
- [x] `test_run_stop_on_error` — `--stop-on-error` 時に最初のエラーで停止する
- [x] `test_run_continue_on_error` — エラーセルの次のセルが実行される

---

## Phase N-1: カーネル（stdio JSON-RPC）

### N-1-A: カーネルプロセス（`kernel.rs`）

- [x] `KernelRequest` / `KernelResponse` 型定義（JSON-RPC）
- [x] `KernelOutput` / `OutputItem` 型定義
- [x] `execute` メソッド実装（コード実行 → outputs 返却）
- [x] `reset` メソッド実装（インタープリタ再初期化）
- [x] `shutdown` メソッド実装（プロセス終了）
- [x] `partial` レスポンス実装（`println` 出力のストリーミング）
- [x] `forge notebook --kernel` CLI エントリポイント追加

### N-1-B: カーネルクライアント（`client.rs`）

- [x] `KernelClient` 構造体定義（`process` / `stdin` / `stdout` / `next_id`）
- [x] `KernelClient::spawn()` 実装（子プロセス起動）
- [x] `KernelClient::execute(&mut self, code: &str)` 実装
- [x] `KernelClient::reset(&mut self)` 実装
- [x] `KernelClient::shutdown(self)` 実装
- [x] `forge notebook run` をカーネルクライアント経由に切り替え

### N-1-C: `.fnb.out.json` 出力（`output.rs`）

- [x] `NotebookOutput` 構造体定義（`version` / `file` / `executed_at` / `cells`）
- [x] `CellOutput` 構造体定義（`index` / `name` / `status` / `outputs` / `duration_ms`）
- [x] `OutputItem` serde 実装（`type` フィールドで `text`/`html`/`json`/`table`/`image`/`markdown`/`error` を判別）
- [x] `save_output(path: &Path, output: &NotebookOutput)` 実装
- [x] `load_output(path: &Path)` 実装（VS Code 拡張用）

### N-1-D: 追加 CLI コマンド

- [x] `forge notebook reset <file>` 実装（カーネル再起動 + 全セル再実行）
- [x] `forge notebook clear <file>` 実装（`.fnb.out.json` を削除）
- [x] `forge notebook show <file>` 実装（セル一覧: 名前・行番号・ステータス）

### N-1-E: テスト

- [x] `test_kernel_execute` — カーネルに `execute` を送り `ok` レスポンスが返る
- [x] `test_kernel_reset` — `reset` 後に変数スコープがクリアされる
- [x] `test_kernel_shutdown` — `shutdown` 後にプロセスが終了している
- [x] `test_output_json_format` — `.fnb.out.json` のフォーマットが spec §5 に準拠
- [x] `test_notebook_show` — `show` の出力にセル名と行番号が含まれる

---

## Phase N-2: VS Code 拡張

### N-2-A: `FnbSerializer`（TypeScript）

- [x] `editors/vscode/src/notebook/serializer.ts` 作成
- [x] `deserializeNotebook(data: Uint8Array)` — `.fnb` テキスト → `vscode.NotebookData`
- [x] `serializeNotebook(data: vscode.NotebookData)` — `NotebookData` → `.fnb` テキスト
- [x] `vscode.notebooks.registerNotebookSerializer("fnb", serializer)` の呼び出し

### N-2-B: `FnbKernelController`（TypeScript）

- [x] `editors/vscode/src/notebook/controller.ts` 作成
- [x] `vscode.notebooks.createNotebookController(...)` で作成
- [x] セル実行時に `forge notebook --kernel` を `child_process.spawn` で起動（初回のみ）
- [x] stdin/stdout JSON-RPC 通信の実装
- [x] `NotebookCellOutput` への変換と表示
- [x] `.fnb.out.json` の更新

### N-2-C: `package.json` / `extension.ts` 更新

- [x] `contributes.notebooks` に `.fnb` を追加
- [x] `activationEvents` に `onNotebook:fnb` を追加
- [x] `extension.ts` から `FnbSerializer` と `FnbKernelController` を初期化

### N-2-D: 出力表示

- [x] `hidden=true` セルのソース折り畳み対応
- [x] `.fnb.out.json` からの既存出力読み込みと表示
- [x] `display::table` の VS Code 表形式レンダリング
- [x] `display::html` の WebView レンダリング

### N-2-E: テスト

- [x] `.fnb` ファイルが VS Code で Notebook ビューとして開ける（手動確認）
- [x] ▶ ボタンでセルを実行できる（手動確認）
- [x] 出力がセル直下に表示される（手動確認）
- [x] `.fnb.out.json` が正しく更新される（手動確認）

---

## Phase N-3: `display()` 組み込み関数

### N-3-A: display 関数群の実装

- [x] `display(value)` — 型に応じて自動選択（`string`→text / `list<map>`→table / `map`→json / その他→text）
- [x] `display::text(s: string)` 実装
- [x] `display::html(html: string)` 実装
- [x] `display::json(value: any)` 実装
- [x] `display::table(rows: list<map>)` 実装
- [x] `display::image(path: string)` 実装
- [x] `display::markdown(md: string)` 実装

### N-3-B: `forge run` フォールバック

- [x] ノートブック環境外では `display::*` を全て `println` に変換
- [x] `display::table(rows)` → 各行を `[val1, val2, ...]` 形式で println

### N-3-C: カーネルモード統合

- [x] カーネルモードで `display::*` の呼び出しを `OutputItem` として収集
- [x] `OutputItem` を JSON-RPC レスポンスの `outputs` に含める

### N-3-D: テスト

- [x] `test_display_text_fallback` — `forge run` で `display::text("hello")` が `"hello\n"` を出力
- [x] `test_display_table_fallback` — `forge run` で `display::table` が各行を println
- [x] `test_display_auto_string` — `display("hello")` が `display::text` に委譲
- [x] `test_display_auto_list_map` — `display(list_of_maps)` が `display::table` に委譲
- [x] `test_display_kernel_json` — カーネルモードで `display::json` が `OutputItem` を生成
- [x] `test_display_kernel_table` — カーネルモードで `display::table` が適切な `OutputItem` を生成

---

## Phase N-G: Goblet パイプライントレース出力

> 前提: N-1（カーネル）完了後に着手。
> 設計参照: `lang/packages/goblet/v2/` スクリーンショット群
>
> **目的**: セルにパイプラインが含まれ、実行中にデータ汚染（null フィールド / 型エラー / スコア異常など）が
> 検出された場合、ノートブック出力として「パイプライン健全性ビュー」を表示する。
> 正常時はシンプルな DATA FLOW のみ。異常時は各ステージの汚染レコード数・詳細・
> ソースコードのハイライトを含む。

### N-G-A: ランタイムトレース（`crates/forge-vm`）

- [x] `PipelineStageTrace` 構造体定義
  ```
  stage_name: String       // "filter", "map", "take" など
  in_count: usize          // 入力レコード数
  out_count: usize         // 出力レコード数
  corrupted: Vec<CorruptedRecord>
  ```
- [x] `CorruptedRecord` 構造体定義
  ```
  index: usize             // レコード番号（1-based）
  fields: Vec<(String, Value)>  // フィールド名と値
  reason: String           // "フィールド名が空", "nameがnull", "スコアがNaN" など
  ```
- [x] `PipelineTrace` 構造体定義（全ステージのまとめ）
  ```
  pipeline_name: String    // let バインド名 or "pipeline_<n>"
  source_snippet: String   // パイプラインのソースコード（複数行）
  stages: Vec<PipelineStageTrace>
  total_records: usize
  total_corrupted: usize
  ```
- [x] VM の `filter` / `map` / `take` / `find` 等の実行フックで汚染レコードを検出・収集
  - `null` / `none` なフィールドへのアクセス
  - 数値フィールドが `NaN` / 負値（スコアなど）
  - 型不一致によりクロージャが panic/error した行
- [x] `Interpreter` に `trace_mode: bool` フラグを追加
- [x] トレースモード時のみ `PipelineTrace` を生成（通常実行のオーバーヘッドなし）

### N-G-B: `OutputItem` 新型 `"pipeline_trace"`（`crates/forge-notebook`）

- [x] `OutputItem::PipelineTrace` バリアント追加
  ```json
  {
    "type": "pipeline_trace",
    "pipeline_name": "names",
    "source_snippet": "let names =\n    students\n    |> filter...",
    "stages": [
      { "name": "source",  "in": 10, "out": 10, "corrupted": 3 },
      { "name": "filter",  "in": 10, "out":  8, "corrupted": 3 },
      { "name": "map",     "in":  8, "out":  8, "corrupted": 3 },
      { "name": "take",    "in":  8, "out":  7, "corrupted": 3 }
    ],
    "corruptions": [
      { "stage": "source", "index": 4, "reason": "フィールド名が空、スコアが負の値" },
      { "stage": "source", "index": 5, "reason": "nameがnull" },
      { "stage": "source", "index": 9, "reason": "スコアがNaN" }
    ],
    "records_by_stage": {
      "source": [ { "id": 1, "name": "田中太郎", "score": 95 }, ... ]
    }
  }
  ```
- [x] カーネルがパイプラインを含むセルを実行する際、トレースモードを自動有効化
- [x] 汚染レコードが 1 件以上ある場合のみ `pipeline_trace` を outputs に追加
- [x] 汚染 0 件でもパイプラインが含まれる場合、簡略版（ステージ数・レコード数のみ）を出力

### N-G-C: VS Code WebView レンダリング（TypeScript）

> スクリーンショットのレイアウトを参考に実装する。

- [x] `editors/vscode/src/notebook/pipeline-trace-renderer.ts` 作成
- [x] **DATA FLOW ビュー**: ステージをボックスで横並び表示
  - 正常: 白枠 `stage\nN records`
  - 汚染あり: 赤枠 `stage\nN records (M corrupted)` ← 赤テキスト
  - ステージ間の矢印（`→`）
- [x] **SOURCE CODE ビュー**: パイプラインのソースコード表示
  - 汚染があるステージに対応する行を暗赤色でハイライト
  - 行末に `N corrupted` ラベルを表示
- [x] **STAGE DETAIL ビュー**: DATA FLOW のステージをクリックすると表示
  - レコード一覧テーブル（id / 各フィールド）
  - 汚染レコードの行を赤くハイライト
  - `Corruption Details` ボックス（汚染理由を箇条書き）
- [x] 汚染 0 件（正常）時は DATA FLOW のみ表示（コンパクト表示）
- [x] `forge run` テキストフォールバック（`pipeline_trace` → ASCII テキスト表形式）

### N-G-D: `forge run` テキストフォールバック

- [x] `pipeline_trace` OutputItem をテキストで出力するフォールバック実装
  ```
  [pipeline: names]  source(10) → filter(8) → map(8) → take(7)
  ⚠ 3 corrupted records detected
    #4: フィールド名が空、スコアが負の値
    #5: nameがnull
    #9: スコアがNaN
  ```

### N-G-E: テスト

- [x] `test_trace_clean_pipeline` — 汚染なしパイプラインで `total_corrupted == 0`
- [x] `test_trace_null_field` — null フィールドを含むレコードが汚染として検出される
- [x] `test_trace_nan_score` — NaN スコアを含むレコードが検出される
- [x] `test_trace_stage_counts` — 各ステージの in/out カウントが正しい
- [x] `test_output_pipeline_trace_json` — `OutputItem::PipelineTrace` が正しい JSON に変換される
- [x] `test_fallback_text` — テキストフォールバックが ASCII 表示を生成する

---

## Phase N-4: エクスポート（オプション）

### N-4-A: ipynb エクスポート（`export.rs`）

- [x] `export_ipynb(cells: &[Cell], output: Option<&NotebookOutput>) -> serde_json::Value` 実装
- [x] コードセル → Jupyter `cell_type: "code"` 変換
- [x] Markdown セル → Jupyter `cell_type: "markdown"` 変換
- [x] `.fnb.out.json` の outputs → Jupyter `outputs` 変換

### N-4-B: CLI 統合

- [x] `forge notebook export <file.fnb> --format ipynb` 実装

### N-4-C: テスト

- [x] `test_export_ipynb_valid_json` — 出力が有効な JSON である
- [x] `test_export_ipynb_cell_count` — セル数が一致している
- [x] `test_export_ipynb_code_cell` — コードセルが正しく変換される
- [x] `test_export_ipynb_markdown_cell` — Markdown セルが正しく変換される
