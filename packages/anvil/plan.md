# Anvil 実装計画

> spec.md を元にした段階的実装プラン

---

## 設計方針

- anvil は **ForgeScript で書かれた**パッケージ（.forge ソースが正）
- `forge build packages/anvil/` で Rust コードが生成され `cargo build` でコンパイルされる
- A-1〜A-4 は `std` のみ（外部クレート依存なし）
- A-5 で `tokio` を導入して非同期対応

---

## 変更ファイル一覧

```
packages/anvil/
├── forge.toml                  ← A-0: パッケージマニフェスト
├── settings.json               ← A-4: 開発用認証トークン（.gitignore 対象）
├── src/
│   ├── main.forge              ← A-1: エントリポイント・Anvil struct・listen()
│   ├── request.forge           ← A-1: Request<T> data + impl
│   ├── response.forge          ← A-1: Response<T> data + impl + ErrorBody
│   ├── router.forge            ← A-2: Router struct・ルート登録・パスマッチング
│   ├── middleware.forge        ← A-3: MiddlewareChain・logger・json_parser・require_role
│   ├── cors.forge              ← A-3: CorsOptions data + impl・cors() ミドルウェア
│   └── auth.forge              ← A-4: AuthProvider trait・BearerAuthProvider・SettingsAuthProvider
├── tests/
│   ├── request.test.forge      ← A-1: Request パースのテスト
│   ├── response.test.forge     ← A-1: Response ビルダーのテスト
│   ├── router.test.forge       ← A-2: ルーティングのテスト
│   ├── middleware.test.forge   ← A-3: ミドルウェアチェーンのテスト
│   ├── cors.test.forge         ← A-3: CORS preflight のテスト
│   └── auth.test.forge         ← A-4: AuthProvider 契約テスト
└── .gitignore                  ← A-4: settings.json を除外
```

### Forge コンパイラ側で必要な機能

| 機能 | 必要ステージ | 対応状況 |
|------|------------|---------|
| `data<T>` ジェネリック型 | A-1 | **実装済み** |
| `impl<T>` ジェネリック impl | A-1 | **実装済み** |
| 高階関数 / fn 型 | A-2 | **実装済み** (`TypeAnn::Fn`, Closure 実行まで完全) |
| クロージャ (`x => expr`) | A-2 | **実装済み** (多引数・環境キャプチャも完全) |
| `map<K,V>` / `list<T>` | A-1 | **実装済み** |
| `trait` / `impl Trait for` | A-4 | **実装済み** (AST + Parser 完全実装) |
| `typestate` | A-3 | **実装済み** (AST + Parser 完全。実行ロジックは部分的) |
| `use` モジュールインポート | A-1 | **実装済み** (Local/External/Stdlib) |
| `forge.toml` パース・ビルド | A-0 | **未実装** (Cargo.toml 管理のみ) |
| `async fn` / `.await` | A-5 | **部分実装** (`.await` 式のみ、`async` キーワードなし) |

> **注意**: `forge.toml` のパースは未実装のため、A-0 が Forge ランタイム側のタスクになる。
> `trait` は実装済みのため、A-4 の `AuthProvider` は最初から `trait` 構文で書ける。
> `typestate` の実行ロジックが部分的なため、A-3 で動作確認しながら進める。

---

## Stage ごとの実装詳細

### Stage A-0: forge.toml + ビルドパイプライン

**目標**: `forge build packages/anvil/` が通る

```toml
# packages/anvil/forge.toml
[package]
name    = "anvil"
version = "0.1.0"
entry   = "src/main.forge"

[dependencies]
# A-1〜A-4: なし
# A-5 以降:
# tokio = "1"
```

- `forge.toml` の `[package]` / `[dependencies]` パース
- `entry` フィールドによるエントリポイント解決
- ビルド成果物: `target/anvil/` 以下に Rust コードを生成

---

### Stage A-1: TCP + HTTP/1.1 基礎

**目標**: `app.listen(3000)` で起動し、固定レスポンスを返せる

#### request.forge

```forge
data Request<T> {
    method:   string,
    path:     string,
    headers:  map<string, string>,
    params:   map<string, string>,
    query:    map<string, string>,
    raw_body: string,
}

impl<T> Request<T> {
    fn body(self) -> T!        // JSON デシリアライズ（A-1 は raw_body 返しのみ）
    fn header(self, name: string) -> string?
}
```

#### response.forge

```forge
data Response<T> {
    status:  number,
    headers: map<string, string>,
    body:    T?,
}

data ErrorBody {
    message: string,
    code:    number,
}

impl<T> Response<T> {
    fn text(body: string)  -> Response<string>
    fn json(body: T)       -> Response<T>
    fn empty(code: number) -> Response<()>
    fn status(self, code: number) -> Response<T>
    fn header(self, key: string, value: string) -> Response<T>
}
```

#### main.forge

```forge
data Anvil {
    // ルート・ミドルウェアのリスト（内部）
}

impl Anvil {
    fn new() -> Anvil
    fn listen(self, port: number)
}
```

**生成 Rust の要点**:
- `std::net::TcpListener::bind()`
- HTTP/1.1 リクエストラインの手動パース（`BufReader` + `read_line`）
- レスポンスの手動シリアライズ（`write_all`）

