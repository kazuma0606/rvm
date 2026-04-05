# Anvil タスクリスト

> spec.md / plan.md を元にした実装タスク一覧
> 各タスクは独立したテスト可能な単位

---

## Stage A-0: forge.toml + ビルドパイプライン

- [ ] `packages/anvil/forge.toml` を作成する（`[package]` name/version/entry）
- [ ] `forge build packages/anvil/` がエラーなく通ることを確認する
- [ ] `forge.toml` の `[dependencies]` セクションが空でパースできることを確認する

---

## Stage A-1: TCP + HTTP/1.1 基礎

### Request\<T\>

- [ ] `src/request.forge` に `data Request<T>` を定義する（method/path/headers/params/query/raw_body）
- [ ] `impl<T> Request<T>` に `fn header(self, name: string) -> string?` を実装する
- [ ] `impl<T> Request<T>` に `fn body(self) -> T!` のスタブを実装する（A-1 は raw_body を返すのみ）
- [ ] `tests/request.test.forge` に `Request` の構築テストを書く

### Response\<T\>

- [ ] `src/response.forge` に `data Response<T>` を定義する（status/headers/body）
- [ ] `src/response.forge` に `data ErrorBody` を定義する（message/code）
- [ ] `impl<T> Response<T>` に `fn text(body: string) -> Response<string>` を実装する
- [ ] `impl<T> Response<T>` に `fn json(body: T) -> Response<T>` を実装する
- [ ] `impl<T> Response<T>` に `fn empty(code: number) -> Response<()>` を実装する
- [ ] `impl<T> Response<T>` に `fn status(self, code: number) -> Response<T>` を実装する
- [ ] `impl<T> Response<T>` に `fn header(self, key: string, value: string) -> Response<T>` を実装する
- [ ] `tests/response.test.forge` に `Response::text` のテストを書く
- [ ] `tests/response.test.forge` に `Response::json` のテストを書く
- [ ] `tests/response.test.forge` に `Response::status` チェーンのテストを書く
- [ ] `tests/response.test.forge` に `Response::header` チェーンのテストを書く

### Anvil struct + listen

- [ ] `src/main.forge` に `data Anvil` を定義する（内部: routes リスト・middlewares リスト）
- [ ] `impl Anvil` に `fn new() -> Anvil` を実装する
- [ ] `impl Anvil` に `fn listen(self, port: number)` のスタブを実装する（TCP 接続受付）
- [ ] 生成 Rust が `std::net::TcpListener::bind()` を使ってコンパイルできることを確認する
- [ ] HTTP/1.1 リクエストライン（`GET / HTTP/1.1`）の手動パースを実装する
- [ ] HTTP ヘッダの手動パース（`key: value\r\n` 形式）を実装する
- [ ] リクエストボディの読み取り（`Content-Length` ヘッダ利用）を実装する
- [ ] レスポンスの手動シリアライズ（ステータスライン + ヘッダ + ボディ）を実装する
- [ ] `app.listen(3000)` で起動し `curl localhost:3000/` に固定レスポンスを返せることを確認する

---

## Stage A-2: ルーティング

### Router

- [ ] `src/router.forge` に `data Route` を定義する（method/pattern/handler）
- [ ] `src/router.forge` に `data Router` を定義する（routes/prefix）
- [ ] `impl Router` に `fn new() -> Router` を実装する
- [ ] `impl Router` に `fn get(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn post(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn put(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn delete(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn patch(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn any(self, path, handler) -> Router` を実装する
- [ ] `impl Router` に `fn mount(self, prefix: string, router: Router) -> Router` を実装する
- [ ] `impl Anvil` に `fn get` / `post` / `put` / `delete` / `patch` / `any` を Router に委譲する

### パスマッチング

- [ ] 固定パス（`/users`）の完全一致マッチを実装する
- [ ] パスパラメータ（`/users/:id`）のマッチと `req.params` への格納を実装する
- [ ] ワイルドカード（`/files/*path`）のマッチと `req.params` への格納を実装する
- [ ] 登録順優先のルート評価を実装する
- [ ] マッチしないルートで `404 ErrorBody` レスポンスを返すことを実装する
- [ ] `tests/router.test.forge` に固定パスマッチのテストを書く
- [ ] `tests/router.test.forge` に `:id` パラメータ抽出のテストを書く
- [ ] `tests/router.test.forge` に `*path` ワイルドカードのテストを書く
- [ ] `tests/router.test.forge` に 404 レスポンスのテストを書く

