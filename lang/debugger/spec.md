# ForgeScript デバッガー仕様

> バージョン対象: v0.3.0 以降
> 作成: 2026-04-19
> 関連: `web-ui/v2/idea.md`（DX ロードマップ）、`lang/di/spec.md`（Anvil ミドルウェア統合）

---

## 概要

ForgeScript エコシステムとして本格的なデバッガーを提供する。単なる `println` デバッグを超え、IDE 統合・ブレークポイント・ステップ実行・変数ウォッチを実現することを目標とする。

実装は段階的に進め、最終的に **DAP（Debug Adapter Protocol）**を通じて VS Code のデバッグ UI とフル統合する。

---

## 設計方針

### なぜ本格的なデバッガーが必要か

ForgeScript の開発中に繰り返し発生した問題：

- エラーメッセージに行番号・ファイル名がなく、原因箇所の特定に時間がかかる
- WASM 実行が SSR フェーズでサイレントに失敗し、フォールバックの原因が追えない
- HTTP ハンドラがハングしても何も出力されず、どこで止まっているか分からない
- `.bloom` → `.forge` 変換の中間状態が見えず、コンパイラのバグを追いにくい
- `use` 二重定義のようなエラーがどのファイルの何行目で起きているか不明

これらは「**何が起きているか見えない**」という共通の根本原因を持つ。デバッガーはエコシステムの信頼性を根本から高めるインフラである。

### 層ごとの対応

```
ForgeScript インタープリタ層
  → 行番号付きエラー・スタックトレース・変数ウォッチ

Bloom コンパイラ層
  → 中間 AST/コード出力・コンパイルトレース

WASM 実行層
  → SSR エラーの可視化・forge_log の強化

HTTP サーバー層（Anvil）
  → リクエスト/レスポンスログ・ミドルウェアトレース
```

---

## Phase DBG-1: エラー品質の改善

最も低コストで最も効果が高い改善。

### DBG-1-1: エラーメッセージへの位置情報追加

すべてのランタイムエラー・コンパイルエラーにファイル名・行番号・列番号を付与する。

**現状:**
```
Error: undefined variable 'count'
```

**目標:**
```
Error: undefined variable 'count'
  --> src/usecase/counter.forge:12:5
   |
12 |     count = count + 1
   |     ^^^^^ この変数は定義されていません
   |
hint: `state count = 0` で宣言してください
```

実装方針:
- `Span { file: String, line: usize, col: usize }` を AST ノード全体に付与
- パーサーがトークン位置を記録し、インタープリタがエラー時に `Span` を参照して出力

### DBG-1-2: スタックトレース

関数呼び出しのネストをランタイムが記録し、エラー時にトレースを出力する。

```
Error: index out of bounds: len=3, index=5
  --> src/domain/user.forge:8:16

Stack trace:
  0: get_user        src/domain/user.forge:8
  1: handle_request  src/interface/handler.forge:24
  2: dispatch        src/main.forge:3
```

### DBG-1-3: Bloom コンパイルエラーの改善

`.bloom` ファイルのコンパイルエラーに `.bloom` 側の行番号を表示する。生成された `.forge` の行番号ではなく、元のソースを指す。

```
Bloom compile error: unknown directive '{#iff}'
  --> src/components/counter.bloom:15:1
   |
15 | {#iff count > 0}
   | ^^^^^^^^^^^^^^^^ '#iff' は未知のディレクティブです。'#if' を使用してください
```

---

## Phase DBG-2: トレース・ログ強化

### DBG-2-1: `forge run --verbose`

実行トレースを標準エラー出力に流すフラグ。

```bash
forge run src/main.forge --verbose
```

出力例:
```
[TRACE] entering fn main                  src/main.forge:1
[TRACE] use bloom/dom                     src/main.forge:1
[TRACE]   → resolved: packages/bloom/dom.forge
[TRACE] evaluating fn increment           src/counter.forge:5
[TRACE]   state count: 0 → 1
[TRACE] calling dom::set_text             src/counter.forge:6
[TRACE]   args: ("text_root_0", "1")
```

### DBG-2-2: Bloom コンパイル中間出力

`.bloom` → AST → `.forge` の各段階を出力するフラグ。

```bash
forge build --web --dump-ast src/components/counter.bloom
forge build --web --dump-forge src/components/counter.bloom
```

`--dump-ast`: テンプレート AST を JSON で出力
`--dump-forge`: 生成された `.forge` コードを出力（現在は `dist/generated/` に書き出されているが、コンパイル失敗時も途中経過を出力）

### DBG-2-3: Anvil リクエストログミドルウェア

`use logger()` 1 行で有効になるリクエスト/レスポンスログ。

```forge
use anvil/middleware.{ logger }

let app = Anvil::new()
    .use(logger())
    .get("/", home_handler)
```

出力例:
```
[Anvil] GET  /counter         200  12ms
[Anvil] GET  /components/counter_page.wasm  200  1ms  (22KB)
[Anvil] POST /api/users       422  3ms
[Anvil] GET  /undefined-path  404  0ms
```

### DBG-2-4: WASM 実行トレース

SSR フェーズでの WASM 実行結果を詳細に出力するフラグ。

```bash
forge serve --wasm-trace
```

