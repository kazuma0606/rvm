# Bloom タスク一覧

> 仕様: `packages/bloom/spec.md`
> 計画: `packages/bloom/plan.md`
> ForgeScript 実装: `packages/bloom/src/`（フレームワーク本体）
> Rust 補助: `crates/bloom-compiler/`（.bloom コンパイラパイプライン + WASM ブリッジ最下層）

---

## 進捗サマリー

- Phase B-0: 0/17 完了（DOM コマンドストリームブリッジ）
- Phase B-1: 0/20 完了（.bloom パーサー + コード生成 MVP）
- Phase B-2: 0/12 完了（コンパイル時リアクティビティ）
- Phase B-3: 0/16 完了（SSR + 全置換アタッチ）
- Phase B-4: 0/12 完了（DOM Morphing）
- Phase B-5: 0/10 完了（typestate ストア + DevTools）
- Phase B-6: 0/16 完了（CLI スキャフォールド）
- **合計: 0/103 完了**

---

## Phase B-0: DOM コマンドストリームブリッジ

### B-0-A: Rust クレート準備（`crates/bloom-compiler/`）

- [ ] `crates/bloom-compiler/` ディレクトリ作成
- [ ] `crates/bloom-compiler/Cargo.toml` 作成（`forge-compiler` / `serde` / `serde_json` 依存）
- [ ] ワークスペース `Cargo.toml` に `bloom-compiler` を追加
- [ ] `crates/bloom-compiler/src/lib.rs` 作成（公開 API の骨格）

### B-0-A2: ForgeScript パッケージ準備（`packages/bloom/`）

- [ ] `packages/bloom/src/` ディレクトリ作成
- [ ] `packages/bloom/src/dom.forge` 作成（DOM API 高レベルラッパーの骨格）
- [ ] `packages/bloom/forge.toml` 作成（パッケージ定義）

### B-0-B: コマンドプロトコル定義（`crates/bloom-compiler/src/bridge.rs`）← Rust

- [ ] `DomOp` enum 定義（`SetText` / `SetAttr` / `AddListener` / `RemoveListener`）
- [ ] `i32` 配列へのシリアライズ / デシリアライズ実装
- [ ] 文字列の線形メモリ共有領域を通じた受け渡し実装
- [ ] イベントバッファ（`EventKind` / `target_id`）の定義

### B-0-C: `forge.min.js` MVP（TypeScript）

- [ ] `web-ui/bloon-ts/` を参考に `forge.min.js` の骨格作成
- [ ] WASM の fetch + instantiate
- [ ] `applyCommands(buf: Int32Array)` — SET_TEXT / SET_ATTR / ADD_LISTENER の実装
- [ ] イベントハンドラ → WASM へのイベントバッファ転送
- [ ] gzip 後 < 5KB であることを確認

### B-0-D: テスト

- [ ] `test_set_text_command` — SET_TEXT コマンドが正しくシリアライズされる
- [ ] `test_add_listener_command` — ADD_LISTENER コマンドが正しく動作する
- [ ] E2E: `dom::set_text("title", "Hello")` が WASM からブラウザの DOM を更新する

---

## Phase B-1: `.bloom` パーサー + コード生成 MVP

### B-1-A: `.bloom` パーサー（`crates/bloom-compiler/src/parser.rs`）← Rust

- [ ] テンプレート部分の字句解析（HTML タグ / テキスト / `{expr}` 補間）
- [ ] `<script>` ブロックの抽出と ForgeScript パーサーへの委譲
- [ ] `@event={handler}` 属性のパース（関数参照・クロージャ両対応）
- [ ] `:attr={expr}` 属性のパース
- [ ] PascalCase タグ（コンポーネント呼び出し）の識別
- [ ] `{#if cond}...{/if}` / `{#for item in items}...{/for}` のパース
- [ ] `<slot />` のパース

### B-1-B: テンプレート AST 定義（`crates/bloom-compiler/src/ast.rs`）← Rust

- [ ] `TemplateNode` enum（`Element` / `Text` / `Interpolation` / `If` / `For` / `Component` / `Slot`）
- [ ] `Attribute` 定義（`Static` / `Dynamic(expr)` / `EventHandler(handler)` / `Bind(expr)`）
- [ ] `BloomFile` 構造体（`template: Vec<TemplateNode>` / `script: ForgeAst`）

