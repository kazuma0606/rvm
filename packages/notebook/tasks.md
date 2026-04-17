# `forge-notebook` タスク一覧

> 仕様: `packages/notebook/spec.md`
> 計画: `packages/notebook/plan.md`
> 実装予定: `crates/forge-notebook/`

---

## 進捗サマリー

- Phase N-0: 0/18 完了
- Phase N-1: 0/20 完了
- Phase N-2: 0/16 完了
- Phase N-3: 0/14 完了
- Phase N-4: 0/6 完了（オプション）
- **合計: 0/74 完了**

---

## Phase N-0: 基盤（クレート・パーサー・run コマンド）

### N-0-A: クレート準備

- [ ] `crates/forge-notebook/` ディレクトリ作成
- [ ] `crates/forge-notebook/Cargo.toml` 作成（`forge-compiler` / `forge-vm` / `serde` / `serde_json` / `chrono` 依存）
- [ ] ワークスペース `Cargo.toml` に `forge-notebook` を追加
- [ ] `crates/forge-notebook/src/lib.rs` 作成（公開 API の骨格）

### N-0-B: `.fnb` パーサー（`parser.rs`）

- [ ] `Cell` enum 定義（`Code(CodeCell)` / `Markdown(MarkdownCell)`）
- [ ] `CodeCell` 構造体定義（`index` / `name` / `hidden` / `skip` / `source` / `start_line`）
- [ ] `MarkdownCell` 構造体定義（`index` / `content`）
- [ ] `parse_notebook(src: &str) -> Vec<Cell>` 実装
  - ` ```forge ` フェンスをコードセルとして抽出
  - それ以外をMarkdownセルとして抽出
- [ ] フェンス属性パース（`name="..."` / `hidden=true` / `skip=true`）
- [ ] `name` 未指定時に `cell_<n>` を自動付与

### N-0-C: セル実行（`runner.rs`）

- [ ] `RunOptions` 構造体定義（`cell_filter` / `stop_on_error`）
- [ ] `CellResult` 構造体定義（`index` / `name` / `status` / `stdout` / `error` / `duration_ms`）
- [ ] `run_notebook(cells: &[Cell], opts: RunOptions) -> Vec<CellResult>` 実装
  - `forge-vm` の `Interpreter` を使い回してセル間スコープを共有
  - `skip=true` のセルをスキップ
  - `--cell <name>` 指定時は指定セルのみ実行
  - `--stop-on-error` 時は最初のエラーで停止
  - エラーはセル境界で止め次のセルへ継続

### N-0-D: CLI 統合

- [ ] `forge-cli` に `notebook` サブコマンドを追加（`clap`）
- [ ] `forge notebook run <file.fnb>` 実装
- [ ] `forge notebook run <file.fnb> --cell <name>` 実装
- [ ] `forge notebook run <file.fnb> --stop-on-error` 実装

### N-0-E: テスト

- [ ] `test_parse_empty_notebook` — セルなしの `.fnb` をパースしてもパニックしない
- [ ] `test_parse_code_cells` — コードセル 3 個を正しく抽出できる
- [ ] `test_parse_fence_attrs` — `name` / `hidden` / `skip` 属性が正しくパースされる
- [ ] `test_parse_default_name` — `name` なしセルに `cell_0`, `cell_1` が付与される
- [ ] `test_run_shared_scope` — セル間で変数が共有される
- [ ] `test_run_skip_cell` — `skip=true` のセルがスキップされる
- [ ] `test_run_stop_on_error` — `--stop-on-error` 時に最初のエラーで停止する
- [ ] `test_run_continue_on_error` — エラーセルの次のセルが実行される

---

## Phase N-1: カーネル（stdio JSON-RPC）

### N-1-A: カーネルプロセス（`kernel.rs`）

- [ ] `KernelRequest` / `KernelResponse` 型定義（JSON-RPC）
- [ ] `KernelOutput` / `OutputItem` 型定義
- [ ] `execute` メソッド実装（コード実行 → outputs 返却）
- [ ] `reset` メソッド実装（インタープリタ再初期化）
- [ ] `shutdown` メソッド実装（プロセス終了）
- [ ] `partial` レスポンス実装（`println` 出力のストリーミング）
- [ ] `forge notebook --kernel` CLI エントリポイント追加

### N-1-B: カーネルクライアント（`client.rs`）

- [ ] `KernelClient` 構造体定義（`process` / `stdin` / `stdout` / `next_id`）
- [ ] `KernelClient::spawn()` 実装（子プロセス起動）
- [ ] `KernelClient::execute(&mut self, code: &str)` 実装
- [ ] `KernelClient::reset(&mut self)` 実装
- [ ] `KernelClient::shutdown(self)` 実装
- [ ] `forge notebook run` をカーネルクライアント経由に切り替え

### N-1-C: `.fnb.out.json` 出力（`output.rs`）

- [ ] `NotebookOutput` 構造体定義（`version` / `file` / `executed_at` / `cells`）
- [ ] `CellOutput` 構造体定義（`index` / `name` / `status` / `outputs` / `duration_ms`）
- [ ] `OutputItem` serde 実装（`type` フィールドで `text`/`html`/`json`/`table`/`image`/`markdown`/`error` を判別）
- [ ] `save_output(path: &Path, output: &NotebookOutput)` 実装
- [ ] `load_output(path: &Path)` 実装（VS Code 拡張用）

### N-1-D: 追加 CLI コマンド

- [ ] `forge notebook reset <file>` 実装（カーネル再起動 + 全セル再実行）
- [ ] `forge notebook clear <file>` 実装（`.fnb.out.json` を削除）
- [ ] `forge notebook show <file>` 実装（セル一覧: 名前・行番号・ステータス）

### N-1-E: テスト

- [ ] `test_kernel_execute` — カーネルに `execute` を送り `ok` レスポンスが返る
- [ ] `test_kernel_reset` — `reset` 後に変数スコープがクリアされる
- [ ] `test_kernel_shutdown` — `shutdown` 後にプロセスが終了している
- [ ] `test_output_json_format` — `.fnb.out.json` のフォーマットが spec §5 に準拠
- [ ] `test_notebook_show` — `show` の出力にセル名と行番号が含まれる

---

## Phase N-2: VS Code 拡張

### N-2-A: `FnbSerializer`（TypeScript）

- [ ] `editors/vscode/src/notebook/serializer.ts` 作成
- [ ] `deserializeNotebook(data: Uint8Array)` — `.fnb` テキスト → `vscode.NotebookData`
- [ ] `serializeNotebook(data: vscode.NotebookData)` — `NotebookData` → `.fnb` テキスト
- [ ] `vscode.notebooks.registerNotebookSerializer("fnb", serializer)` の呼び出し

### N-2-B: `FnbKernelController`（TypeScript）

- [ ] `editors/vscode/src/notebook/controller.ts` 作成
- [ ] `vscode.notebooks.createNotebookController(...)` で作成
- [ ] セル実行時に `forge notebook --kernel` を `child_process.spawn` で起動（初回のみ）
- [ ] stdin/stdout JSON-RPC 通信の実装
- [ ] `NotebookCellOutput` への変換と表示
- [ ] `.fnb.out.json` の更新

### N-2-C: `package.json` / `extension.ts` 更新

- [ ] `contributes.notebooks` に `.fnb` を追加
- [ ] `activationEvents` に `onNotebook:fnb` を追加
- [ ] `extension.ts` から `FnbSerializer` と `FnbKernelController` を初期化

### N-2-D: 出力表示

- [ ] `hidden=true` セルのソース折り畳み対応
- [ ] `.fnb.out.json` からの既存出力読み込みと表示
- [ ] `display::table` の VS Code 表形式レンダリング
- [ ] `display::html` の WebView レンダリング

### N-2-E: テスト

- [ ] `.fnb` ファイルが VS Code で Notebook ビューとして開ける（手動確認）
- [ ] ▶ ボタンでセルを実行できる（手動確認）
- [ ] 出力がセル直下に表示される（手動確認）
- [ ] `.fnb.out.json` が正しく更新される（手動確認）

---

## Phase N-3: `display()` 組み込み関数

### N-3-A: display 関数群の実装

- [ ] `display(value)` — 型に応じて自動選択（`string`→text / `list<map>`→table / `map`→json / その他→text）
- [ ] `display::text(s: string)` 実装
- [ ] `display::html(html: string)` 実装
- [ ] `display::json(value: any)` 実装
- [ ] `display::table(rows: list<map>)` 実装
- [ ] `display::image(path: string)` 実装
- [ ] `display::markdown(md: string)` 実装

### N-3-B: `forge run` フォールバック

- [ ] ノートブック環境外では `display::*` を全て `println` に変換
- [ ] `display::table(rows)` → 各行を `[val1, val2, ...]` 形式で println

### N-3-C: カーネルモード統合

- [ ] カーネルモードで `display::*` の呼び出しを `OutputItem` として収集
- [ ] `OutputItem` を JSON-RPC レスポンスの `outputs` に含める

### N-3-D: テスト

- [ ] `test_display_text_fallback` — `forge run` で `display::text("hello")` が `"hello\n"` を出力
- [ ] `test_display_table_fallback` — `forge run` で `display::table` が各行を println
- [ ] `test_display_auto_string` — `display("hello")` が `display::text` に委譲
- [ ] `test_display_auto_list_map` — `display(list_of_maps)` が `display::table` に委譲
- [ ] `test_display_kernel_json` — カーネルモードで `display::json` が `OutputItem` を生成
- [ ] `test_display_kernel_table` — カーネルモードで `display::table` が適切な `OutputItem` を生成

---

## Phase N-4: エクスポート（オプション）

### N-4-A: ipynb エクスポート（`export.rs`）

- [ ] `export_ipynb(cells: &[Cell], output: Option<&NotebookOutput>) -> serde_json::Value` 実装
- [ ] コードセル → Jupyter `cell_type: "code"` 変換
- [ ] Markdown セル → Jupyter `cell_type: "markdown"` 変換
- [ ] `.fnb.out.json` の outputs → Jupyter `outputs` 変換

### N-4-B: CLI 統合

- [ ] `forge notebook export <file.fnb> --format ipynb` 実装

### N-4-C: テスト

- [ ] `test_export_ipynb_valid_json` — 出力が有効な JSON である
- [ ] `test_export_ipynb_cell_count` — セル数が一致している
- [ ] `test_export_ipynb_code_cell` — コードセルが正しく変換される
- [ ] `test_export_ipynb_markdown_cell` — Markdown セルが正しく変換される
