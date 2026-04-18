# Bloom 仕様

> バージョン: 0.1.0
> 設計ドキュメント: `web-ui/idea.md`
> リポジトリ配置: `packages/bloom/`（ForgeScript 実装）/ `crates/bloom-compiler/`（Rust補助）

### 実装言語方針

**Bloom は ForgeScript で書かれたフレームワークである。**

```
packages/bloom/src/          ← ForgeScript（フレームワーク本体）
  dom.forge                  ← DOM ラッパー
  ssr.forge                  ← SSR レンダリング
  router.forge               ← ルーター
  store.forge                ← ストアプリミティブ
  link.bloom                 ← <Link> コンポーネント
  ...

crates/bloom-compiler/       ← Rust（補助：どうしても必要な部分のみ）
  .bloom パーサー・コード生成  ← forge-compiler パイプラインの拡張
  WASM↔JS ブリッジ最下層      ← wasm_bridge プリミティブ
forge.min.js                 ← JavaScript（ブラウザ側ローダー、< 5KB gzip）
```

Rust が必要な理由：
- `.bloom` パーサーは `forge-compiler`（Rust）のパイプラインに組み込む必要がある
- WASM ↔ JS のバイナリプロトコル最下層は Rust で実装し `forge/std/wasm_bridge` として公開
- `forge.min.js` はブラウザで動くため JavaScript

**それ以外はすべて ForgeScript で実装する。**
Bloom 自身が ForgeScript で書かれていることが、エコシステム全体の説得力になる。

---

## 1. 設計方針

Bloom は ForgeScript 製の WASM フロントエンドフレームワーク。

```
Bloom (.bloom) / ForgeScript (.forge) → forge build --web → WASM + JS glue → ブラウザ
```

- **Vue/Svelte ユーザーが自然に書ける** UI フレームワークを目標とする
- **仮想 DOM なし** — コンパイル時リアクティビティ（Svelte 方式）
- **WASM に直接コンパイル** — JS へのトランスパイルではなく差別化
- **Tailwind CSS** に一本化。カスタム CSS スコーピング機構は持たない
- **MCP ファースト** — `forge dev` 中のエラーを MCP 経由で AI に投げられる

---

## 2. ファイル種別

| 拡張子 | 種別 | 説明 |
|---|---|---|
| `.forge` | ForgeScript | 純粋な ForgeScript（型定義・ユーティリティ・共有ロジック） |
| `.bloom` | Bloom コンポーネント | テンプレート + `<script>` を持つ UI コンポーネント・ページ・レイアウト |
| `.flux.bloom` | Bloom ストア | `store` / `typestate` ブロックを持つ状態管理ファイル |
| `.model.bloom` | Bloom モデル | フォーム状態・バリデーション・送信ロジック（`v-model` 相当を責務分離） |

`.blade.php` と同じ構造: `[種別].[ランタイム]`。
`.bloom` / `.flux.bloom` / `.model.bloom` はすべて ForgeScript ランタイム上で動作する。

---

## 3. コンポーネント構文

### 3-1. 基本構造

```bloom
<!-- counter.bloom -->
<div class="flex flex-col items-center p-4">
  <p class="text-lg font-bold">{count}</p>
  <button class="mt-2 px-4 py-2 bg-blue-500 text-white rounded" @click={increment}>
    +
  </button>
</div>

<script>
  state count: i32 = 0

  fn increment() {
    count += 1
  }
</script>
```

- `{expr}` — テンプレート補間
- `state` — リアクティブ変数（変更で最小 DOM 更新が発生）
- `let` — 非リアクティブ変数
- `<ComponentName />` — PascalCase タグでコンポーネント呼び出し（HTML タグはすべて小文字）

### 3-2. 条件・ループ

```bloom
{#if condition}
  <p>表示</p>
{/if}

{#for item in items}
  <li>{item.name}</li>
{/for}
```

### 3-3. スロット

```bloom
<!-- button.bloom -->
<button class="px-4 py-2" @click={onClick}>
  <slot />
</button>

<!-- 使う側 -->
<Button @click={save}>保存する</Button>
```

---

## 4. イベントバインディング

### 4-1. イベントハンドラ（`@`）

関数参照とクロージャの両方を受け付ける。

