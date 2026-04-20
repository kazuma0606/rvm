# Bloom v2 アイデアメモ

## UI 基盤としての方向性

Bloom の WASM + JS ランタイムは ForgeScript 専用ではなく、**Bloom Component ABI を満たす WASM であればどの言語から生成されたものでも動かせる**設計になっている。

WASM 自体は汎用仕様であり、サーバーは Go / Python / Rails など何でもよい。フロントエンド基盤として Bloom を育てるという方向性は現実的で、以下の 2 段階で考えられる。

### Stage 1 — サーバーは何でもよい、WASM だけ Bloom
- サーバーは静的 HTML を返すだけ
- `forge.min.js` をロードして WASM をハイドレーション
- SSR なし、JS ハイドレーションのみ
- 現時点でも技術的に可能

### Stage 2 — SSR も他言語から呼べる
- WASM を他言語から instantiate して command buffer を解釈する SSR ライブラリが必要
- Go なら `wasmtime-go`、Python なら `wasmtime-py` など
- Bloom SSR ランタイムの多言語移植になる

### 実現のための優先順位

1. **ABI v1 を固定する** — command buffer のレイアウト表と conformance test
2. **hydration-only モードを正式サポート** — SSR 不要でどのサーバーとも繋がる
3. **Bloom SSR ランタイムの分離** — SSR が必要な場合、WAT/wasm ライブラリとして独立させるのが長期的に正しい

なお `.bloom` ファイルのコンパイル（WASM 生成）は引き続き `bloom-compiler` が担う。command buffer パターンはサーバー側 SSR でも同じコードが動くという利点もあり、将来 Component Model が安定しても有効な設計。

---

## DX ロードマップ

最低限の実装が完成した段階で、次に必要になるのはデバッグ機能とホットリロードを中心とした開発体験の向上。他のバックエンドフレームワーク（Axum、Phoenix など）が持つエコシステムに相当する層。

### デバッグ・観測性

| 機能 | 現状 | 優先度 |
|---|---|---|
| ForgeScript エラーの行番号表示 | なし（エラーが追いにくい） | 高 |
| リクエスト/レスポンスログ | 手動 `log()` のみ | 高 |
| ミドルウェアによる自動計測 | なし | 中 |
| WASM エラーを人間が読める形で返す | JS console のみ | 中 |

Anvil に `use logger()` 1 行で有効になるミドルウェアが最もユーザーにとって自然な体験。

### ホットリロード

変更対象によってリロードの重さが大きく異なるため、**何をホットリロードするか**が設計の核心になる。

```
.forge サーバーロジック変更
  → ForgeScript インタープリタ再起動
  → 重さ: 軽〜中（秒単位）

.bloom の HTML/CSS 変更
  → SSR HTML 再生成のみ
  → 重さ: 軽い

.bloom の script/state 変更
  → WASM 再コンパイル（rustc が走る）
  → 重さ: 重い（10〜30 秒）

Rust crate 変更（forge-vm 等）
  → cargo build
  → 重さ: 非常に重い（開発者向けのみ）
```

現実的な戦略として、まず **`.forge` と `.bloom` の HTML 変更のみ**を対象にしたファイル監視ベースのホットリロードを実装し、WASM 再コンパイルは後回しにする。WASM 変更が検知された場合は「再ビルドが必要」と通知するだけでも十分な体験になる。

実装の置き場は `forge serve --watch` コマンドとして `forge-cli` に追加するのが自然。ファイル監視 → WebSocket でブラウザに通知 → 自動リロードが最小構成。

### 優先順位

1. **`forge serve --watch`（`.forge` / `.html` の変更のみ）** — WASM 再コンパイルなし、最も費用対効果が高い
2. **Anvil リクエストログミドルウェア** — `use logger()` 1 行で有効になる体験
3. **ForgeScript エラーの行番号** — これがないとデバッグが根本的に辛い
4. **WASM 変更検知 → 再ビルド通知** — 再コンパイルは手動、検知だけ自動
5. **WASM 差分ホットリロード** — 重いため長期目標

---

## テスト戦略

Bloom のテストは三層に分ける。ForgeScript で意味論を固め、Rust で統合面を固め、実ブラウザ E2E は少数に絞るのが妥当。全部をブラウザで見るのは遅く、全部を言語内で見るのは無理がある。

### 1. ForgeScript — 意味論を固定する

`.bloom` の parser、AST、依存解析、codegen、SSR 出力は ForgeScript テストで固める。速く、壊れたときの原因も追いやすい。

例:
- `{#if}` と `{#for}` の展開
- `@click` や `:class` の解釈
- `render(<Component />)` の変換
- `log(count)` が生成コードに残ること

### 2. Rust — 統合面を固定する

build、WASM 生成、runtime bridge、SSR + hydrate の接続を見る。ForgeScript だけでは届かない層。

例:
- `forge build --web` で `dist/*.wasm` と `forge.min.js` が出る
- SSR で返した HTML に対して attach できる
- DOM command が期待どおり出る
- `forge_log` import が入る

### 3. 実ブラウザ E2E — 本物確認は少数精鋭

3〜5 本程度に絞る。全部を E2E にすると保守が重い。

最低限ほしいもの:
- 初期 SSR 表示が見える
- hydration 後に click で state が変わる
- console に WASM `log()` が出る
- `if/for` を含む画面で再描画が壊れない

### 4. テスト対象を「画面」ではなく「契約」にする

