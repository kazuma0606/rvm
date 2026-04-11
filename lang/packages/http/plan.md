# `forge/http` 実装計画

> 仕様: `lang/packages/http/spec.md`
> 前提: forge-stdlib クレート・モジュールシステム・トランスパイラ B-0〜B-8 が完成済み

---

## フェーズ構成

```
Phase H-1: Rust 実装（RequestBuilder / Response / reqwest::blocking）
Phase H-2: インタープリタ統合（forge/http モジュールとして登録）
Phase H-3: トランスパイラ変換（reqwest async への codegen）
```

H-1 → H-2 → H-3 の順に実施する。H-3 は H-2 完成後。

---

## Phase H-1: Rust 実装

### 目標

`reqwest::blocking` を使って `forge run` 上で HTTP リクエストを同期的に送受信できること。
`RequestBuilder` チェーンと `Response` 型を Rust で実装する。

### 実装ステップ

1. **`crates/forge-stdlib/src/http.rs` を新規作成**
   - `RequestBuilder` 構造体
     - フィールド: `method`, `url`, `headers`, `query`, `body`, `timeout_ms`, `retry_count`
     - メソッド: `.header(k, v)` / `.query(map)` / `.json(value)` / `.form(map)`
     - メソッド: `.timeout(ms)` / `.retry(n)` / `.send() -> Result<Response, String>`
   - `Response` 構造体
     - フィールド: `status: u16`, `ok: bool`, `headers: HashMap<String, String>`
     - メソッド: `.text() -> Result<String, String>`
     - メソッド: `.json() -> Result<serde_json::Value, String>`
     - メソッド: `.bytes() -> Result<Vec<u8>, String>`
   - トップレベル関数: `get` / `post` / `put` / `patch` / `delete`
     - いずれも `RequestBuilder` を返す
   - リトライ: `retry_count` 回まで再送信（線形バックオフ 100ms ×回数）
   - タイムアウト: `reqwest::blocking::ClientBuilder::timeout` に渡す

2. **`crates/forge-stdlib/Cargo.toml` に依存追加**
   - `reqwest = { version = "0.12", features = ["blocking", "json"] }`

3. **`crates/forge-stdlib/src/lib.rs` に `pub mod http` を追加**

### テスト方針（`crates/forge-stdlib/tests/http.rs`）

- ユニットテストは `mockito` または `wiremock` でローカルサーバーをモックする
- ネットワーク依存をなくし CI で確実に通過させる
- `mockito = "1"` を dev-dependencies に追加

---

## Phase H-2: インタープリタ統合

### 目標

`use forge/http.{ get, post }` で ForgeScript ファイルから HTTP 関数を呼べること。

### 実装ステップ

1. **モジュールディスパッチへの登録**
   - `forge-stdlib` のネイティブ関数ディスパッチテーブルに `forge/http` を追加
   - `get(url)` → `http::get(url)` → `RequestBuilder` を `Value::NativeObject` として返す
   - `RequestBuilder` のメソッド呼び出し（`.header` 等）を `Value` レベルで処理
   - `Response` も `Value::NativeObject` でラップし `.status` / `.ok` / `.text()` / `.json()` を公開

2. **`Value::NativeObject` パターンの確認**
   - 既存の実装（`forge-vm` 側の `Value` 型）に合わせてラップする
   - `forge run` で `let res = get("http://...")?.text()?` が動くことを確認

### テスト方針

- `test_http_get_via_interpreter`: インタープリタ経由で GET を呼び出し、`Response` の `ok` / `status` を確認
- `mockito` のモックサーバーを使用

---

## Phase H-3: トランスパイラ変換

### 目標

`forge build` 時に `reqwest` 非同期コードが生成されること。

### 変換ルール

| ForgeScript | 生成 Rust |
|---|---|
| `get(url).send()?` | `reqwest::get(url).await?` |
| `post(url).json(v).send()?` | `reqwest::Client::new().post(url).json(&v).send().await?` |
| `.header(k, v)` | `.header(k, v)` |
| `.query(map)` | `.query(&map)` |
| `.form(map)` | `.form(&map)` |
| `.timeout(ms)` | (ClientBuilder に移動) |
| `.retry(n)` | `forge_retry` ヘルパーにラップ |
| `res.text()?` | `.text().await?` |
| `res.json()?` | `.json::<serde_json::Value>().await?` |

### 実装ステップ

1. **`crates/forge-transpiler/src/codegen.rs` に `forge/http` 変換ルールを追加**
   - `use forge/http.*` を検出して `use reqwest;` に変換
   - `get` / `post` 等のトップレベル呼び出しを対応する `reqwest::...` に変換
   - `.send()` を `.send().await` に変換（`analyze_async` で自動昇格）

2. **生成 `Cargo.toml` に `reqwest` を追加**
   - `forge build` 生成物の `Cargo.toml` に `reqwest = { version = "0.12", features = ["json"] }` を自動追記
   - `tokio` は既存の async 変換で既に追加されているため重複しない

### テスト方針（スナップショット）

- `test_transpile_http_get`: `get(url).send()?` の変換結果を確認
- `test_transpile_http_post_json`: `post(url).json(payload).send()?` の変換結果を確認

---

## 依存クレート

| クレート | バージョン | 用途 |
|---|---|---|
| `reqwest` | 0.12 | HTTP クライアント本体（blocking + json feature） |
| `mockito` | 1 | テスト用モックサーバー（dev-dependency） |

---

## 実装後の確認

```
cargo test -p forge-stdlib http   # H-1・H-2 のテストが全通過
cargo test -p forge-transpiler http  # H-3 スナップショットが通過
```
