# Anvil タスクリスト

> spec.md / plan.md を元にした実装タスク一覧
> 各タスクは独立したテスト可能な単位

---

## Stage AS-0: forge/std 標準ライブラリ拡張（Rust・ツールチェーン側）

> Anvil が依存する stdlib プリミティブを `crates/forge-stdlib` に追加する
> ここが完成すると Anvil は 100% ForgeScript で書ける

### `forge/std/net` — TCP ネットワーク

- [x] `crates/forge-stdlib/src/net.rs` を新規作成する
- [x] `RawRequest` 構造体を定義する（method / path / query / headers / body: String）
- [x] `RawResponse` 構造体を定義する（status: u16 / headers / body: String）
- [x] HTTP/1.1 リクエストライン（`METHOD /path?query HTTP/1.1`）のパースを実装する
- [x] HTTP ヘッダ（`Key: Value\r\n`）の繰り返しパースを実装する
- [x] `Content-Length` に基づくボディ読み取りを実装する
- [x] レスポンスのシリアライズ（ステータスライン + ヘッダ + 空行 + ボディ）を実装する
- [x] `tcp_listen(port: u16, handler: impl Fn(RawRequest) -> RawResponse)` を実装する
- [x] `std::thread::spawn` で接続を並列処理する（A-5 まで）
- [x] `use forge/std/net.tcp_listen` で Forge コードから呼べるよう forge-stdlib に登録する
- [x] `crates/forge-stdlib` のテストに HTTP パーサーのユニットテストを追加する

### `forge/std/fs` — ファイル I/O

- [x] `crates/forge-stdlib/src/fs.rs` を新規作成する
- [x] `read_file(path: &str) -> Result<String, String>` を実装する（`std::fs::read_to_string`）
- [x] `write_file(path: &str, content: &str) -> Result<(), String>` を実装する
- [x] `file_exists(path: &str) -> bool` を実装する
- [x] `use forge/std/fs.read_file` で Forge コードから呼べるよう forge-stdlib に登録する
- [x] 存在しないパスで `read_file` が `err` を返すことのテストを書く

### `forge/std/json` — JSON パース

- [x] `crates/forge-stdlib/src/json.rs` を新規作成する
- [x] `parse(src: &str) -> Result<Value, String>` を実装する（std のみで簡易パース）
- [x] ネストしたオブジェクト・配列・文字列・数値をパースできることを確認する
- [x] `use forge/std/json.parse` で Forge コードから呼べるよう forge-stdlib に登録する
- [x] `parse` のユニットテストを書く（正常系・不正 JSON でのエラー）

---

## Stage A-0: forge.toml

- [x] `packages/anvil/forge.toml` を作成する（外部依存なし、stdlib のみ）
- [x] `forge build packages/anvil/` がエラーなく通ることを確認する

---

## Stage A-1: 型定義層（純粋 ForgeScript）

### Request\<T\>

- [x] `src/request.forge` に `data Request<T>` を定義する（method / path / headers / params / query / raw_body）
- [x] `impl<T> Request<T>` に `fn header(self, name: string) -> string?` を実装する
- [x] `impl<T> Request<T>` に `fn body(self) -> T!` のスタブを実装する（A-1 は raw_body を返すのみ）
- [x] `tests/request.test.forge` に `Request` の構築テストを書く
- [x] `tests/request.test.forge` に `header()` の取得テストを書く

### Response\<T\>

- [x] `src/response.forge` に `data Response<T>` を定義する（status / headers / body）
- [x] `src/response.forge` に `data ErrorBody` を定義する（message / code）
- [x] `impl<T> Response<T>` に `fn text(body: string) -> Response<string>` を実装する
- [x] `impl<T> Response<T>` に `fn json(body: T) -> Response<T>` を実装する
- [x] `impl<T> Response<T>` に `fn empty(code: number) -> Response<()>` を実装する
- [x] `impl<T> Response<T>` に `fn status(self, code: number) -> Response<T>` を実装する
- [x] `impl<T> Response<T>` に `fn header(self, key: string, value: string) -> Response<T>` を実装する
- [x] `tests/response.test.forge` に `Response::text` のテストを書く
- [x] `tests/response.test.forge` に `Response::json` のテストを書く
- [x] `tests/response.test.forge` に `Response::status` チェーンのテストを書く
- [x] `tests/response.test.forge` に `Response::header` チェーンのテストを書く

### Anvil struct + listen

- [x] `src/main.forge` に `use forge/std/net.{ tcp_listen, RawRequest, RawResponse }` を記述する
- [x] `src/main.forge` に `data Anvil` を定義する（routes / middlewares リスト）
- [x] `impl Anvil` に `fn new() -> Anvil` を実装する
- [x] `impl Anvil` に `fn listen(self, port: number)` を実装する（`tcp_listen` に委譲）
- [x] `impl Anvil` に `fn dispatch(self, raw: RawRequest) -> RawResponse` を実装する（ルーター呼び出しのスタブ）
- [x] `app.listen(3000)` で起動し `curl localhost:3000/` に固定レスポンスを返せることを確認する

---

## Stage A-2: ルーティング

### Router

