# `forge-notebook` 実装計画

> 仕様: `packages/notebook/spec.md`
> 前提: `forge-compiler` / `forge-vm` / `forge-cli` が利用可能であること

---

## フェーズ構成

```
Phase N-0:  基盤     — .fnb パーサー・forge notebook run（stdout のみ）
Phase N-1:  カーネル — stdio JSON-RPC カーネル・reset/clear/show
Phase N-2:  VS Code  — FnbSerializer + FnbKernelController + .fnb.out.json 読み書き
Phase N-3:  display  — display::text/json/table/html/markdown/image 組み込み関数
Phase N-4:  エクスポート — forge notebook export --format ipynb
Phase N-5:  可視化   — display::plot + forge/std/plot 統合（将来）
Phase N-6:  MCP/AI   — @ai: コメント構文・エラー時 AI サポート（将来）
Phase N-7:  WASM     — WasmOptions::sandboxed() によるセル隔離（将来）
```

N-0 → N-1 → N-2 → N-3 の順に実施する。N-4 以降は N-3 完成後に判断。

---

## Phase N-0: 基盤

### 目標

`.fnb` ファイルをパースしてセルリストを生成し、
`forge notebook run` で全セルを順番に実行して stdout に出力できること。

### 実装ステップ

1. **クレート作成（`crates/forge-notebook/`）**
   - `Cargo.toml`（`forge-compiler` / `forge-vm` / `serde` / `serde_json` 依存）
   - ワークスペース `Cargo.toml` に `forge-notebook` を追加

2. **`parser.rs` — `.fnb` パーサー**

   `.fnb` は通常の Markdown。` ```forge ` フェンスのコードブロックをコードセルとして抽出する。

   ```rust
   pub struct CodeCell {
       pub index: usize,
       pub name: String,        // デフォルト: "cell_<n>"
       pub hidden: bool,
       pub skip: bool,
       pub source: String,
       pub start_line: usize,
   }

   pub struct MarkdownCell {
       pub index: usize,
       pub content: String,
   }

   pub enum Cell {
       Code(CodeCell),
       Markdown(MarkdownCell),
   }

   pub fn parse_notebook(src: &str) -> Vec<Cell>
   ```

   フェンス属性の解析:
   - ` ```forge name="setup" hidden=true skip=false ` → `name`, `hidden`, `skip` を抽出
   - 属性なしの場合はデフォルト値を使用

3. **`runner.rs` — セル実行（stdout モード）**

   ```rust
   pub fn run_notebook(cells: &[Cell], opts: RunOptions) -> Vec<CellResult>
   ```

   - `skip=true` のセルをスキップ
   - `--cell <name>` 指定時は指定セルのみ実行
   - セル間で変数スコープを共有（`forge-vm` の `Interpreter` インスタンスを使い回す）
   - `--stop-on-error` 時は最初のエラーで停止
   - エラーが発生した場合はそのセルの出力にエラーメッセージを表示して次のセルへ進む
   - `?` 演算子によるエラー伝播はセル境界で止める

4. **`forge notebook run` CLI 統合**
   - `crates/forge-cli/src/main.rs` に `notebook` サブコマンドを追加
   - `forge notebook run <file.fnb>`
   - `forge notebook run <file.fnb> --cell <name>`
   - `forge notebook run <file.fnb> --stop-on-error`

### テスト方針

- セルなしの `.fnb` をパースしてもパニックしない
- コードセル 3 個を正しく抽出できる（name / hidden / skip 属性含む）
- `skip=true` のセルがスキップされる
- セル間で変数が共有される（Cell 1 の `let x = 42` を Cell 2 が参照できる）
- エラーセルの次のセルが実行される（`--stop-on-error` なし時）

---

## Phase N-1: カーネル

### 目標

`forge notebook --kernel` を子プロセスとして起動し、
stdio JSON-RPC でセルを実行できること。
`reset` / `clear` / `show` サブコマンドも実装する。

### 実装ステップ

