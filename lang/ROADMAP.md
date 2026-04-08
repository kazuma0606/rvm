# ForgeScript ロードマップ

> 最終更新: 2026-04-05
> テスト総数: 293本（全通過）
> 言語仕様: v0.2.0（ジェネリクス・forge.toml は v0.3.0 予定）

---

## 凡例

| 記号 | 意味 |
|---|---|
| ✅ | 実装済み・テスト通過 |
| 📐 | 設計済み・未実装（spec / plan / tasks あり） |
| 💭 | 設計中（design-v3.md に方針あり・spec 未作成） |
| ⬜ | 未設計 |

---

## 現在地（実装済み）

### コア言語（forge run）✅

| 機能 | 詳細 | テスト数 |
|---|---|---|
| Lexer | 全トークン・演算子・リテラル | 15本 |
| Parser / AST | 全構文（let/state/const/fn/if/for/while/match/closure） | 29本 |
| インタープリタ | ツリーウォーカー・スコープチェーン・クロージャ・? 演算子 | 44本 |
| 型システム基礎 | T? / T! / list\<T\> / 型推論 / match 網羅性 | 13本 |
| コレクション API | 30種（map/filter/fold/order_by 等） | 13本 |
| 組み込み関数 | print/println/string/number/float/len/type_of | （上記に含む）|
| forge run | ファイル実行 | 29本（E2E）|
| forge repl | 対話型 REPL | — |
| forge check | 型チェックのみ実行 | 3本（E2E）|
| forge help | ヘルプ表示 | — |

### トランスパイラ（forge build）✅

| 機能 | 詳細 | テスト数 |
|---|---|---|
| B-0: クレート準備 | forge-transpiler / forge transpile / forge build | — |
| B-1: 基本変換 | let/fn/if/for/while/match/文字列補間/組み込み関数 | 7本（snapshot）|
| B-2: 型システム | T?/T!/Option/Result/? 演算子 | 1本（snapshot）|
| B-3: クロージャ | Fn推論（FnMutは未完了・TODO済み） | 2本（snapshot）|
| B-4: コレクション | Vec + イテレータチェーン変換 | 2本（snapshot）|
| B-5: 型定義変換 | struct/data/enum/impl/trait/mixin → Rust変換 | 14本（snapshot）|
| B-6: モジュール変換 | use/when/use raw/test → Rust変換 | 7本（snapshot）|
| B-7: async/await | async fn 自動昇格・tokio・Box::pin再帰 | 実装済み |
| B-8: typestate変換 | PhantomData パターン・制約チェック付き | 実装済み |
| ラウンドトリップ | forge run == forge build + 実行 の等価確認 | 18本 |

### 型定義（forge run）✅

| 機能 | 詳細 | テスト数 |
|---|---|---|
| struct | 定義・impl・@derive(Debug/Clone/Eq/Hash/Ord/Default/Accessor/Singleton) | T-1: 11本 |
| enum | Unit/Tuple/Struct バリアント・match パターンマッチ | T-2: 6本 |
| trait / mixin | 純粋契約・デフォルト実装・impl Trait for Type | T-3: 7本 |
| data | 全 derive 自動付与・validate ブロック | T-4: 5本 |
| typestate | 状態遷移・ランタイム状態チェック | T-5: 4本 |

### モジュールシステム（forge run）✅

| 機能 | 詳細 |
|---|---|
| M-0: ファイル解決 | `use ./path/module.symbol` / エイリアス / ワイルドカード |
| M-1: pub 可視性 | 公開・非公開アクセス制御 |
| M-2: mod.forge | ファサード・`pub use` re-export・深さ警告 |
| M-3: 外部クレート | `use serde` → Cargo.toml 自動追記 |
| M-4: 静的解析 | 循環参照検出・未使用インポート警告・シンボル衝突検出 |
| M-5: when | 条件付きコンパイル（platform/feature/env/test） |
| M-6: use raw | 生 Rust コード埋め込み（`forge run` ではスキップ） |
| M-7: REPL | `:modules` / `:reload` / `:unload` |

