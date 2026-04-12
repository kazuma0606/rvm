# ForgeScript Notebook (.fnb) 仕様

> バージョン: 0.1.0
> ファイル拡張子: `.fnb`（ForgeScript NoteBook）
> 参照: ROADMAP.md §[16], dev/design-v3.md §ノートブック

---

## 1. 設計方針

- **Markdown ファースト**: `.fnb` は有効な Markdown ファイル。そのまま GitHub / プレビューで読める
- **共有カーネル**: Jupyter 方式。セル間で変数・関数が共有される。上から順に実行
- **出力分離**: コード実行結果は `.fnb.out.json` に保存。`.fnb` 本体は常に git 差分がクリーン
- **ZeroMQ 不要**: VS Code との通信は stdio JSON-RPC のみ。追加デーモン不要
- **`forge run` 互換**: `display()` は `println` にフォールバック。ノートブックなしでも動く
- **配置規約**: プロジェクト内のノートブックは `/notebook` ディレクトリに置く

---

## 2. ファイル形式

### 2-1. 基本構造

`.fnb` ファイルは通常の Markdown ファイル。コードブロックの言語タグが `forge` のものがコードセルとして実行される。

```
# タイトル

説明テキスト（Markdown セル）

```forge
let x = 42
println(x)
```

さらに説明。

```forge name="setup" hidden=true
let base_url = "https://api.example.com"
```
```

### 2-2. セル種別

| 種別 | 判定条件 | 説明 |
|---|---|---|
| **Markdown セル** | コードブロック以外のすべて | レンダリングのみ。実行なし |
| **コードセル** | ` ```forge ` で始まるブロック | ForgeScript として実行 |

### 2-3. コードセルのメタデータ（フェンス属性）

フェンス開始行にスペース区切りで属性を記述する:

```
```forge [name="<id>"] [hidden=true] [skip=true]
```

| 属性 | 型 | デフォルト | 意味 |
|---|---|---|---|
| `name` | `string` | `cell_<n>` | セルの識別子。`forge notebook run --cell <name>` で単体実行可能 |
| `hidden` | `bool` | `false` | VS Code でセルのソースを折り畳む。出力は表示 |
| `skip` | `bool` | `false` | `forge notebook run` 時にこのセルをスキップ |

**例:**

```
```forge name="db_setup" hidden=true
let conn = connect("localhost", 5432, "postgres", "test", "mydb")?
```
```

---

## 3. 実行モデル

### 3-1. 共有カーネル（Jupyter 方式）

```
Cell 1: let x = 10          ← 実行 → x がカーネルスコープに追加
Cell 2: let y = x * 2       ← 実行 → y = 20（Cell 1 の x を参照）
Cell 3: println(y)           ← 実行 → "20" を出力
```

- セル間で `let` / `state` / `fn` / `struct` などすべての束縛が共有される
- カーネルは `forge notebook --kernel` サブプロセスとして起動し、セッション中は維持される
- **再実行**: セルを単独で再実行した場合、それ以降のセルは自動再実行しない（Jupyter と同仕様）
- **リセット**: `forge notebook reset <file>` でカーネルを再起動し全セルを最初から再実行

### 3-2. エラー処理

- コードセルでエラーが発生した場合、そのセルの出力にエラーメッセージを表示して停止
- `?` 演算子によるエラー伝播はセル境界で止まる（次のセルには伝播しない）
- `--stop-on-error` オプション時は最初のエラーでカーネル全体を停止

---

## 4. `display()` 組み込み関数

### 4-1. API

```forge
// リッチ出力（ノートブック環境でのみ有効。forge run では println にフォールバック）
display(value)                    // 型に応じて自動選択
display::text(s: string)          // テキスト
display::html(html: string)       // HTML（VS Code の WebView でレンダリング）
display::json(value: any)         // JSON（折りたたみ可能なツリー表示）
display::table(rows: list<map>)   // テーブル（カラム名は map のキーから自動取得）
display::image(path: string)      // PNG/JPEG/SVG（ファイルパス or data URI）
display::markdown(md: string)     // Markdown テキスト
```

### 4-2. `display(value)` の自動選択ルール

| 値の型 | フォールバック挙動 |
|---|---|
| `string` | `display::text` |
| `number` / `bool` | `display::text` |
| `list<map>` | `display::table` |
| `map` | `display::json` |
| その他 | `display::text`（`string(value)` で文字列化） |

