# ForgeScript デバッガー タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: エラーに行番号が付き、REPL で式を評価でき、
>             VS Code からブレークポイント・ステップ実行・変数ウォッチが使えること

---

## Phase DBG-1: エラー品質の改善

### DBG-1-A: `Span` 型の追加

- [x] `Span { file: String, line: usize, col: usize }` を `forge-compiler` に追加
- [x] `Expr` の全バリアントに `span: Option<Span>` を追加
- [x] `Stmt` の全バリアントに `span: Option<Span>` を追加

### DBG-1-B: レキサー拡張

- [x] `Token` に `span: Span` フィールドを追加
- [x] レキサーがトークン生成時に行番号・列番号を記録する

### DBG-1-C: パーサー拡張

- [x] パーサーが AST ノード生成時にトークンの `Span` を引き継ぐ

### DBG-1-D: インタープリタのエラー出力改善

- [x] `RuntimeError` の診断コンテキストに `span: Option<Span>` 相当を追加
- [x] エラー表示に rustc スタイルのソース引用を実装
- [x] 一般的なエラーに `hint:` メッセージを追加
- [x] `undefined variable` エラーに類似変数名のサジェストを追加

### DBG-1-E: コールスタックの記録

- [x] `CallFrame { fn_name: String, span: Span }` を定義
- [x] `fn` 呼び出し時にスタックへ `CallFrame` を積む
- [x] エラー時に `Stack trace:` として全フレームを出力

### DBG-1-F: Bloom コンパイルエラーの改善

- [x] `.bloom` → `.forge` のソースマップを生成（行番号対応表）
- [x] コンパイルエラー時に `.bloom` 側の行番号・列番号を表示
- [x] 未知のディレクティブに `hint:` を追加（例: `{#iff}` → `{#if}` を提案）

### DBG-1-G: テスト

- [x] テスト: `test_error_span` — エラーメッセージに正しい行番号が含まれること
- [x] テスト: `test_stack_trace` — ネストした関数呼び出しのトレースが正しいこと
- [x] テスト: `test_bloom_error_span` — Bloom コンパイルエラーの行番号が .bloom 側を指すこと
- [x] E2E テスト: `error_messages.forge` — 各エラーの出力形式確認

---

## Phase DBG-2: トレース・ログ強化

### DBG-2-A: `forge run --verbose`

- [x] インタープリタに verbose フラグを追加（実装名: `verbose_mode`）
- [x] 文実行前に `[TRACE]` を `stderr` に出力
- [x] 関数入退出時に `[TRACE]` を出力
- [x] 変数代入時に `[TRACE]` を出力（旧値→新値）
- [x] `forge-cli` の `run` サブコマンドに `--verbose` / `-v` フラグを追加

### DBG-2-B: `forge build --web --dump-ast`

- [x] `bloom-compiler` に AST JSON ダンプオプションを追加
- [x] `forge-cli` の `build --web` に `--dump-ast` フラグを追加
- [x] `forge-cli` の `build --web` に `--dump-forge` フラグを追加
- [x] コンパイル失敗時も途中経過を出力する

### DBG-2-C: Anvil リクエストログミドルウェア

- [x] `packages/anvil/src/middleware_test` ディレクトリを作成
- [x] `logger.forge` を実装（メソッド・パス・ステータス・所要時間を出力。既存構成のため `src/middleware.forge` に実装）
- [x] `Anvil::use(middleware)` のミドルウェアチェーンを実装
- [x] テスト: `test_logger_middleware` — ログ出力の形式確認

### DBG-2-D: `forge serve` コマンド新設（DBG-2-D の前提）

`forge serve` は現時点で未実装。`forge run` のサーバー特化エイリアスとして新設する。
将来の `--watch`（ホットリロード）・`--wasm-trace` はこのコマンドに統合する。

- [x] `forge-cli` に `serve` サブコマンドを追加
- [x] 内部実装は `forge run` と同じ（エントリポイントを実行）
- [x] `forge serve [path]` で Anvil サーバーを起動できること
- [x] `--port <n>` フラグでポート指定（デフォルト 8080）
- [x] `forge serve --help` でサーバー用途向けのヘルプを表示

### DBG-2-E: WASM 実行トレース

- [x] `forge serve --wasm-trace` フラグを追加
- [x] WASM ロード時のファイルサイズ・パスをログ出力
- [x] `__forge_init` / `__forge_attach` 呼び出しをログ出力
- [x] コマンドバッファの内容（opcode・引数）をログ出力
- [x] SSR 失敗時のエラー詳細をログ出力（現状はサイレント失敗）

### DBG-2-F: テスト

- [x] テスト: `test_verbose_output` — `--verbose` の出力形式確認
- [x] テスト: `test_dump_ast` — `--dump-ast` の JSON 出力確認
- [x] テスト: `test_serve_command` — `forge serve` でサーバーが起動すること
- [x] E2E テスト: Anvil サーバーで logger ミドルウェアが出力されること
- [x] E2E テスト: `forge serve --wasm-trace` でトレースログが出力されること

---

## Phase DBG-3: REPL

### DBG-3-A: `repl` サブコマンド

- [x] `forge-cli` に `repl` サブコマンドを追加
- [x] `rustyline` クレートを依存関係に追加
- [x] 行編集・履歴・基本補完を有効化