1. **`kernel.rs` — カーネルプロセス本体**

   `forge notebook --kernel` として起動される。
   stdin から newline-delimited JSON を受け取り、stdout に JSON レスポンスを返す。

   対応メソッド:
   ```
   execute  — コードを実行し outputs を返す
   reset    — インタープリタを再初期化（スコープをクリア）
   shutdown — プロセスを終了
   ```

   ストリーミング出力（partial レスポンス）:
   - `println` の出力をリアルタイムに `{ "id": N, "status": "partial", ... }` として返す
   - 最後に `{ "id": N, "status": "ok", "duration_ms": T }` を返す

   プロトコル詳細は `spec.md §7` 参照。

2. **`client.rs` — カーネルクライアント**

   `forge notebook run` 側から子プロセスを起動・通信するクライアント:
   ```rust
   pub struct KernelClient {
       process: Child,
       stdin: BufWriter<ChildStdin>,
       stdout: BufReader<ChildStdout>,
       next_id: u64,
   }
   impl KernelClient {
       pub fn spawn() -> Result<Self>
       pub fn execute(&mut self, code: &str) -> Result<KernelResponse>
       pub fn reset(&mut self) -> Result<()>
       pub fn shutdown(self) -> Result<()>
   }
   ```

3. **`output.rs` — `.fnb.out.json` 書き込み**

   実行結果を `.fnb.out.json` に保存する:
   ```rust
   pub struct NotebookOutput { version, file, executed_at, cells }
   pub struct CellOutput    { index, name, status, outputs, duration_ms }
   pub struct OutputItem    { type_, value, ... }
   ```

   フォーマット詳細は `spec.md §5` 参照。

4. **`forge notebook reset/clear/show` CLI**

   - `forge notebook reset <file>`: カーネル再起動 + 全セル再実行
   - `forge notebook clear <file>`: `.fnb.out.json` を削除
   - `forge notebook show <file>`: セル一覧表示（名前・行番号・ステータス）

### テスト方針

- カーネルプロセスに `execute` を送り `ok` レスポンスが返る
- `reset` 後に変数スコープがクリアされる
- `shutdown` 後にプロセスが終了している
- `.fnb.out.json` のフォーマットが spec §5 に準拠している
- `forge notebook show` の出力にセル名と行番号が含まれる

---

## Phase N-2: VS Code 拡張

### 目標

既存の ForgeScript VS Code 拡張（`editors/vscode/`）に
Notebook API サポートを追加し、`.fnb` をノートブックとして開ける。

### 実装ステップ

1. **`FnbSerializer` の追加（TypeScript）**
   - `vscode.notebooks.registerNotebookSerializer("fnb", ...)` で登録
   - `.fnb` ↔ `NotebookData` 変換
   - `deserializeNotebook`: `.fnb` テキストを `NotebookData` に変換
   - `serializeNotebook`: `NotebookData` を `.fnb` テキストに変換

2. **`FnbKernelController` の追加（TypeScript）**
   - `vscode.notebooks.createNotebookController(...)` で作成
   - セル実行ボタン押下で `forge notebook --kernel` を `child_process.spawn` 起動（初回のみ）
   - stdin/stdout で JSON-RPC 通信
   - レスポンスを `NotebookCellOutput` として表示
   - `.fnb.out.json` の更新

3. **`package.json` の更新**
   - `notebookType: "fnb"` を activationEvents に追加
   - `contributes.notebooks` セクションを追加
   - `vscode.notebooks` API の利用

4. **`.fnb.out.json` の読み込み**
   - ノートブックを開いた際に既存の出力ファイルを読み込んで表示
   - `hidden=true` セルのソースを折り畳み表示

### テスト方針

- `.fnb` ファイルを VS Code で開くと Notebook ビューになる
- ▶ ボタンでセルを実行できる
- 出力がセル直下に表示される
- `.fnb.out.json` が正しく更新される

---

## Phase N-3: display() 組み込み関数

