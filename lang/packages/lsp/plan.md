# `forge-lsp` 実装計画

> 仕様: `lang/packages/lsp/spec.md`
> 前提: `forge-compiler` (parser / typechecker) および `forge-goblet` (G-0〜G-3) が完成済みであること

---

## フェーズ構成

```
Phase L-0:  土台       — クレート作成・tower-lsp 骨格・stdio 疎通
Phase L-1:  診断       — didOpen/didChange・publishDiagnostics
Phase L-2:  ホバー基本 — 変数・関数・struct 型情報のホバー
Phase L-3:  |> ホバー  — Goblet 統合・パイプライン展開ホバー
Phase L-4:  定義ジャンプ — textDocument/definition（同一ファイル内）
Phase L-5:  補完        — textDocument/completion
Phase L-6:  VS Code 拡張 — editors/vscode/ 作成・シンタックスハイライト
```

L-0 → L-1 → L-2 → L-3 の順に実施。L-4/L-5/L-6 は L-3 完成後。

---

## Phase L-0: 土台

### 目標

`crates/forge-lsp` クレートを作成し、`tower-lsp` による LSP サーバー骨格を立ち上げる。
`initialize` / `shutdown` が正常に動作し、エディタとの stdio 通信を確認する。

### 実装ステップ

1. **クレート作成**
   - `crates/forge-lsp/Cargo.toml` 作成
   - 依存: `tower-lsp`, `tokio`, `serde_json`, `lsp-types`, `forge-compiler`
   - ワークスペース `Cargo.toml` に追加

2. **`main.rs` — エントリポイント**
   - `tokio::main` で非同期ランタイム起動
   - `tower_lsp::Server::new(stdin, stdout).serve(service)` でサーバー起動

3. **`backend.rs` — LSP バックエンド**
   - `#[tower_lsp::async_trait] impl LanguageServer for Backend`
   - `initialize`: `ServerCapabilities` を返す（全機能 false で初期化）
   - `initialized`: ログ出力のみ
   - `shutdown`: 正常終了

4. **疎通確認**
   - `forge-cli` の `forge lsp` サブコマンドでバイナリ起動
   - Neovim / VS Code の手動接続、または `initialize` メッセージ stdin 直接入力で確認

### テスト方針

- `Backend::new()` が panic せず構築できる
- `initialize` が `ServerCapabilities` を返す（ユニットテスト）

---

## Phase L-1: 診断

### 目標

ファイルを開く・変更するたびに `forge-compiler` でパースし、
エラーを `publishDiagnostics` として push する。
VS Code でパースエラーが赤波線として表示される。

### 実装ステップ

1. **ドキュメントキャッシュ**

   ```rust
   struct DocumentState {
       text: String,
       version: i32,
       ast: Option<Vec<Stmt>>,
       pipeline_graphs: Vec<PipelineGraph>,
   }
   type DocCache = Arc<Mutex<HashMap<Url, DocumentState>>>;
   ```

   `Backend` に `DocCache` を保持させる。

2. **`did_open` / `did_change` ハンドラ**
   - テキストをキャッシュに保存
   - `tokio::spawn` で非同期に `analyze_document` を起動

3. **`analyze_document` — 解析タスク**
   - `forge_compiler::parser::parse_source` でパース
   - パースエラーを `lsp_types::Diagnostic` に変換 (`span_to_range`)
   - `forge_compiler::typechecker::type_check_source` で型チェック
   - 型エラーも `Diagnostic` に変換
   - `client.publish_diagnostics(url, diagnostics, version)` で push
   - 成功時はキャッシュに AST を保存

4. **`span_to_range` 変換**
   - `forge_compiler::lexer::Span { line, col, end }` → `lsp_types::Range`
   - 注: LSP の行・列は 0-indexed、Span は 1-indexed なので -1 変換が必要

5. **debounce（省略可能・後回し）**
   - 高速タイプ時の過剰解析を防ぐ 300ms debounce
   - L-1 初期は省略し、L-2 以降で必要なら追加

### テスト方針

- `span_to_range` のユニットテスト（1-indexed → 0-indexed 変換）
- パースエラーが `Diagnostic { severity: Error }` に変換される
- `did_change` でキャッシュが更新される

---

## Phase L-2: ホバー（基本）

### 目標

変数・関数・struct の型情報をホバーで返す。
`|>` 演算子ホバーは L-3 で実装するため、ここでは `Unknown` を返してよい。