### DBG-3-B: REPL ループ

- [x] プロンプト `forge> ` の表示
- [x] 入力を 1 行ずつインタープリタに渡す
- [x] 複数行入力の検出（`{` が閉じていない場合に `....> ` プロンプトで継続）
- [x] 式の評価結果を自動表示（`println` なしで）
- [x] エラー時もセッションを継続（exit しない）

### DBG-3-C: REPL コマンド

- [x] `:type <expr>` — 式の型を表示
- [x] `:trace on/off` — 実行トレースの切り替え
- [x] `:load <file>` — ファイルをスコープに読み込む
- [x] `:reset` — スコープリセット
- [x] `:help` — コマンド一覧表示
- [x] `:quit` / `:q` / Ctrl+D — 終了

### DBG-3-D: テスト

- [x] テスト: `test_repl_expression` — 式評価の結果が正しい
- [x] テスト: `test_repl_multiline` — 複数行入力の継続が正しい
- [x] テスト: `test_repl_commands` — `:type` / `:reset` / `:load` が動作する

---

## Phase DBG-4: DAP 統合

### DBG-4-A: `forge-vm` へのデバッグフック追加

- [x] `DebugHook` trait を定義
- [x] `on_statement(span, env)` フックを追加
- [x] `on_enter_fn(name, args)` フックを追加
- [x] `on_exit_fn(name, ret)` フックを追加
- [x] `on_assign(name, value, span)` フックを追加
- [x] インタープリタが各フックを呼ぶよう拡張

### DBG-4-B: `crates/forge-dap` クレートの新設

- [x] `forge-dap` クレートを workspace に追加
- [x] DAP JSON メッセージの stdio 送受信基盤を実装
- [x] `initialize` リクエストへの応答
- [x] `launch` リクエストでインタープリタを起動
- [x] `disconnect` リクエストへの応答

### DBG-4-C: ブレークポイント

- [x] `setBreakpoints` リクエストでブレークポイントを登録・解除
- [x] `on_statement` フックでブレークポイントと `Span` を照合
- [x] 一致時に `stopped` イベントを送信
- [x] 条件付きブレークポイント（`condition` フィールドの式評価）

### DBG-4-D: ステップ実行

- [x] `continue` リクエストで実行を再開
- [x] `next`（Step Over）の実装
- [x] `stepIn`（Step Into）の実装
- [x] `stepOut`（Step Out）の実装

### DBG-4-E: 変数ウォッチ

- [x] `scopes` リクエストにスコープ一覧を返す
- [x] `variables` リクエストにスコープ内変数を返す
- [x] struct / list / map のネスト展開
- [x] `evaluate` リクエストで任意の式を評価して返す

### DBG-4-F: Bloom ソースマップ統合

- [x] `forge-dap` が `.bloom` ソースマップを読み込む
- [x] `.bloom` 上のブレークポイントを `.forge` 行番号に変換
- [x] `stopped` イベントで `.bloom` の行番号を返す

### DBG-4-G: VS Code 拡張への統合

- [x] `package.json` に `debuggers` コントリビューションを追加
- [x] `launch.json` スキーマ（`type: "forge"`）を定義
- [x] `forge-dap` バイナリのパス設定を追加
- [ ] E2E テスト: VS Code でブレークポイントを設定して実行が停止すること

### DBG-4-H: テスト

- [x] テスト: `test_dap_initialize` — initialize シーケンスが正しい
- [x] テスト: `test_dap_breakpoint` — ブレークポイントで停止する
- [x] テスト: `test_dap_step_over` — Step Over が正しく動作する
- [x] テスト: `test_dap_variables` — 変数一覧が正しく返る
- [x] テスト: `test_dap_evaluate` — 任意の式を評価できる

---

## Phase DBG-5: ホットリロード統合

### DBG-5-A: ファイル監視

- [x] `notify` クレートを依存関係に追加
- [x] `forge serve --watch` フラグを追加（`forge serve` 新設後に実装）
- [x] `.forge` / `.html` 変更時にインタープリタを再起動
- [x] WebSocket サーバーを組み込みブラウザに変更通知を送る
- [x] `.bloom` の HTML 変更時に SSR のみ再生成
- [x] `.bloom` の script 変更時にバックグラウンドで WASM 再コンパイル

### DBG-5-B: デバッグセッションの再接続

- [x] ファイル変更後もDAP セッションを維持
- [x] リロード後にブレークポイントを再登録

### DBG-5-C: テスト

- [x] テスト: `test_watch_forge_reload` — .forge 変更時に再起動が走る
- [x] テスト: `test_watch_bloom_ssr` — .bloom HTML 変更時に SSR のみ再生成
- [ ] E2E テスト: ファイル変更 → ブラウザ自動リロードの動作確認（手動確認として残す）

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| DBG-1: エラー品質の改善 | 20 / 20 |
| DBG-2: トレース・ログ強化 | 28 / 28 |
| DBG-3: REPL | 17 / 17 |
| DBG-4: DAP 統合 | 34 / 35 |
| DBG-5: ホットリロード統合 | 10 / 11 |
| **合計** | **109 / 111** |

> DBG-5 残り 1 件: E2E テスト（ファイル変更 → ブラウザ自動リロード）は実際のブラウザが必要なため手動確認として残す。
