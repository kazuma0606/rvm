# ForgeScript ロードマップ

> 最終更新: 2026-04-04
> テスト総数: 199本（全通過）

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
| ラウンドトリップ | forge run == forge build + 実行 の等価確認 | 9本 |

### 型定義（forge run）✅

| 機能 | 詳細 | テスト数 |
|---|---|---|
| struct | 定義・impl・@derive(Debug/Clone/Eq/Hash/Ord/Default/Accessor/Singleton) | T-1: 11本 |
| enum | Unit/Tuple/Struct バリアント・match パターンマッチ | T-2: 6本 |
| trait / mixin | 純粋契約・デフォルト実装・impl Trait for Type | T-3: 7本 |
| data | 全 derive 自動付与・validate ブロック | T-4: 5本 |
| typestate | 状態遷移・ランタイム状態チェック | T-5: 4本 |

### ツール・周辺 ✅

| 機能 | 詳細 |
|---|---|
| VS Code シンタックスハイライト | TextMate grammar / ~/.vscode/extensions/ にローカルインストール済み |
| UAT ディレクトリ | UAT/hello.forge で動作確認済み |

---

## 設計済み・未実装 📐

### モジュールシステム 📐

### モジュールシステム 📐

- **参照**: `forge/modules/spec.md`
- **内容**: `use ./module` / `mod.forge` / pub 可視性 / when キーワード / 循環参照検出
- **次のアクション**: `forge/modules/` に plan.md / tasks.md を作成して実装着手
- **ブロッカー**: struct/enum が先に必要（型をまたいだインポートに影響）

### トランスパイラ残タスク 📐

- **参照**: `forge/transpiler/tasks.md`
- **内容**:
  - B-3: FnMut（state キャプチャ）/ FnOnce（消費キャプチャ）→ TODO コメント済み
  - B-5: struct / data / enum の Rust 変換
  - B-6: モジュールシステムの Rust 変換
  - ラウンドトリップテスト残20本の選別・追加
- **ブロッカー**: B-5 は型定義実装後、B-6 はモジュール実装後

---

## 設計中・方針確定 💭

以下は `dev/design-v3.md` に設計方針が記録されているが、spec / tasks は未作成。

| 機能 | 設計状況 | 参照 |
|---|---|---|
| `data` キーワード | 方針確定（`struct` と分離・シリアライズ自動） | design-v3.md |
| `validate` ブロック + `Validated<T>` | 方針確定（gardeの問題を解決） | design-v3.md |
| `mixin` / `interface` / `@derive` | 方針確定（trait/implの代替） | design-v3.md |
| `typestate` キーワード | 方針確定（型状態パターンの言語組み込み） | design-v3.md |
| `when` キーワード | 方針確定（#[cfg(...)]の代替） | forge/modules/spec.md |
| `async` / `await` | 方針確定（.await検出で自動昇格・tokio自動挿入） | design-v3.md |
| 名前付き引数・デフォルト引数 | 方針確定（Builderパターン自動生成） | design-v3.md |
| REPL コード補完 | 方針確定（3段階: 静的→動的→型対応） | future_task |
| Playground→REPL→Local ワークフロー | 方針確定（:save コマンド） | future_task |
| セルフホスティング | 方針確定（rustc依存は維持・コンパイラをForgeで書く） | future_task |

---

## 未設計 ⬜

| 機能 | 備考 |
|---|---|
| `forge test` + `test "..." { }` ブロック | future_task に概要のみ。インライン→コンパニオン順 |
| LSP（言語サーバー） | future_task に概要のみ。型チェッカーを活用 |
| Playground（WASM） | future_task に概要のみ。forge-wasm クレートが必要 |
| `forge.toml` パッケージ管理 | design-v3.md に最小仕様あり。詳細未設計 |
| `use raw {}` ブロック | design-v3.md に方針あり。パーサー拡張が必要 |
| ジェネリクス `<T>` | spec に「将来」として記載のみ |
| `forge fmt` | design-v3.md に言及のみ |
| `forge generate` | design-v3.md に言及のみ |
| GitHub Actions / バイナリ配布 | future_task に概要のみ |
| Tree-sitter grammar | syntax/tasks.md にオプションとして記載 |

---

## 推奨実装順序

```
今すぐ着手可能
  │
  ├─ [1] struct / enum / trait の仕様策定 + 実装
  │       → forge/v0.1.0/spec_v0.1.0.md を作成
  │       → インタープリタ対応 → トランスパイラ対応（B-5）
  │
  struct/enum 実装後
  │
  ├─ [2] モジュールシステム実装
  │       → forge/modules/plan.md + tasks.md を作成
  │       → トランスパイラ対応（B-6）
  │
  モジュール実装後
  │
  ├─ [3] data / validate / Validated<T>
  ├─ [4] mixin / interface / @derive
  ├─ [5] forge test + test "..." ブロック
  │
  言語仕様安定後
  │
  ├─ [6] LSP（言語サーバー）
  ├─ [7] Playground（WASM）
  ├─ [8] async / await
  └─ [9] セルフホスティング
```

---

## ファイル構成

```
forge/
  ROADMAP.md              ← 本ファイル（進行状況・ロードマップ）
  v0.1.0/
    spec_v0.0.1.md        ← 実装済み言語仕様（v0.0.1）
    plan.md               ← Phase 0〜6 実装計画
    tasks.md              ← Phase 0〜4 タスク（全完了）
  typedefs/
    spec.md               ← 型定義仕様（struct/enum/trait/mixin/data/typestate）
    plan.md               ← Phase T-1〜T-5 実装計画
    tasks.md              ← Phase T-1〜T-5 タスク（未着手）
  transpiler/
    spec.md               ← forge build 変換仕様
    plan.md               ← Phase B-0〜B-8 実装計画
    tasks.md              ← Phase B-0〜B-4 タスク（完了・残TODO注記済み）
  modules/
    spec.md               ← モジュールシステム仕様（設計済み・未実装）
  syntax/
    spec.md               ← シンタックスハイライト仕様
    plan.md               ← Phase S-1〜S-3 計画
    tasks.md              ← Phase S-1 完了
  future_task_20260330.md ← 将来タスク一覧（設計メモ）

dev/
  design-v2.md            ← 詳細仕様書
  design-v3.md            ← 設計視点・方針（typestate/mixin/data/validate等）
```
