# ForgeScript ロードマップ

> 最終更新: 2026-04-04
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

## 設計済み・未実装 📐（言語機能）

| 機能 | 仕様 | 備考 |
|---|---|---|
| ジェネリクス `<T>` | `lang/generics/spec.md` | Anvil の前提。v0.3.0 |
| `forge.toml` パッケージ管理 | `lang/package/spec.md` | forge build との統合。v0.3.0 |

## 設計済み・未実装 📐（パッケージ）

| パッケージ | 仕様 | 概要 |
|---|---|---|
| **Anvil** | `packages/anvil/spec.md` | Express スタイル HTTP マイクロフレームワーク |
| forge-grpc | `packages/forge-grpc/spec.md`（未作成） | gRPC サービス定義 DSL（tonic ラッパー） |
| forge-graphql | `packages/forge-graphql/spec.md`（未作成） | GraphQL スキーマ DSL（async-graphql ラッパー） |

## 未設計 ⬜

| 機能 | 備考 |
|---|---|
| `forge test` FT-2 | コンパニオンファイル・ディレクトリ走査 |
| LSP（言語サーバー） | 型チェッカーを活用。ホバー・補完・定義ジャンプ |
| Playground（WASM） | forge-wasm クレートが必要 |
| `forge fmt` | コードフォーマッタ |
| `forge generate` | コードジェネレータ |
| GitHub Actions / バイナリ配布 | CI/CD・リリース自動化 |
| Tree-sitter grammar | シンタックスハイライトの拡張 |

---

## 推奨実装順序

```
✅ 完了済み
  ├─ [1] struct / enum / trait / mixin / data / typestate 実装
  │       → forge/typedefs/ に spec/plan/tasks 完備
  │
今すぐ着手可能
  │
✅ [2] モジュールシステム実装（M-0〜M-7 完了）
  │
今すぐ着手可能
  │
✅ [3] forge test + test "..." ブロック（FT-1 完了）
  │       FT-2（コンパニオンファイル）は言語仕様安定後
  │
  次のステップ
  │
  ├─ [4] ジェネリクス <T>（Anvil の前提）
  ├─ [5] forge.toml（パッケージ管理）
  ├─ [6] Anvil HTTP フレームワーク
  │       Stage A-1: std のみ基礎 HTTP
  │       Stage A-2: ルーティング
  │       Stage A-3: ミドルウェア
  │       Stage A-4: 非同期（tokio）
  │
  言語仕様安定後
  │
  ├─ [7] LSP（言語サーバー）
  ├─ [8] forge-grpc / forge-graphql
  ├─ [9] Playground（WASM）
  └─ [10] セルフホスティング
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

crates/                         ← RVM 実装（Rust クレート群）
  forge-compiler/
  forge-vm/
  forge-stdlib/
  forge-transpiler/
  forge-cli/

packages/                       ← ForgeScript パッケージ群
  anvil/
    spec.md                     ← Anvil HTTP フレームワーク仕様（📐 設計済み）
  forge-grpc/                   ← gRPC（spec 未作成）
  forge-graphql/                ← GraphQL（spec 未作成）

dev/
  design-v2.md / design-v3.md  ← 設計方針
```
