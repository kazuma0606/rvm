# Bloom 実装計画

> 仕様: `packages/bloom/spec.md`
> 設計ドキュメント: `web-ui/idea.md`
> 前提: `forge-compiler` / `forge-vm` / `forge-cli` が利用可能であること

---

## フェーズ構成

```
B-0: ブリッジ    — DOM コマンドストリームブリッジ（最初に動かすコアピース）
B-1: パーサー    — .bloom パーサー + コード生成 MVP
B-2: リアクティブ — コンパイル時リアクティビティ
B-3: SSR         — SSR + 全置換アタッチ
B-4: Morphing    — DOM Morphing（差分化）
B-5: ストア      — typestate ストア + DevTools
B-6: CLI         — forge new --bloom + forge bloom add
```

B-0 → B-1 → B-2 → B-3 の順に実施する。
**B-0 がなければ B-1 以降はすべてデッドコード。**
B-3 全置換で SSR アタッチ全体が動くことを確認してから B-4 に進む。

---

## Phase B-0: DOM コマンドストリームブリッジ

### 目標

WASM からブラウザの DOM を操作できること。
`document.getElementById` + `textContent` が ForgeScript から動く状態。

### 設計

WASM → JS へ「DOM 操作命令を送るチャンネル」を自作する（`web-sys` / `wasm-bindgen` は使用しない）。

```
WASM (Bloom runtime)                JS (forge.min.js)
      │                                    │
      │── [SET_TEXT, "h1", "hello"] ──────→│
      │── [ADD_LISTENER, "btn", "click"] ──→│
      │← [CLICK, "btn"] ──────────────────│
```

コマンドは `i32` 配列としてシリアライズ。文字列は線形メモリの共有領域を通じて渡す。

### 実装の分担

```
crates/bloom-compiler/src/bridge.rs   ← Rust（DomOp プロトコル定義・シリアライズ）
                                         forge-compiler パイプラインへの接続
packages/bloom/src/dom.forge          ← ForgeScript（DOM API の高レベルラッパー）
forge.min.js                          ← JavaScript（ブラウザ側ローダー）
```

Rust は「プロトコルの最下層（i32 配列 ↔ DomOp）」と「WASM ビルドパイプライン組み込み」のみ担当。
`dom::set_text(id, value)` などの使いやすい API は ForgeScript で実装する。

### MVP で実装する op

| op | 内容 |
|---|---|
| `SET_TEXT` | `element.textContent = value` |
| `SET_ATTR` | `element.setAttribute(name, value)` |
| `ADD_LISTENER` | `element.addEventListener(event, handler)` |
| `REMOVE_LISTENER` | `element.removeEventListener` |

### 達成基準

```bloom
<!-- hello.bloom -->
<h1 id="title">Hello</h1>
<button id="btn" @click={greet}>押す</button>

<script>
  fn greet() {
    // WASM から DOM の textContent を変更できる
    dom::set_text("title", "Hello, Bloom!")
  }
</script>
```

`forge dev` でブラウザを開き、ボタンを押したらテキストが変わること。

---

## Phase B-1: `.bloom` パーサー + コード生成 MVP

### 目標

`.bloom` ファイルをパースし、WASM にコンパイルできること。
`state` / `@click` / テンプレート補間（`{expr}`）の最小セットが動く。

### 実装ステップ

1. **`.bloom` パーサー（`packages/bloom/src/compiler.forge`）** ← ForgeScript
   - テンプレート部分（HTML + `{expr}` 補間）の AST 生成（文字列処理）
   - `<script>` ブロックの抽出と ForgeScript コードとしての保持
   - `@event={handler}` / `:attr={expr}` 属性の解析

2. **コード生成（同上 `compiler.forge`）** ← ForgeScript
   - テンプレート AST → DOM コマンド列を発行する ForgeScript コードへの変換
   - `state` 変数の変更を検知してコマンドを発行するコードを自動生成
   - `@click` → `ADD_LISTENER` コマンドに変換

3. **`forge build --web` への統合（`crates/bloom-compiler/` Rust グルー）**
   - `.bloom` ファイルを検出し、ForgeScript コンパイラ（`packages/bloom/src/compiler.forge`）を呼び出す
   - 生成された ForgeScript コードを WASM にコンパイル
   - `forge.min.js` と WASM を `dist/` に出力

4. **`forge.min.js`（TypeScript → JS、目標 < 5KB gzip）**
   - WASM の fetch + instantiate
   - DOM コマンドバッファの適用
   - イベントバッファの WASM への転送
   - クリティカル CSS のインライン注入

### 達成基準

```bloom
<!-- counter.bloom -->
<p>{count}</p>
<button @click={increment}>+</button>

<script>
  state count: i32 = 0
  fn increment() { count += 1 }
</script>
```

ブラウザでカウンターが動作すること。

---

## Phase B-2: コンパイル時リアクティビティ

### 目標

`state` 変数の変更時に最小限の DOM コマンドだけを発行する。
仮想 DOM を使わず、コンパイラが更新対象を静的に決定する。

### 実装ステップ

1. **依存解析（`packages/bloom/src/reactivity.forge`）** ← ForgeScript
   - テンプレート中で `state` 変数を参照しているノードを静的に収集
   - `{count}` → `<p>` の `textContent` が `count` に依存、と記録

2. **更新コード生成（同上）** ← ForgeScript
   - `count += 1` の後に `SET_TEXT(p_id, string(count))` を自動挿入
   - 変更されていない DOM ノードは一切触れない