### 実装ステップ

1. **`ServerCapabilities` に `hover_provider: true` を追加**

2. **`hover` ハンドラ**
   - キャッシュから `DocumentState` を取得
   - `ast` が `None` の場合は `None` を返す
   - `position_to_expr(ast, position)` でカーソル位置の AST ノードを特定

3. **`position_to_expr` — 位置→AST ノード検索**
   - AST を走査し、`span.line / col` がカーソル位置を含む最小ノードを返す
   - `Stmt::Let` → バインド名・型アノテーションを取得
   - `Expr::Ident(name)` → スコープから型を解決
   - `Expr::Call` / `Expr::MethodCall` → 関数シグネチャを取得
   - `Stmt::FnDecl` → 引数・戻り値型をフォーマット

4. **型情報の Markdown フォーマット**
   - 変数: `` `name: Type` ``
   - 関数: `` `fn name(arg: T) -> U` ``
   - struct フィールド: `` `field: Type` ``

5. **スコープ解析（最小版）**
   - `Stmt::Let` をスキャンして `{ name → type_annotation }` マップを構築
   - `Stmt::FnDecl` を収集して関数シグネチャを保持
   - 初期版は同一ファイル内のトップレベルスコープのみ

### テスト方針

- `position_to_expr` が正しいノードを返す（ユニットテスト）
- `let x: number = 5` の `x` にホバーすると `` `x: number` `` が返る
- `fn foo(a: string) -> bool` の `foo` にホバーするとシグネチャが返る

---

## Phase L-3: `|>` ホバー（Goblet 統合）

### 目標

`|>` 演算子または `|>` を含む式の上にカーソルを合わせると、
`forge-goblet` による解析結果が展開された Markdown ポップアップを返す。
これが forge-lsp のキラー機能である。

### 実装ステップ

1. **`Cargo.toml` に `forge-goblet` を追加**

2. **`DocumentState` に `pipeline_graphs` を追加**
   - `analyze_document` 内で `goblet_analyze_source` も呼び出す
   - Goblet エラーは診断として push せず、ログに記録するだけでよい

3. **ホバー位置判定の拡張**
   - `position_to_expr` の結果が `Expr::MethodCall` かつパイプライン連鎖に属する場合
   - またはカーソル行に `|>` が含まれる場合

4. **パイプライン特定**
   - キャッシュの `pipeline_graphs` から、カーソル位置の span を含むグラフを検索
   - `PipelineNode::span.line` がカーソル行に一致するものを探す

5. **ホバー Markdown 生成**
   ```markdown
   **Pipeline: `names`**

   ```
   [1] students          list<Student>   Definite
    ▶  filter(score>=80)  list<Student>   MaybeEmpty
   [3] map(s => s.name)  list<string>    MaybeEmpty
   [4] take(10)          list<string>    MaybeEmpty
   ```
   ```
   - `▶` はカーソルが属するステップに付与
   - エラーステップには `⚠` を付与
   - Goblet の `Diagnostic` があれば末尾に追記

6. **`ServerCapabilities` の `hover_provider` を確認（L-2 で設定済み）**

### テスト方針

- `format_pipeline_hover(graph, cursor_node_id)` のユニットテスト
- カーソルが N2 にある場合に `▶` が N2 行に付く
- エラーノードに `⚠` が付く
- `pipeline_graphs` が空の場合は通常ホバーにフォールバック

---

## Phase L-4: 定義ジャンプ

### 目標

`textDocument/definition` を実装し、関数・変数・struct 定義へジャンプできる。
初期フェーズは同一ファイル内のみ対応。

### 実装ステップ

1. **`ServerCapabilities` に `definition_provider: true` を追加**

2. **`goto_definition` ハンドラ**
   - カーソル位置のシンボル名を特定（`position_to_ident`）
   - `symbol_table` からシンボルの定義 span を検索
   - `lsp_types::Location { uri, range }` を返す

3. **`SymbolTable` 構築**
   - `Stmt::FnDecl { name, span }` → 関数シンボル
   - `Stmt::Let { name, span }` → 変数シンボル
   - `Stmt::StructDecl { name, span }` → struct シンボル
   - `DocumentState` に `symbol_table: HashMap<String, SourceSpan>` を追加
   - `analyze_document` で AST スキャン時に構築

4. **`use` 文の解決（省略可能）**
   - `use path::to::module` で import された名前は将来対応

