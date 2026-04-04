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

---

## 2. 使用イメージ

```forge
use anvil.{Anvil, Request, Response, Json}

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

// ミドルウェア
app.use(logger())
app.use(json_parser())

// 起動
app.listen(3000)
println("Listening on http://localhost:3000")
```

---

## 3. 型定義

### 3-1. Request<T>

```forge
data Request<T> {
    method:  string,
    path:    string,
    headers: map<string, string>,
    params:  map<string, string>,   // パスパラメータ (:id など)
    query:   map<string, string>,   // クエリパラメータ (?key=val)
    raw_body: string,               // 生ボディ文字列
}

impl<T> Request<T> {
    // ボディを T にデシリアライズ（JSON）
    fn body(self) -> T! { /* JSON parse */ }

    fn header(self, name: string) -> string? { /* ... */ }
}
```

### 3-2. Response<T>

```forge
data Response<T> {
    status:  number,
    headers: map<string, string>,
    body:    T?,
}

impl<T> Response<T> {
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

    fn status(self, code: number) -> Response<T> {
        Response { status: code, ..self }
    }

    fn header(self, key: string, value: string) -> Response<T> {
        // headers にキーを追加して返す
    }

    fn empty(code: number) -> Response<()> {
        Response { status: code, headers: {}, body: none }
    }
}
```

### 3-3. Handler 型

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

```forge
// ミドルウェアの型
// fn(req: Request<string>, next: fn(Request<string>) -> Response<string>!) -> Response<string>!

// 組み込みミドルウェア
app.use(logger())            // リクエストログ出力
app.use(json_parser())       // Content-Type: application/json の自動パース
app.use(cors("*"))           // CORS ヘッダ付与
app.use(static_files("./public"))  // 静的ファイル配信
```

### ミドルウェアの定義

```forge
fn my_middleware(
    req: Request<string>,
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

## 6. typestate によるリクエストライフサイクル

```forge
typestate RequestLifecycle {
    states: [Raw, Parsed, Authorized, Handled]

    Raw {
        fn parse(self) -> Parsed { /* ヘッダ・ボディのパース */ }
    }

    Parsed {
        fn authorize(self, token: string) -> Authorized! { /* 認証チェック */ }
        fn skip_auth(self) -> Authorized { /* 認証なしで通過 */ }
    }

    Authorized {
        fn handle(self, router: Router) -> Handled! { /* ルーティング */ }
    }
}
```

ルーティング・ミドルウェアパイプライン内部でこの typestate を使い、
**未認証のリクエストがハンドラに到達できない**ことをコンパイル時に保証する。

---

## 7. エラーハンドリング

```forge
// ハンドラが err() を返した場合のデフォルト動作
app.on_error(fn(err: string, req: Request<string>) -> Response<string> {
    Response::json(ErrorBody { message: err }).status(500)
})

// 404 ハンドラ
app.not_found(fn(req: Request<()>) -> Response<string>! {
    ok(Response::text("Not Found").status(404))
})
```

---

## 8. 実装ステージ

### Stage A-0: forge.toml
- `forge.toml` のパース・`forge build packages/anvil/` の動作

### Stage A-1: TCP + HTTP/1.1 基礎（`std` のみ）
- `std::net::TcpListener` でコネクション受付
- HTTP/1.1 リクエストライン・ヘッダの手動パース
- `Request<string>` / `Response<string>` の最小実装
- 固定レスポンスが返せる最小サーバ

### Stage A-2: ルーティング
- GET / POST / PUT / DELETE の登録
- パスパラメータ（`:id`）
- クエリパラメータ
- ルーターのネスト

### Stage A-3: ミドルウェア・組み込み機能
- ミドルウェアチェーン
- `logger()` / `json_parser()` / `cors()` / `static_files()`
- エラーハンドラ・404 ハンドラ
- typestate による Request lifecycle

### Stage A-4: 非同期
- `tokio` 統合（B-7 トランスパイラ対応済み）
- async ハンドラ・async ミドルウェア
- コネクションプール

---

## 9. 依存クレート

| Stage | 依存 | 理由 |
|---|---|---|
| A-1〜A-3 | なし（`std` のみ） | TCP・HTTP/1.1 を手書き |
| A-4 | `tokio` | 非同期ランタイム |
| JSON サポート | `serde` / `serde_json` | JSON シリアライズ |

JSON は `use serde.{Serialize, Deserialize}` により自動で `forge.toml` に追記される。

---

## 10. 未サポート（v1）

- HTTPS / TLS（`rustls` 依存になるため将来対応）
- HTTP/2
- WebSocket（将来対応）
- テンプレートエンジン
- セッション管理
