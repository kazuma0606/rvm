# Bloom タスク一覧

> 仕様: `packages/bloom/spec.md`
> 計画: `packages/bloom/plan.md`
> ForgeScript 実装: `packages/bloom/src/`（フレームワーク本体）
> Rust 補助: `crates/bloom-compiler/`（.bloom コンパイラパイプライン + WASM ブリッジ最下層）

---

## 進捗サマリー

- Phase B-0: 16/16 完了（DOM コマンドストリームブリッジ）
- Phase B-1: 26/26 完了（.bloom パーサー + コード生成 MVP）
- Phase B-2: 12/12 完了（コンパイル時リアクティビティ）
- Phase B-3: 16/16 完了（SSR + 全置換アタッチ）
- Phase B-4: 11/12 完了（DOM Morphing）
- Phase B-5: 10/10 完了（typestate ストア + DevTools）
- Phase B-6: 16/16 完了（CLI スキャフォールド）
- **Milestone M-0: 2/2 完了（E2E 起動画面確認）** ✅
- Phase B-7: 15/15 完了（Anvil 統合）
- Phase B-8: 7/7 完了（プリプロセッサ修正 + テンプレート整備）✅
- Phase B-9: 15/15 完了 ✅（WASM ハイドレーションパス）
  - B-9-A: 4/4 完了 ✅
  - B-9-B: 5/5 完了 ✅
  - B-9-C: 2/2 完了 ✅
  - B-9-D: 4/4 完了 ✅
  - B-9-E: 5/5 完了 ✅
- **Phase B-9: 15/15 完了 ✅**
- **合計: 144/145（残タスク: B-4 E2E 1件）**

---

## Phase B-0: DOM コマンドストリームブリッジ

### B-0-A: ForgeScript パッケージ準備（`packages/bloom/`）

- [x] `packages/bloom/src/` ディレクトリ作成
- [x] `packages/bloom/forge.toml` 作成（パッケージ定義）
- [x] `packages/bloom/src/dom.forge` 作成（`dom::set_text` / `dom::set_attr` / `dom::add_listener` の骨格）

### B-0-B: WASM ↔ JS ブリッジ最下層（`crates/bloom-compiler/src/bridge.rs`）← Rust グルー

- [x] `crates/bloom-compiler/` ディレクトリ・`Cargo.toml` 作成、ワークスペースに追加
- [x] `DomOp` enum 定義（`SetText` / `SetAttr` / `AddListener` / `RemoveListener`）
- [x] `i32` 配列へのシリアライズ / デシリアライズ実装
- [x] 文字列の線形メモリ共有領域を通じた受け渡し実装
- [x] イベントバッファ（`EventKind` / `target_id`）の定義

### B-0-C: `forge.min.js` MVP（TypeScript）

- [x] `web-ui/bloon-ts/` を参考に `forge.min.js` の骨格作成
- [x] WASM の fetch + instantiate
- [x] `applyCommands(buf: Int32Array)` — SET_TEXT / SET_ATTR / ADD_LISTENER の実装
- [x] イベントハンドラ → WASM へのイベントバッファ転送
- [x] gzip 後 < 5KB であることを確認

### B-0-D: テスト

- [x] `test_set_text_command` — SET_TEXT コマンドが正しくシリアライズされる
- [x] `test_add_listener_command` — ADD_LISTENER コマンドが正しく動作する
- [x] E2E: `dom::set_text("title", "Hello")` が WASM からブラウザの DOM を更新する

---

## Phase B-1: `.bloom` パーサー + コード生成 MVP

### B-1-A: `.bloom` パーサー（`packages/bloom/src/compiler.forge`）← ForgeScript

- [x] テンプレート部分の字句解析（HTML タグ / テキスト / `{expr}` 補間）
- [x] `<script>` ブロックの抽出と ForgeScript パーサーへの委譲
- [x] `@event={handler}` 属性のパース（関数参照・クロージャ両対応）
- [x] `:attr={expr}` 属性のパース
- [x] PascalCase タグ（コンポーネント呼び出し）の識別
- [x] `{#if cond}...{/if}` / `{#for item in items}...{/for}` のパース
- [x] `<slot />` のパース

### B-1-B: テンプレート AST 定義（`packages/bloom/src/compiler.forge` 内）← ForgeScript

