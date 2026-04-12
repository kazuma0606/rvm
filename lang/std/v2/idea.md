# `forge/std` v2 — 追加モジュール構想

> v1（forge/std 第2〜3層）の延長として、WASM実行・ブラウザ統合を支えるモジュールを追加する。
> Bloom（フロントエンドクレート）のSSR基盤として位置づけるが、Bloom専用ではなくForge全体の標準ライブラリ拡張。

---

## モジュール一覧

| モジュール | 概要 | 優先度 |
|---|---|---|
| `forge/std/wasm` | サーバーサイドWASM実行（wasmtimeラッパー） | 高 |
| `forge/std/crypto` | ハッシュ・署名・検証 | 中 |
| `forge/std/compress` | gzip / brotli 圧縮 | 中 |
| `forge/std/ai` | AI生成・推論（構想のみ） | 低・将来 |

---

## `forge/std/wasm`

### 概要

サーバーサイドでWASMバイナリを実行するためのプリミティブ。
主にBloomのSSR（サーバーサイドレンダリング）でAnvilから呼び出すことを想定しているが、
汎用のWASM実行エンジンとして設計する。

**Rust実装**: `crates/forge-stdlib/src/wasm.rs`（wasmtimeクレートをラップ）

---

### 動機

Bloomのチラつきゼロなハイドレーションを実現するには、サーバーとクライアントが
**同一WASMバイナリ**を実行する必要がある。

- 別途Rustコードを生成する方式 → サーバー/クライアントで異なるコードパスが生まれ、ハイドレーションミスマッチによるチラつきが発生しやすい
- 同一WASMを実行する方式 → 出力HTMLが構造的に一致するため、ハイドレーションはイベントハンドラのアタッチのみ = ゼロチラつき

---

### API設計

ForgeScriptのスタイル（`data` + `impl`、エラーは `T!`）に合わせる。
wasmtimeの `Engine` / `Module` / `Store` / `Instance` の複雑さは内部に隠蔽し、
呼び出し側はシンプルな2操作だけ意識すればよい。

```forge
use forge/std/wasm.Wasm

// 起動時に一度だけ（コンパイルは重いので使い回す）
let app = Wasm.load("dist/app.wasm")?

// リクエストごと（instance生成 → 実行 → 破棄、軽い）
let html: string = app.call("render", json.stringify({
  component: "UserProfile",
  props: { user: user },
}))?
```

ForgeScriptから見えるAPI：

```forge
data Wasm { }  // 内部は Arc<wasmtime::Module>。スレッドセーフ

impl Wasm {
  // WASMバイナリをファイルからロード・コンパイル
  fn load(path: string) -> Wasm!

  // 指定した関数を呼び出す
  // input / output ともにJSON文字列（シリアライズは呼び出し側が担う）
  fn call(self, fn_name: string, input: string) -> string!
}
```

input/outputをJSON文字列にした理由：
- `forge/std/json` と自然に組み合わせられる
- `forge/std/wasm` がシリアライズ形式に依存しない（将来MessagePack等に変えやすい）
- 型境界が明確（WASMに渡る時点でstring確定）

### Anvilからの利用イメージ

```forge
// packages/anvil/src/ssr.forge
use forge/std/wasm.Wasm
use forge/std/json.stringify

state _wasm: Wasm? = none

fn init(path: string) {
  _wasm = Wasm.load(path)?
}

fn render(component: string, props: map<string, string>) -> string! {
  let wasm = _wasm?
  let input = stringify({ component: component, props: props })
  wasm.call("render", input)
}
```

```forge
// Anvilのルート定義側
use packages/anvil/ssr.{ init, render }

init("dist/app.wasm")

app.get("/users/:id", fn(req) => {
  let user = UserRepo.find(req.params.id)?
  let html = render("UserProfile", { user: user })?
  Response.html(layout(html))
})
```

`Wasm.load` は起動時に `init()` で一度だけ呼び、以降は `state` で保持して使い回す。

### 実装メモ（forge-stdlib / Rust側）

```rust
// crates/forge-stdlib/src/wasm.rs
use wasmtime::{Engine, Module, Store, Instance};
use std::sync::Arc;

pub struct Wasm {
    engine: Arc<Engine>,
    module: Arc<Module>,
}

impl Wasm {
    // Module::from_file でコンパイル済みバイナリをロード
    pub fn load(path: &str) -> Result<Self, ...>

    // Store・Instance は call() ごとに生成して破棄（ステートレス）
    pub fn call(&self, fn_name: &str, input: &str) -> Result<String, ...>
}
```

- Rust依存: `wasmtime`（Bytecode Alliance公式）
- `Arc` によりAnvilの並列リクエストをまたいでモジュールを共有
- コンパイル（重い）は起動時1回、インスタンス生成（軽い）はリクエストごと

---

### 汎用活用例

`forge/std/wasm` はBloom/SSR以外でも広く活用できる。

#### 1. プラグインシステム

