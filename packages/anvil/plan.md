# Anvil 実装計画

> spec.md を元にした段階的実装プラン

---

## 設計方針

- anvil は **100% ForgeScript** で書かれたパッケージ
- TCP・ファイル I/O・JSON は `forge/std` の標準ライブラリプリミティブとして提供する
- anvil は `forge/std/net` / `forge/std/fs` / `forge/std/json` を `use` するだけ
- Rust を書くのは Forge ツールチェーン側（`crates/forge-stdlib`）のみ

### 責務の分担

```
packages/anvil/src/*.forge       ← Anvil（100% ForgeScript）
  use forge/std/net.tcp_listen
  use forge/std/fs.read_file
  use forge/std/json.parse
         │
         ▼
crates/forge-stdlib/src/         ← Forge 標準ライブラリ（Rust 実装・ツールチェーン側）
  net.rs   : tcp_listen / tcp_connect
  fs.rs    : read_file / write_file / file_exists
  json.rs  : parse / stringify
```

Anvil ユーザーは Rust を一切書かない。

---

## 変更ファイル一覧

```
crates/forge-stdlib/src/         ← AS-0: 標準ライブラリ拡張（Rust）
  net.rs       (新規)            ← tcp_listen / tcp_connect
  fs.rs        (新規)            ← read_file / write_file / file_exists
  json.rs      (新規)            ← parse / stringify

packages/anvil/
├── forge.toml                   ← A-0: 依存クレートなし（stdlib のみ）
├── settings.json                ← A-4: 開発用認証トークン（.gitignore 対象）
├── src/
│   ├── main.forge               ← A-1: Anvil struct・listen()
│   ├── request.forge            ← A-1: Request<T> data + impl
│   ├── response.forge           ← A-1: Response<T> data + impl + ErrorBody
│   ├── router.forge             ← A-2: Router struct・ルート登録・パスマッチング
│   ├── middleware.forge         ← A-3: MiddlewareChain・logger・json_parser・require_role
│   ├── cors.forge               ← A-3: CorsOptions data + impl・cors() ミドルウェア
│   └── auth.forge               ← A-4: AuthProvider trait・BearerAuthProvider・SettingsAuthProvider
├── tests/
│   ├── request.test.forge       ← A-1
│   ├── response.test.forge      ← A-1
│   ├── router.test.forge        ← A-2
│   ├── middleware.test.forge    ← A-3
│   ├── cors.test.forge          ← A-3
│   └── auth.test.forge          ← A-4
└── .gitignore                   ← A-4: settings.json を除外
```

### Forge コンパイラ・標準ライブラリの確認済み機能

| 機能 | 必要ステージ | 対応状況 |
|------|------------|---------|
| `data<T>` ジェネリック型 | A-1 | **実装済み** |
| `impl<T>` ジェネリック impl | A-1 | **実装済み** |
| 高階関数 / fn 型 | A-2 | **実装済み** |
| クロージャ (`x => expr`) | A-2 | **実装済み** |
| `map<K,V>` / `list<T>` | A-1 | **実装済み** |
| `trait` / `impl Trait for` | A-4 | **実装済み** |
| `typestate` | A-3 | **実装済み**（実行ロジックは動作確認必要） |
| `use forge/std/net` | A-1 | **未実装** → AS-0 で追加 |
| `use forge/std/fs` | A-4 | **未実装** → AS-0 で追加 |
| `use forge/std/json` | A-4 | **未実装** → AS-0 で追加 |
| `async fn` / `.await` | A-5 | 部分実装（`async` キーワードなし） |

---

## Stage ごとの実装詳細

### Stage AS-0: forge/std 標準ライブラリ拡張

**目標**: Forge コードから TCP・ファイル I/O・JSON が使えるようになる

#### `forge/std/net` — TCP ネットワーク

```forge
// ForgeScript から見える API
use forge/std/net.{ tcp_listen, RawRequest, RawResponse }

// tcp_listen: ポートをバインドし、接続ごとに handler を呼ぶ（blocking）
fn tcp_listen(port: number, handler: fn(RawRequest) -> RawResponse)

data RawRequest {
    method:  string,
    path:    string,   // クエリを除いたパス
    query:   string,   // "key=val&key2=val2"
    headers: map<string, string>,
    body:    string,
}

data RawResponse {
    status:  number,
    headers: map<string, string>,
    body:    string,
}
```