### テストシステム（forge test）✅

| 機能 | 詳細 |
|---|---|
| `test "..." { }` | インラインテストブロック（FT-1） |
| assert / assert_eq / assert_ne / assert_ok / assert_err | アサーション組み込み関数 |
| `forge test <file>` | テスト収集・実行・結果表示（✅/❌・exit code） |
| `--filter <pattern>` | テスト名の部分一致フィルタ |
| テストスコープ分離 | 各テストで state がリセット |

### パッケージ・エコシステム ✅

| パッケージ | 詳細 |
|---|---|
| **Anvil HTTP フレームワーク** | Express スタイル。ルーティング・ミドルウェア・CORS・JSON パーサー・logger 実装済み（A-1〜A-5 完了） |
| **`[dependencies]` ローカルパス解決** | `forge.toml` の `dep = { path = "..." }` → `use depname/module.*` で参照可能 |
| **`examples/anvil`** | Anvil を外部依存として使うサンプルサーバ（/health・/echo・/users/:id の3エンドポイント） |

### ツール・周辺 ✅

| 機能 | 詳細 |
|---|---|
| VS Code シンタックスハイライト | TextMate grammar / ~/.vscode/extensions/ にローカルインストール済み |
| UAT ディレクトリ | UAT/hello.forge で動作確認済み |

---

## 設計済み・未実装 📐

### トランスパイラ残タスク 📐

- **参照**: `forge/transpiler/tasks.md`
- **内容**:
  - B-3: FnOnce の判定が tail position 限定（spawn 未実装のため実用上問題なし）
- **ブロッカー**: なし

---

## 設計中・方針確定 💭

以下は `dev/design-v3.md` に設計方針が記録されているが、spec / tasks は未作成。

| 機能 | 設計状況 | 参照 |
|---|---|---|
| `async` / `await` | 方針確定（.await検出で自動昇格・tokio自動挿入） | design-v3.md |
| 名前付き引数・デフォルト引数 | 方針確定（Builderパターン自動生成） | design-v3.md |
| REPL コード補完 | 方針確定（3段階: 静的→動的→型対応） | future_task |
| Playground→REPL→Local ワークフロー | 方針確定（:save コマンド） | future_task |
| セルフホスティング | 方針確定（rustc依存は維持・コンパイラをForgeで書く） | future_task |

---

## 設計済み・未実装 📐（言語機能拡張）

| 機能 | 仕様 | 備考 |
|---|---|---|
| E-1: `\|>` パイプ演算子 | `lang/extends/spec.md` | Lexer + Parser のみ。インタープリタ変更なし |
| E-2: `?.` / `??` | `lang/extends/spec.md` | T? 型へのオプショナルアクセス |
| E-3: 演算子オーバーロード | `lang/extends/spec.md` | `impl` ブロック内 `operator +` |
| E-4: 非同期クロージャ / `spawn` | `lang/extends/spec.md` | B-7 async/await の完成 |
| E-5: `const fn` | `lang/extends/spec.md` | コンパイル時定数関数 |
| E-6: ジェネレータ / `yield` | `lang/extends/spec.md` | E-4 完成後。`generate<T>` 型 |
| ジェネリクス `<T>` | `lang/generics/spec.md` | Anvil の前提。v0.3.0 |
| `forge.toml` パッケージ管理（完全版） | `lang/package/spec.md` | レジストリ・バージョン解決・forge build 統合。v0.3.0（ローカルパス依存は ✅ 実装済み） |

## 設計中・方針確定 💭（標準ライブラリ拡充）

現状の `forge/std` に不足している実用関数。`app_ideas.md` の素案を実現するために必要な最小セット。

### 第2層：実用ライン（近日対応）

