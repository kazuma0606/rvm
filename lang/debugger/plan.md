# ForgeScript デバッガー 実装計画

> 仕様: `lang/debugger/spec.md`
> 前提: `forge-vm`（インタープリタ）・`forge-lsp`・`editors/vscode` が存在すること

---

## フェーズ構成

```
Phase DBG-1: エラー品質の改善（Span・スタックトレース）
Phase DBG-2: トレース・ログ強化（--verbose / --dump-ast / Anvil logger）
Phase DBG-3: REPL
Phase DBG-4: DAP（Debug Adapter Protocol）統合
Phase DBG-5: ホットリロード統合
```

---

## Phase DBG-1: エラー品質の改善

### 目標

すべてのエラーにファイル名・行番号・列番号が付与され、原因箇所を即座に特定できること。

### 実装ステップ

1. **`Span` 型の追加（`crates/forge-compiler/src/`）**
   ```rust
   pub struct Span {
       pub file:  String,
       pub line:  usize,
       pub col:   usize,
   }
   ```
   AST の全ノード（`Expr` / `Stmt`）に `span: Option<Span>` フィールドを追加。

2. **レキサー拡張**
   - トークン生成時に `(line, col)` を記録
   - `Token { kind, value, span }` に変更

3. **パーサー拡張**
   - AST ノード生成時にトークンの `Span` を引き継ぐ

4. **インタープリタのエラー出力改善**
   - `EvalError` に `span: Option<Span>` を追加
   - エラー表示時に `rustc` スタイルのソース引用を出力
   - `hint:` メッセージを一般的なエラーに追加

5. **コールスタックの記録**
   - `fn` 呼び出し時に `CallFrame { fn_name, span }` をスタックに積む
   - エラー時に `Stack trace:` として出力

6. **Bloom コンパイルエラーの改善**
   - `.bloom` の行番号を生成 `.forge` の行番号にマッピングするソースマップを生成
   - コンパイルエラー時に `.bloom` 側の行番号を表示

7. **テスト**

---

## Phase DBG-2: トレース・ログ強化

### 目標

`--verbose` / `--dump-ast` などのフラグで実行・コンパイルの内部状態を可視化できること。Anvil のリクエストログが 1 行で有効化できること。

### 実装ステップ

1. **`forge run --verbose`**
   - インタープリタに `DebugMode::Verbose` フラグを追加
   - 文実行前・関数入退出・変数代入時に `stderr` へ `[TRACE]` を出力
   - `forge-cli` の `run` サブコマンドに `--verbose` フラグを追加

2. **`forge build --web --dump-ast`**
   - `bloom-compiler` の `plan_from_bloom_source` が AST を JSON でダンプするオプションを追加
   - `forge-cli` の `build --web` に `--dump-ast` / `--dump-forge` フラグを追加

3. **Anvil リクエストログミドルウェア（`packages/anvil/src/middleware/logger.forge`）**
   - `logger()` 関数を実装: リクエスト受信・レスポンス送信のタイムスタンプ・ステータス・所要時間を出力
   - `Anvil::use(middleware)` のミドルウェアチェーンを実装（未実装なら合わせて実装）

4. **WASM 実行トレース（`--wasm-trace`）**
   - `forge serve --wasm-trace` で WASM ロード・コマンド実行を `stderr` に出力
   - `vm_bloom_render_wasm` の各ステップにトレースログを追加

5. **テスト**

---

## Phase DBG-3: REPL

### 目標

`forge repl` でインタラクティブに ForgeScript の式・文を評価できること。

### 実装ステップ

1. **`forge-cli` に `repl` サブコマンドを追加**

2. **REPL ループの実装**
   - 入力を 1 行ずつ読んでインタープリタに渡す
   - 複数行入力の検出（`{` が閉じていない場合に続行）
   - 式の評価結果を自動的に表示（`println` なしで）

3. **REPL コマンドの実装**
   - `:type <expr>` — 型情報を表示
   - `:trace on/off` — 実行トレースの切り替え
   - `:load <file>` — ファイルをスコープに読み込む
   - `:reset` — スコープリセット
   - `:help` / `:quit`

4. **入力補完・履歴**
   - `rustyline` クレートで行編集・履歴・補完を実装

5. **テスト**

---

## Phase DBG-4: DAP（Debug Adapter Protocol）統合

### 目標

VS Code のデバッグ UI からブレークポイント・ステップ実行・変数ウォッチが使えること。

### 実装ステップ

1. **`forge-vm` へのデバッグフック追加**
   ```rust
   pub trait DebugHook {
       fn on_statement(&mut self, span: &Span, env: &Env);
       fn on_enter_fn(&mut self, name: &str, args: &[(String, Value)]);
       fn on_exit_fn(&mut self, name: &str, ret: &Value);
       fn on_assign(&mut self, name: &str, value: &Value, span: &Span);
   }
   ```
   インタープリタが各フックを呼ぶよう拡張。

2. **`crates/forge-dap` クレートの新設**
   - DAP の JSON メッセージを stdio で送受信する基盤を実装
   - `dap` クレート（`rust-dap` 等）を利用
   - 対応するリクエスト: `initialize` / `launch` / `setBreakpoints` / `continue` / `next` / `stepIn` / `stepOut` / `variables` / `evaluate`

3. **ブレークポイント管理**
   - `setBreakpoints` リクエストでブレークポイントを登録
   - `on_statement` フック内でブレークポイントと `Span` を照合
   - 一致したら `stopped` イベントを VS Code に送信

4. **変数ウォッチ**
   - `variables` リクエストに対してインタープリタの現在スコープを返す
   - ネストした struct / list / map も展開して返す

5. **ソースマップ統合（Bloom）**
   - `.bloom` → `.forge` のソースマップを `forge-dap` が参照
   - `.bloom` 上のブレークポイントを生成 `.forge` の行番号に変換

6. **VS Code 拡張への統合（`editors/vscode`）**
   - `package.json` に `debuggers` コントリビューションを追加
   - `launch.json` スキーマの定義
   - `forge-dap` バイナリのパス設定

7. **テスト**

---

## Phase DBG-5: ホットリロード統合

### 目標

デバッグセッションを維持したまま `.forge` / `.bloom` の変更を反映できること。

### 実装ステップ

1. **ファイル監視の実装（`forge serve --watch`）**
   - `notify` クレートでファイル変更を監視
   - `.forge` / `.html` 変更時: インタープリタ再起動 + WebSocket でブラウザに通知
   - `.bloom` の HTML 変更時: SSR 再生成のみ
   - `.bloom` の script 変更時: WASM 再コンパイル（バックグラウンド）+ 完了後にブラウザ通知

2. **デバッグセッションの再接続**
   - ファイル変更によるリロード後も DAP セッションを維持
   - ブレークポイントをリロード後に再登録

3. **テスト**

---

## テスト方針

- DBG-1 / DBG-2: ユニットテスト（エラーメッセージの形式検証・トレース出力の検証）
- DBG-3: REPL の入出力 E2E テスト
- DBG-4: DAP プロトコルの統合テスト（モック VS Code クライアントを使用）
- DBG-5: ファイル変更 → リロード → ブレークポイント再登録のシナリオテスト