サードパーティのコードをサンドボックス内で安全に実行。WASMはOSのリソースに直接アクセスできないため、信頼できないコードを動かすのに適している。SaaSでのユーザー定義ロジック実行（Shopify Scripts、Cloudflare Workersのユーザーコードと同様のモデル）。

```forge
let plugin = Wasm.load("plugins/user-transform.wasm")?
let result = plugin.call("transform", json.stringify(data))?
```

#### 2. AI生成コードのセルフホスティング実行

AnvilでユーザーからForgeScriptを受け取り、コンパイルしてWASMで実行。

```forge
let code   = ai.generate(prompt)?
let wasm   = forge.compile(code)?
let result = Wasm.from_bytes(wasm).call("run", input)?
```

#### 3. 差し替え可能なデータ変換ロジック

ETLやデータ処理で、変換ルールだけをWASMとして差し替え可能にする。本体を再デプロイせずにロジックを更新できる。

```forge
let transformer = Wasm.load("rules/invoice-v3.wasm")?
let output = records.map(r => transformer.call("transform", json.stringify(r))?)
```

#### 4. マルチ言語資産の活用

wasmtimeはどの言語から生成されたWASMも実行できる。RustやGoやC++の既存ライブラリをWASMにコンパイルすれば、ForgeScriptから呼び出せる。

---

### セキュリティ用途

WASMはサンドボックスが構造に組み込まれているため、セキュリティ機能を後付けするのではなく最初から安全な実行環境として使える。

#### XSS対策

HTMLサニタイズをWASMモジュールに通す。Rustの `ammonia` クレートをWASMにコンパイルすることで高速かつ安全なサニタイザーになる。

```forge
let sanitizer = Wasm.load("security/html-sanitizer.wasm")?

let raw_input  = req.body.get("comment")?
let clean_html = sanitizer.call("sanitize", raw_input)?
// スクリプトタグ・イベントハンドラ属性等が除去済み
Response.html(render_comment(clean_html))
```

#### ユーザー定義コードの制御

wasmtimeのFuel・Memory limit・Epoch interruptionでリソースを制限し、不審なコードをブロックできる。

```forge
let sandbox = Wasm.load("plugins/user-code.wasm", WasmOptions {
  max_instructions: 1_000_000,  // 命令数上限 → 無限ループをブロック
  max_memory_mb:    16,         // メモリ上限 → メモリ爆弾をブロック
  timeout_ms:       500,        // タイムアウト → 強制終了
})?

let result = sandbox.call("run", input)?
// タイムアウト・メモリ超過・無限ループはすべて T! のエラーとして返る
```

デフォルトで遮断されるもの：

```
✗ ファイルシステム
✗ ネットワーク
✗ OS syscall
✗ WASM自身のメモリ外の領域
```

| セキュリティ用途 | 仕組み |
|---|---|
| XSSサニタイズ | ammonia等をWASMにコンパイルして入力を必ず通す |
| ユーザーコード実行 | Fuel + Memory limitでリソース制限 |
| 不審コードのブロック | Capability制限でOS・ネットへのアクセスを完全遮断 |
| タイムアウト | Epoch interruptionで強制終了 |

`WasmOptions` はAPI設計の未確定部分。オプション付きの `Wasm.load` か、別の `Wasm.sandboxed` コンストラクタにするかは実装時に決める。

---

### WasmOptions 設計

#### 基本方針

- `data WasmOptions` を主軸とする（型安全・LSP補完・ForgeScriptらしさ）
- JSON設定ファイルの読み込みは `WasmOptions` 自体には持たせない → `forge/std/config` の責務
- `Wasm.load` の第2引数を省略した場合は `trusted()` がデフォルト（SSR等の自前コード用途が最多のため）

```forge
data WasmOptions {
  // リソース制限（none = 無制限）
  max_instructions: number? = none,  // 無限ループ対策
  max_memory_mb:    number? = none,  // メモリ爆弾対策
  timeout_ms:       number? = none,  // タイムアウト

  // Capability制限（デフォルトはすべて拒否）
  allow_fs:  bool = false,           // ファイルシステムアクセス
  allow_net: bool = false,           // ネットワークアクセス
  allow_env: bool = false,           // 環境変数の読み取り
}
```

#### プリセット

```forge
impl WasmOptions {
  // 自前WASM（Bloom SSR等）向け。制限なし・全許可
  fn trusted() -> WasmOptions

  // ユーザープラグイン向け。適切な制限・外部アクセス不可
  fn sandboxed() -> WasmOptions  // max_instructions: 1_000_000 / max_memory_mb: 16 / timeout_ms: 500

  // XSSサニタイザー等の純粋な変換処理向け。最も厳格
  fn strict() -> WasmOptions    // max_instructions: 100_000 / max_memory_mb: 4 / timeout_ms: 100
}
```

#### 利用パターン

