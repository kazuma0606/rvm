# `forge/http` 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-08

---

## 概要

HTTP クライアントライブラリ。
REST API 呼び出し・webhook 送信・外部サービス連携に使用する。
サーバー側 HTTP（Anvil）と対になるクライアント側ライブラリ。

---

## API

```forge
use forge/http.{ get, post, put, delete, patch, request, Response }

// シンプルな GET
let res = get("https://api.example.com/users")?

// クエリパラメータ付き GET
let res = get("https://api.example.com/users")
    .query({ page: "1", limit: "20" })
    .send()?

// ヘッダー付き POST（JSON ボディ）
let res = post("https://api.example.com/orders")
    .header("Authorization", "Bearer {token}")
    .json(payload)
    .send()?

// フォームデータ POST
let res = post("https://api.example.com/upload")
    .form({ name: "Alice", age: "30" })
    .send()?

// PUT / PATCH / DELETE
let res = put("https://api.example.com/users/1").json(update).send()?
let res = patch("https://api.example.com/users/1").json(partial).send()?
let res = delete("https://api.example.com/users/1").send()?

// タイムアウト・リトライ
let res = get("https://api.example.com/slow")
    .timeout(5000)      // ミリ秒
    .retry(3)           // 最大リトライ回数
    .send()?
```

---

## Response 型

```forge
// res.status   : number     （HTTP ステータスコード）
// res.ok       : bool       （2xx なら true）
// res.headers  : map<string, string>
// res.text()   : string!    （ボディを文字列として取得）
// res.json()   : map<string, any>!  （ボディを JSON としてパース）
// res.bytes()  : list<number>!      （バイナリボディ）
```

### 使用例

```forge
let res = get("https://api.example.com/users/1")?

if res.ok {
    let user = res.json()?
    println("ユーザー名: {user.name}")
} else {
    println("エラー: {res.status}")
}
```

---

## リクエストビルダーのメソッド一覧

| メソッド | シグネチャ | 説明 |
|---|---|---|
| `.header(k, v)` | `(string, string) -> Request` | ヘッダーを追加 |
| `.query(map)` | `(map<string, string>) -> Request` | クエリパラメータを追加 |
| `.json(value)` | `(any) -> Request` | JSON ボディを設定（Content-Type 自動付与） |
| `.form(map)` | `(map<string, string>) -> Request` | フォームデータを設定 |
| `.timeout(ms)` | `(number) -> Request` | タイムアウトをミリ秒で設定 |
| `.retry(n)` | `(number) -> Request` | リトライ回数を設定 |
| `.send()` | `() -> Response!` | リクエストを送信 |

---

## トップレベル関数

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `get` | `(url: string) -> Request` | GET リクエストを構築 |
| `post` | `(url: string) -> Request` | POST リクエストを構築 |
| `put` | `(url: string) -> Request` | PUT リクエストを構築 |
| `patch` | `(url: string) -> Request` | PATCH リクエストを構築 |
| `delete` | `(url: string) -> Request` | DELETE リクエストを構築 |

---

## Rust 変換

内部実装は `reqwest` クレートを使用。

```rust
// get(url).send()?   →  reqwest::get(url).await?
// post(url).json(v)  →  reqwest::Client::new().post(url).json(&v).send().await?
```

`forge build` では `tokio` ランタイム上で非同期実行される。
`forge run` ではインタープリタの同期的な HTTP 呼び出しとして動作（`reqwest::blocking`）。

---

## 制約

- HTTPS（TLS）はデフォルト有効
- HTTP/2 は将来対応
- 認証ヘルパー（Bearer / Basic / OAuth）は将来拡張