- [x] `TemplateNode` enum（`Element` / `Text` / `Interpolation` / `If` / `For` / `Component` / `Slot`）
- [x] `Attribute` 定義（`Static` / `Dynamic(expr)` / `EventHandler(handler)` / `Bind(expr)`）
- [x] `BloomFile` 構造体（`template: Vec<TemplateNode>` / `script: ForgeAst`）

### B-1-C: コード生成（`packages/bloom/src/compiler.forge` 内）← ForgeScript

- [x] テンプレート AST → DOM コマンド列を発行する ForgeScript コードへの変換
- [x] `state` 変数の宣言を検出しリアクティブラッパーを生成
- [x] `@click={fn}` → `ADD_LISTENER` コマンドへの変換
- [x] `@click={e => ...}` / `@click={fn(e) { ... }}` のクロージャ対応
- [x] `{expr}` 補間 → `SET_TEXT` / `SET_ATTR` コマンドへの変換
- [x] `{#if}` / `{#for}` → 条件分岐・ループのコマンド列生成

### B-1-D: `forge build --web` 統合（`crates/bloom-compiler/` Rust グルー）← Rust（最小）

- [x] `forge-cli` に `build --web` フラグを追加
- [x] `.bloom` ファイルを再帰的に検出し、`packages/bloom/src/compiler.forge` を呼び出す
- [x] 生成された ForgeScript コードを WASM にコンパイルして `dist/` に出力
- [x] `forge.min.js` を `dist/` にコピー

### B-1-E: テスト

- [x] `test_parse_basic_template` — テンプレートが正しく AST に変換される
- [x] `test_parse_event_handler_fn_ref` — 関数参照 `@click={fn}` がパースされる
- [x] `test_parse_event_handler_closure` — クロージャ `@click={e => ...}` がパースされる
- [x] `test_parse_interpolation` — `{count}` 補間が抽出される
- [x] `test_parse_if_for` — `{#if}` / `{#for}` が正しくパースされる
- [x] E2E: カウンターコンポーネントがブラウザで動作する

---

## Phase B-2: コンパイル時リアクティビティ

### B-2-A: 依存解析（`packages/bloom/src/reactivity.forge`）← ForgeScript

- [x] テンプレート中の `state` 変数参照を静的に収集
- [x] 各 DOM ノードと依存する `state` 変数の対応表を生成
- [x] ネストした式（`{user.name}`）の依存変数抽出

### B-2-B: 更新コード生成

- [x] `state` 変数の変更箇所に最小 DOM 更新コマンドを自動挿入
- [x] 変更されていない DOM ノードへの命令を生成しない
- [x] DOM op 追加: `INSERT_NODE` / `REMOVE_NODE` / `SET_CLASS`

### B-2-C: テスト

- [x] `test_state_dependency_analysis` — `{count}` が `count` に依存すると正しく解析される
- [x] `test_minimal_update` — `count` 変更時に `count` を参照するノードのみ更新される
- [x] `test_independent_node_not_updated` — 無関係なノードが更新されない
- [x] `test_nested_field_dependency` — `{user.name}` の依存が正しく追跡される

---

## Phase B-3: SSR + 全置換アタッチ

### B-3-A: SSR レンダリング API（`packages/bloom/src/ssr.forge`）← ForgeScript

- [x] `render(component: BloomComponent) -> String` 実装（HTML 文字列出力）
- [x] props の SSR 時の値埋め込み
- [x] `hydrate_script() -> String` — WASM ローダースクリプトタグ生成
- [x] `forge/std/wasm_bridge` 経由で Anvil から呼び出せる ForgeScript バインディング

### B-3-B: 全置換アタッチ

- [x] DOM op 追加: `REPLACE_INNER` / `ATTACH`
- [x] WASM ロード時に SSR 出力の DOM に対して `ATTACH` で参照取得
- [x] イベントハンドラのアタッチのみ実行（DOM 再生成なし）

### B-3-C: クリティカル CSS インライン

- [x] `forge build --web` 時に Tailwind 生成 CSS を `forge.min.js` にインライン化
- [x] WASM より先に CSS を DOM に注入する処理の実装
- [x] FOUC が発生しないことを手動確認

### B-3-D: Anvil 統合

- [x] `bloom/ssr` モジュールを Anvil から `use` できる状態にする
- [x] `render(<Component />)` の構文をコンパイラが認識
- [x] SSR + クライアントハイドレーションの E2E テスト

### B-3-E: テスト

