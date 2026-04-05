# Anvil 仕様

> Express スタイルの ForgeScript マイクロ HTTP フレームワーク
> バージョン: 0.1.0
> 前提: ForgeScript ジェネリクス・forge.toml 実装済み

---

## 1. 設計方針

- Express.js に近い開発体験（`app.get()` / `app.post()` / `app.listen()`）
- **Rust 外部クレート依存を最小化**（Stage 1〜2 は `std` のみ）
- リクエスト・レスポンスはジェネリック型（`Request<T>` / `Response<T>`）
- typestate でリクエストのライフサイクルをコンパイル時に保証
- 非同期は Stage 4 から（それまでスレッドプールで blocking）
- **認証はプロバイダ抽象で差し替え可能**（開発用 settings.json → 本番 DB へ移行）

---

## 2. 使用イメージ

### 2-1. 基本的なルーティング

```forge
use anvil.{Anvil, Request, Response}

let app = Anvil::new()

// ルート登録
app.get("/", fn(req: Request<()>) -> Response<string>! {
    ok(Response::text("Hello, Anvil!"))
})

app.get("/users/:id", fn(req: Request<()>) -> Response<User>! {
    let id = req.params.get("id")?
    let user = find_user(id)?
    ok(Response::json(user))
})

app.post("/users", fn(req: Request<CreateUserInput>) -> Response<User>! {
    let input = req.body()?
    let user = create_user(input)?
    ok(Response::json(user).status(201))
})

// ミドルウェア（登録順に実行される）
app.use(logger())
app.use(json_parser())
app.use(cors(CorsOptions::any()))

// 起動
app.listen(3000)
println("Listening on http://localhost:3000")
```

### 2-2. メソッドチェーンによるレスポンス整形デモ

```forge
// クエリパラメータでフィルタ・ソート・整形して返す例
app.get("/users", fn(req: Request<()>) -> Response<list<UserSummary>>! {
    let role_filter  = req.query.get("role")   // string?
    let sort_by      = req.query.get("sort").unwrap_or("name")
    let limit_str    = req.query.get("limit").unwrap_or("20")
    let limit        = number(limit_str)?

    let users = list_all_users()?
        .filter(u => match role_filter {
            some(role) => u.role == role,
            none       => true,
        })
        .order_by(u => match sort_by {
            "name"    => u.name,
            "created" => u.created_at,
            _         => u.name,
        })
        .take(limit)
        .map(u => UserSummary { id: u.id, name: u.name, role: u.role })

    ok(Response::json(users))
})

data UserSummary {
    id:   number,
    name: string,
    role: string,
}
```

---

## 3. 型定義

### 3-1. Request\<T\>

```forge
data Request<T> {
    method:   string,
    path:     string,
    headers:  map<string, string>,
    params:   map<string, string>,   // パスパラメータ (:id など)
    query:    map<string, string>,   // クエリパラメータ (?key=val)
    raw_body: string,                // 生ボディ文字列
}

impl<T> Request<T> {
    // ボディを T にデシリアライズ（JSON）
    fn body(self) -> T! { /* JSON parse */ }

    fn header(self, name: string) -> string? { /* ... */ }
}
```

### 3-2. Response\<T\>

```forge
data Response<T> {
    status:  number,
    headers: map<string, string>,
    body:    T?,
}

impl<T> Response<T> {
    // 静的コンストラクタ
    fn text(body: string) -> Response<string> {
        Response { status: 200, headers: {}, body: some(body) }
    }

    fn json(body: T) -> Response<T> {
        Response {
            status: 200,
            headers: { "Content-Type": "application/json" },
            body: some(body),
        }
    }

    fn empty(code: number) -> Response<()> {
        Response { status: code, headers: {}, body: none }
    }

    // メソッドチェーン用ビルダー
    fn status(self, code: number) -> Response<T> {
        Response { status: code, ..self }
    }

    fn header(self, key: string, value: string) -> Response<T> {
        // headers にキーを追加して返す
    }
}
```

### 3-3. エラー用共通型

```forge
// アプリケーションエラーの標準レスポンスボディ
data ErrorBody {
    message: string,
    code:    number,
}
```

### 3-4. Handler 型

```forge
// ハンドラは Request<T> を受け取り Response<U>! を返す関数
// fn(req: Request<T>) -> Response<U>!
```

---