```bloom
<!-- 関数参照 -->
<button @click={increment}>+</button>

<!-- クロージャ（アロー記法） -->
<button @click={e => count += 1}>+</button>

<!-- クロージャ（ブロック記法） -->
<button @click={fn(e) { count += 1 }}>+</button>
```

### 4-2. 一方向バインディング（`:`）

```bloom
<!-- 値をテンプレートに渡す -->
<Input :value={name} />
<div :class={active ? "bg-blue-500" : "bg-gray-500"} />
```

### 4-3. 双方向バインディング（`:bind`）

`v-model` 相当。フォームモデルファイル（`.model.bloom`）と組み合わせて使う。

```bloom
<input :bind={form.name} placeholder="名前" />
<input :bind={form.email} placeholder="メール" type="email" />
```

---

## 5. フォームモデル（`.model.bloom`）

フォーム状態・バリデーション・送信ロジックを `*.model.bloom` に分離する。
ビューと完全に責務分離されテスト可能な純粋なロジックとなる。

```
src/app/users/new/
  page.bloom            ← ビュー
  page.model.bloom      ← モデル
```

```
{{! page.model.bloom }}
model UserForm {
  state name:  string = ""
  state email: string = ""
  state error: string? = none

  fn validate() -> bool {
    name.len() > 0 && email.contains("@")
  }

  fn submit() -> Result<(), string>! {
    if !validate() { return Err("入力内容を確認してください") }
    UserApi.create({ name, email }).await?
  }
}
```

```bloom
{{! page.bloom }}
use ./page.model.UserForm

<form @submit={form.submit}>
  <input :bind={form.name}  placeholder="名前" />
  <input :bind={form.email} placeholder="メール" />
  {#if form.error}
    <p class="text-red-500">{form.error}</p>
  {/if}
  <button type="submit">送信</button>
</form>

<script>
  let form = UserForm.new()
</script>
```

---

## 6. 状態管理（`.flux.bloom`）

### 6-1. 基本ストア

```
{{! stores/cart.flux.bloom }}
store Cart {
  state items: list<Item> = []

  fn add(item: Item)  { items = items.push(item) }
  fn clear()          { items = [] }

  get total() -> number { items.fold(0, (acc, x) => acc + x.price) }
}
```

### 6-2. typestate ストア

不正な状態遷移をコンパイル時に防ぐ。

```
{{! stores/cart.flux.bloom }}
typestate CartState {
  Empty → HasItems → CheckingOut → Confirmed
}

store Cart<CartState> {
  state items: list<Item> = []

  fn add_item(item: Item) -> Cart<HasItems> {
    items = items.push(item)
  }

  // HasItems → CheckingOut のみ（Empty から呼ぶとコンパイルエラー）
  fn checkout() -> Cart<CheckingOut> { ... }

  fn confirm(payment: Payment) -> Cart<Confirmed>! { ... }
}
```

---

## 7. ルーティング

### 7-1. ファイルシステムルーティング

`src/app/` ディレクトリの構造が URL に 1:1 で対応する（Next.js App Router 方式）。

```
src/app/
  layout.bloom          ← / 全体に適用されるルートレイアウト
  page.bloom            ← /
  about/
    page.bloom          ← /about
  users/
    layout.bloom        ← /users/* に自動適用されるネストレイアウト
    page.bloom          ← /users
    [id]/
      page.bloom        ← /users/:id（動的ルート）
      page.model.bloom  ← /users/:id のフォームモデル（必要な場合）
```

### 7-2. 予約ファイル名

| ファイル名 | 役割 |
|---|---|
| `layout.bloom` | そのディレクトリ以下すべてに適用されるレイアウト |
| `page.bloom` | ルートのメインコンテンツ |
| `page.model.bloom` | ページのフォームモデル |
| `[param]/` | 動的ルートセグメント |

### 7-3. クライアントナビゲーション

```bloom
<Link to={Routes.users(user.id)}>{user.name}</Link>
```

- `<Link>` は SSR 時に `<a href="...">` としてレンダリング（JS なしでも遷移可能）
- `@contract` が定義されたルートから型安全な URL ビルダーを自動生成
- 存在しないルートへの `<Link>` はコンパイルエラー

---

## 8. Islands アーキテクチャ

### 8-1. `@island` デコレータ

コンポーネント定義側で宣言する。使う側は通常のコンポーネントと変わらない。