- [x] `src/router.forge` に `data Route` を定義する（method / pattern / handler）
- [x] `src/router.forge` に `data Router` を定義する（routes / prefix）
- [x] `impl Router` に `fn new() -> Router` を実装する
- [x] `impl Router` に `fn get(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn post(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn put(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn delete(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn patch(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn any(self, path, handler) -> Router` を実装する
- [x] `impl Router` に `fn mount(self, prefix: string, router: Router) -> Router` を実装する
- [x] `impl Anvil` に `fn get` / `post` / `put` / `delete` / `patch` / `any` を Router に委譲する

### パスマッチング

- [x] 固定パス（`/users`）の完全一致マッチを実装する
- [x] パスパラメータ（`/users/:id`）のマッチと `req.params` への格納を実装する
- [x] ワイルドカード（`/files/*path`）のマッチと `req.params` への格納を実装する
- [x] 登録順優先のルート評価を実装する
- [x] マッチしないルートで `404 ErrorBody` レスポンスを返すことを実装する
- [x] `tests/router.test.forge` に固定パスマッチのテストを書く
- [x] `tests/router.test.forge` に `:id` パラメータ抽出のテストを書く
- [x] `tests/router.test.forge` に `*path` ワイルドカードのテストを書く
- [x] `tests/router.test.forge` に 404 レスポンスのテストを書く

### クエリパラメータ

- [x] `RawRequest.query` 文字列を `&` 分割 → `key=value` パースして `req.query` に格納する
- [x] `req.query.get("key")` で値が取得できることを確認する
- [x] `tests/router.test.forge` にクエリパラメータ解析のテストを書く

### ルーターのネスト

- [x] `app.mount("/users", user_router)` でプレフィックスが結合されることを確認する
- [x] `tests/router.test.forge` にネストルーターのテストを書く

### エラーハンドラ

- [x] `impl Anvil` に `fn on_error(self, handler: fn(string, Request<()>) -> Response<ErrorBody>!)` を実装する
- [x] `impl Anvil` に `fn not_found(self, handler: fn(Request<()>) -> Response<ErrorBody>!)` を実装する
- [x] `tests/router.test.forge` にカスタム 404 ハンドラのテストを書く

---

## Stage A-3: ミドルウェア・組み込み機能

### ミドルウェアチェーン

- [x] `src/middleware.forge` にミドルウェア型（`fn(Request<string>, fn(...) -> Response<string>!) -> Response<string>!`）を定義する
- [x] `impl Anvil` に `fn use(self, middleware: fn(...)) -> Anvil` を実装する
- [x] ミドルウェアが登録順に実行される（チェーン）ことを実装する
- [x] `tests/middleware.test.forge` にチェーン順序のテストを書く

### 組み込みミドルウェア

- [x] `fn logger()` を実装する（`method path → status` をコンソール出力）
- [x] `fn json_parser()` を実装する（`Content-Type: application/json` 時に raw_body をパース済みとしてマーク）
- [x] `fn static_files(dir: string)` のスタブを実装する
- [x] `tests/middleware.test.forge` に `logger` のテストを書く
- [x] `tests/middleware.test.forge` に `json_parser` のテストを書く

### CORS ミドルウェア

- [x] `src/cors.forge` に `data CorsOptions` を定義する（allow_origins / allow_methods / allow_headers / allow_credentials / max_age）
- [x] `impl CorsOptions` に `fn any() -> CorsOptions` を実装する
- [x] `impl CorsOptions` に `fn origin(o: string) -> CorsOptions` を実装する
- [x] `fn cors(opts: CorsOptions) -> fn(...)` を実装する
- [x] preflight（`OPTIONS`）リクエストに 200 + CORS ヘッダを返すことを実装する
- [x] 通常リクエストのレスポンスに CORS ヘッダを追記することを実装する
- [x] `Access-Control-Allow-Credentials` / `Access-Control-Max-Age` を条件付きで設定する
- [x] `tests/cors.test.forge` に `"cors: preflight に Access-Control-Allow-Origin が付く"` テストを書く
- [x] `tests/cors.test.forge` に `CorsOptions::origin()` で特定オリジンのみ許可されるテストを書く

### typestate RequestLifecycle

- [x] `src/main.forge` から利用する `src/lifecycle.forge` に `typestate RequestLifecycle` を定義する（states: [Raw, Parsed, Authorized, Handled]）
- [x] `Raw::parse()` → `Parsed` 遷移を実装する
- [x] `Parsed::authorize(provider: AuthProvider) -> Authorized!` を実装する
- [x] `Parsed::skip_auth() -> Authorized` を実装する
- [x] `Authorized::handle(router: Router) -> Handled!` を実装する
- [x] typestate の動作を `forge run` で確認する（インタープリタ実行ロジックの検証）

---

## Stage A-4: 認証・認可

### AuthContext + AuthProvider

- [x] `src/auth.forge` に `data AuthContext` を定義する（user_id / roles）
- [x] `src/auth.forge` に `trait AuthProvider` を定義する
  ```forge
  trait AuthProvider {
      fn authenticate(self, req: Request<string>) -> AuthContext!
  }
  ```