### 4-3. `forge run` でのフォールバック

ノートブック環境外（`forge run`）では、すべての `display::*` が `println` に変換される:

```forge
display::table(rows)
// ↓ forge run では
// [row1_val1, row1_val2, ...]
// [row2_val1, ...]
// のように println で出力
```

---

## 5. 出力ファイル形式（`.fnb.out.json`）

### 5-1. 概要

コードセルの実行結果は `.fnb.out.json` に保存する。`.fnb` 本体には書き込まない。

```
notebook/
  tutorial.fnb           ← ソース（git 管理）
  tutorial.fnb.out.json  ← 出力（.gitignore 推奨 or git 管理どちらも可）
```

### 5-2. フォーマット

```json
{
  "version": 1,
  "file": "tutorial.fnb",
  "executed_at": "2026-04-12T10:00:00Z",
  "cells": [
    {
      "index": 0,
      "name": "cell_0",
      "status": "ok",
      "outputs": [
        {
          "type": "text",
          "value": "42\n"
        }
      ],
      "duration_ms": 12
    },
    {
      "index": 1,
      "name": "db_setup",
      "status": "ok",
      "outputs": [],
      "duration_ms": 45
    },
    {
      "index": 2,
      "name": "cell_2",
      "status": "error",
      "outputs": [
        {
          "type": "error",
          "message": "variable 'x' is not defined",
          "line": 3
        }
      ],
      "duration_ms": 2
    }
  ]
}
```

### 5-3. output の `type` 一覧

| type | 内容 |
|---|---|
| `"text"` | `display::text` / `println` の出力。`value: string` |
| `"html"` | `display::html` の出力。`value: string`（HTML） |
| `"json"` | `display::json` の出力。`value: any`（JSON値） |
| `"table"` | `display::table` の出力。`columns: list<string>`, `rows: list<list<any>>` |
| `"image"` | `display::image` の出力。`mime: string`, `data: string`（base64 or path） |
| `"markdown"` | `display::markdown` の出力。`value: string` |
| `"error"` | 実行エラー。`message: string`, `line?: number` |

---

## 6. CLI: `forge notebook`

### 6-1. コマンド一覧

```
forge notebook run <file.fnb>              # 全セルを順番に実行
forge notebook run <file.fnb> --cell <name>  # 指定セルのみ実行
forge notebook run <file.fnb> --stop-on-error # エラーで停止
forge notebook reset <file.fnb>            # カーネルリセット + 全セル再実行
forge notebook clear <file.fnb>            # .fnb.out.json を削除
forge notebook show <file.fnb>             # セル一覧を表示（名前・行番号・ステータス）
forge notebook export <file.fnb> --format ipynb  # .ipynb 形式にエクスポート（後期）
```

### 6-2. `forge notebook run` の動作フロー

```
1. .fnb ファイルをパース → セルリストを生成
2. forge notebook --kernel を子プロセスとして起動（stdio JSON-RPC）
3. skip=true のセルをスキップ
4. 各セルを上から順番に kernel へ送信
5. 結果を受け取り .fnb.out.json に書き込む
6. カーネルプロセスを終了
```

---

## 7. カーネルプロトコル（stdio JSON-RPC）

### 7-1. 概要

`forge notebook --kernel` は stdin から JSON リクエストを受け取り、stdout に JSON レスポンスを返す。1リクエスト1行（newline-delimited JSON）。

### 7-2. リクエスト

```json
{ "id": 1, "method": "execute", "params": { "code": "let x = 42\nprintln(x)" } }
{ "id": 2, "method": "reset",   "params": {} }
{ "id": 3, "method": "shutdown","params": {} }
```

### 7-3. レスポンス

```json
{ "id": 1, "status": "ok", "outputs": [{ "type": "text", "value": "42\n" }], "duration_ms": 8 }
{ "id": 1, "status": "error", "outputs": [{ "type": "error", "message": "...", "line": 1 }], "duration_ms": 2 }
{ "id": 2, "status": "ok", "outputs": [] }
```

### 7-4. ストリーミング出力

長時間実行セルでは `println` の出力をリアルタイムに返す（`partial` レスポンス）:

```json
{ "id": 4, "status": "partial", "outputs": [{ "type": "text", "value": "step 1\n" }] }
{ "id": 4, "status": "partial", "outputs": [{ "type": "text", "value": "step 2\n" }] }
{ "id": 4, "status": "ok",      "outputs": [], "duration_ms": 320 }
```