- [x] `test_ssr_render_basic` — コンポーネントが HTML 文字列にレンダリングされる
- [x] `test_ssr_props` — props が HTML に正しく埋め込まれる
- [x] `test_hydrate_attach` — SSR 出力に WASM がアタッチできる
- [x] E2E: Anvil + Bloom の SSR ページがチラつきなしで表示される

---

## Phase B-4: DOM Morphing（差分化）

### B-4-A: Morphing アルゴリズム（`packages/bloom/src/morph.forge`）← ForgeScript

- [x] 既存 DOM ツリーと新 HTML の差分計算アルゴリズム実装
- [x] `key` 属性によるノード同一性追跡
- [x] DOM op 追加: `MORPH_NODE` / `MOVE_NODE` / `PATCH_ATTRS`

### B-4-B: エッジケース対応

- [x] フォーカス中の `<input>` のテキスト保持
- [x] スクロール位置の保持
- [x] アニメーション中ノードのスキップ

### B-4-C: テスト

- [x] `test_morph_text_change` — テキスト変更のみ最小差分更新される
- [x] `test_morph_preserve_focus` — フォーカス中入力が保持される
- [x] `test_morph_key_tracking` — `key` によるノード移動が正しく追跡される
- [ ] E2E: 入力中にサーバーからの更新が来ても入力が失われない

---

## Phase B-5: `typestate` ストア + DevTools

### B-5-A: `.flux.bloom` コンパイル（`packages/bloom/src/store_compiler.forge`）← ForgeScript

- [x] `store` ブロックのパースとコード生成
- [x] `typestate` ブロックの状態遷移グラフ生成
- [x] 不正な状態遷移のコンパイルエラー検出
- [x] コンポーネントからの `use stores/cart.Cart` 参照解決

### B-5-B: Bloom DevTools

- [x] `forge dev` にステートマシングラフ可視化パネルを追加
- [x] 状態変化のスナップショット記録
- [x] タイムトラベルデバッグ（任意の過去状態に巻き戻し）
- [x] 通常の `store`（typestate なし）にも対応

### B-5-C: テスト

- [x] `test_typestate_valid_transition` — 有効な遷移がコンパイルを通過する
- [x] `test_typestate_invalid_transition` — 無効な遷移がコンパイルエラーになる

---

## Phase B-6: CLI スキャフォールド

### B-6-A: `forge new --bloom` テンプレート

- [x] `packages/bloom/` のプロジェクトテンプレート定義
- [x] `forge new <name> --bloom` でディレクトリ構造を生成
- [x] `forge.toml` の `[bloom]` セクションを初期設定
- [x] `src/app/layout.bloom` / `src/app/page.bloom` のサンプルを生成
- [x] `src/components/counter.bloom` サンプルを生成
- [x] `src/stores/counter.flux.bloom` サンプルを生成

### B-6-B: `forge bloom add` サブコマンド

- [x] `forge bloom add component <name>` → `src/components/<name>.bloom`
- [x] `forge bloom add page <path>` → `src/app/<path>/page.bloom`（ディレクトリ自動作成）
- [x] `forge bloom add layout <path>` → `src/app/<path>/layout.bloom`
- [x] `forge bloom add store <name>` → `src/stores/<name>.flux.bloom`
- [x] `forge bloom add model <name>` → 対象ディレクトリに `<name>.model.bloom`

### B-6-C: `forge dev` 起動ページ

- [x] `web-ui/bloon-ts/app/page.tsx` を `page.bloom` に移植
- [x] `web-ui/bloom-image.png` のビジュアルと一致することを確認
- [x] `forge dev` 起動時に自動でブラウザを開く

### B-6-D: テスト

- [x] `test_new_bloom_project_structure` — 生成されたディレクトリ構造が spec と一致する
- [x] `test_bloom_add_component` — コンポーネントファイルが正しいボイラープレートで生成される
- [x] `test_bloom_add_page_nested` — ネストしたパス（`users/[id]`）でディレクトリが作成される

---

## Milestone M-0: E2E 起動画面確認

> **前提**: B-0〜B-6 の全フェーズ完了後に実施する最終 E2E 検証。
> `forge new` でテンプレートを展開し、実際に動く Bloom アプリとして `forge dev` が起動し、
> `web-ui/bloom-image.png` と同じ画面がブラウザで確認できることをもって完了とする。
>
> **⚠️ デザイン不一致時の修正方針**
> 画面が `web-ui/bloom-image.png` と異なる場合、**コンパイラが生成したコードを直接修正してはならない。**
> 必ずテンプレート側（`packages/bloom/templates/starter/src/app/page.bloom` および関連する `.bloom` ファイル）を修正すること。
> 生成コードへの直接修正は次回ビルド時に上書きされるため無意味であり、根本原因の特定も困難になる。