### テスト方針

- `build_symbol_table(ast)` のユニットテスト
- `fn foo() {}` の `foo` 呼び出し位置で definition が `fn foo` のスパンを返す

---

## Phase L-5: 補完

### 目標

`textDocument/completion` を実装し、メソッド・変数・キーワードの補完候補を返す。

### 実装ステップ

1. **`ServerCapabilities` に `completion_provider` を追加**
   - `trigger_characters: Some(vec![".".to_string(), "|".to_string()])`

2. **`completion` ハンドラ**
   - `.` の前にある式の型を解析
   - `|>` 入力後はパイプライン操作候補を返す

3. **補完候補の種類**
   - **メソッド補完**: `.` の前の型に応じて `builtin_sigs()` からメソッド一覧を返す
   - **パイプライン補完**: `|>` 後は `filter`, `map`, `find`, `fold`, `take`, `skip`, `group_by` 等
   - **変数補完**: スコープ内のローカル変数名
   - **キーワード補完**: `let`, `fn`, `if`, `else`, `match`, `for`, `in`, `return`, `struct`

4. **`CompletionItem` フォーマット**
   - `label`: メソッド名
   - `detail`: シグネチャ（例: `fn(T)->U? → find`）
   - `kind`: `Method` / `Variable` / `Keyword`

### テスト方針

- `.` 入力後に `list<T>` 型でメソッド候補が返る
- `|>` 入力後にパイプライン操作候補が返る
- ローカル変数名が補完候補に含まれる

---

## Phase L-6: VS Code 拡張

### 目標

`editors/vscode/` に VS Code 拡張を作成し、`.forge` ファイルで
LSP 機能（診断・ホバー・定義ジャンプ・補完）が動作することを確認する。

### 実装ステップ

1. **ディレクトリ構成**
   ```
   editors/vscode/
     package.json
     tsconfig.json
     src/extension.ts
     src/client.ts
     syntaxes/forge.tmLanguage.json
   ```

2. **`package.json`**
   - `contributes.languages`: `forge` 言語、`.forge` 拡張子
   - `contributes.grammars`: TextMate Grammar のパス
   - `activationEvents`: `onLanguage:forge`
   - `engines.vscode`: `^1.80.0`

3. **`extension.ts` — エントリポイント**
   - `activate`: `LanguageClient` を起動
   - `deactivate`: クライアントを停止

4. **`client.ts` — LSP クライアント**
   - `forge-lsp` バイナリを `serverOptions` として子プロセス起動
   - `clientOptions`: `documentSelector: [{ language: "forge" }]`
   - stdio 通信

5. **`syntaxes/forge.tmLanguage.json` — シンタックスハイライト**
   - キーワード: `let|fn|if|else|match|for|in|use|struct|enum|return|some|none|ok|err`
   - 演算子: `|>`, `=>`, `?`, `!`, `..`
   - 型アノテーション: `:` 以降の識別子
   - 文字列リテラル: `"..."` (補間 `{...}` を区別)
   - 数値リテラル
   - コメント: `// ...`

6. **`forge lsp` サブコマンド**
   - `forge-cli` に `lsp` サブコマンドを追加
   - `forge-lsp` のバイナリを起動するだけ（実体は `forge-lsp/main.rs`）

7. **ローカル動作確認**
   - `vsce package` でパッケージ化
   - VS Code に `.vsix` をインストールして `.forge` ファイルを開く
   - 診断・ホバー・補完が動作することを目視確認

### テスト方針

- `package.json` が valid JSON で必須フィールドを持つ
- TextMate Grammar が valid JSON
- `vsce package` が成功する（拡張ビルド確認）

---

## 依存クレート

| クレート | バージョン | 用途 |
|---------|-----------|------|
| `tower-lsp` | 0.20 | LSP サーバーフレームワーク |
| `tokio` | 1 (features: rt-multi-thread, macros) | 非同期ランタイム |
| `lsp-types` | 0.95 | LSP 型定義 |
| `serde_json` | 1 | JSON シリアライズ |
| `forge-compiler` | workspace | AST / parser / typechecker |
| `forge-goblet` | workspace | パイプライン解析（L-3 以降） |

---

## 実装後の確認コマンド

```bash
cargo test -p forge-lsp           # ユニットテスト全通過
cargo build -p forge-lsp          # バイナリビルド確認
forge lsp                         # LSP サーバー起動（stdin 待機）
```