### B-1-C: コード生成（`crates/bloom-compiler/src/codegen.rs`）← Rust

- [ ] テンプレート AST → DOM コマンド列を発行する ForgeScript コードへの変換
- [ ] `state` 変数の宣言を検出しリアクティブラッパーを生成
- [ ] `@click={fn}` → `ADD_LISTENER` コマンドへの変換
- [ ] `@click={e => ...}` / `@click={fn(e) { ... }}` のクロージャ対応
- [ ] `{expr}` 補間 → `SET_TEXT` / `SET_ATTR` コマンドへの変換
- [ ] `{#if}` / `{#for}` → 条件分岐・ループのコマンド列生成

### B-1-D: `forge build --web` 統合

- [ ] `forge-cli` に `build --web` フラグを追加
- [ ] `.bloom` ファイルを再帰的に検出してパース
- [ ] 生成コードを WASM にコンパイルして `dist/` に出力
- [ ] `forge.min.js` を `dist/` にコピー

### B-1-E: テスト

- [ ] `test_parse_basic_template` — テンプレートが正しく AST に変換される
- [ ] `test_parse_event_handler_fn_ref` — 関数参照 `@click={fn}` がパースされる
- [ ] `test_parse_event_handler_closure` — クロージャ `@click={e => ...}` がパースされる
- [ ] `test_parse_interpolation` — `{count}` 補間が抽出される
- [ ] `test_parse_if_for` — `{#if}` / `{#for}` が正しくパースされる
- [ ] E2E: カウンターコンポーネントがブラウザで動作する

---

## Phase B-2: コンパイル時リアクティビティ

### B-2-A: 依存解析（`crates/bloom-compiler/src/reactivity.rs`）← Rust

- [ ] テンプレート中の `state` 変数参照を静的に収集
- [ ] 各 DOM ノードと依存する `state` 変数の対応表を生成
- [ ] ネストした式（`{user.name}`）の依存変数抽出

### B-2-B: 更新コード生成

- [ ] `state` 変数の変更箇所に最小 DOM 更新コマンドを自動挿入
- [ ] 変更されていない DOM ノードへの命令を生成しない
- [ ] DOM op 追加: `INSERT_NODE` / `REMOVE_NODE` / `SET_CLASS`

### B-2-C: テスト

- [ ] `test_state_dependency_analysis` — `{count}` が `count` に依存すると正しく解析される
- [ ] `test_minimal_update` — `count` 変更時に `count` を参照するノードのみ更新される
- [ ] `test_independent_node_not_updated` — 無関係なノードが更新されない
- [ ] `test_nested_field_dependency` — `{user.name}` の依存が正しく追跡される

---

## Phase B-3: SSR + 全置換アタッチ

### B-3-A: SSR レンダリング API（`packages/bloom/src/ssr.forge`）← ForgeScript

- [ ] `render(component: BloomComponent) -> String` 実装（HTML 文字列出力）
- [ ] props の SSR 時の値埋め込み
- [ ] `hydrate_script() -> String` — WASM ローダースクリプトタグ生成
- [ ] `forge/std/wasm_bridge` 経由で Anvil から呼び出せる ForgeScript バインディング

### B-3-B: 全置換アタッチ

- [ ] DOM op 追加: `REPLACE_INNER` / `ATTACH`
- [ ] WASM ロード時に SSR 出力の DOM に対して `ATTACH` で参照取得
- [ ] イベントハンドラのアタッチのみ実行（DOM 再生成なし）

### B-3-C: クリティカル CSS インライン

- [ ] `forge build --web` 時に Tailwind 生成 CSS を `forge.min.js` にインライン化
- [ ] WASM より先に CSS を DOM に注入する処理の実装
- [ ] FOUC が発生しないことを手動確認

### B-3-D: Anvil 統合

- [ ] `bloom/ssr` モジュールを Anvil から `use` できる状態にする
- [ ] `render(<Component />)` の構文をコンパイラが認識
- [ ] SSR + クライアントハイドレーションの E2E テスト

