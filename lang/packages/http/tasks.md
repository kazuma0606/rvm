# `forge/http` タスク一覧

> 仕様: `lang/packages/http/spec.md`
> 計画: `lang/packages/http/plan.md`
> 実装: `crates/forge-stdlib/src/http.rs`

---

## Phase H-1: Rust 実装（RequestBuilder / Response）

### H-1-A: 依存・ファイル準備
- [x] `crates/forge-stdlib/Cargo.toml` に `reqwest = { version = "0.12", features = ["blocking", "json"] }` を追加
- [x] `crates/forge-stdlib/Cargo.toml` の `dev-dependencies` に `mockito = "1"` を追加
- [x] `crates/forge-stdlib/src/http.rs` を新規作成
- [x] `crates/forge-stdlib/src/lib.rs` に `pub mod http` を追加

### H-1-B: RequestBuilder 実装
- [x] `RequestBuilder` 構造体（method / url / headers / query / body / timeout_ms / retry_count）
- [x] `get(url: &str) -> RequestBuilder` 関数
- [x] `post(url: &str) -> RequestBuilder` 関数
- [x] `put(url: &str) -> RequestBuilder` 関数
- [x] `patch(url: &str) -> RequestBuilder` 関数
- [x] `delete(url: &str) -> RequestBuilder` 関数
- [x] `.header(key, value) -> RequestBuilder` メソッド
- [x] `.query(map) -> RequestBuilder` メソッド
- [x] `.json(value) -> RequestBuilder` メソッド（`Content-Type: application/json` 自動付与）
- [x] `.form(map) -> RequestBuilder` メソッド
- [x] `.timeout(ms) -> RequestBuilder` メソッド
- [x] `.retry(n) -> RequestBuilder` メソッド
- [x] `.send() -> Result<Response, String>` メソッド（reqwest::blocking 使用）

### H-1-C: Response 実装
- [x] `Response` 構造体（status / ok / headers / body_bytes）
- [x] `.text() -> Result<String, String>` メソッド
- [x] `.json() -> Result<serde_json::Value, String>` メソッド
- [x] `.bytes() -> Result<Vec<u8>, String>` メソッド

### H-1-D: リトライ実装
- [x] `.send()` 内でリトライループ（`retry_count` 回・100ms × attempt の線形バックオフ）
- [x] リトライ対象: ネットワークエラー・5xx レスポンス

### H-1-E: ユニットテスト（`crates/forge-stdlib/tests/http.rs`）
- [x] `test_get_request_builder` — `get(url)` が正しい method / url を持つ `RequestBuilder` を返す
- [x] `test_post_with_json_body` — `.json(value)` が body と `Content-Type` を設定する
- [x] `test_form_body` — `.form(map)` が body を設定する
- [x] `test_header_chaining` — 複数 `.header()` 呼び出しで全ヘッダーが蓄積される
- [x] `test_query_params` — `.query(map)` がクエリパラメータを設定する
- [x] `test_timeout_setting` — `.timeout(5000)` が timeout_ms を設定する
- [x] `test_retry_setting` — `.retry(3)` が retry_count を設定する
- [x] `test_response_ok_flag` — status 200 で `ok == true`、status 400 で `ok == false`
- [x] `test_send_get_mock` — mockito モックサーバーで GET リクエストを送信し `Response` を受け取る
- [x] `test_send_post_json_mock` — mockito で POST + JSON ボディを確認
- [x] `test_response_text` — `res.text()` でボディ文字列を取得
- [x] `test_response_json` — `res.json()` で JSON オブジェクトを取得
- [x] `test_retry_on_server_error` — 5xx レスポンスで指定回数リトライされる（mockito で制御）

---

## Phase H-2: インタープリタ統合

### H-2-A: モジュール登録
- [x] `forge-vm` / `forge-stdlib` のネイティブ関数ディスパッチに `forge/http` を追加
- [x] `get` / `post` / `put` / `patch` / `delete` を `Value` レベルで呼び出し可能にする
- [x] `RequestBuilder` を `Value::Struct { type_name: "HttpRequest" }` でラップ
- [x] `.header` / `.query` / `.json` / `.form` / `.timeout` / `.retry` / `.send` メソッドを `Value` 経由で呼び出せるようにする
- [x] `Response` を `Value::Struct { type_name: "HttpResponse" }` でラップ
- [x] `res.status` / `res.ok` をフィールドアクセスで取得できるようにする
- [x] `res.text()` / `res.json()` / `res.bytes()` をメソッド呼び出しで取得できるようにする

### H-2-B: インタープリタテスト
- [x] `test_http_get_via_interpreter` — `use forge/http.{ get }` + `get(url).send()` をインタープリタで実行し `ok` / `status` を確認（mockito 使用）

---

## Phase H-3: トランスパイラ変換

### H-3-A: codegen 変換ルール
- [x] `use forge/http.*` → `use reqwest;` への変換
- [x] `get(url)` → `reqwest::Client::new().get(url)` への変換
- [x] `post(url)` → `reqwest::Client::new().post(url)` への変換
- [x] `put(url)` / `patch(url)` / `delete(url)` → 対応する `reqwest::Client::new().メソッド(url)` への変換
- [x] `.json(v)` → `.json(&v)` への変換
- [x] `.query(map)` → `.query(&map)` への変換
- [x] `.send()` → `.send().await` への変換（async 自動昇格）
- [x] `res.text()?` → `.text().await?` への変換
- [x] `res.json()?` → `.json::<serde_json::Value>().await?` への変換

### H-3-B: 生成 Cargo.toml への追記
- [x] `forge build` 時の生成 `Cargo.toml` に `reqwest = { version = "0.12", features = ["json"] }` を自動追記

### H-3-C: スナップショットテスト
- [x] `test_transpile_http_get` — `get(url).send()?` の変換結果スナップショット
- [x] `test_transpile_http_post_json` — `post(url).json(payload).send()?` の変換結果スナップショット

---

## 進捗サマリ

| Phase | タスク数 | 完了 |
|---|---|---|
| H-1 | 26 | 26 |
| H-2 | 7 | 7 |
| H-3 | 10 | 10 |
| **合計** | **43** | **43** |
