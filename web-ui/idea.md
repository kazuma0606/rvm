# Bloom — フロントエンドクレート構想

> ForgeScript製のWASMフロントエンドライブラリ。
> Forge言語・ツールチェーンとは独立したクレートとして位置づける。

---

## ForgeScriptが生まれた理由

> Bloomを作りたかったから、ForgeScriptを作った。

WASMでフロントエンドを書きたい。でもRustは敷居が高い。
既存の選択肢（Leptos, Dioxus, Yew）はすべて「Rustができる前提」で設計されている。

そこでVue/Svelteユーザーが自然に書けるUIフレームワークを目標に置き、
それを実現するための言語としてForgeScriptを設計した——というのがBloomの出自。

バックエンドはRustでも、Node.jsでも、何でも良い。
**Bloomのためだけにでも、ForgeScriptを選ぶ理由がある。**

### 「なぜRustで直接書かないのか」への回答

Rustで直接UIを書く場合（Leptos等）：

```rust
#[component]
fn Counter() -> impl IntoView {
    let (count, set_count) = create_signal(0);
    view! {
        <button on:click=move |_| set_count.update(|n| *n += 1)>
            "Count: " {count}
        </button>
    }
}
```

Bloomで書いた場合：

```forge
<button @click={increment} class="px-4 py-2 bg-blue-500 text-white">
  Count: {count}
</button>

<script>
  let count: i32 = 0
  fn increment() { count += 1 }
</script>
```

出力されるWASMは同等でも、書けるひとの数が全く違う。

| | Leptos / Dioxus | Bloom |
|---|---|---|
| 想定ユーザー | Rustacean | Vue/Svelteユーザー |
| 必要な知識 | 所有権・ライフタイム・トレイト | ForgeScript（TypeScript風） |
| 学習コスト | 高 | 低 |
| 出力 | WASM | WASM（同じ） |

「RustのUIエコシステムが弱い」のは技術的な問題ではなく、**書けるひとが少ない**という問題。
Bloomはその入口を作る。Elmが「フロントエンドのために生まれた言語」であるように、
ForgeScriptは「Bloomのために生まれた言語」という立て付けで語れる。

---

## 背景・動機

- RustのUI周辺エコシステムはまだ弱い
- ForgeScriptはTypeScript風の構文を持ち、`forge build`でWASMに変換できる
- `<script type="forgescript">`をHTMLに埋め込む発想はVue/Svelteユーザーに親しみやすい
- JavaScriptにトランスパイルするのではなく、**WASMに直接コンパイル**することで差別化

---

## 基本コンセプト

```
ForgeScript (.forge) → forge build --web → WASM + JS glue → ブラウザ
```

- コンパイル型（TypeScript同様、コンパイルしてから動かす）
- `forge dev` によるホットリロードでDXを担保
- 仮想DOMなし → **コンパイル時リアクティビティ**（Svelte方式）
- コンポーネントは `.forge` ファイル単位（Vue SFCに近い）

---

## スコープ境界

### Forge側（言語・ツールチェーン）が担う
- `.forge` ファイルのパース・型チェック・コンパイル
- `forge build --web` サブコマンド（WASM出力）
- `forge dev` サブコマンド（ホットリロードサーバ）
- HTMLから `<script type="forgescript">` ブロックの抽出

### Forge側の拡張（forge-stdlib）が担う
- `forge/std/wasm` — wasmtimeラッパー。SSR時にサーバーサイドでWASMを実行するためのプリミティブ。Bloom専用ではなくForge全体の標準ライブラリ拡張として位置づける

### Bloom側（新クレート）が担う
- DOM API（web-sysの高レベルラッパー）
- イベントシステム（click, input, submit等）
- リアクティビティモデル（変数変更 → 最小DOM更新）
- コンポーネントモデル（`.bloom` SFC形式）
- `forge.min.js`（WASMローダー、ブラウザ側エントリポイント）
- SSRレンダリングAPI（`forge/std/wasm`を利用）
- Islands境界管理（後述）
- ※ CSSはTailwindに委譲するため、スコーピング機構は持たない

---

## スタイリング方針：Tailwind CSS

カスタムCSSは持たない。スタイリングは**Tailwind CSSのユーティリティクラス**に一本化する。