### クエリパラメータ

- [ ] URL の `?` 以降を `&` 分割 → `key=value` パースして `req.query` に格納する
- [ ] `req.query.get("key")` で値が取得できることを確認する
- [ ] `tests/router.test.forge` にクエリパラメータ解析のテストを書く

### ルーターのネスト

- [ ] `app.mount("/users", user_router)` でプレフィックスが結合されることを確認する
- [ ] `tests/router.test.forge` にネストルーターのテストを書く

### エラーハンドラ

- [ ] `impl Anvil` に `fn on_error(self, handler: fn(string, Request<()>) -> Response<ErrorBody>!)` を実装する
- [ ] `impl Anvil` に `fn not_found(self, handler: fn(Request<()>) -> Response<ErrorBody>!)` を実装する
- [ ] `tests/router.test.forge` にカスタム 404 ハンドラのテストを書く

---

## Stage A-3: ミドルウェア・組み込み機能

### ミドルウェアチェーン

- [ ] `src/middleware.forge` にミドルウェア型（`fn(Request<string>, fn(...) -> Response<string>!) -> Response<string>!`）を定義する
- [ ] `impl Anvil` に `fn use(self, middleware: fn(...)) -> Anvil` を実装する
- [ ] ミドルウェアが登録順に実行される（チェーン）ことを実装する
- [ ] `tests/middleware.test.forge` にチェーン順序のテストを書く

### 組み込みミドルウェア

- [ ] `fn logger()` を実装する（`method path → status` をコンソール出力）
- [ ] `fn json_parser()` を実装する（`Content-Type: application/json` 時に raw_body をパース済みとしてマーク）
- [ ] `fn static_files(dir: string)` のスタブを実装する（ファイル存在確認 + Content-Type 判定）
- [ ] `tests/middleware.test.forge` に `logger` が出力することのテストを書く
- [ ] `tests/middleware.test.forge` に `json_parser` のテストを書く

### CORS ミドルウェア

- [ ] `src/cors.forge` に `data CorsOptions` を定義する（allow_origins/allow_methods/allow_headers/allow_credentials/max_age）
- [ ] `impl CorsOptions` に `fn any() -> CorsOptions` を実装する
- [ ] `impl CorsOptions` に `fn origin(o: string) -> CorsOptions` を実装する
- [ ] `fn cors(opts: CorsOptions) -> fn(...)` を実装する
- [ ] preflight（`OPTIONS`）リクエストに 200 + CORS ヘッダを返すことを実装する
- [ ] 通常リクエストのレスポンスに CORS ヘッダを追記することを実装する
- [ ] `Access-Control-Allow-Origin` / `Access-Control-Allow-Methods` / `Access-Control-Allow-Headers` ヘッダを正しく設定する
- [ ] `allow_credentials: true` のとき `Access-Control-Allow-Credentials: true` を付けることを実装する
- [ ] `max_age` が `some(n)` のとき `Access-Control-Max-Age: n` を付けることを実装する
- [ ] `tests/cors.test.forge` に `"cors: preflight に Access-Control-Allow-Origin が付く"` テストを書く
- [ ] `tests/cors.test.forge` に `CorsOptions::origin()` で特定オリジンのみ許可されるテストを書く

### typestate RequestLifecycle（仮実装）

- [ ] `Raw` / `Parsed` / `Authorized` / `Handled` を `data` + state フィールドで仮実装する
- [ ] `fn parse()` → Parsed 遷移を実装する
- [ ] `fn skip_auth()` → Authorized 遷移（identity = none）を実装する
- [ ] `fn handle(router)` → Handled 遷移を実装する
- [ ] Forge の `typestate` キーワード実装後に正式実装へ移行する（TODO コメント残す）

---

## Stage A-4: 認証・認可