Rust 実装: `crates/forge-stdlib/src/net.rs`
- `std::net::TcpListener::bind()`
- HTTP/1.1 リクエストライン・ヘッダ・ボディのパース
- レスポンスのシリアライズ
- `std::thread::spawn` で並列処理（A-5 まで）

#### `forge/std/fs` — ファイル I/O

```forge
use forge/std/fs.{ read_file, write_file, file_exists }

fn read_file(path: string) -> string!
fn write_file(path: string, content: string) -> ()!
fn file_exists(path: string) -> bool
```

Rust 実装: `crates/forge-stdlib/src/fs.rs`
- `std::fs::read_to_string`
- `std::fs::write`
- `std::path::Path::exists`

#### `forge/std/json` — JSON パース

```forge
use forge/std/json.{ parse, stringify }

fn parse(src: string) -> map<string, string>!   // 簡易パース（v1）
fn stringify(value: string) -> string
```

Rust 実装: `crates/forge-stdlib/src/json.rs`
- `std` のみで簡易 JSON パース（`serde_json` を使わず）
- または `serde_json` をオプション依存として forge-stdlib に追加

---

### Stage A-0: forge.toml

```toml
# packages/anvil/forge.toml
[package]
name    = "anvil"
version = "0.1.0"
entry   = "src/main.forge"

# stdlib のみ使用 — 外部クレート依存なし
# A-5 以降:
# [dependencies]
# tokio = { version = "1", features = ["full"] }
```

---

### Stage A-1: 型定義層（純粋 ForgeScript）

```forge
// main.forge
use forge/std/net.{ tcp_listen, RawRequest, RawResponse }

data Anvil {
    routes:      list<Route>,
    middlewares: list<fn(...)>,
}

impl Anvil {
    fn new() -> Anvil
    fn listen(self, port: number) {
        tcp_listen(port, fn(raw) => self.dispatch(raw))
    }
}
```

---

### Stage A-2: ルーティング（純粋 ForgeScript）

- `data Route` / `data Router` の定義
- 固定パス・`:id` パラメータ・`*path` ワイルドカードのマッチング（純 Forge ロジック）
- クエリ文字列の `&` 分割・`key=value` パース
- `app.mount("/prefix", router)` によるネスト

---

### Stage A-3: ミドルウェア・CORS・typestate（純粋 ForgeScript）

- ミドルウェアチェーン（高階関数）
- `data CorsOptions` + preflight 対応
- `typestate RequestLifecycle` — `typestate` キーワードで実装

---

### Stage A-4: 認証・認可（ForgeScript + forge/std/fs・json）

```forge
use forge/std/fs.read_file
use forge/std/json.parse

trait AuthProvider {
    fn authenticate(self, req: Request<string>) -> AuthContext!
}

impl SettingsAuthProvider {
    fn load() -> SettingsAuthProvider! {
        let content = read_file("settings.json")?
        let config  = parse(content)?
        // config から BearerAuthProvider を構築
    }
}
```

---

### Stage A-5: 非同期（将来）

- forge/std/net を tokio ベースに切り替え（Forge 側のコード変更なし）
- `async fn` ハンドラ・ミドルウェアへの対応

---

## 依存クレートまとめ

| Stage | 依存 | 理由 |
|-------|------|------|
| AS-0〜A-4 | なし（forge/std のみ） | stdlib を使用 |
| A-5 | `tokio`（forge.toml に追加） | 非同期ランタイム |

---

## 実装順序（推奨）

```
AS-0 (forge/std/net・fs・json を forge-stdlib に追加) →
A-0 (forge.toml 作成) →
A-1 (Request/Response 型定義) →
A-1 (Anvil::listen — forge/std/net 使用) →
A-2 (Router/パスマッチング) →
A-3 (ミドルウェアチェーン・cors・typestate) →
A-4 (BearerAuthProvider) →
A-4 (SettingsAuthProvider — forge/std/fs・json 使用) →
A-4 (require_role) →
A-5 (async)
```