3. **DOM op 追加**

| op | 内容 |
|---|---|
| `INSERT_NODE` | 新しいノードを挿入 |
| `REMOVE_NODE` | ノードを削除 |
| `SET_CLASS` | `classList` の更新 |

### 達成基準

- `state` 変数が変更されたとき、依存するノードのみ更新される
- `state` に依存しないノードは再レンダリングされない
- React DevTools 相当で「更新されたノード」が視認できる

---

## Phase B-3: SSR + 全置換アタッチ

### 目標

Anvil が HTML を出力し、WASM がそれに「アタッチ」できること。
チラつきなし・FOUC なしを確認する。

### 実装ステップ

1. **SSR レンダリング API（`packages/bloom/src/ssr.forge`）** ← ForgeScript
   - `.bloom` コンポーネントを文字列 HTML としてレンダリング
   - `forge/std/wasm_bridge` 経由で Anvil から呼び出せるようにする

2. **全置換アタッチ（B-3 限定）**
   - `REPLACE_INNER` op を追加
   - WASM ロード時に `innerHTML` 相当で全置換（B-4 の Morphing への布石）

3. **クリティカル CSS インライン**
   - `forge build --web` 時に Tailwind が生成した CSS を `forge.min.js` にインライン化
   - WASM より先に CSS を DOM に注入して FOUC を排除

4. **Anvil 統合**

```forge
// server/routes.forge
use bloom/ssr.{ render, hydrate_script }

app.get("/", fn(req) => {
  let html = render(<App />)
  Response.html(layout(html, hydrate_script()))
})
```

### 達成基準

- Anvil がスタイル済みの HTML を返し、ブラウザが即座に表示する
- WASM ロード後にイベントハンドラがアタッチされ、インタラクションが動く
- リロードしてもチラつきが発生しない

---

## Phase B-4: DOM Morphing（差分化）

### 目標

全置換（B-3）を差分 Morph に置き換え、フォーカス・入力状態を保持したまま更新できること。

### 実装ステップ

1. **Morphing アルゴリズム**
   - 既存 DOM と新しい HTML を比較して最小差分を計算
   - 変更のないノードはそのまま保持
   - `key` 属性でノードの同一性を追跡

2. **DOM op 追加**

| op | 内容 |
|---|---|
| `MORPH_NODE` | ノードを差分更新 |
| `MOVE_NODE` | ノードを移動（`key` 追跡） |
| `PATCH_ATTRS` | 属性だけ更新 |

3. **エッジケース対応**
   - フォーカス中の `<input>` はテキスト内容を保持
   - アニメーション中のノードは置き換えない
   - カスタム要素（`<Counter />`）は再マウントしない

### 達成基準

- テキスト入力中にサーバーからの更新が来ても入力が失われない
- スクロール位置が保持される
- React hydration error に相当するミスマッチが発生しない

---

## Phase B-5: `typestate` ストア + DevTools

### 目標

`store` / `typestate` ブロックをコンパイルでき、
DevTools でステートマシングラフとタイムトラベルデバッグができること。

### 実装ステップ

1. **`.flux.bloom` パーサー + コンパイル（`packages/bloom/src/store_compiler.forge`）** ← ForgeScript
   - `store` / `typestate` ブロックの AST 生成
   - 不正な状態遷移をコンパイルエラーとして検出

2. **Bloom DevTools（`forge dev` 組み込み）**
   - ステートマシングラフの可視化（現在地・有効な次の遷移）
   - タイムトラベルデバッグ（状態スナップショット + 巻き戻し）
   - 通常の `store`（typestate なし）にも対応

---

## Phase B-6: CLI スキャフォールド

### 目標

`forge new my-app --bloom` と `forge bloom add` が動作すること。

### 実装ステップ

1. **`forge new --bloom` テンプレート**
   - プロジェクト構造の自動生成（`spec.md §12` 参照）
   - `forge.toml` の `[bloom]` セクション初期設定
   - サンプルコンポーネント・ストアを含む

2. **`forge bloom add` サブコマンド**
   - `component` / `page` / `layout` / `store` / `model` の各テンプレート生成
   - ディレクトリが存在しない場合は自動作成

3. **`forge dev` 起動ページ**
   - `web-ui/bloon-ts/app/page.tsx` を `.bloom` に移植
   - ダークテーマ・グラデーションロゴ・3 カードフッター（`web-ui/bloom-image.png` 参照）

---

## 依存クレート

| クレート / パッケージ | 種別 | 用途 |
|---|---|---|
| `packages/bloom/src/` | ForgeScript | フレームワーク本体 + コンパイラ（compiler / reactivity / store_compiler / dom / ssr / router / store / morph） |
| `crates/bloom-compiler/` | Rust（最小グルー） | `forge build --web` オーケストレーション・WASM ブリッジ最下層のみ |
| `forge-compiler` | Rust（既存） | 生成された ForgeScript → WASM コンパイル |
| `forge-vm` | Rust（既存） | ForgeScript コンパイラ実行（`compiler.forge` を動かす） |
| `serde` / `serde_json` | Rust（既存） | DevTools データのシリアライズ |

---

## 実装後の確認

```bash
cargo test -p bloom-compiler
forge new hello-bloom --bloom
cd hello-bloom && forge dev        # 起動ページが表示される
forge bloom add component counter  # コンポーネントが生成される
forge build --web                  # dist/ に WASM + JS が出力される
```