---

### Stage A-2: ルーティング

**目標**: GET/POST 等のルート登録・パスパラメータ解決が動く

#### router.forge

```forge
data Route {
    method:  string,
    pattern: string,           // "/users/:id" など
    handler: fn(Request<string>) -> Response<string>!,
}

data Router {
    routes:  list<Route>,
    prefix:  string,
}

impl Router {
    fn new() -> Router
    fn get(self, path: string, handler: fn(...) -> ...) -> Router
    fn post(self, path: string, handler: fn(...) -> ...) -> Router
    fn put(self, path: string, handler: fn(...) -> ...) -> Router
    fn delete(self, path: string, handler: fn(...) -> ...) -> Router
    fn patch(self, path: string, handler: fn(...) -> ...) -> Router
    fn any(self, path: string, handler: fn(...) -> ...) -> Router
    fn mount(self, prefix: string, router: Router) -> Router
}
```

**パスマッチングアルゴリズム**:
1. パターンを `/` で分割
2. `:name` セグメントは任意文字列にマッチ → `params` に格納
3. `*name` セグメントは残りのパスにマッチ（ワイルドカード）
4. 登録順に評価し最初にマッチしたルートを使用

**クエリパラメータ**:
- URL の `?` 以降を `&` で分割 → `key=value` をパース
- `req.query` に格納

---

### Stage A-3: ミドルウェア・組み込み機能

**目標**: `app.use(middleware)` が動き、CORS preflight が通る

#### middleware.forge

```forge
// ミドルウェアの型: fn(req, next) -> Response!
fn logger() -> fn(Request<string>, fn(Request<string>) -> Response<string>!) -> Response<string>!
fn json_parser() -> fn(Request<string>, ...) -> Response<string>!
fn static_files(dir: string) -> fn(Request<string>, ...) -> Response<string>!
fn require_role(role: string) -> fn(Request<string>, ...) -> Response<string>!
```

#### cors.forge

```forge
data CorsOptions {
    allow_origins:     list<string>,
    allow_methods:     list<string>,
    allow_headers:     list<string>,
    allow_credentials: bool,
    max_age:           number?,
}

impl CorsOptions {
    fn any()              -> CorsOptions
    fn origin(o: string)  -> CorsOptions
}

fn cors(opts: CorsOptions) -> fn(Request<string>, ...) -> Response<string>!
```

**CORS 処理**:
- `OPTIONS` リクエスト（preflight）: 200 + CORS ヘッダを返して終了
- 通常リクエスト: `next(req)` を呼び、レスポンスに CORS ヘッダを追記

**typestate (RequestLifecycle)** は A-3 フェーズ後半で実装。
Forge の `typestate` キーワード対応が必要なため、先行して `data` + state フィールドで仮実装する。

---

### Stage A-4: 認証・認可

**目標**: `SettingsAuthProvider::load()` で settings.json を読み、Bearer トークン認証が動く

#### auth.forge

```forge
data AuthContext {
    user_id: string,
    roles:   list<string>,
}

// trait が未実装の間は data + fn フィールドで代替
data AuthProvider {
    authenticate: fn(Request<string>) -> AuthContext!,
}

data BearerAuthProvider {
    tokens: map<string, AuthContext>,
}

data SettingsAuthProvider {
    inner: BearerAuthProvider,
}
```

**settings.json の読み込み**:
- `std::fs::read_to_string("settings.json")` → 手動 JSON パース
  （`serde_json` を使わず `std` のみで実装する）
- または `serde_json` を `[dependencies]` に追加する（JSON パースのため許容）

**auth_middleware**:
```forge
fn auth_middleware(provider: AuthProvider) -> fn(Request<string>, ...) -> Response<string>!
```

**.gitignore**:
```
packages/anvil/settings.json
packages/*/settings.json
```

---

### Stage A-5: 非同期（将来）

- `forge.toml` に `tokio = "1"` を追加
- `async fn` ハンドラ・ミドルウェアへの対応
- スレッドプール（`std::thread`）から `tokio::spawn` へ移行
- コネクションプール

---

## 依存クレートまとめ

| Stage | クレート | forge.toml |
|-------|---------|-----------|
| A-1〜A-3 | なし | (空) |
| A-4 | `serde_json`（JSON パース用・任意） | `serde_json = "1"` |
| A-5 | `tokio` | `tokio = { version = "1", features = ["full"] }` |

---

## テスト方針

- テストファイルは `tests/*.test.forge` に配置
- `forge test packages/anvil/` で実行
- 組み込みの `assert_ok` / `assert_err` / `assert_eq` を使用
- HTTP サーバを実際に起動するテストは Stage A-2 以降（ポート衝突に注意）
- 純粋関数（パスマッチング・CORS ヘッダ生成・認証ロジック）は単体テストで網羅

---

## 実装順序（推奨）

```
A-0 → A-1 (Request/Response) → A-1 (Anvil/listen) →
A-2 (Router/routing) → A-3 (middleware chain) →
A-3 (cors) → A-3 (logger/json_parser) →
A-4 (BearerAuthProvider) → A-4 (SettingsAuthProvider) →
A-4 (require_role) → A-5 (async)
```