- `<style>`ブロック不要 → コンポーネント形式がシンプルになる
- CSSスコーピング機構も不要 → Bloom側の実装が軽くなる
- `forge build --web` 時にテンプレート内のクラス名をスキャンしてTailwindのPurgeCSSと連携
- LeptosやDioxusでも同様のアプローチが採られており、実績あり

```
forge build --web
  └── テンプレート内 class="..." をスキャン
  └── Tailwind CLI（または standalone binary）でCSS生成
  └── WASM + 最小CSS を出力
```

---

## コンポーネントイメージ（未確定）

```forge
<!-- counter.forge -->
<div class="flex flex-col items-center p-4">
  <p class="text-lg font-bold">{count}</p>
  <button class="mt-2 px-4 py-2 bg-blue-500 text-white rounded" @click={increment}>
    +
  </button>
</div>

<script>
  let count: i32 = 0

  fn increment() {
    count += 1
  }
</script>
```

- `{count}` — テンプレート補間
- `@click={fn}` — イベントハンドラ（構文は未確定）
- `class="..."` — Tailwindユーティリティクラスをそのまま使用

HTMLへの直接埋め込みも可能：

```html
<div id="app"></div>
<script type="forgescript" mount="#app">
  let message: string = "Hello, Bloom!"
  <!-- ... -->
</script>
<script src="forge.min.js"></script>
```

---

## Vueユーザーへの訴求

| Vue | Bloom |
|-----|-------|
| `.vue` SFC | `.forge` SFC |
| `<script setup>` | `<script>` in `.forge` |
| `v-model` / `@click` | 未確定（類似の記法を検討） |
| Vite | `forge dev` |
| JS/TSにコンパイル | WASMにコンパイル |
| GCあり | GCなし（Rust/WASM） |

---

## 状態管理

### 2層構造

**層1: コンポーネントローカル状態**（Bloom組み込み）

ForgeScriptの`state`キーワードをそのまま使用。変更があればコンパイラが最小DOM更新コードを生成する。

```forge
<script>
  state count: i32 = 0        // リアクティブ（変更でDOM更新）
  let title: string = "Counter"  // 非リアクティブ

  fn increment() { count += 1 }
</script>
```

**層2: グローバルストア**（`store`ブロック）

`store`ブロックを公式の状態管理プリミティブとして提供。Piniaに近い感覚で使える。

```forge
// stores/cart.forge
store Cart {
  state items: list<Item> = []

  fn add_item(item: Item) { items = items.push(item) }
  fn clear() { items = [] }

  get total() -> number { items.fold(0, (acc, x) => acc + x.price) }
}
```

---

### typestateによる状態管理（Bloomの独自性）

ForgeScriptの`typestate`パターンをストアに適用することで、**不正な状態遷移をコンパイル時に防ぐ**。
Redux/Piniaが実行時にしか検出できないバグを、そもそも表現不可能にする。

```forge
// stores/cart.forge
typestate CartState {
  Empty → HasItems → CheckingOut → Confirmed
}

store Cart<CartState> {
  state items: list<Item> = []

  // Empty | HasItems → HasItems
  fn add_item(item: Item) -> Cart<HasItems> {
    items = items.push(item)
  }

  // HasItems → CheckingOut のみ（Emptyから呼ぶとコンパイルエラー）
  fn checkout() -> Cart<CheckingOut> { ... }

  // CheckingOut → Confirmed のみ
  fn confirm(payment: Payment) -> Cart<Confirmed>! { ... }
}
```

コンポーネント側：

```forge
<script>
  use stores/cart.Cart

  let cart = Cart.get()  // Cart<Empty>

  fn handle_add(item: Item) {
    cart = cart.add_item(item)  // Cart<HasItems> に昇格
  }
  // cart.confirm() をここで呼ぶとコンパイルエラー → UIの不可能な状態をそもそも表現できない
</script>
```

| | Redux / Pinia | Bloom typestate store |
|---|---|---|
| 不正遷移の検出 | 実行時（手書きガード） | **コンパイル時** |
| 現在の状態 | 任意の値 | 型として表現 |
| 有効なアクション | 実行するまで不明 | 型から静的に決まる |

---

### DevTools

ReduxのDevToolsを超える情報量を目指す。typestateのグラフ構造があるため、単なる状態スナップショット以上のものを表示できる。