### 目標

`display()` ファミリーを ForgeScript 組み込み関数として実装し、
ノートブック環境と `forge run` 両方で動作させる。

### 実装ステップ

1. **`display` モジュールの追加（`forge-vm` または `forge-stdlib`）**

   実装する関数:
   - `display(value)` — 型に応じて自動選択
   - `display::text(s: string)` — テキスト
   - `display::html(html: string)` — HTML
   - `display::json(value: any)` — JSON
   - `display::table(rows: list<map>)` — テーブル
   - `display::image(path: string)` — 画像
   - `display::markdown(md: string)` — Markdown

2. **`display(value)` の自動選択ロジック**

   | 値の型 | フォールバック |
   |---|---|
   | `string` | `display::text` |
   | `number` / `bool` | `display::text` |
   | `list<map>` | `display::table` |
   | `map` | `display::json` |
   | その他 | `display::text`（`string(value)` で文字列化） |

3. **`forge run` フォールバック**
   - ノートブック環境外では `display::*` が全て `println` に変換される
   - `display::table(rows)` → 各行を `[val1, val2, ...]` 形式で println

4. **`OutputItem` への変換**
   - カーネルモードでは `display::*` の呼び出しを `OutputItem` として収集してレスポンスに含める

### テスト方針

- `display::text("hello")` が forge run で `"hello\n"` を出力する
- `display::table(rows)` が forge run で各行を println する
- `display(42)` が `display::text("42")` と同等に動作する
- カーネルモードで `display::json({})` が `OutputItem { type: "json", value: {} }` を生成する

---

## Phase N-4: エクスポート（オプション）

### 目標

`forge notebook export --format ipynb` で Jupyter 互換の `.ipynb` ファイルを生成する。

### 実装ステップ

1. **`export.rs` — ipynb 変換**
   - `.fnb` セルリストを Jupyter `nbformat` v4 形式の JSON に変換
   - コードセル → `cell_type: "code"`, `source: [...]`
   - Markdown セル → `cell_type: "markdown"`, `source: [...]`
   - `.fnb.out.json` の出力を `outputs` フィールドに変換

2. **CLI 統合**
   - `forge notebook export <file.fnb> --format ipynb`
   - 出力先はデフォルトで同ディレクトリ（`<name>.ipynb`）

### テスト方針

- 出力された `.ipynb` が有効な JSON である
- コードセルの数が一致している
- Jupyter で開けること（手動確認）

---

## Phase N-G: Goblet パイプライントレース出力

> 前提: N-1（カーネル）完了後に着手。N-3 と並行可。
> 設計参照: `lang/packages/goblet/v2/` スクリーンショット群（Goblet v2 UI モックアップ）

### 目標

パイプラインを含むセルを実行したとき、ノートブック出力としてパイプラインの健全性ビューを表示する。
エラーや null / NaN などのデータ汚染が検出された場合、どのステージで何件が汚染されたかを
視覚的に示し、具体的なレコードと汚染理由を提示する。

```
正常時（出力はコンパクト）:
  [source: 10] → [filter: 7] → [map: 7] → [take: 7]

異常時（詳細ビュー）:
  [source: 10 ⚠3] → [filter: 8 ⚠3] → [map: 8 ⚠3] → [take: 7 ⚠3]
  + SOURCE CODE ハイライト（汚染ステージの行が赤）
  + STAGE DETAIL（クリックで汚染レコード一覧）
```

### 設計方針

**汚染（Corruption）の定義:**
パイプラインをデータが流れるとき、以下の条件を満たすレコードを「汚染レコード」として追跡する。
- `null` / `none` なフィールドへのアクセスが発生したレコード
- 数値フィールドが `NaN` または異常値（文脈依存）のレコード
- クロージャ実行時に型エラー / panic が発生したレコード

汚染レコードはパイプラインを通り抜けることがあるため、
「最初に汚染が検出されたステージ」を記録しつつ以降のステージにも伝播する。

### 実装ステップ