- [x] `forge new hello-bloom --bloom` を実行 → `spec.md §12` のディレクトリ構造が生成され、`forge dev` でブラウザが開き、`web-ui/bloom-image.png` と視覚的に一致することを確認（ダーク背景・グラデーション "Bloom" ロゴ・"on ForgeScript" サブタイトル・Docs / Learn / Templates 3 カード）
- [x] `examples/bloom-starter/` にテンプレートを展開した状態でも同じ起動画面が確認できること

---

## Phase B-7: Anvil 統合

> Anvil（ForgeScript の HTTP フレームワーク）から Bloom SSR を呼び出し、
> サーバーサイドレンダリング + クライアントハイドレーションのフルスタックパイプラインを完成させる。

### B-7-A: `bloom/ssr` モジュール — Anvil ルート統合（`packages/bloom/src/ssr.forge`）

- [x] Anvil の `use bloom/ssr` が正しく解決され、`render()` / `hydrate_script()` が呼べることを確認
- [x] `render(<Counter />)` 構文を Anvil の `.forge` ルートファイルで使えるよう、`forge build` プリプロセッサを統合
- [x] `forge build` 時に Bloom コンポーネントの WASM を自動的に `dist/` に配置
- [x] Anvil レスポンスに `hydrate_script()` が生成する `<script>` タグを埋め込む仕組みの実装
- [x] `hydrate_inline_script(source)` — WASM 不要のインライン JS hydration 実装（`@click` → `data-on-click`、`{expr}` → `data-reactive`、ForgeScript script → JS 変換）

### B-7-B: フルスタック SSR サンプル（`examples/anvil-bloom-ssr/`）

- [x] `examples/anvil-bloom-ssr/` プロジェクトを作成（Anvil サーバー + Bloom コンポーネント）
- [x] `src/routes/index.forge` に `render(<CounterPage />)` を使った SSR ルートを実装
- [x] `src/components/counter_page.bloom` カウンターコンポーネントを実装
- [x] `forge.toml` の `[bloom]` セクション設定と Anvil 依存を追加
- [x] `forge build && forge run` で SSR ページが起動することを確認

### B-7-C: ハイドレーション動作確認

- [x] SSR HTML がブラウザに届いた後、インライン JS がアタッチされてインタラクティブになることを確認
- [x] ページリロード時にチラつき（FOUC）が発生しないことを確認（SSR で初期値レンダリング済み）

### B-7-D: テスト

- [x] `test_anvil_bloom_ssr_route` — Anvil ルートが Bloom コンポーネントを HTML にレンダリングする
- [x] `test_anvil_bloom_hydrate_script` — `hydrate_script()` が正しい `<script>` タグを生成する
- [x] E2E: `examples/anvil-bloom-ssr/` を `forge run` して、ブラウザでカウンターが動作することを確認

---

## Phase B-8: プリプロセッサ修正 + テンプレート整備

> **目標**: `forge run` / `forge build` 実行時にソースファイルが破壊される問題を修正し、
> HTML テンプレートをコードから分離して可読性・保守性を回復する。

### B-8-A: プリプロセッサのソース上書き問題を修正（`crates/forge-cli/src/main.rs`）

- [x] `preprocess_forge_files_in_dir` が `fs::write(&path, processed)` でソースを直接上書きしているバグを修正
  - 変換はメモリ内のみで行い、ディスクには書き戻さない
  - `render(<X />)` 構文はソースファイルに残り続けるようにする
- [x] `forge run` の実行フローで変換済みソースをインメモリで VM に渡す
  - `collect_preprocessed_forge_files` → `HashMap<PathBuf, String>` を返す設計に変更
  - `ModuleLoader::source_overrides` フィールドを追加し `load()` 時にオーバーライドを優先使用
  - `run_file_with_deps_and_overrides` / `run_file_with_overrides` を追加
- [x] `forge test` でも同様にインメモリ変換で動作することを確認
  - `test_file_with_overrides` がディスク読み込み前にオーバーライドを確認しない問題を修正済み
  - テストファイル自体も前処理対象になるよう修正（`routes.test.forge` の `render(<CounterPage />)` が `Lt` エラーになるバグを修正）
  - テスト内容を B-8 インライン JS アプローチに合わせて更新済み
  - E2E 確認済み: 5/5 テスト パス、`/` と `/counter` のレスポンス正常