## 4. ルーティング

### 4-1. HTTP メソッド

```forge
app.get(path,    handler)
app.post(path,   handler)
app.put(path,    handler)
app.delete(path, handler)
app.patch(path,  handler)
app.any(path,    handler)   // 全メソッド
```

### 4-2. パスパターン

| パターン | 例 | 説明 |
|---|---|---|
| 固定パス | `/users` | 完全一致 |
| パラメータ | `/users/:id` | `req.params.get("id")` で取得 |
| ワイルドカード | `/files/*path` | `req.params.get("path")` で取得 |

### 4-3. ルーターのネスト

```forge
let user_router = Router::new()
user_router.get("/:id", get_user_handler)
user_router.post("/",   create_user_handler)

app.mount("/users", user_router)
// → GET /users/:id, POST /users/ が登録される
```

---

## 5. ミドルウェア

### 5-1. ミドルウェアの型

```forge
// ミドルウェアの型シグネチャ
// fn(req: Request<string>, next: fn(Request<string>) -> Response<string>!) -> Response<string>!
```

### 5-2. 組み込みミドルウェア

```forge
app.use(logger())                   // リクエストログ出力
app.use(json_parser())              // Content-Type: application/json の自動パース
app.use(cors(CorsOptions::any()))   // CORS ヘッダ付与
app.use(static_files("./public"))   // 静的ファイル配信
```

### 5-3. CORS ミドルウェア

```forge
data CorsOptions {
    allow_origins:     list<string>,   // ["*"] または ["https://example.com"]
    allow_methods:     list<string>,   // ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
    allow_headers:     list<string>,   // ["Content-Type", "Authorization"]
    allow_credentials: bool,
    max_age:           number?,        // preflight キャッシュ秒数
}

impl CorsOptions {
    // 全オリジン許可（開発用）
    fn any() -> CorsOptions {
        CorsOptions {
            allow_origins:     ["*"],
            allow_methods:     ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"],
            allow_headers:     ["Content-Type", "Authorization"],
            allow_credentials: false,
            max_age:           some(86400),
        }
    }

    // 本番用: 特定オリジンのみ許可
    fn origin(origin: string) -> CorsOptions {
        CorsOptions {
            allow_origins:     [origin],
            allow_methods:     ["GET", "POST", "PUT", "DELETE", "PATCH"],
            allow_headers:     ["Content-Type", "Authorization"],
            allow_credentials: true,
            max_age:           some(3600),
        }
    }
}

// 使用例
app.use(cors(CorsOptions::any()))
app.use(cors(CorsOptions::origin("https://app.example.com")))
```

### 5-4. カスタムミドルウェアの定義

```forge
fn my_middleware(
    req:  Request<string>,
    next: fn(Request<string>) -> Response<string>!
) -> Response<string>! {
    println("Before: {req.method} {req.path}")
    let res = next(req)?
    println("After: {res.status}")
    ok(res)
}

app.use(my_middleware)
```

---

## 6. 認証・認可

### 6-1. 設計思想

認証ロジックは `AuthProvider` トレイトに抽象化する。
プロバイダを差し替えることで、開発用の簡易認証から本番 DB 認証まで移行できる。

```
開発フェーズ: SettingsAuthProvider（settings.json）
  ↓ 差し替えのみ
本番フェーズ: DbAuthProvider（DB クエリ）/ OAuthProvider（外部 IDP）
```

### 6-2. AuthProvider トレイト

```forge
// 認証結果（成功時にハンドラに渡されるコンテキスト）
data AuthContext {
    user_id: string,
    roles:   list<string>,
}

// 認証プロバイダの抽象インターフェイス
trait AuthProvider {
    // リクエストを検査し、認証成功なら AuthContext を返す
    // 認証失敗は err("unauthorized") または err("forbidden") で表現
    fn authenticate(self, req: Request<string>) -> AuthContext!
}
```

### 6-3. Bearer トークン認証プロバイダ

```forge
// トークン → AuthContext のマップを持つ汎用プロバイダ
data BearerAuthProvider {
    tokens: map<string, AuthContext>,
}

impl AuthProvider for BearerAuthProvider {
    fn authenticate(self, req: Request<string>) -> AuthContext! {
        let auth  = req.header("Authorization").ok_or("Authorization header missing")?
        let token = auth.strip_prefix("Bearer ").ok_or("invalid Bearer format")?
        self.tokens.get(token).ok_or("invalid or expired token")?
    }
}
```