### BearerAuthProvider

- [x] `data BearerAuthProvider` を定義する（tokens: map\<string, AuthContext\>）
- [x] `impl AuthProvider for BearerAuthProvider` を実装する（Authorization ヘッダ → Bearer → token 検索）
- [x] `tests/auth.test.forge` に `"BearerAuthProvider: 有効なトークンで認証成功"` テストを書く
- [x] `tests/auth.test.forge` に `"BearerAuthProvider: 無効なトークンで認証失敗"` テストを書く
- [x] `tests/auth.test.forge` に `Authorization` ヘッダなしで認証失敗するテストを書く
- [x] `tests/auth.test.forge` に `Bearer ` プレフィックスなしで認証失敗するテストを書く

### SettingsAuthProvider

- [x] `packages/anvil/settings.json` を作成する（spec §6-4 の形式）
- [x] `src/auth.forge` に `use forge/std/fs.read_file` と `use forge/std/json.parse` を追加する
- [x] `data SettingsAuthProvider` を定義する（inner: BearerAuthProvider）
- [x] `fn load() -> SettingsAuthProvider!` を実装する（`read_file` + `parse` で tokens を構築）
- [x] `impl AuthProvider for SettingsAuthProvider` を BearerAuthProvider に委譲する
- [x] `tests/auth.test.forge` に `SettingsAuthProvider::load()` のテストを書く

### auth_middleware

- [x] `fn auth_middleware(provider: AuthProvider)` を実装する
- [x] 認証成功時に `X-Auth-Context` ヘッダにコンテキストを格納して `next(req)` を呼ぶ
- [x] 認証失敗時に `handle_auth_error(err)` で 401/403 レスポンスを返す
- [x] `fn handle_auth_error(err: string) -> Response<ErrorBody>!` を実装する
- [x] `tests/auth.test.forge` に auth_middleware の成功・失敗テストを書く

### require_role ミドルウェア

- [x] `src/middleware.forge` に `fn require_role(role: string) -> fn(...)` を実装する
- [x] `ctx.roles.contains(role)` で判定し、不足時は `err("forbidden: role '{role}' required")` を返す
- [x] `tests/auth.test.forge` に `"require_role: 必要ロールなしで 403"` テストを書く
- [x] `tests/auth.test.forge` に `require_role` 成功（ロールあり）のテストを書く

### .gitignore

- [x] ルートの `.gitignore` に `packages/*/settings.json` を追加する

---

## Stage A-6: SSR統合（forge/std/wasm W-0〜W-3 完了後）

> **前提**: `lang/std/v2` Phase W-0〜W-3 が完了し `forge/std/wasm` が使えること。
> Bloom コンポーネントをサーバーサイドでレンダリングし、チラつきなしのハイドレーションを実現する。

- [x] `src/ssr.forge` を新規作成する
- [x] `use forge/std/wasm.Wasm` で WASMモジュールを保持する `state _wasm: Wasm?` を定義する
- [x] `fn init(path: string)` を実装する（`Wasm.load_with(path, WasmOptions.trusted())` で起動時1回ロード）
- [x] `fn render(component: string, props: map<string, string>) -> string!` を実装する（`Wasm.call("render", ...)` でHTML文字列を取得）
- [x] `fn hydrate_script() -> string` を実装する（`forge.min.js` のローダータグを返す）
- [x] `fn layout(html: string, script: string) -> string` を実装する（SSRレスポンス用のHTMLラッパー）
- [x] Anvilのルートから `ssr.render` を呼び `Response.html()` で返せることを確認する
- [x] `tests/ssr.test.forge` に `init → render → layout` の結合テストを書く
- [x] `tests/ssr.test.forge` に `render` が有効なHTML文字列を返すテストを書く
- [x] `tests/ssr.test.forge` に `hydrate_script` が `<script>` タグを含むテストを書く

---

## Stage A-5: 非同期（将来 / Forge async 実装後）

- [x] `forge/std/net` を tokio ベースに切り替える（Anvil 側のコード変更なし）
- [x] `forge.toml` に `tokio = { version = "1", features = ["full"] }` を追加する
- [x] `fn listen()` を `async fn listen()` に変更する（`tcp_listen_async` + 名前付き dispatch 関数）
- [x] ハンドラ型を `async fn(Request<T>) -> Response<U>!` に対応する（`.await?` 自動昇格、spec §13）
- [x] ミドルウェアを async 対応にする（`dispatch_async` 経由で既存ミドルウェアチェーンをそのまま呼ぶ）
- [x] `tests/` に非同期ハンドラのテストを追加する（`tests/async_handler.test.forge`）

---

## 進捗サマリ

| Stage | タスク数 | 完了 | 進捗 |
|-------|---------|------|------|
| AS-0  | 20      | 20   | 100% |
| A-0   | 2       | 2    | 100% |
| A-1   | 22      | 22   | 100% |
| A-2   | 20      | 20   | 100% |
| A-3   | 19      | 19   | 100% |
| A-4   | 21      | 21   | 100% |
| A-5   | 6       | 6    | 100% |
| A-6   | 10      | 10   | 100% |
| **合計** | **120** | **120** | — |

