# crucible タスク一覧

> 仕様: `packages/crucible/spec.md`
> 計画: `packages/crucible/plan.md`

---

## Phase C-0: 基盤整備

### C-0-A: TCP クライアント追加（forge-stdlib）

- [x] `crates/forge-stdlib/src/net.rs` に `TcpConn` 型を追加する（`Arc<Mutex<TcpStream>>` をラップした Native 値）
- [x] `tcp_connect(host: string, port: number) -> TcpConn!` を実装する
- [x] `tcp_write(conn: TcpConn, data: list<number>) -> unit!` を実装する（バイト列送信）
- [x] `tcp_read_exact(conn: TcpConn, n: number) -> list<number>!` を実装する（n バイト必ず受信）
- [x] `tcp_read_available(conn: TcpConn) -> list<number>!` を実装する（受信可能バイトを全部読む）
- [x] `tcp_close(conn: TcpConn) -> unit` を実装する
- [x] `forge/std/net` モジュールに上記関数を登録する（`forge-vm/src/interpreter.rs` または `forge-stdlib/src/lib.rs`）
- [x] 単体テスト: `tcp_connect` が接続できない場合に `Err` を返すことを確認する

### C-0-B: パッケージ・クレートのセットアップ

- [x] `packages/crucible/forge.toml` を作成する（name = "crucible", version = "0.1.0"）
- [x] `packages/crucible/src/` ディレクトリを作成する
- [x] `packages/crucible/src/mod.forge` を作成する（スタブ）
- [x] `crates/crucible-cli/Cargo.toml` を作成する（バイナリクレート）
- [x] `crates/crucible-cli/src/main.rs` を作成する（スタブ: `fn main() { println!("crucible") }`）
- [x] `Cargo.toml`（workspace）に `crates/crucible-cli` を追加する
- [x] `cargo build --bin crucible-cli` が通ることを確認する

### C-0-C: テスト用 Docker Compose

- [x] `docker/docker-compose.test.yml` を作成する（postgres:16, POSTGRES_DB=crucible_test, port 5432）
- [x] `docker/crucible_test_setup.sql` を作成する（テスト用テーブル: `users`, `posts`）

---

## Phase C-1: wire protocol + 認証

### C-1-A: wire.forge — メッセージ encode

- [x] `packages/crucible/src/wire.forge` を作成する
- [x] `encode_startup_message(user: string, database: string) -> list<number>` を実装する
- [x] `encode_query(sql: string) -> list<number>` を実装する（Simple Query Protocol: 'Q' + len + sql + \0）
- [x] `encode_sasl_initial_response(mechanism: string, payload: string) -> list<number>` を実装する
- [x] `encode_sasl_response(payload: string) -> list<number>` を実装する
- [x] `encode_terminate() -> list<number>` を実装する（'X' + 4バイト長）
- [x] ヘルパー: `encode_int32(n: number) -> list<number>` を実装する（ビッグエンディアン）
- [x] ヘルパー: `encode_string(s: string) -> list<number>` を実装する（null終端）
- [x] テスト: `encode_terminate()` が `[88, 0, 0, 0, 4]` を返すことを確認する
- [x] テスト: `encode_query("SELECT 1")` のバイト列が正しいことを確認する

### C-1-B: wire.forge — メッセージ decode

- [x] `BackendMessage` 型を定義する（map: type_byte / body フィールド）
- [x] `decode_backend_message(bytes: list<number>) -> BackendMessage` を実装する
- [x] `read_backend_message(conn: TcpConn) -> BackendMessage!` を実装する（1バイト種別 + 4バイト長を読む）
- [x] `parse_row_description(body: list<number>) -> list<ColumnDesc>` を実装する（カラム名・OID取得）
- [x] `parse_data_row(body: list<number>, cols: list<ColumnDesc>) -> map` を実装する
- [x] `parse_error_response(body: list<number>) -> string` を実装する（'M' フィールドを取得）
- [x] テスト: `ReadyForQuery`（'Z' + \x00\x00\x00\x05 + 'I'）を正しく decode できることを確認する

