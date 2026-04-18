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
- Phase B-3: 14/16 完了（SSR + 全置換アタッチ）
- Phase B-4: 11/12 完了（DOM Morphing）
- Phase B-5: 10/10 完了（typestate ストア + DevTools）
- Phase B-6: 16/16 完了（CLI スキャフォールド）
- **Milestone M-0: 0/2 完了（E2E 起動画面確認）** ← 全フェーズ完了後の最終確認
- **合計: 105/110 完了**

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
- [ ] FOUC が発生しないことを手動確認

### B-3-D: Anvil 統合

- [x] `bloom/ssr` モジュールを Anvil から `use` できる状態にする
- [x] `render(<Component />)` の構文をコンパイラが認識
- [ ] SSR + クライアントハイドレーションの E2E テスト

### B-3-E: テスト

- [x] `test_ssr_render_basic` — コンポーネントが HTML 文字列にレンダリングされる
- [x] `test_ssr_props` — props が HTML に正しく埋め込まれる
- [x] `test_hydrate_attach` — SSR 出力に WASM がアタッチできる
- [ ] E2E: Anvil + Bloom の SSR ページがチラつきなしで表示される

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
- [ ] `web-ui/bloom-image.png` のビジュアルと一致することを確認
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

- [ ] `forge new hello-bloom --bloom` を実行 → `spec.md §12` のディレクトリ構造が生成され、`forge dev` でブラウザが開き、`web-ui/bloom-image.png` と視覚的に一致することを確認（ダーク背景・グラデーション "Bloom" ロゴ・"on ForgeScript" サブタイトル・Docs / Learn / Templates 3 カード）
- [ ] `examples/bloom-starter/` にテンプレートを展開した状態でも同じ起動画面が確認できること