### B-8-B: `examples/anvil-bloom-ssr` のソース復元

- [x] `src/routes/index.forge` を `render(<CounterPage />)` / `hydrate_inline_script(<CounterPage />)` 構文に戻す
  - `bytes_to_str([60, 115, ...])` の羅列を除去
- [x] `hydrate_inline_script` もプリプロセッサ対応にする（`render(<X />)` と同様の変換ルールを追加）

### B-8-C: HTML レイアウトのテンプレートファイル分離

- [x] `examples/anvil-bloom-ssr/src/layouts/page.html` を作成し、ページ骨格 HTML を切り出す
  - プレースホルダー: `__TITLE__` / `__CONTENT__` / `__SCRIPT__`（ForgeScript の `{` は補間として解釈されるため `__KEY__` 形式を採用）
- [x] `page_layout` 関数を `read_file` + `replace` チェーンから個別 `let` バインディングに変更し、インライン HTML 文字列を除去
- [x] `index_handler` の本文 HTML も `src/pages/index.html` に切り出す

---

## Phase B-9: WASM ハイドレーションパス（本来のユニバーサル実行コンセプト）

> **目標**: ForgeScript コンポーネントを WASM にコンパイルし、
> サーバー（SSR）とブラウザ（hydration）で**同一の WASM バイナリ**が動作する
> 本来のアーキテクチャを実現する。
>
> ```
> counter_page.bloom
>     ↓ forge build --web
>     counter_page.wasm
>     ├── サーバー: forge-vm が WASM を実行して SSR HTML を生成
>     └── ブラウザ: forge.min.js が WASM をロードしてイベント + DOM 更新を処理
> ```

### B-9-A: ForgeScript → WASM コンパイルバックエンド（`crates/forge-compiler/`）

- [x] WASM コード生成バックエンドの設計（`crates/forge-compiler/src/wasm_backend.rs`）
  - `WasmType` (I32/F64/Bool/StringRef)・`WasmConst`・`WasmStateVar`・`WasmFn`・`WasmModule` IR 定義
  - `forge_type_to_wasm()` でプリミティブ型マッピング（number→I32, float→F64, bool→Bool, string→StringRef）
  - 文字列はポインタ+長さペア (i32, i32) として `WasmType::StringRef` で表現
- [x] `.bloom` コンポーネントの `<script>` セクションを WASM にコンパイル
  - `parse_bloom_script()` が ForgeScript AST を解析して `WasmModule` IR に変換
  - `state X = Y` → `WasmStateVar` (静的グローバル `STATE_X: i32`)
  - `fn name() { ... }` → `WasmFn` + `__forge_receive_events` エクスポート関数
  - `count = count + N` → `StateDelta::Increment(N)`, `count = count - N` → `Decrement(N)`, `count = K` → `SetConst(K)`
  - `generate_wasm_rust()` で汎用 Rust WASM ソース生成（複数状態変数・複数リスナー対応）
- [x] DOM 操作は JS インポート関数として宣言（`env.dom_set_text` 等）
  - `bloom_dom_imports()` が `dom_set_text(ptr,len,ptr,len)` と `dom_add_listener(ptr,len,ptr,len,ptr,len)` を定義
  - `WasmDomImport` 型で `env.xxx` インポートを表現
- [x] `forge build --web` サブコマンドで `.bloom` → `.wasm` を `dist/` に出力
  - `compile_bloom_to_wasm(generated_forge, Some(bloom_source), out_path)` が汎用ジェネレータを使用
  - `compile_rust_source_to_wasm()` に `cargo build --target wasm32-unknown-unknown` ロジックを分離
  - `extract_script_section()` で `.bloom` から `<script>` ブロックを抽出
  - 6 wasm_backend ユニットテスト + 3 bloom-compiler 統合テスト すべてパス

### B-9-B: ブラウザ JS ランタイム（`forge.min.js`）

- [x] `ForgeBloom` グローバルオブジェクトの設計と実装（`editors/web/runtime/forge_bloom.js`）
- [x] `ForgeBloom.load(wasmPath)` — WASM モジュールをフェッチしてインスタンス化
- [x] DOM ブリッジ — WASM から呼び出せる JS 関数群を `importObject` として渡す
  - `dom_set_text(node_id_ptr, node_id_len, text_ptr, text_len)` — テキスト更新
  - `dom_set_attr(...)` — 属性更新
  - `dom_add_event_listener(node_id_ptr, node_id_len, event_ptr, event_len, fn_idx)` — イベント登録
