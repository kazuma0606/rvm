# `forge-lsp` タスク一覧

> 仕様: `lang/packages/lsp/spec.md`
> 計画: `lang/packages/lsp/plan.md`
> 実装対象: `crates/forge-lsp/` / `editors/vscode/`

---

## Phase L-0: 土台（クレート・サーバー骨格）

### L-0-A: クレート作成
- [x] `crates/forge-lsp/` ディレクトリ作成
- [x] `crates/forge-lsp/Cargo.toml` 作成
- [x] `tower-lsp` / `tokio` / `lsp-types` / `serde_json` / `forge-compiler` を依存に追加
- [x] ワークスペース `Cargo.toml` に `forge-lsp` を追加
- [x] `crates/forge-lsp/src/main.rs` 作成
- [x] `tokio::main` + stdio サーバー起動を実装
- [x] `crates/forge-lsp/src/backend.rs` 作成

### L-0-B: LSP 骨格
- [x] `Backend` に `LanguageServer` トレイト実装
- [x] `initialize` ハンドラ実装
- [x] `ServerCapabilities` を返す初期実装を追加
- [x] `initialized` ハンドラ実装
- [x] `shutdown` ハンドラ実装

### L-0-C: CLI 統合
- [x] `forge-cli/Cargo.toml` に `forge-lsp` を追加
- [x] `forge lsp` サブコマンドを `forge-cli/main.rs` に追加

### L-0-D: L-0 テスト
- [x] `test_backend_new`
- [x] `test_initialize_capabilities`

---

## Phase L-1: 診断

### L-1-A: ドキュメントキャッシュ
- [x] `DocumentState` を定義
- [x] `DocumentState` に `text` / `version` / `ast` / `pipeline_graphs` を保持
- [x] `DocCache = Arc<Mutex<HashMap<Url, DocumentState>>>` を定義
- [x] `Backend` に `doc_cache: DocCache` を追加

### L-1-B: span 変換
- [x] `span_to_range(span: &Span) -> lsp_types::Range` を実装
- [x] `range_contains_position(range: &Range, pos: &Position) -> bool` を実装
- [x] `span_to_range` の 1-indexed → 0-indexed 変換を確認
- [x] `range_contains_position` の境界判定を確認

### L-1-C: didOpen / didChange / didClose
- [x] `did_open` でドキュメントをキャッシュして `analyze_document` を起動
- [x] `did_change` でキャッシュを更新して `analyze_document` を再実行
- [x] `did_close` でキャッシュと diagnostics をクリア

### L-1-D: `analyze_document`
- [x] `analyze_document(url, text, version)` を実装
- [x] `parse_source` のパースエラーを `Diagnostic` に変換
- [x] `type_check_source` の型エラーを `Diagnostic` に変換
- [x] 成功時に `ast` をキャッシュへ保存
- [x] `client.publish_diagnostics(url, diagnostics, version)` を実行

### L-1-E: L-1 テスト
- [x] `test_parse_error_to_diagnostic`
- [x] `test_type_error_to_diagnostic`
- [x] `test_did_open_updates_cache`
- [x] `test_no_diagnostics_on_valid_source`

---

## Phase L-2: ホバー（基本）

### L-2-A: ServerCapabilities 更新
- [x] `hover_provider: Some(HoverProviderCapability::Simple(true))` を追加

### L-2-B: シンボル収集
- [x] `collect_symbols(stmts: &[Stmt]) -> SymbolMap` を実装
- [x] `SymbolMap = HashMap<String, SymbolInfo>` を定義
- [x] `SymbolInfo` に kind / type_display / span を保持
- [x] `Stmt::FnDecl` から関数シグネチャを収集
- [x] `Stmt::Let` から変数情報を収集
- [x] `Stmt::StructDecl` から struct 情報を収集
- [x] `DocumentState` に `symbol_map` を追加して `analyze_document` で更新