見た目全体ではなく、壊れると困る契約だけ見る。

例:
- `data-on-click` が attach される
- `data-reactive="count"` が更新される
- `/forge.min.js` と `.wasm` が正しい URL で配信される
- SSR fallback ではなく WASM 本線に入る

### 5. 手確認はチェックリスト化して最小化する

完全自動化できないものだけ手確認に残す。

例:
- FOUC がない
- 体感的な描画崩れがない
- DevTools Console に期待ログが出る

---

## .bloom シンタックスハイライト

`.forge` は VS Code 拡張でシンタックスハイライトを実装済みだが、`.bloom` が未対応。

### ファイル形式の特殊性

`.bloom` は `<script>` ブロック（ForgeScript）+ HTML テンプレートの混在形式。

```
<script>
  state count = 0          ← ForgeScript
  fn increment() { ... }
</script>

<div class="counter">
  <span>{count}</span>             ← ForgeScript 式補間
  <button @click={increment}>+</button>  ← イベントバインディング
  {#if count > 0}                  ← テンプレートディレクティブ
    <p>positive</p>
  {/if}
</div>
```

### 実装方針

`editors/vscode/syntaxes/bloom.tmLanguage.json` を新規作成し、`package.json` に言語登録する。

- ベースは HTML 文法
- `<script>...</script>` ブロック内に `source.forge` を埋め込み
- `{expr}` 補間に ForgeScript 式の文法を適用
- `@click` / `@input` 等のイベントバインディング属性を専用スコープで色付け
- `:class` / `:style` 等の動的バインディング属性も同様
- `{#if}` / `{#for}` / `{/if}` / `{/for}` ディレクティブをキーワードとしてハイライト

### 対応が必要なトークン

| トークン | 例 | スコープ |
|---|---|---|
| `<script>` ブロック | `<script>...</script>` | `source.forge` を埋め込み |
| 式補間 | `{count}` | ForgeScript 式 |
| イベントバインディング | `@click={handler}` | 専用属性スコープ |
| 動的バインディング | `:class={expr}` | 専用属性スコープ |
| ディレクティブ開始 | `{#if}` / `{#for}` | キーワード |
| ディレクティブ終了 | `{/if}` / `{/for}` | キーワード |
| HTML タグ・属性 | 通常の HTML | 既存 HTML 文法に委譲 |

---

## Bloom コンポーネント間通信

現状の Bloom コンパイラはコンポーネントの識別（PascalCase タグ）と slot のパースまでは実装されているが、以下の 3 機能がいずれも未実装・未仕様のままになっている。

### 現状のギャップ

| 機能 | 仕様 | 実装 |
|---|---|---|
| コンポーネント export/import | 暗黙的な言及のみ | コード生成なし |
| Props（親→子の値渡し） | 暗黙的な言及のみ | パースのみ、受け取り側なし |
| 子→親イベント通知 | 未定義 | なし |
| slot | 仕様あり | パースのみ、展開コードなし |

これらは依存関係があり、実装順序は export/import → Props / slot → 子→親通知 の順が自然。

### 1. コンポーネント export/import

`.bloom` ファイルが 1 コンポーネントに対応し、PascalCase タグで呼び出す。呼び出し側は `use` でインポートする。

```forge
// 親コンポーネント側（page.bloom の script ブロック内）
use ./counter.bloom
```

```html
<!-- 親テンプレート -->
<Counter initial={0} on_change={fn(v) { count = v }} />
```

### 2. Props（親→子の値渡し）

コンポーネント側の `<script>` で `props` ブロックを宣言し、受け取る値の型を明示する。

```html
<!-- counter.bloom -->
<script>
props {
    initial: number
    on_change: fn(number)
}

state count = props.initial
</script>

<div>
  <span>{count}</span>
  <button @click={fn() { count = count + 1; props.on_change(count) }}>+</button>
</div>
```

### 3. 子→親イベント通知（関数渡し）

**設計方針: コールバック Props として関数を渡す。**

Vue の `$emit` や EventBus ではなく、ForgeScript の設計思想に合わせて **関数を props として渡す**形を採用する。理由：

- ForgeScript は第一級関数をサポートしている
- `forge/std/event` の EventBus との混在を避けられる
- 型が明示的で、どのコンポーネントが何を受け取るか一目でわかる

```html
<!-- 親 -->
<script>
state selected_id = none

fn handle_select(id: number) {
    selected_id = id
}
</script>

<ItemList on_select={handle_select} />
<Detail item_id={selected_id} />
```

```html
<!-- 子: item_list.bloom -->
<script>
props {
    on_select: fn(number)
}
</script>

{#for item in items}
  <li @click={fn() { props.on_select(item.id) }}>{item.name}</li>
{/for}
```

### 4. slot

コンポーネントの開閉タグの間に渡したコンテンツを `<slot />` の位置に展開する。

```html
<!-- button.bloom -->
<button class="btn" @click={props.on_click}>
  <slot />
</button>

<!-- 使う側 -->
<Button on_click={save}>保存する</Button>
<!-- → <button class="btn">保存する</button> に展開 -->
```

### WASM との関係

コンポーネント分割は SSR（ForgeScript インタープリタ側）と WASM（ブラウザ側）の両方で動く必要がある。現状は単一コンポーネントの WASM 生成のみ対応。複数コンポーネントを組み合わせた場合の WASM バイナリ生成戦略（1 ファイル 1 WASM か、バンドルするか）は別途検討が必要。