```forge
// コードで直接指定（基本）
let app      = Wasm.load("dist/app.wasm")?                         // trusted（省略）
let plugin   = Wasm.load("plugins/user.wasm", WasmOptions.sandboxed())?
let sanitizer = Wasm.load("security/sanitizer.wasm", WasmOptions.strict())?

// プリセットをベースに一部だけ上書き
let custom = Wasm.load("plugins/partner.wasm", WasmOptions.sandboxed() {
  max_memory_mb: 64,
  timeout_ms:    2000,
})?

// ops が設定ファイルで管理したい場合 → forge/std/config に任せる
use forge/std/config.load

let config = load("wasm.toml")?
let opts   = WasmOptions {
  max_memory_mb: config.get("wasm.max_memory_mb")?,
  timeout_ms:    config.get("wasm.timeout_ms")?,
}
let plugin = Wasm.load("plugins/user.wasm", opts)?
```

| プリセット | 用途 | 制限 |
|---|---|---|
| `trusted()` | 自前WASM（Bloom SSR等） | なし |
| `sandboxed()` | ユーザープラグイン | 中程度 |
| `strict()` | 純粋な変換処理（XSS等） | 厳格 |
| カスタム | プリセットをベースに上書き | 任意 |

---

### スコープ外

- **WASI（ブラウザ側）**: ブラウザはセキュリティサンドボックスの設計上WASIを実装しておらず、今後も変わらない見込み。ブラウザでのファイルI/Oはクラウドベンダー（S3等）へAnvil経由で委ねる責務分離とする。Rust + Forge基盤で完結する方針と一致。
- **WASI（サーバー側）**: wasmtimeは内部的にWASIをサポートしているため、必要になれば自然に活用できる。Bloomのスコープとしては意識しない。

### 設計決定：型マッピング

WASMが直接扱える型は `i32` / `i64` / `f32` / `f64` のみ。ForgeScriptの `string` / `map` / `struct` はそのまま渡せないため、**JSONシリアライズ経由**を採用する。

```
ForgeScript の値
  → JSON文字列にシリアライズ
  → WASMの線形メモリ（バイト列）に書き込む（ポインタ + 長さ）
  → WASM側がデシリアライズして処理
  → 結果（HTML文字列）を同様に返す
```

SSRのレンダリング用途では速度的に十分。他のSSRフレームワークも同じアプローチを採っている。

### 設計決定：ホットリロード

`forge dev` 時のフロー：

```
.bloom ファイルが変更
  → forge が再コンパイル → 新しい .wasm を生成
  → wasmtime がキャッシュしているモジュールを差し替え
  → WebSocket でブラウザに通知
  → ブラウザが新しい WASM をロード
```

wasmtime側はキャッシュしているモジュールを新しいバイナリで上書きするだけ。Viteのホットリロードと構造的に同じ。実行中のリクエストが終わり次第、古いインスタンスは自然に破棄される。

---

## `forge/std/crypto`

### 概要

ハッシュ・署名・検証を提供する。WASMプラグインの署名検証・パスワードハッシュ・JWTの検証等に使う。
Rust実装: `ring` または `sha2` / `hmac` クレートをラップ。

```forge
use forge/std/crypto.{ hash, hmac, sign, verify }

// ハッシュ
let digest = hash(content, HashAlgo.sha256)
let digest = hash(content, HashAlgo.blake3)

// HMAC（APIシグネチャ検証等）
let mac = hmac(payload, secret, HashAlgo.sha256)

// 署名・検証（WASMプラグインの信頼性確認等）
let sig = sign(payload, private_key)?
let ok  = verify(payload, sig, public_key)?
```

### 活用例

```forge
// WASMプラグインを実行前に署名検証
let sig     = fs.read("plugins/user.wasm.sig")?
let payload = fs.read_bytes("plugins/user.wasm")?
verify(payload, sig, trusted_public_key)?   // 検証失敗なら T! のエラー

let plugin = Wasm.load("plugins/user.wasm", WasmOptions.sandboxed())?
```

---

## `forge/std/compress`

### 概要

gzip・brotli 圧縮を提供する。`forge build --web` のWASMバイナリ圧縮・HTTPレスポンスの圧縮に使う。
Rust実装: `flate2`（gzip）/ `brotli` クレートをラップ。

```forge
use forge/std/compress.{ gzip, brotli, decompress }

let compressed = brotli(wasm_bytes)?    // Brotliはブラウザ配信の標準
let compressed = gzip(content)?
let original   = decompress(compressed)?
```

`forge build --web` のビルドパイプラインで自動的に使用され、ユーザーが直接呼ぶ機会は少ない。
ただし Anvil でHTTPレスポンスを手動で圧縮したい場合に公開APIとして提供する。

---

## `forge/std/ai`（構想のみ）

> どのモデルが主流になるか現時点では不明なため、詳細設計は行わない。
> アイデアの記録として残す。

AIの生成・推論をstdlibとして提供するという方向性。
外部SDKを後付けするのではなく**標準ライブラリとして持つ言語**は現状ほぼ存在しない。

```forge
// イメージのみ（API未確定）
use forge/std/ai.generate

let code = generate("ログインフォームを作って", format: .forge)?
```

- モデルの呼び出し先（ローカル / クラウド）・APIの形式は将来確定
- MCPサーバーがすでにForgeに組み込まれており、拡張の素地はある