```bloom
// like-button.bloom
@island(load: "visible")
<button @click={handleLike} class="px-4 py-2 bg-blue-500 text-white rounded">
  {liked ? "Liked!" : "Like"}
</button>

<script>
  state liked: bool = false
  fn handleLike() { liked = !liked }
</script>
```

### 8-2. ロード戦略

コンポーネント単位で指定する。

| デコレータ | タイミング | 実装 |
|---|---|---|
| `@island` | 即時（デフォルト） | ページロード時にロード |
| `@island(load: "immediate")` | 即時（明示） | 同上 |
| `@island(load: "visible")` | viewport 進入時 | IntersectionObserver |
| `@island(load: "idle")` | ブラウザ idle 時 | requestIdleCallback |

### 8-3. WASM 共有チャンク

```
dist/
  bloom-runtime.wasm   ← 共有ランタイム（全 Island が参照）
  like-button.wasm     ← アプリコードのみ（数 KB 程度）
  comment-form.wasm
  forge.min.js         ← ローダー + クリティカル CSS インライン（< 5KB gzip）
```

---

## 9. SSR + Anvil 統合

```forge
// server/routes.forge（Anvil 側）
use bloom/ssr.{ render, hydrate_script }

app.get("/users/:id", fn(req) => {
  let user = UserRepo.find(req.params.id)?
  let html = render(<UserProfile user={user} />)
  Response.html(layout(html, hydrate_script()))
})
```

- サーバー・クライアントが**同一 WASM バイナリ**を実行 → ハイドレーションミスマッチなし
- `forge build --web` 時にコンパイラが SSR/CSR の DOM ツリーを照合 → ビルドエラーで事前検出

---

## 10. Tailwind 統合

- `forge new my-app --bloom` で最初から設定済み。ユーザーが設定ファイルを書く必要なし
- Tailwind v4 Standalone Binary を初回ビルド時に自動取得（Node.js 不要）
- コンパイラがクラス名をビルド時に検証（typo を warning/error として検出）

```toml
# forge.toml
[bloom]
tailwind         = true
tailwind_version = "4.1.0"
```

---

## 11. CLI

### 11-1. `forge new`

```bash
forge new my-app --bloom          # Bloom フロントエンド
forge new my-app --anvil          # Anvil バックエンド
forge new my-app --fullstack      # Anvil + Bloom フルスタック
```

### 11-2. `forge bloom add`

```bash
forge bloom add component <name>  # src/components/<name>.bloom
forge bloom add page <path>       # src/app/<path>/page.bloom
forge bloom add layout <path>     # src/app/<path>/layout.bloom
forge bloom add store <name>      # src/stores/<name>.flux.bloom
forge bloom add model <name>      # src/app/<path>/<name>.model.bloom
```

### 11-3. `forge dev`

開発サーバー起動。ホットリロード対応。

初回起動ページ:
- **参照実装**: `web-ui/bloon-ts/`
- **ビジュアル確定稿**: `web-ui/bloom-image.png`
- ダークテーマ固定、"Bloom" ロゴ（ピンク→パープルグラデーション）、"on **ForgeScript**" サブタイトル
- フッター: Docs / Learn / Templates の 3 カード

---

## 12. プロジェクト構造（`forge new my-app --bloom` 生成物）

```
my-app/
├── forge.toml
├── public/
│   └── favicon.ico
└── src/
    ├── app/
    │   ├── layout.bloom          ← ルートレイアウト
    │   └── page.bloom            ← / （サンプルコード入り）
    ├── components/
    │   └── counter.bloom         ← サンプルコンポーネント
    ├── stores/
    │   └── counter.flux.bloom    ← サンプルストア
    └── lib/
        └── utils.forge           ← 共有ロジック
```

---

## 13. 実装スコープ（フェーズ分割）

| フェーズ | 内容 |
|---|---|
| **B-0** | DOM コマンドストリームブリッジ |
| **B-1** | `.bloom` パーサー + コード生成 MVP |
| **B-2** | コンパイル時リアクティビティ |
| **B-3** | SSR + 全置換アタッチ |
| **B-4** | DOM Morphing（差分化） |
| **B-5** | `typestate` ストア + DevTools |
| **B-6** | `forge new --bloom` + `forge bloom add` CLI |