### L-2-C: 位置 → ノード検索
- [x] `find_node_at(stmts: &[Stmt], pos: Position) -> Option<HoverTarget>` を実装
- [x] `HoverTarget` enum を追加
- [x] `HoverTarget::Ident(String)` を追加
- [x] `HoverTarget::FnCall(String)` を追加
- [x] `HoverTarget::FieldAccess { obj, field }` を追加
- [x] `HoverTarget::PipeOp` を追加
- [x] `Expr::Ident` の span を元に `HoverTarget::Ident` を返す
- [x] `Expr::MethodCall` / `Expr::Call` の span を元に `HoverTarget::FnCall` を返す
- [x] `|>` に対応する `HoverTarget::PipeOp` 判定を追加

### L-2-D: hover ハンドラ
- [x] `hover` ハンドラを実装
- [x] `HoverTarget::Ident(name)` で変数ホバーを返す
- [x] `HoverTarget::FnCall(name)` で関数シグネチャを返す
- [x] `HoverTarget::PipeOp` は L-3 実装前は `None` を返す動作を用意
- [x] AST が無い場合は `None` を返す

### L-2-E: L-2 テスト
- [x] `test_collect_fn_symbols`
- [x] `test_collect_let_symbols`
- [x] `test_find_node_at_ident`
- [x] `test_hover_returns_fn_signature`
- [x] `test_hover_returns_var_type`

---

## Phase L-3: `|>` ホバー（Goblet 統合）

### L-3-A: Goblet 依存追加
- [x] `crates/forge-lsp/Cargo.toml` に `forge-goblet` を追加
- [x] `analyze_document` 内で `goblet_analyze_source` を呼び出す
- [x] `pipeline_graphs` をキャッシュへ保存する
- [x] Goblet 解析エラーは diagnostics に push せずログのみとする

### L-3-B: パイプライン特定
- [x] `find_pipeline_at(graphs: &[PipelineGraph], pos: Position) -> Option<(&PipelineGraph, NodeId)>` を実装
- [x] `PipelineNode::span` からカーソル位置に対応するノードを探索
- [x] 複数グラフがある場合に最も適切なグラフを選択

### L-3-C: ホバー Markdown 生成
- [x] `format_pipeline_hover(graph: &PipelineGraph, cursor_node: NodeId) -> String` を実装
- [x] `[N] label  output_type  state` 形式で整形
- [x] カーソル位置のノードに `▶` を付与
- [x] `NodeStatus::Error` のノードに `⚠` を付与
- [x] `graph.diagnostics` を末尾に追記

### L-3-D: hover ハンドラ拡張
- [x] `HoverTarget::PipeOp` で `find_pipeline_at` → `format_pipeline_hover` を呼ぶ
- [x] パイプラインが見つからない場合は通常ホバーにフォールバック
- [x] `hover_provider` が有効なことを再確認

### L-3-E: L-3 テスト
- [x] `test_format_pipeline_hover_marks_cursor_node`
- [x] `test_format_pipeline_hover_marks_error_node`
- [x] `test_find_pipeline_at_correct_graph`
- [x] `test_pipe_hover_fallback_when_no_pipeline`

---

## Phase L-4: 定義ジャンプ

### L-4-A: ServerCapabilities 更新
- [x] `definition_provider: Some(OneOf::Left(true))` を追加

### L-4-B: SymbolTable
- [x] `SymbolTable = HashMap<String, SymbolLocation>` を定義
- [x] `SymbolLocation` に `uri` / `span` を保持
- [x] `build_symbol_table(uri, stmts)` を実装
- [x] `DocumentState` に `symbol_table` を追加して `analyze_document` で更新

### L-4-C: `goto_definition`
- [x] `goto_definition` ハンドラを実装
- [x] `find_ident_at(ast, pos)` でカーソル位置の識別子を取得
- [x] `symbol_table` から定義位置を検索
- [x] `GotoDefinitionResponse::Scalar(Location { uri, range })` を返す
- [x] 見つからない場合は `None` を返す

### L-4-D: L-4 テスト
- [x] `test_build_symbol_table_fn`
- [x] `test_build_symbol_table_let`
- [x] `test_goto_definition_fn_call`
- [x] `test_goto_definition_not_found`