| モジュール | 追加する関数 | 必要な素案 |
|---|---|---|
| `forge/std/env` | `env(key)` / `env_or(key, default)` | forge-env・forge-cron・全 CLI ツール |
| `forge/std/process` | `args()` / `exit(code)` / `run(cmd, args)` | forge-todo・forge-pipeline 等 CLI全般 |
| `forge/std/json` | `stringify(value)` | forge-webhook・全 REST API（現状は文字列補間で代替） |
| `forge/std/fs`（拡張） | `list_dir` / `delete_file` / `make_dir` / `path_join` | forge-migrate・forge-config |
| `forge/std/string`（拡張） | `split` / `trim` / `starts_with` / `ends_with` / `replace` / `to_upper` / `to_lower` | 全般 |
| 組み込み関数（時間計測） | `time_ms()` / `time_ns()` | ベンチマーク・パフォーマンス計測全般 |

### 第3層：パッケージとして切り出す（標準ライブラリには含めない）

| パッケージ | 内部実装 | 概要 |
|---|---|---|
| **forge-http** | `reqwest` ラッパー | HTTP クライアント。`get` / `post` / `put` / `delete` / `request` + レスポンス型 |
| forge-time | `chrono` ラッパー | `now()` / `format_date` / `parse_date` |
| forge-crypto | `sha2` / `base64` ラッパー | `hash_sha256` / `base64_encode` / `base64_decode` |

> **設計方針**: `forge/std/net` はサーバー側 TCP（Anvil の土台）を担当。
> クライアント側 HTTP は責務が異なるため `packages/forge-http` として独立させる。
> Anvil（サーバー）と forge-http（クライアント）が対になる構造。

### forge-http パッケージ API 案

```forge
use forge_http.{ get, post, put, delete, request, Response }

// シンプルな GET
let res = get("https://api.example.com/users")?

// ヘッダー付き POST
let res = post("https://api.example.com/orders")
    .header("Authorization", "Bearer {token}")
    .json(payload)
    .send()?

// レスポンス操作
let body   = res.text()
let parsed = res.json()?
let status = res.status          // number
let ok     = res.ok              // bool (2xx)
```

## 設計済み・未実装 📐（パッケージ）

| パッケージ | 仕様 | 概要 |
|---|---|---|
| **forge-http** | `packages/forge-http/spec.md`（未作成） | HTTP クライアント（reqwest ラッパー）。get / post / put / delete + Response 型 |
| forge-grpc | `packages/forge-grpc/spec.md`（未作成） | gRPC サービス定義 DSL（tonic ラッパー） |
| forge-graphql | `packages/forge-graphql/spec.md`（未作成） | GraphQL スキーマ DSL（async-graphql ラッパー） |

## 設計中・方針確定 💭（トランスパイラ最適化）

詳細は `lang/transpiler_perf.md` を参照。

| 最適化 | 概要 | 優先度 |
|---|---|---|
| イテレータ融合 | filter/map/fold チェーンを中間 Vec なしの単一パスに変換 | ◎ |
| `Vec::with_capacity` 自動挿入 | map の出力に容量事前確保を自動付与 | ◎ |
| クロージャ静的展開 | `Box<dyn Fn>` を使わず常に `impl Fn` でモノモーフィズム展開 | ◎ |
| 文字列補間の事前確保 | 補間変数の長さ合算で `String::with_capacity` を自動挿入 | ○ |
| 小 struct への Copy 自動付与 | 全フィールドが数値型の小さい struct に `#[derive(Copy)]` | ○ |

## 設計中・方針確定 💭（DX ツール）