### 6-4. settings.json プロバイダ（開発・軽量デプロイ用）

`packages/anvil/settings.json`（`.gitignore` 対象）にトークンを定義する。

```json
{
  "auth": {
    "tokens": {
      "dev-token-abc123": {
        "user_id": "dev-user",
        "roles": ["admin", "user"]
      },
      "readonly-token-xyz": {
        "user_id": "guest",
        "roles": ["user"]
      }
    }
  }
}
```

```forge
// Anvil が settings.json を自動検出して構築するプロバイダ
data SettingsAuthProvider {
    // 内部: settings.json の auth.tokens から BearerAuthProvider を構築
}

impl AuthProvider for SettingsAuthProvider {
    fn authenticate(self, req: Request<string>) -> AuthContext! {
        // BearerAuthProvider に委譲
    }
}

// 使用例
let auth = SettingsAuthProvider::load()?  // settings.json を読み込む
app.use(auth_middleware(auth))
```

> **将来の移行**: `SettingsAuthProvider` を `DbAuthProvider` に差し替えるだけでよい。
> `DbAuthProvider` は同じ `AuthProvider` トレイトを実装する。

### 6-5. 認可（ロールチェック）ミドルウェア

```forge
// 特定ロールを必要とするルートへのアクセス制限
fn require_role(role: string) -> fn(Request<string>, fn(Request<string>) -> Response<string>!) -> Response<string>! {
    fn(req, next) {
        let ctx = req.header("X-Auth-Context")?.parse::<AuthContext>()?
        if ctx.roles.contains(role) {
            next(req)
        } else {
            err("forbidden: role '{role}' required")
        }
    }
}

// 使用例
let admin_router = Router::new()
admin_router.use(require_role("admin"))
admin_router.get("/dashboard", admin_dashboard_handler)
app.mount("/admin", admin_router)
```

---

## 7. typestate によるリクエストライフサイクル

typestate はフレームワーク内部のパイプライン実装に使う。
ユーザーコードからは直接操作しない（ミドルウェアチェーンが内部で処理する）。

```forge
typestate RequestLifecycle {
    states: [Raw, Parsed, Authorized, Handled]

    Raw {
        fn parse(self) -> Parsed { /* ヘッダ・ボディのパース */ }
    }

    Parsed {
        // 認証プロバイダを注入して認証を実行
        fn authorize(self, provider: AuthProvider) -> Authorized! {
            /* provider.authenticate(req) を呼び出す */
        }
        // 認証不要ルート（public エンドポイント）
        fn skip_auth(self) -> Authorized {
            /* identity = none で通過 */
        }
    }

    Authorized {
        // identity が none = public ルート、some = 認証済みユーザー
        identity: AuthContext?,

        fn handle(self, router: Router) -> Handled! { /* ルーティング */ }
    }
}
```

**保証**: typestate の型システムにより、`Parsed` 状態を経由しない限り
`Authorized` 状態には遷移できない。`authorize()` または `skip_auth()` のどちらかを
必ず呼ぶ必要があり、**未認証のリクエストがハンドラに到達できない**ことが
コンパイル時に保証される。

---

## 8. エラーハンドリング

```forge
// ハンドラが err() を返した場合のデフォルト動作
// - on_error の戻り値型は Response<ErrorBody>!（! で Result を返す）
app.on_error(fn(err: string, req: Request<()>) -> Response<ErrorBody>! {
    ok(Response::json(ErrorBody { message: err, code: 500 }).status(500))
})

// 404 ハンドラ
app.not_found(fn(req: Request<()>) -> Response<ErrorBody>! {
    ok(Response::json(ErrorBody { message: "Not Found", code: 404 }).status(404))
})

// 認証エラーのカスタムハンドリング（ミドルウェアで使用）
fn handle_auth_error(err: string) -> Response<ErrorBody>! {
    let code = if err == "forbidden" { 403 } else { 401 }
    ok(Response::json(ErrorBody { message: err, code: code }).status(code))
}
```

---

## 9. 実装ステージ

### Stage A-0: forge.toml
- `forge.toml` のパース・`forge build packages/anvil/` の動作