1. **`forge-vm` ランタイムトレース**

   `Interpreter` にトレースモードフラグを追加し、有効時のみ以下を記録する（通常実行のオーバーヘッドなし）:

   - 各パイプラインステージの `in_count` / `out_count`
   - 汚染レコードの index / フィールド値 / 汚染理由

   トレース結果を `PipelineTrace` 構造体として返す。

2. **`OutputItem::PipelineTrace`（`forge-notebook`）**

   `.fnb.out.json` に格納するフォーマット:
   ```json
   {
     "type": "pipeline_trace",
     "pipeline_name": "names",
     "source_snippet": "let names =\n    students\n    |> filter(s => s.score >= 80)\n    |> map(s => s.name)\n    |> take(7)",
     "stages": [
       { "name": "source", "in": 10, "out": 10, "corrupted": 3 },
       { "name": "filter", "in": 10, "out":  8, "corrupted": 3 },
       { "name": "map",    "in":  8, "out":  8, "corrupted": 3 },
       { "name": "take",   "in":  8, "out":  7, "corrupted": 3 }
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

3. **VS Code WebView レンダリング（`pipeline-trace-renderer.ts`）**

   スクリーンショットのレイアウトを参考に 3 ペイン構成で実装:

   | ペイン | 内容 |
   |---|---|
   | DATA FLOW | ステージボックスを横並び。正常=白枠、汚染あり=赤枠＋`(N corrupted)` |
   | SOURCE CODE | パイプラインコードを表示。汚染ステージの行を暗赤でハイライト＋行末ラベル |
   | STAGE DETAIL | ステージクリックで展開。レコードテーブル＋汚染行ハイライト＋Corruption Details |

   汚染 0 件の場合は DATA FLOW のみコンパクト表示。

4. **`forge run` テキストフォールバック**
   ```
   [pipeline: names]  source(10) → filter(8) → map(8) → take(7)
   ⚠ 3 corrupted records detected
     #4: フィールド名が空、スコアが負の値
     #5: nameがnull
     #9: スコアがNaN
   ```

### テスト方針

- 汚染なしパイプラインで `total_corrupted == 0`、DATA FLOW のみ出力
- null フィールド含むレコードが汚染として検出される
- 汚染レコードが複数ステージに「伝播」してカウントされる
- `OutputItem::PipelineTrace` が仕様 JSON に変換される

---

## Phase N-5〜N-7: 将来フェーズ

### N-5: 可視化

- `display::plot` output タイプの追加
- `forge/std/plot` との統合（`scatter` / `line` / `histogram` / `bar` / `heatmap`）
- Plotly JSON spec を `.fnb.out.json` に格納
- VS Code WebView で Plotly.js をレンダリング
- `forge run` フォールバック: テキスト要約

### N-6: MCP + AI 統合

- `@ai:` コメント構文の認識と AI への転送
- エラー時の "AI に説明を聞く" ボタン
- MCP 経由でエラーコンテキストを送信

### N-7: WASM サンドボックス

- セル実行を `WasmOptions::sandboxed()` 相当の制約下で隔離
- タイムアウト: 500ms
- メモリ上限: 16MB
- FS アクセス: プロジェクトディレクトリのみ
- ネットワーク: デフォルト無効

---

## 依存クレート

| クレート | バージョン | 用途 |
|---|---|---|
| `forge-compiler` | workspace | パース・AST |
| `forge-vm` | workspace | セル実行 |
| `serde` | 1 | JSON シリアライズ |
| `serde_json` | 1 | `.fnb.out.json` 出力 |
| `chrono` | 0.4 | `executed_at` タイムスタンプ |
| `clap` | 4 | CLI 引数解析（forge-cli 側） |

---

## 実装後の確認

```bash
cargo test -p forge-notebook           # 全ユニットテスト通過
forge notebook run examples/tutorial.fnb
forge notebook show examples/tutorial.fnb
forge notebook clear examples/tutorial.fnb
```