```
┌─────────────────────────────────────────┐
│ Bloom DevTools                          │
├─────────────────────────────────────────┤
│ State Machine: Cart                     │
│                                         │
│  [Empty] → [HasItems] → [CheckingOut] → [Confirmed]
│                ↑ 現在地                 │
│                                         │
│ History:                                │
│  10:42:01  add_item("Apple")            │
│  10:42:03  add_item("Bread")            │
│  ← time travel                          │
└─────────────────────────────────────────┘
```

- ステートマシングラフの可視化（現在地・有効な次の遷移が一目でわかる）
- タイムトラベルデバッグ（Redux同様）
- 通常の`store`ブロックにも対応（typestateなしでも使える）

---

## DX拡張アイデア

### 1. Anvil + Bloom フルスタック型共有

バックエンドをAnvil（ForgeScript）で書いていれば、API型定義をフロントとバックで完全共有できる。
OpenAPIもGraphQLコード生成も不要。ForgeScript同士なので型がコンパイル時に保証される。

```forge
// shared/types.forge（バック・フロント共通）
data User {
  id:   number,
  name: string,
  role: Role,
}
```

```forge
// frontend/pages/profile.forge
use shared/types.User
use backend/api.UserApi  // Anvilのルート定義から自動生成

<script>
  let user: User? = none

  async fn load() {
    user = UserApi.get_me().await?  // 型安全・コード生成なし
  }
</script>
```

他のWASMフレームワーク（Leptos等）には真似できないForgeScriptエコシステム固有の強み。
Bloomだけでなく、Anvilの訴求にもなる。

---

### 2. `story {}` ブロック（Storybook不要）

Storybookを外部ツールとして入れる代わりに、`.forge`ファイルに直接書ける。
`forge story` コマンドでブラウザ上にコンポーネントカタログが立ち上がる。

```forge
<script>
  state count: i32 = 0
  state label: string = "Click me"

  story "default" {
    count = 0
  }

  story "high count" {
    count = 999
  }

  story "custom label" {
    count = 0
    label = "押してください"
  }
</script>
```

---

### 3. `use raw {}` × WebGL / WebGPU

Bloomで表現しにくいリッチな描画部分だけ`use raw {}`でRustを直接書ける。
UIロジックはBloom、パフォーマンスクリティカルな描画はRustという分業が自然に成立する。

```forge
<!-- chart.forge -->
<canvas id="graph" class="w-full h-64"></canvas>

<script>
  state data: list<float> = []

  fn render() {
    use raw {
      // wgpu や web_sys::WebGl2RenderingContext を直接操作
      let canvas = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("graph");
      // ... WebGPUで描画
    }
  }
</script>
```

`use raw {}`の存在がBloomを「UIフレームワークの限界で詰まらない」ものにする。

---

### 4. `forge dev` × MCP × AIアシスト

MCPサーバーがすでに稼働しているため、`forge dev`中のコンパイルエラーをMCP経由でAIに投げ、
修正提案をブラウザのオーバーレイに表示するフローが比較的近い距離で実現できる。

```
コンパイルエラー発生
  → forge dev がエラーをMCPに送信
  → AIが該当箇所の修正提案を返す
  → ブラウザのオーバーレイに表示
```

---

### 優先度の整理

| アイデア | 独自性 | 実装コスト | インパクト |
|---|---|---|---|
| Anvil+Bloom型共有 | ◎ | 中 | ◎ |
| `story {}`ブロック | ○ | 低 | ○ |
| `use raw {}`×WebGL | ◎ | 低（仕組みはある） | ○ |
| MCP×AIアシスト | ◎ | 中 | ◎ |

---

## 設計決定事項

| 項目 | 決定内容 |
|------|---------|
| リアクティブ変数 | `state`キーワードで宣言（ForgeScriptと統一）。Bloom側のデコレータで将来拡張可能 |
| イベントバインディング | `@click`, `:value` 等の記号ベース（PascalCase+記号で区別） |
| コンポーネント呼び出し | PascalCaseタグ（`<Counter />`）。HTMLタグはすべて小文字なので衝突しない |
| 拡張子 | `.bloom`（Vue=`.vue`、Svelte=`.svelte`と同様に専用拡張子）|
| `bind`キーワード | ForgeScriptとして使用可能だが、Bloom側では**予約済み**（特別な意味は持たせない）|
| forge.min.jsサイズ | **< 5KB gzip** を目標。WASMのfetch+instantiateのみのローダーに絞る。WASMサイズ自体はコンポーネント量に依存するため別カウント |
| Tailwind連携 | **Tailwind v4 Standalone Binary** 方式を採用（後述） |
| SSR | スコープに含める。Anvilとの統合で実現（後述） |