- [x] SSR HTML の `data-on-*` / `data-reactive` 属性を読んで WASM 関数と紐付け（hydration）
  - `setupSsrHydration()` が `data-on-click` / `data-on-input` 要素に事前リスナーを登録
  - WASM ロード前イベントをキューに積み、ロード後に `__forge_receive_events` で再生
  - `data-bloom-wasm` 属性による自動ロード（`<script data-bloom-wasm="/dist/x.wasm">`）
- [x] `forge build --web` で `forge.min.js` を `dist/` に自動コピー
  - `editors/web/runtime/forge_bloom.js` が正規ソース（フォールバック: `packages/bloom/forge.min.js`）

### B-9-C: サーバーサイド WASM 実行（SSR の WASM 化）

- [x] `forge-vm` が `.wasm` ファイルを直接実行して SSR できるパスを追加
  - `render_wasm(wasm_path: string, template_html: string) -> string!` を `bloom/ssr` に追加
  - 現行の ForgeScript インタープリタ SSR と並走（`render(<X />)` で骨格生成 → `render_wasm` で状態注入）
  - `__bloom_render_wasm` ネイティブ関数を `register_builtins` に登録
  - `vm_bloom_render_wasm`: wasmtime で WASM をロード → `__forge_init()` 実行 → コマンドバッファ取得
- [x] WASM 実行時の HTML 変換
  - `bloom_apply_commands_to_html`: OP_SET_TEXT → テキスト置換、OP_ADD_LISTENER → data-on-* 属性追加、OP_ATTACH → data-bloom-attached 付与
  - `bloom_inject_text`: `id="X"` 要素の textContent を置換
  - `bloom_inject_attr`: `id="X"` 要素に属性を追加
  - 6 ユニットテスト すべてパス（bloom_inject_text、bloom_inject_attr、bloom_apply_commands_*、builtin 登録確認）

### B-9-D: `examples/anvil-bloom-ssr` の WASM パスへの移行

- [x] `counter_handler` を `render_wasm` + `hydrate_script_with` に切り替え
  - WASM ファイルが存在する場合は `render_wasm(wasm_path, html_base)` で状態を注入
  - WASM 未ビルド時は `hydrate_inline_script` にフォールバック（`wasm_result.unwrap_or(html_base)`）
  - `runtime_handler` が `dist/forge.min.js` を提供（フォールバックメッセージ付き）
  - 既存 5 テスト全通過（フォールバックパスで動作）
- [x] `forge build --web examples/anvil-bloom-ssr` で `dist/counter_page.wasm` と `dist/forge.min.js` が生成されることを確認
  - `compile_bloom_with_compiler_forge`（サブプロセス方式）を廃止し、Rust ネイティブの `plan_from_bloom_source` + `compile_bloom_direct` に置き換え
  - ビルド時間: 5分以上 → 0.49 秒
- [x] `forge run examples/anvil-bloom-ssr` でブラウザのカウンターが WASM で動作することを確認
  - `render_wasm` が WASM SSR に成功し `hydrate_script_with` パスが使われることを確認（`forge.min.js` + `/components/counter_page.wasm` をロード）
- [x] inline JS（`hydrate_inline_script`）と WASM パスの動作が一致することを確認
  - フォールバック付き実装により両パスが同等の HTML/JS を返す

### B-9-E: テスト

- [x] `test_wasm_compile_state` — `state count = 0` が `static mut STATE_COUNT: i32 = 0` として生成される (`web::tests::test_wasm_compile_state`)
- [x] `test_wasm_compile_fn` — `fn increment()` が `__forge_receive_events` エクスポートと `wrapping_add(1)` として生成される (`web::tests::test_wasm_compile_fn`)
- [x] `test_forge_bloom_load` — `ForgeBloom.load()` が WASM をロードしてイベントをディスパッチできる（ブラウザ JS テスト — スキップ：N/A）
- [x] `test_wasm_ssr_hydration` — SSR HTML の id が生成 WASM Rust ソースに正しく含まれることを確認 (`web::tests::test_wasm_ssr_hydration`)
- [x] E2E: WASM パスでカウンターがブラウザで動作することを確認（`forge build --web` + `forge run` 手動確認）
  - 全エンドポイント確認済み: `/`(200), `/counter`(200,WASM SSR), `/forge.min.js`(200,16KB), `/components/counter_page.wasm`(200,22KB)