| 機能 | 設計方針 | 備考 |
|---|---|---|
| `forge fmt` | AST から整形出力（Prettier スタイル）。CI/CD で必須 | parser 安定後に着手 |
| `forge check`（強化版） | 現行の基礎型チェックに加え、未使用変数・到達不能コード・型推論エラーの詳細表示 | 現行は基礎のみ ✅ |
| **forge-mcp** | ForgeScript 専用 MCP サーバ。`parse_file` / `type_check` / `run_snippet` / `search_symbol` / `get_spec_section` を tool として公開。AI コーディング支援の不確実性を低減 | 設計方針確定 |

## 設計中・方針確定 💭（配布・インストール）

| 機能 | 設計方針 | 備考 |
|---|---|---|
| GitHub Releases バイナリ配布 | Linux x86_64 / ARM64 / macOS / Windows の pre-built バイナリを CI でビルド・配布 | GitHub Actions で自動化 |
| `install.sh` インストーラー | rustup スタイルのワンライナー。OS 検出 → バイナリ DL → `~/.forge/bin/` 配置 → PATH 追記 | Rust 不要でインストール可能に |
| `cargo install` 対応 | `cargo install --git <repo> forge-cli` で Rust 環境ならすぐ使えるように | 短期で対応可能 |
| `forge upgrade` コマンド | インストール済みバイナリの自動アップデート | install.sh と連動 |

## 設計中・方針確定 💭（ノートブック）

| 機能 | 設計方針 | 備考 |
|---|---|---|
| `.fnb` 形式 | Markdown ベース。コードブロックを ForgeScript セルとして実行。出力は `.fnb.out.json` に分離（git 差分を清潔に保つ） | Quarto `.qmd` に近いコンセプト |
| VS Code Notebook 拡張 | 既存 VS Code 拡張に Notebook kernel を追加。ZeroMQ 不要・依存なし | WASM より先に着手可能 |
| `forge notebook <file>` | `.fnb` ファイルをノートブックとして実行するコマンド | `forge run` の拡張 |
| `display()` 組み込み | `display::html` / `display::json` / `display::table`。`forge run` では `println` に fallback | ノートブック向けリッチ出力 |
| Jupyter 互換（後期） | `.ipynb` エクスポート対応（`forge nbconvert`）。Colab・JupyterHub での利用を可能にする | `.fnb` 設計後に追加 |

## 未設計 ⬜

| 機能 | 備考 |
|---|---|
| `forge test` FT-2 | コンパニオンファイル・ディレクトリ走査 |
| LSP（言語サーバー） | `forge check` の型チェッカーを転用。ホバー・補完・定義ジャンプ・インラインエラー。Rust より親切な DX を目標 |
| Playground（WASM） | forge-wasm クレートが必要 |
| `forge generate` | コードジェネレータ |
| Tree-sitter grammar | シンタックスハイライトの拡張（より高精度な構文解析） |
| forge.toml レジストリ / `forge publish` | パッケージ公開・バージョン解決・`forge.lock` |

---

## 推奨実装順序