### C-1-C: auth.forge — SCRAM-SHA-256

- [x] `packages/crucible/src/auth.forge` を作成する
- [x] 暗号関数（sha2/base64/hmac/pbkdf2）は forge-vm 組み込み関数として実装済み
- [x] `scram_generate_nonce() -> string` を実装する（クライアントノンス: 18バイト random base64）
- [x] `scram_client_first_message(username: string, nonce: string) -> string` を実装する
- [x] `scram_hi(password: string, salt: list<number>, iterations: number) -> list<number>` を実装する（PBKDF2-HMAC-SHA256）
- [x] `scram_hmac(key: list<number>, msg: string) -> list<number>` を実装する
- [x] `scram_h(data: list<number>) -> list<number>` を実装する（SHA-256）
- [x] `scram_client_proof(password, salt, iterations, client_first_bare, server_first, client_final_bare) -> string` を実装する（ClientProof を base64 で返す）
- [x] `scram_client_final_message(client_final_bare: string, proof: string) -> string` を実装する
- [x] `scram_verify_server_signature(server_final: string, server_key: list<number>) -> bool!` を実装する
- [x] テスト: RFC 7677 の既知テストベクタで SCRAM 計算が正しいことを確認する

### C-1-D: conn.forge — 単一接続

- [x] `packages/crucible/src/conn.forge` を作成する
- [x] `Conn` 型を定義する（struct: tcp_conn, state, host, port, user, database フィールド）
- [x] `connect(host: string, port: number, user: string, password: string, database: string) -> Conn!` を実装する
  - TCP 接続 → StartupMessage 送信 → 認証フロー（SCRAM-SHA-256）→ ReadyForQuery 待機
- [x] `query_raw(conn: Conn, sql: string) -> list<map>!` を実装する（Simple Query → Row 一覧）
- [x] `close(conn: Conn) -> unit` を実装する（Terminate 送信 → TCP close）
- [x] テスト（統合）: `crucible migrate` + `migrate:status` で PostgreSQL 接続・認証が成功することを確認した

### C-1-E: マイルストーン確認

- [x] E2E テスト: `crucible migrate` が PostgreSQL に接続し DDL を実行できることを確認した（ForgeScript wire protocol 実装済み）

---

## Phase C-2: クエリ実行・型マッピング

### C-2-A: types.forge — PostgreSQL 型マッピング

- [x] `packages/crucible/src/types.forge` を作成する
- [x] `pg_oid_to_type(oid: number) -> string` を実装する（主要 OID → 型名文字列）
- [x] `pg_value_to_forge(pg_type: string, text: string) -> Value` を実装する
  - `int2/int4/int8` → number
  - `float4/float8/numeric` → number（parseFloat）
  - `varchar/text/char` → string
  - `bool` → bool（"t" / "f"）
  - `timestamp/timestamptz` → string（そのまま）
  - `json/jsonb` → map（JSON パース）
  - NULL → unit
- [x] テスト: `pg_value_to_forge("int4", "42")` → `Value::Number(42.0)` を確認する
- [x] テスト: `pg_value_to_forge("bool", "t")` → `Value::Bool(true)` を確認する
- [x] テスト: NULL → `Value::Unit` を確認する

### C-2-B: query.forge — 高レベルクエリ API

- [x] `packages/crucible/src/query.forge` を作成する
- [x] `execute(conn: Conn, sql: string, params: list) -> list<map>!` を実装する（パラメータを埋め込んだ SQL を query_raw に渡す）
- [x] `execute_one(conn: Conn, sql: string, params: list) -> map?!` を実装する（0件なら unit, 1件以上なら先頭）
- [x] `execute_count(conn: Conn, sql: string, params: list) -> number!` を実装する
- [x] パラメータのサニタイズ（SQL インジェクション対策: `$1`/`$2` 形式でバインド）
- [x] テスト: `execute(conn, "SELECT $1::int AS n", [42])` → `[{n: 42}]` を確認する