---

## Phase L-5: 補完

### L-5-A: ServerCapabilities 更新
- [x] `completion_provider: Some(CompletionOptions { trigger_characters: Some(vec![".".to_string(), "|".to_string()]), ..Default::default() })` を追加

### L-5-B: 補完コンテキスト判定
- [x] `completion_context(text: &str, pos: Position) -> CompletionCtx` を実装
- [x] `CompletionCtx` enum: `AfterDot(String)` / `AfterPipe` / `General`
- [x] カーソル前後のトークンからコンテキストを判定

### L-5-C: 補完候補生成
- [x] `method_completions(type_display: &str) -> Vec<CompletionItem>` を実装
- [x] `pipeline_completions() -> Vec<CompletionItem>` を実装
- [x] `keyword_completions() -> Vec<CompletionItem>` を実装
- [x] `local_var_completions(symbol_map: &SymbolMap) -> Vec<CompletionItem>` を実装

### L-5-D: completion ハンドラ
- [x] `completion` ハンドラを実装
- [x] `AfterDot(type_str)` で `method_completions(type_str)` を返す
- [x] `AfterPipe` で `pipeline_completions()` を返す
- [x] `General` で `keyword_completions()` + `local_var_completions()` を返す

### L-5-E: L-5 テスト
- [x] `test_completion_ctx_after_dot`
- [x] `test_completion_ctx_after_pipe`
- [x] `test_method_completions_list`
- [x] `test_pipeline_completions_include_filter`
- [x] `test_keyword_completions_include_let`

---

## Phase L-6: VS Code 拡張

### L-6-A: ディレクトリとファイル作成
- [x] `editors/vscode/` ディレクトリ作成
- [x] `editors/vscode/package.json` 作成
- [x] `editors/vscode/tsconfig.json` 作成
- [x] `editors/vscode/src/extension.ts` 作成
- [x] `editors/vscode/src/client.ts` 作成

### L-6-B: `package.json` 設定
- [x] `contributes.languages`: `{ id: "forge", extensions: [".forge"] }`
- [x] `contributes.grammars` を設定
- [x] `activationEvents`: `["onLanguage:forge"]`
- [x] `main`: `"./out/extension.js"`
- [x] `dependencies`: `vscode-languageclient` を追加

### L-6-C: LSP クライアント実装
- [x] `forge-lsp` バイナリの `serverOptions` を設定
- [x] `documentSelector: [{ language: "forge" }]` を設定
- [x] `new LanguageClient(...)` でクライアント起動を実装

### L-6-D: シンタックスハイライト
- [x] `forge.tmLanguage.json` を作成
- [x] `scopeName: "source.forge"` を設定
- [x] キーワードパターンを追加
- [x] 演算子パターン `|>`, `=>`, `?`, `!`, `..` を追加
- [x] 文字列リテラルと補間 `{...}` を追加
- [x] 数値リテラル `\b\d+(\.\d+)?\b` を追加
- [x] コメント `//.*$` を追加
- [x] 型アノテーション `: [A-Za-z][A-Za-z0-9_<>?!]*` を追加

### L-6-E: ビルドと動作確認
- [x] `npm install` を実行
- [x] `npm run compile` で TypeScript をコンパイル
- [x] `vsce package` で `.vsix` を生成
- [x] VS Code に `.vsix` をインストールして `.forge` で診断・ホバー・補完を目視確認

### L-6-F: L-6 テスト
- [x] `package.json` が valid JSON
- [x] `forge.tmLanguage.json` が valid JSON
- [x] `vsce package` が成功

---

## 進捗サマリ

| Phase | タスク数 | 完了 |
|---|---:|---:|
| L-0 | 13 | 13 |
| L-1 | 16 | 16 |
| L-2 | 14 | 14 |
| L-3 | 13 | 13 |
| L-4 | 8 | 8 |
| L-5 | 12 | 12 |
| L-6 | 16 | 16 |
| **合計** | **92** | **92** |