出力例:
```
[WASM] loading: dist/components/counter_page.wasm (22KB)
[WASM] calling __forge_init()
[WASM] commands: 3
[WASM]   SET_TEXT  "text_root_0_1_1_1_0"  "0"
[WASM]   ADD_LISTENER  "node_root_0_1_1_0"  "click"  "decrement"
[WASM]   ADD_LISTENER  "node_root_0_1_1_2"  "click"  "increment"
[WASM] SSR complete: injected 3 commands into HTML
```

---

## Phase DBG-3: REPL

```bash
forge repl
```

インタラクティブに ForgeScript の式を評価できる。

```
ForgeScript REPL v0.3.0
Type :help for help, :quit to exit

forge> let x = 10
forge> x * 2
20
forge> struct Point { x: number, y: number }
forge> let p = Point { x: 1, y: 2 }
forge> p.x
1
forge> :type p
Point
forge> :trace on
forge> fn double(n: number) -> number { n * 2 }
forge> double(5)
[TRACE] entering fn double
[TRACE]   n = 5
[TRACE]   returning 10
10
```

REPL コマンド:

| コマンド | 説明 |
|---|---|
| `:type <expr>` | 式の型を表示 |
| `:trace on/off` | 実行トレースの切り替え |
| `:load <file>` | ファイルを読み込んでスコープに展開 |
| `:reset` | スコープをリセット |
| `:help` | ヘルプ表示 |
| `:quit` | 終了 |

---

## Phase DBG-4: DAP（Debug Adapter Protocol）統合

VS Code のデバッグ UI とフル統合する。`forge-lsp` と同様に `forge-dap` バイナリとして実装し、VS Code 拡張から起動する。

### 対応機能

| 機能 | 説明 |
|---|---|
| ブレークポイント | 任意の行で実行を一時停止 |
| 条件付きブレークポイント | `count > 5` など条件を指定 |
| ステップ実行 | Step Over / Step Into / Step Out |
| 変数ウォッチ | スコープ内の変数一覧・値の確認 |
| 式の評価 | 一時停止中に任意の式を評価 |
| 呼び出しスタック | 現在のコールスタックを表示 |
| ホットリロード統合 | ファイル変更時にデバッグセッションを維持 |

### VS Code での使用イメージ

`.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "forge",
      "request": "launch",
      "name": "Run ForgeScript",
      "program": "${workspaceFolder}/src/main.forge"
    },
    {
      "type": "forge",
      "request": "launch",
      "name": "Serve Anvil (debug)",
      "program": "${workspaceFolder}/src/main.forge",
      "mode": "serve",
      "port": 8080
    }
  ]
}
```

### `forge-dap` のアーキテクチャ

```
VS Code
  ↕ DAP (JSON over stdio)
forge-dap（バイナリ）
  ↕ 内部 API
forge-vm（インタープリタ）
  ↕ debug hook
実行中の ForgeScript
```

`forge-vm` に以下のデバッグフックを追加する:
- `on_statement(span: Span)` — 文を実行する前に呼ばれる
- `on_enter_fn(name: &str, args: &[Value])` — 関数に入る前
- `on_exit_fn(name: &str, ret: &Value)` — 関数から出る前
- `on_assign(name: &str, value: &Value)` — 変数代入時

`forge-dap` はこれらのフックを受け取り、ブレークポイントと照合して DAP のイベントに変換する。

### Bloom デバッグ対応

ブレークポイントを `.bloom` の行に設定した場合、ソースマップを通じて生成された `.forge` の行番号に変換する。

```
counter.bloom:8  →  dist/generated/components/counter_page.forge:24
```

---

## Phase DBG-5: ホットリロード統合

デバッグセッションを維持したままファイル変更を反映する。DX ロードマップ（`web-ui/v2/idea.md`）のホットリロードとデバッガーを統合する。

- `.forge` / `.bloom` の HTML 変更 → セッションを維持してリロード
- `.bloom` の script 変更（WASM 再コンパイル必要）→ 再コンパイル後にセッションを再接続
- ブレークポイントはリロード後も保持

---

## 実装ロードマップ

```
[DBG-1] エラー品質の改善
  → 行番号・スタックトレース・Bloom エラー改善
  → 実装コスト: 低〜中
  → 効果: 最大（今すぐ痛みを解消）

[DBG-2] トレース・ログ強化
  → --verbose / --dump-ast / Anvil logger / WASM trace
  → 実装コスト: 低〜中
  → 効果: 高

[DBG-3] REPL
  → インタラクティブ評価
  → 実装コスト: 中
  → 効果: 中（プロトタイピング・学習コスト低下）

[DBG-4] DAP 統合（forge-dap）
  → ブレークポイント・ステップ実行・変数ウォッチ
  → 実装コスト: 高
  → 効果: 高（本格的な IDE 体験）

[DBG-5] ホットリロード統合
  → デバッグセッション維持
  → 実装コスト: 中（DBG-4 後）
  → 効果: 高
```

| フェーズ | 実装コスト | 優先度 |
|---|---|---|
| DBG-1: エラー品質 | 低〜中 | 最高 |
| DBG-2: トレース強化 | 低〜中 | 高 |
| DBG-3: REPL | 中 | 中 |
| DBG-4: DAP 統合 | 高 | 高（長期目標） |
| DBG-5: ホットリロード統合 | 中 | 中（DBG-4 後） |