### B-3-E: テスト

- [ ] `test_ssr_render_basic` — コンポーネントが HTML 文字列にレンダリングされる
- [ ] `test_ssr_props` — props が HTML に正しく埋め込まれる
- [ ] `test_hydrate_attach` — SSR 出力に WASM がアタッチできる
- [ ] E2E: Anvil + Bloom の SSR ページがチラつきなしで表示される

---

## Phase B-4: DOM Morphing（差分化）

### B-4-A: Morphing アルゴリズム（`packages/bloom/src/morph.forge`）← ForgeScript

- [ ] 既存 DOM ツリーと新 HTML の差分計算アルゴリズム実装
- [ ] `key` 属性によるノード同一性追跡
- [ ] DOM op 追加: `MORPH_NODE` / `MOVE_NODE` / `PATCH_ATTRS`

### B-4-B: エッジケース対応

- [ ] フォーカス中の `<input>` のテキスト保持
- [ ] スクロール位置の保持
- [ ] アニメーション中ノードのスキップ

### B-4-C: テスト

- [ ] `test_morph_text_change` — テキスト変更のみ最小差分更新される
- [ ] `test_morph_preserve_focus` — フォーカス中入力が保持される
- [ ] `test_morph_key_tracking` — `key` によるノード移動が正しく追跡される
- [ ] E2E: 入力中にサーバーからの更新が来ても入力が失われない

---

## Phase B-5: `typestate` ストア + DevTools

### B-5-A: `.flux.bloom` コンパイル（`crates/bloom-compiler/src/store.rs`）← Rust

- [ ] `store` ブロックのパースとコード生成
- [ ] `typestate` ブロックの状態遷移グラフ生成
- [ ] 不正な状態遷移のコンパイルエラー検出
- [ ] コンポーネントからの `use stores/cart.Cart` 参照解決

### B-5-B: Bloom DevTools

- [ ] `forge dev` にステートマシングラフ可視化パネルを追加
- [ ] 状態変化のスナップショット記録
- [ ] タイムトラベルデバッグ（任意の過去状態に巻き戻し）
- [ ] 通常の `store`（typestate なし）にも対応

### B-5-C: テスト

- [ ] `test_typestate_valid_transition` — 有効な遷移がコンパイルを通過する
- [ ] `test_typestate_invalid_transition` — 無効な遷移がコンパイルエラーになる

---

## Phase B-6: CLI スキャフォールド

### B-6-A: `forge new --bloom` テンプレート

- [ ] `packages/bloom/` のプロジェクトテンプレート定義
- [ ] `forge new <name> --bloom` でディレクトリ構造を生成
- [ ] `forge.toml` の `[bloom]` セクションを初期設定
- [ ] `src/app/layout.bloom` / `src/app/page.bloom` のサンプルを生成
- [ ] `src/components/counter.bloom` サンプルを生成
- [ ] `src/stores/counter.flux.bloom` サンプルを生成

### B-6-B: `forge bloom add` サブコマンド

- [ ] `forge bloom add component <name>` → `src/components/<name>.bloom`
- [ ] `forge bloom add page <path>` → `src/app/<path>/page.bloom`（ディレクトリ自動作成）
- [ ] `forge bloom add layout <path>` → `src/app/<path>/layout.bloom`
- [ ] `forge bloom add store <name>` → `src/stores/<name>.flux.bloom`
- [ ] `forge bloom add model <name>` → 対象ディレクトリに `<name>.model.bloom`

### B-6-C: `forge dev` 起動ページ

- [ ] `web-ui/bloon-ts/app/page.tsx` を `page.bloom` に移植
- [ ] `web-ui/bloom-image.png` のビジュアルと一致することを確認
- [ ] `forge dev` 起動時に自動でブラウザを開く

### B-6-D: テスト

- [ ] `test_new_bloom_project_structure` — 生成されたディレクトリ構造が spec と一致する
- [ ] `test_bloom_add_component` — コンポーネントファイルが正しいボイラープレートで生成される
- [ ] `test_bloom_add_page_nested` — ネストしたパス（`users/[id]`）でディレクトリが作成される