---

## Tailwind連携方針：v4 Standalone Binary

Tailwind v4はエンジンがRustベース（Lightning CSS）に刷新され、設定ファイル不要・非常に高速。
Standalone Binaryが提供されているため**Node.js/npm不要**で動作する。

ForgeScriptの「依存最小」方針と相性が良い。

```
forge build --web
  └── Tailwind v4 standalone binary を自動取得（初回のみ）
  └── .bloomテンプレート内の class="..." をスキャン
  └── 使われているクラスだけを含む最小CSSを生成
  └── WASM + CSS を出力
```

- npmもviteも不要
- `forge build --web` 一発でWASM+CSS両方が揃う

---

## SSR + Anvil によるフルスタック構成

AnvilはForgeScriptで書かれた最初のバックエンドクレートである。
Anvilが直接Rustクレートを埋め込むことはできないため、wasmtimeによるWASM実行は
`forge-stdlib`側にRust実装（`forge/std/wasm`）として追加し、ForgeScriptから呼び出す形をとる。

```
crates/forge-stdlib/src/wasm.rs   ← wasmtime をラップ（Rust実装）
  ↓ forge/std/wasm として公開
packages/anvil/src/*.forge        ← use forge/std/wasm で利用
  ↓
Bloomコンポーネントのサーバーサイドレンダリング
```

### リクエストからレスポンスまでの流れ

```
リクエスト
  → Anvilがルーティング（既存ルーター）
  → forge/std/wasm 経由でWASMを実行 → HTML文字列生成
  → HTMLをレスポンス（FCP高速・SEO対応）
  → ブラウザがWASMをロード
  → WASMがDOMをハイドレーション（イベントハンドラをアタッチのみ）
  → DOM再生成なし = ゼロチラつき
```

```forge
// server/routes.forge（Anvil側）
use forge/std/wasm.execute
use bloom/ssr.{ render, hydrate_script }

app.get("/users/:id", fn(req) => {
  let user = UserRepo.find(req.params.id)?
  let html = render(<UserProfile user={user} />)  // forge/std/wasm経由でWASM実行
  Response.html(layout(html, hydrate_script()))
})
```

### チラつきゼロの理由

サーバーとクライアントが**同一WASMバイナリ**を実行するため、ハイドレーションミスマッチが構造的に起きない。
別途Rustコードを生成する方式では微妙な差異が生まれやすく、チラつきの原因になる。

### Islandsアーキテクチャ

ページ全体をハイドレーションするのではなく、動的部分（island）だけを個別にハイドレーションすることで
WASMのロード量を最小化し、チラつきをさらに抑える。

**Islands境界はコンポーネント定義単位で宣言する。**
`@island`デコレータを`.bloom`ファイルに付けるだけで、そのコンポーネントがislandとして扱われる。
使う側は普通のコンポーネントと同じ記法でよく、ビルド時に自動で分離される。

```bloom
// like-button.bloom
@island                          ← このコンポーネントはisland（定義側で一度だけ宣言）
<button
  @click={handleLike}
  class="px-4 py-2 bg-blue-500 text-white rounded"
>
  {liked ? "Liked!" : "Like"}
</button>

<script>
  state liked: bool = false
  fn handleLike() { liked = !liked }
</script>
```

使う側は通常のコンポーネントと変わらない：

```bloom
// post.bloom
<article class="prose mx-auto">
  <h1>{post.title}</h1>        <!-- 静的 → WASMなし -->
  <p>{post.content}</p>        <!-- 静的 → WASMなし -->

  <LikeButton postId={post.id} />   <!-- @islandなので自動的に個別ハイドレーション -->
  <CommentForm postId={post.id} />  <!-- 同上 -->
</article>
```

`forge build --web`がislandコンポーネントを検出し、個別のWASMチャンクとして分割出力する。
`forge.min.js`はスクロールやビューポート進入などのタイミングで各islandを遅延ロードできる。