### Stage A-1: TCP + HTTP/1.1 基礎（`std` のみ）
- `std::net::TcpListener` でコネクション受付
- HTTP/1.1 リクエストライン・ヘッダの手動パース
- `Request<string>` / `Response<string>` の最小実装
- 固定レスポンスが返せる最小サーバ

### Stage A-2: ルーティング
- GET / POST / PUT / DELETE の登録
- パスパラメータ（`:id`）・クエリパラメータ
- ルーターのネスト
- `ErrorBody` 型の定義とエラーレスポンス

### Stage A-3: ミドルウェア・組み込み機能
- ミドルウェアチェーン
- `logger()` / `json_parser()` の実装
- `cors(CorsOptions)` の実装（preflight 対応）
- エラーハンドラ・404 ハンドラ
- typestate による Request lifecycle

### Stage A-4: 認証・認可
- `AuthProvider` トレイトの定義
- `BearerAuthProvider` の実装
- `SettingsAuthProvider`（settings.json 読み込み）の実装
- `auth_middleware(provider)` の実装
- `require_role(role)` ミドルウェアの実装
- `.gitignore` への `settings.json` 追記

### Stage A-5: 非同期
- `tokio` 統合（B-7 トランスパイラ対応済み）
- async ハンドラ・async ミドルウェア
- コネクションプール

---

## 10. 依存クレート

| Stage | 依存 | 理由 |
|---|---|---|
| A-1〜A-3 | なし（`std` のみ） | TCP・HTTP/1.1 を手書き |
| A-4 | なし（`std` のみ） | settings.json は `std::fs` で読む |
| A-5 | `tokio` | 非同期ランタイム |
| JSON サポート | `serde` / `serde_json` | JSON シリアライズ・settings.json パース |

JSON は `use serde.{Serialize, Deserialize}` により自動で `forge.toml` に追記される。

---

## 11. ディレクトリ規約

```
packages/anvil/
  forge.toml              ← パッケージマニフェスト
  settings.json           ← 認証設定（.gitignore 対象・開発用）
  src/
    main.forge            ← エントリポイント
    auth.forge            ← AuthProvider / SettingsAuthProvider
    cors.forge            ← CorsOptions / cors()
    middleware.forge      ← logger / json_parser / require_role
  tests/
    *.test.forge          ← コンパニオンテスト
```

`.gitignore` に追加すべきエントリ：
```
packages/anvil/settings.json
packages/*/settings.json
```

---

## 12. テスト方針

### テストすべき抽象（実装の詳細に依存しない）

```forge
// AuthProvider の契約テスト: 任意のプロバイダが正しく動作することを確認
test "BearerAuthProvider: 有効なトークンで認証成功" {
    let provider = BearerAuthProvider {
        tokens: {
            "valid-token": AuthContext { user_id: "user1", roles: ["user"] }
        }
    }
    let req = Request {
        method: "GET", path: "/", raw_body: "",
        headers: { "Authorization": "Bearer valid-token" },
        params: {}, query: {},
    }
    let result = provider.authenticate(req)
    assert_ok(result)
    let ctx = result.unwrap()
    assert_eq(ctx.user_id, "user1")
}

test "BearerAuthProvider: 無効なトークンで認証失敗" {
    let provider = BearerAuthProvider { tokens: {} }
    let req = Request {
        method: "GET", path: "/",
        headers: { "Authorization": "Bearer invalid" },
        params: {}, query: {}, raw_body: "",
    }
    assert_err(provider.authenticate(req))
}

// CORS: preflight リクエストに適切なヘッダが付くこと
test "cors: preflight に Access-Control-Allow-Origin が付く" {
    let opts = CorsOptions::any()
    let res = apply_cors(opts, "OPTIONS", "https://example.com")
    assert_eq(res.headers.get("Access-Control-Allow-Origin"), some("*"))
}

// require_role: ロール不足で 403 が返ること
test "require_role: 必要ロールなしで 403" {
    let ctx = AuthContext { user_id: "u1", roles: ["user"] }
    let result = check_role("admin", ctx)
    assert_err(result)
}
```

---

## 13. 未サポート（v1）

- HTTPS / TLS（`rustls` 依存になるため将来対応）
- HTTP/2
- WebSocket（将来対応）
- テンプレートエンジン
- セッション管理
- JWT 検証（JWK フェッチが必要なため将来対応）
- OAuth 2.0 / OIDC（外部クレート依存になるため将来対応）