### AuthContext + AuthProvider

- [ ] `src/auth.forge` に `data AuthContext` を定義する（user_id/roles）
- [ ] `src/auth.forge` に `data AuthProvider` を定義する（authenticate fn フィールド）
  - Forge の `trait` 実装後に `trait AuthProvider` へ移行する（TODO）

### BearerAuthProvider

- [ ] `data BearerAuthProvider` を定義する（tokens: map\<string, AuthContext\>）
- [ ] `fn authenticate(self, req: Request<string>) -> AuthContext!` を実装する
  - `Authorization` ヘッダを取得する
  - `Bearer ` プレフィックスを除去する
  - `self.tokens.get(token)` でコンテキストを取得する
- [ ] `tests/auth.test.forge` に `"BearerAuthProvider: 有効なトークンで認証成功"` テストを書く
- [ ] `tests/auth.test.forge` に `"BearerAuthProvider: 無効なトークンで認証失敗"` テストを書く
- [ ] `tests/auth.test.forge` に `Authorization` ヘッダなしで認証失敗するテストを書く
- [ ] `tests/auth.test.forge` に `Bearer ` プレフィックスなしで認証失敗するテストを書く

### SettingsAuthProvider

- [ ] `packages/anvil/settings.json` のサンプルファイルを作成する
- [ ] `data SettingsAuthProvider` を定義する（inner: BearerAuthProvider）
- [ ] `fn load() -> SettingsAuthProvider!` を実装する（`std::fs::read_to_string("settings.json")` + JSON パース）
- [ ] `fn authenticate(self, req: Request<string>) -> AuthContext!` を BearerAuthProvider に委譲する
- [ ] `tests/auth.test.forge` に `SettingsAuthProvider::load()` のテストを書く（サンプル settings.json を使用）

### auth_middleware

- [ ] `fn auth_middleware(provider: AuthProvider)` を実装する
- [ ] 認証成功時に `X-Auth-Context` ヘッダにコンテキストを格納して `next(req)` を呼ぶ
- [ ] 認証失敗時に `handle_auth_error(err)` で 401/403 レスポンスを返す
- [ ] `fn handle_auth_error(err: string) -> Response<ErrorBody>!` を実装する（401/403 分岐）
- [ ] `tests/auth.test.forge` に auth_middleware の成功・失敗テストを書く

### require_role ミドルウェア

- [ ] `src/middleware.forge` に `fn require_role(role: string) -> fn(...)` を実装する
- [ ] `X-Auth-Context` ヘッダから `AuthContext` を取得する
- [ ] `ctx.roles.contains(role)` で判定し、不足時は `err("forbidden: role '{role}' required")` を返す
- [ ] `tests/auth.test.forge` に `"require_role: 必要ロールなしで 403"` テストを書く
- [ ] `tests/auth.test.forge` に `require_role` 成功（ロールあり）のテストを書く

### .gitignore

- [ ] `packages/anvil/.gitignore`（または ルート `.gitignore`）に `packages/anvil/settings.json` を追加する
- [ ] `packages/*/settings.json` もグローバル除外に追加する

---

## Stage A-5: 非同期（将来 / Forge async 実装後）

- [ ] `forge.toml` に `tokio = { version = "1", features = ["full"] }` を追加する
- [ ] `fn listen()` を `async fn listen()` に変更する
- [ ] ハンドラ型を `async fn(Request<T>) -> Response<U>!` に対応する
- [ ] ミドルウェアを async 対応にする
- [ ] `std::thread::spawn` から `tokio::spawn` に移行する
- [ ] コネクションプールを実装する
- [ ] `tests/` に非同期ハンドラのテストを追加する

---

## 進捗サマリ

| Stage | タスク数 | 完了 | 進捗 |
|-------|---------|------|------|
| A-0   | 3       | 0    | 0%   |
| A-1   | 18      | 0    | 0%   |
| A-2   | 18      | 0    | 0%   |
| A-3   | 20      | 0    | 0%   |
| A-4   | 20      | 0    | 0%   |
| A-5   | 7       | 0    | 0%   |
| **合計** | **86** | **0** | **0%** |