---

## 8. VS Code Notebook 拡張

### 8-1. 既存拡張への統合

既存の ForgeScript VS Code 拡張（シンタックスハイライト）に以下を追加する:

| 追加コンポーネント | VS Code API | 役割 |
|---|---|---|
| `FnbSerializer` | `vscode.notebooks.registerNotebookSerializer("fnb", ...)` | `.fnb` ↔ `NotebookData` 変換 |
| `FnbKernelController` | `vscode.notebooks.createNotebookController(...)` | セル実行・カーネル管理 |

### 8-2. 表示

`.fnb` を VS Code で開くと:

```
┌─────────────────────────────────────────────────────┐
│ # ForgeScript チュートリアル                          │  ← Markdown セル（レンダリング）
│                                                      │
│ 基本的な変数の使い方を学びます。                       │
├─────────────────────────────────────────────────────┤
│ ▶  [forge]                                          │  ← コードセル
│    let x = 42                                        │
│    println(x)                                        │
│ ─────────────────────────────────────────────────── │
│    42                                               │  ← 出力（.fnb.out.json から）
└─────────────────────────────────────────────────────┘
```

- Markdown セル: VS Code の標準 Markdown レンダラーで表示
- コードセル: ForgeScript シンタックスハイライト + ▶ 実行ボタン
- 出力: セル直下に表示。`display::table` は VS Code の表形式でレンダリング
- `hidden=true` のセル: ソース部分を折り畳み表示

### 8-3. カーネル接続フロー

```
VS Code 拡張
  ↓ セル実行ボタン押下
  ↓ forge notebook --kernel を child_process.spawn で起動（初回のみ）
  ↓ stdin に JSON-RPC リクエストを書き込む
  ↓ stdout から JSON-RPC レスポンスを受け取る
  ↓ NotebookCellOutput として表示
  ↓ .fnb.out.json を更新
```

---

## 9. ディレクトリ構成

### 9-1. プロジェクト内のノートブック

```
my-project/
  notebook/                        ← ノートブックディレクトリ
    tutorial.fnb                   ← チュートリアル
    tutorial.fnb.out.json          ← 実行結果（.gitignore 推奨）
    data_analysis.fnb
    data_analysis.fnb.out.json
  src/
    main.forge
  forge.toml
```

### 9-2. `.gitignore` 推奨設定

```gitignore
# ForgeScript Notebook 出力
notebook/**/*.fnb.out.json
```

出力を git 管理したい場合（再現性の記録として）は追加しなくてよい。

---

## 10. 使用例

### 10-1. データ分析ノートブック

```markdown
# ユーザー統計分析

データベースからユーザーデータを取得して集計します。

```forge name="setup" hidden=true
use crucible.*
let conn = connect("localhost", 5432, "postgres", "test", "mydb")?
```

## データ取得

```forge name="fetch"
let users = query_raw(conn, "SELECT * FROM users ORDER BY created_at")?
display::table(users)
```

## 集計

```forge name="stats"
let total = users.len()
let by_month = users
    |> group_by(u => u["created_at"][..7])
    |> map(g => { "month": g["key"], "count": g["values"].len() })
    |> order_by(g => g["month"])

println("総ユーザー数: {total}")
display::table(by_month)
```
```

### 10-2. チュートリアル形式

```markdown
# ForgeScript 入門

## 変数と型

```forge
let name = "Alice"
let age  = 30
println("{name} は {age} 歳です")
```

`T?` 型を使うと Optional を表現できます。

```forge
let maybe: string? = none
println(maybe)          // none

let value = maybe ?? "デフォルト"
println(value)          // デフォルト
```
```

---

## 11. 実装スコープ（フェーズ分割）

| フェーズ | 内容 |
|---|---|
| **N-0: 基盤** | `.fnb` パーサー（Markdown → セルリスト）・`forge notebook run`（stdout のみ）|
| **N-1: カーネル** | `forge notebook --kernel`（stdio JSON-RPC）・`forge notebook reset/clear/show` |
| **N-2: VS Code** | `FnbSerializer` + `FnbKernelController`・`.fnb.out.json` の読み書き |
| **N-3: display()** | `display::text/json/table/html/markdown/image` 組み込み関数 |
| **N-4: エクスポート** | `forge notebook export --format ipynb`（Jupyter 互換） |