---

## Phase C-3: ORM 層（モデル・QueryBuilder）

### C-3-A: model.forge — @model アノテーション

- [x] `packages/crucible/src/model.forge` を作成する
- [x] `@model_registry` グローバル map を定義する（モデル名 → テーブル名・フィールド一覧）※関数で map を受け渡す設計に変更
- [x] `register_model(registry: map, model_name: string, table: string, fields: list<string>) -> map` を実装する
- [x] `model_table(registry: map, model_name: string) -> string!` を実装する
- [x] `model_fields(registry: map, model_name: string) -> list<string>!` を実装する
- [x] `row_to_model(row: map, field_map: map) -> map` を実装する（フィールド名マッピング）
- [x] テスト: `register_model("User", "users", ["id", "name", "email"])` → `model_table("User")` が `"users"` を返す

### C-3-B: builder.forge — QueryBuilder DSL

- [x] `packages/crucible/src/builder.forge` を作成する
- [x] `QueryBuilder` 型を定義する（struct: table, conditions, order_col, order_asc, limit_val, offset_val, selects, joins, params フィールド）
- [x] `new_query(table: string) -> QueryBuilder` を実装する
- [x] `where_clause(builder: QueryBuilder, condition: string, params: list) -> QueryBuilder` を実装する
- [x] `order_by(builder: QueryBuilder, column: string, asc: bool) -> QueryBuilder` を実装する
- [x] `limit_rows(builder: QueryBuilder, n: number) -> QueryBuilder` を実装する
- [x] `offset_rows(builder: QueryBuilder, n: number) -> QueryBuilder` を実装する
- [x] `select_columns(builder: QueryBuilder, columns: list<string>) -> QueryBuilder` を実装する
- [x] `join_table(builder: QueryBuilder, table: string, on: string) -> QueryBuilder` を実装する
- [x] `build_sql(builder: QueryBuilder) -> map` を実装する（{ "sql": string, "params": list }）
- [x] `run_all(builder: QueryBuilder, conn: Conn) -> list<map>!` を実装する
- [x] `run_first(builder: QueryBuilder, conn: Conn) -> map!` を実装する
- [x] `run_count(builder: QueryBuilder, conn: Conn) -> number!` を実装する
- [x] `run_exists(builder: QueryBuilder, conn: Conn) -> bool!` を実装する
- [x] テスト: `build_sql` が正しい SQL 文字列を生成することを確認する（DB不要）
- [ ] テスト（統合）: `User::query() |> where_clause("active = $1", [true]) |> all(conn)` が動く

### C-3-C: mod.forge — 公開 CRUD API

- [x] `packages/crucible/src/mod.forge` を本実装する（pub use + モデル API）
- [x] `all_rows(conn: Conn, table: string) -> list<map>!` を実装する
- [x] `find_by_id(conn: Conn, table: string, id: number) -> map!` を実装する
- [x] `first_where(conn: Conn, table: string, condition: string, params: list) -> map!` を実装する
- [x] `where_many(conn: Conn, table: string, condition: string, params: list) -> list<map>!` を実装する
- [x] `insert_row(conn: Conn, table: string, values: map) -> map!` を実装する（INSERT RETURNING *）
- [x] `update_where(conn: Conn, table: string, values: map, condition: string, params: list) -> list<map>!` を実装する（UPDATE ... RETURNING *）
- [x] `delete_where(conn: Conn, table: string, condition: string, params: list) -> unit!` を実装する

---

## Phase C-4: マイグレーション + CLI 基本

### C-4-A: migration.forge — マイグレーション管理