```
✅ 完了済み
  ├─ [1] struct / enum / trait / mixin / data / typestate 実装
  ├─ [2] モジュールシステム実装（M-0〜M-7）
  ├─ [3] forge test + test "..." ブロック（FT-1）
  ├─ [4] ジェネリクス <T>
  ├─ [5] forge.toml ローカルパス依存（`dep = { path = "..." }`）
  ├─ [6] Anvil HTTP フレームワーク（A-1〜A-5 全完了）
  └─ [7] examples/anvil サンプルサーバ

  次のステップ（stdlib 拡充 + パッケージ）
  │
  ├─ [8] forge/std 第2層（env / process / stringify / fs拡張 / string拡張 / time_ms・time_ns）
  │       args() / env() / stringify() が揃うと CLI・API ツールが書けるようになる
  │       time_ms() / time_ns() は実装コスト極小・ベンチマーク体験に直結
  │
  ├─ [9] forge-http パッケージ（reqwest ラッパー）
  │       get / post / put / delete / request + Response 型
  │       → forge/std/net（サーバー）と対になるクライアント側 HTTP
  │
  DX 強化
  │
  ├─ [10] Linux インストール対応
  │        cargo install --git 対応 → GitHub Releases バイナリ → install.sh
  │
  ├─ [11] forge build: Rust コード保存（target/forge_rs/）
  │        build 時にデフォルトで target/forge_rs/ にコピーを残す
  │        Rust 学習材料・トランスパイラデバッグ・脱出ハッチとして機能
  │        --no-keep-rs フラグで CI 向けに抑制可能
  │
  ├─ [12] forge fmt（フォーマッタ）
  │        CI/CD で必須。AST から整形出力
  │
  ├─ [13] forge check 強化
  │        未使用変数・到達不能コード・詳細エラー表示
  │
  ├─ [14] forge-mcp（MCP サーバ）
  │        parse_file / type_check / run_snippet / search_symbol / get_spec_section
  │        → AI コーディング支援の不確実性を低減
  │
  言語仕様安定後
  │
  ├─ [15] LSP（言語サーバー）
  │        forge check の型チェッカーを転用
  │        ホバー・補完・定義ジャンプ
  │
  ├─ [16] ノートブック `.fnb` + VS Code Notebook 拡張
  │        forge notebook コマンド・display() 組み込み
  │        後から Jupyter 互換（.ipynb エクスポート）を追加可能
  │
  ├─ [17] forge.toml 完全版（レジストリ・バージョン解決・forge build 統合）
  ├─ [18] forge-grpc / forge-graphql
  ├─ [19] Playground（WASM）
  └─ [20] セルフホスティング
```

---

## ファイル構成

```
lang/                           ← 言語仕様・ドキュメント
  ROADMAP.md                    ← 本ファイル
  v0.1.0/
    spec_v0.0.1.md              ← 実装済み言語仕様
    plan.md / tasks.md          ← Phase 0〜4（全完了）
  typedefs/
    spec.md / plan.md / tasks.md ← T-1〜T-5（全完了）
  modules/
    spec.md / plan.md / tasks.md ← M-0〜M-7（全完了）
  transpiler/
    spec.md / plan.md / tasks.md ← B-0〜B-8（全完了）
  tests/
    spec.md / plan.md / tasks.md ← FT-1（完了）・FT-2（未着手）
  generics/
    spec.md                     ← ジェネリクス仕様（📐 設計済み）
  package/
    spec.md                     ← forge.toml 仕様（📐 設計済み）
  syntax/
    spec.md / plan.md / tasks.md ← S-1（完了）
  future_task_20260330.md       ← 将来タスク一覧
  app_ideas.md                  ← ForgeScript で作ると有用なアプリケーション素案集
  extend_idea.md                ← 他言語から取り込みたい言語拡張アイデア集
  transpiler_perf.md            ← トランスパイラ最適化アイデア（イテレータ融合・with_capacity 等）
  extends/
    spec.md                     ← E-1〜E-6 言語拡張仕様（|> / ?. / operator / spawn / const fn / yield）
    plan.md                     ← E-1〜E-6 実装計画・フェーズ構成
    tasks.md                    ← E-1〜E-6 タスク一覧（全 84 タスク・未着手）

crates/                         ← RVM 実装（Rust クレート群）
  forge-compiler/
  forge-vm/
  forge-stdlib/
  forge-transpiler/
  forge-cli/

packages/                       ← ForgeScript パッケージ群
  anvil/
    spec.md                     ← Anvil HTTP フレームワーク仕様（✅ 実装済み）
    src/                        ← anvil.forge / request.forge / response.forge / middleware.forge / cors.forge
  forge-grpc/                   ← gRPC（spec 未作成）
  forge-graphql/                ← GraphQL（spec 未作成）

examples/                       ← サンプルプロジェクト群
  anvil/                        ← Anvil を外部依存として使うサンプルサーバ（✅ 動作確認済み）

dev/
  design-v2.md / design-v3.md  ← 設計方針
```