| 役割 | 担当 |
|------|------|
| サーバーサイドルーティング | Anvil（既存） |
| WASM実行エンジン（SSR） | `forge/std/wasm`（forge-stdlib, Forge側拡張） |
| SSRレンダリングAPI | Bloom SSRモジュール（新規） |
| Islands境界宣言 | `@island`デコレータ（コンポーネント定義側） |
| クライアントナビゲーション | Bloom `<Link>` コンポーネント |
| ハイドレーション | forge.min.js + WASMチャンク（Islandsモデル） |

- [ ] `@island`の遅延ロード戦略（即時 / viewport進入時 / idle時）

## 他言語・フレームワークとの差別化

### 1. Anvil ↔ Bloom 型安全APIコントラクト

両端がForgeScriptなので、APIの型をコンパイル時に検証できる。
OpenAPI・GraphQL・tRPCと比較しても**コード生成ステップがない**点が唯一無二。

```forge
// Anvil側：@contract を付けるだけ
@contract
app.get("/users/:id", fn(req) -> User! {
  UserRepo.find(req.params.id)
})

// Bloom側：自動的に型付きクライアントが使える
use backend/api.UserApi

let user: User = UserApi.get_user(id).await?
// 型が一致しなければコンパイルエラー
```

---

### 2. `typestate` による HTTP セキュリティの型レベル保証

認証・認可をコンパイル時に強制。「認証前のリクエストにユーザー情報を渡してしまう」バグが構造的に起きない。

```forge
typestate RequestState {
  Unauthenticated → Authenticated → Authorized
}

// Unauthenticated な req からユーザー情報を取ろうとするとコンパイルエラー
app.get("/profile", fn(req: Request<Authenticated>) => {
  let user = req.user  // Authenticated が型で保証されているので安全
})
```

---

### 3. `@stream` — ストリーミングSSR

HTMLチャンクを生成しながら順次送信。TTFBを改善しつつ、ユーザーが画面上部を先に見られる。

```forge
@stream
fn render_page(user: User) -> html {
  <Layout>
    <Header user={user} />   // すぐ送信
    <Suspense>
      <HeavyContent />       // 遅延コンテンツを待つ間もブラウザが描画開始
    </Suspense>
  </Layout>
}
```

---

### 4. `forge deploy` — フルスタック単一バイナリ

Goの「シングルバイナリ」の利便性をフルスタックに持ち込む。Dockerすら不要。

```
forge deploy
  → Anvil + Bloom WASM + 静的ファイル を単一バイナリに梱包
  → ./myapp だけで起動
```

---

### 差別化まとめ

| アイデア | 他にできるフレームワーク |
|---|---|
| Anvil↔Bloom 型安全コントラクト（コード生成なし） | なし |
| typestate HTTPセキュリティ | なし |
| `forge deploy` 単一バイナリ（フルスタック） | なし |
| WASMで同一バイナリSSR（チラつきなし） | なし |
| `@island` コンポーネント定義側で宣言 | Astroが近いが言語統合はない |

---

## スコープ外

- **WASI対応（ブラウザ）**: ブラウザはセキュリティサンドボックスの設計上WASIを実装しておらず、今後も変わらない見込み。ブラウザでのファイルI/OはAnvil経由でクラウドベンダーに委ねる責務分離とする。
- **WASI対応（サーバー）**: wasmtimeは内部的にWASIをサポートしているため必要になれば自然に活用できる。Bloomのスコープとしては意識しない。
- Rust + Forge基盤で完結する方針のもと、ブラウザの制約に合わせた設計を優先する。

---

## このリポジトリとの関係

未定。選択肢：
1. `packages/bloom/` としてこのリポジトリ内に置く
2. 別リポジトリ（`forge-bloom`）として独立させる
3. `crates/bloom/` としてRustクレートに

---

## 参考

- [Svelte](https://svelte.dev/) — コンパイル時リアクティビティの先例
- [Vue SFC](https://vuejs.org/guide/scaling-up/sfc.html) — コンポーネント形式の参考
- [Leptos](https://leptos.dev/) — Rust/WASMフロントエンドの先行実装（競合ではなく参考）
- [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen) — WASMバインディング基盤