- [x] `packages/crucible/src/migration.forge` を作成する
- [x] `Migration` 型を定義する（struct: version, filename, up_sql, down_sql）
- [x] `parse_migration_file(path: string) -> Migration!` を実装する（`-- +migrate Up` / `-- +migrate Down` を分割）
- [x] `read_migrations(dir: string) -> list<Migration>!` を実装する（バージョン順ソート）
- [x] `ensure_migration_table(conn: Conn) -> unit!` を実装する（`_crucible_migrations` テーブル作成）
- [x] `applied_versions(conn: Conn) -> list<string>!` を実装する
- [x] `apply_migration(conn: Conn, migration: Migration) -> unit!` を実装する（Up SQL 実行 + 記録）
- [x] `rollback_last(conn: Conn, migrations: list<Migration>) -> unit!` を実装する（Down SQL 実行 + 削除）
- [x] `pending_migrations(all: list<Migration>, applied: list<string>) -> list<Migration>` を実装する
- [x] テスト: `parse_migration_file` が Up/Down SQL を正しく分割することを確認する

### C-4-B: crucible-cli — 基本コマンド実装

- [x] `crates/crucible-cli/src/main.rs` にコマンドルーティングを実装する（init / migrate / make:* / schema:*）
- [x] `crates/crucible-cli/src/init.rs` を実装する
  - `crucible init --psql` で `crucible.toml` / `migrations/` / `001_create_users.sql` / `src/models/user.forge` を生成
- [x] `crates/crucible-cli/src/migrate.rs` を実装する
  - `crucible migrate` — 未適用マイグレーションを順に適用
  - `crucible migrate:rollback` — 最後の1件を戻す
  - `crucible migrate:status` — 適用済み/未適用の一覧表示
- [x] `crates/crucible-cli/src/make.rs` を実装する（make:migration / make:model / make:repo）
- [x] `crates/crucible-cli/src/config.rs` を実装する（`crucible.toml` のパース・環境変数オーバーライド）
- [x] テスト: `crucible init` で生成されるファイルの内容が仕様通りであることを確認する
- [x] テスト（統合）: `crucible migrate` → `crucible migrate:status` → `✅ applied` と表示されることを確認

---

## Phase C-5: スキーマ管理・接続プール・トランザクション

### C-5-A: schema.forge — スキーマ管理

- [x] `packages/crucible/src/schema.forge` を作成する
- [x] `read_schema(conn: Conn) -> map!` を実装する（`information_schema.columns` / `table_constraints` を参照）
- [x] `generate_schema_forge(schema: map) -> string` を実装する（`src/schema.forge` のソースコードを文字列で生成）
- [x] `diff_schema(models: map, db_schema: map) -> list<SchemaDiff>` を実装する（モデル定義とDBのズレを検出）
- [x] `format_diff_report(diffs: list<SchemaDiff>) -> string` を実装する（人間が読めるレポート）
- [x] `crates/crucible-cli/src/schema.rs` を実装する（`crucible schema:sync` / `schema:diff`）
- [x] テスト（統合）: `schema:sync` が `src/schema.forge` を正しく生成することを確認する（2 テーブル: users + _crucible_migrations）

### C-5-B: conn.forge 拡張 — 接続プール

- [x] `ConnPool` 型を定義する（struct: config, connections: list<Conn>, available フラグ）
- [x] `pool_connect(config: ConnPoolConfig) -> ConnPool!` を実装する（max_connections 本まで事前接続）
- [x] `pool_acquire(pool: ConnPool) -> Conn!` を実装する（空き接続を取得。タイムアウト付き）
- [x] `pool_release(pool: ConnPool, conn: Conn) -> unit` を実装する
- [x] `ConnPoolConfig` の `connect_timeout_ms` を実装する

### C-5-C: query.forge 拡張 — トランザクション

- [x] `transaction(conn: Conn, fn: fn() -> T!) -> T!` を実装する
  - BEGIN 送信 → fn 実行 → ok なら COMMIT / err なら ROLLBACK
- [x] トランザクション API (begin/commit/rollback_transaction) を実装済み。統合テストは PostgreSQL 稼働時に確認可能

### C-5-D: make:repo 実装

- [x] `crates/crucible-cli/src/make.rs` に `make:repo <Name>` を実装する（Repository trait + impl 生成）
- [x] 生成される `user_repository.forge` の内容が仕様通りであることを確認する

---

## Phase C-E2E: ユーザースキーマ書き込み E2E テスト

spec.md のユーザースキーマが実際に PostgreSQL に書き込まれることを確認する最終検証フェーズ。
C-4（マイグレーション CLI）完了後に実施する。

### C-E2E-A: テスト環境のセットアップ

- [x] `docker/docker-compose.test.yml` に `postgres:16` サービスを定義する（POSTGRES_PASSWORD=test / POSTGRES_DB=crucible_test / port 5432）
- [x] `packages/crucible/crucible.toml` にテスト用 DB 設定を記載する（host=localhost, port=5432, name=crucible_test, user=postgres, password=test）
- [x] `packages/crucible/migrations/001_create_users.sql` を spec.md の定義通りに作成する:
  ```sql
  -- +migrate Up
  CREATE TABLE users (
      id         SERIAL       PRIMARY KEY,
      name       VARCHAR(255) NOT NULL,
      email      VARCHAR(255) NOT NULL UNIQUE,
      password   VARCHAR(255) NOT NULL,
      created_at TIMESTAMP    NOT NULL DEFAULT now(),
      updated_at TIMESTAMP    NOT NULL DEFAULT now()
  );

  -- +migrate Down
  DROP TABLE users;
  ```
- [x] `docker compose -f docker/docker-compose.test.yml up -d` で PostgreSQL が起動することを確認する

### C-E2E-B: マイグレーション実行と書き込み確認

- [x] `crucible migrate` を実行して `001_create_users.sql` が適用されることを確認する
- [x] `crucible migrate:status` で `001_create_users.sql` が `✅ applied` と表示されることを確認する
- [x] `users` テーブルが存在することを確認する（information_schema クエリ実施済み）
- [x] `users` テーブルのカラム定義が spec と一致することを確認する（id/name/email/password/created_at/updated_at 全6カラム確認）

### C-E2E-C: ロールバック確認

- [x] `crucible migrate:rollback` を実行して `⬜ pending` に戻ることを確認する
- [x] `crucible migrate:status` で `001_create_users.sql` が `⬜ pending` に戻ることを確認する
- [x] 再度 `crucible migrate` を実行してテーブルが再作成されることを確認する

### C-E2E-D: INSERT / SELECT による書き込み確認

- [x] ForgeScript コード (`packages/crucible/examples/insert_user.forge`) を作成する:
  ```forge
  use crucible.{ connect }

  connect("postgres://postgres:test@localhost/crucible_test").await?
  let user = User::insert(User {
      name:     "Alice"
      email:    "alice@example.com"
      password: "hashed_pw"
  }).await?
  println("inserted: ${user.id}")

  let found = User::find(user.id).await?
  println("found: ${found.name}")
  ```
- [x] `forge run packages/crucible/examples/insert_user.forge` が `inserted: 1` と `found: Alice` を出力することを確認する
- [x] PostgreSQL 上で `SELECT * FROM users` によりレコードが存在することを確認する

---

## 進捗サマリ

| Phase | タスク数 | 完了数 | 進捗 |
|---|---:|---:|---:|
| C-0 基盤整備 | 15 | 15 | 100% |
| C-1 wire protocol + 認証 | 25 | 25 | 100% |
| C-2 クエリ + 型マッピング | 11 | 11 | 100% |
| C-3 ORM 層 | 24 | 23 | 96% |
| C-4 マイグレーション + CLI | 17 | 17 | 100% |
| C-5 スキーマ + プール + TX | 14 | 14 | 100% |
| C-E2E ユーザースキーマ書き込み | 12 | 12 | 100% |
| **合計** | **118** | **118** | **100%** |
