# crucible タスク一覧

> 仕様: `packages/crucible/spec.md`
> 計画: `packages/crucible/plan.md`

---

## Phase C-0: 基盤整備

### C-0-A: TCP クライアント追加（forge-stdlib）

- [ ] `crates/forge-stdlib/src/net.rs` に `TcpConn` 型を追加する（`Arc<Mutex<TcpStream>>` をラップした Native 値）
- [ ] `tcp_connect(host: string, port: number) -> TcpConn!` を実装する
- [ ] `tcp_write(conn: TcpConn, data: list<number>) -> unit!` を実装する（バイト列送信）
- [ ] `tcp_read_exact(conn: TcpConn, n: number) -> list<number>!` を実装する（n バイト必ず受信）
- [ ] `tcp_read_available(conn: TcpConn) -> list<number>!` を実装する（受信可能バイトを全部読む）
- [ ] `tcp_close(conn: TcpConn) -> unit` を実装する
- [ ] `forge/std/net` モジュールに上記関数を登録する（`forge-vm/src/interpreter.rs` または `forge-stdlib/src/lib.rs`）
- [ ] 単体テスト: `tcp_connect` が接続できない場合に `Err` を返すことを確認する

### C-0-B: パッケージ・クレートのセットアップ

- [ ] `packages/crucible/forge.toml` を作成する（name = "crucible", version = "0.1.0"）
- [ ] `packages/crucible/src/` ディレクトリを作成する
- [ ] `packages/crucible/src/mod.forge` を作成する（スタブ）
- [ ] `crates/crucible-cli/Cargo.toml` を作成する（バイナリクレート）
- [ ] `crates/crucible-cli/src/main.rs` を作成する（スタブ: `fn main() { println!("crucible") }`）
- [ ] `Cargo.toml`（workspace）に `crates/crucible-cli` を追加する
- [ ] `cargo build --bin crucible-cli` が通ることを確認する

### C-0-C: テスト用 Docker Compose

- [ ] `docker/docker-compose.test.yml` を作成する（postgres:16, POSTGRES_DB=crucible_test, port 5432）
- [ ] `docker/crucible_test_setup.sql` を作成する（テスト用テーブル: `users`, `posts`）

---

## Phase C-1: wire protocol + 認証

### C-1-A: wire.forge — メッセージ encode

- [ ] `packages/crucible/src/wire.forge` を作成する
- [ ] `encode_startup_message(user: string, database: string) -> list<number>` を実装する
- [ ] `encode_query(sql: string) -> list<number>` を実装する（Simple Query Protocol: 'Q' + len + sql + \0）
- [ ] `encode_sasl_initial_response(mechanism: string, payload: string) -> list<number>` を実装する
- [ ] `encode_sasl_response(payload: string) -> list<number>` を実装する
- [ ] `encode_terminate() -> list<number>` を実装する（'X' + 4バイト長）
- [ ] ヘルパー: `encode_int32(n: number) -> list<number>` を実装する（ビッグエンディアン）
- [ ] ヘルパー: `encode_string(s: string) -> list<number>` を実装する（null終端）
- [ ] テスト: `encode_terminate()` が `[88, 0, 0, 0, 4]` を返すことを確認する
- [ ] テスト: `encode_query("SELECT 1")` のバイト列が正しいことを確認する

### C-1-B: wire.forge — メッセージ decode

- [ ] `BackendMessage` 型を定義する（struct with type_byte / body フィールド）
- [ ] `decode_backend_message(bytes: list<number>) -> BackendMessage` を実装する
- [ ] `read_backend_message(conn: TcpConn) -> BackendMessage!` を実装する（1バイト種別 + 4バイト長を読む）
- [ ] `parse_row_description(body: list<number>) -> list<ColumnDesc>` を実装する（カラム名・OID取得）
- [ ] `parse_data_row(body: list<number>, cols: list<ColumnDesc>) -> map` を実装する
- [ ] `parse_error_response(body: list<number>) -> string` を実装する（'M' フィールドを取得）
- [ ] テスト: `ReadyForQuery`（'Z' + \x00\x00\x00\x05 + 'I'）を正しく decode できることを確認する

### C-1-C: auth.forge — SCRAM-SHA-256

- [ ] `packages/crucible/src/auth.forge` を作成する
- [ ] `use raw {}` ブロックで `sha2` / `base64` / `hmac` を `#[allow(unused)]` 付きで宣言する
- [ ] `scram_generate_nonce() -> string` を実装する（クライアントノンス: 18バイト random base64）
- [ ] `scram_client_first_message(username: string, nonce: string) -> string` を実装する
- [ ] `scram_hi(password: string, salt: list<number>, iterations: number) -> list<number>` を実装する（PBKDF2-HMAC-SHA256）
- [ ] `scram_hmac(key: list<number>, msg: string) -> list<number>` を実装する
- [ ] `scram_h(data: list<number>) -> list<number>` を実装する（SHA-256）
- [ ] `scram_client_proof(password, salt, iterations, client_first_bare, server_first, client_final_bare) -> string` を実装する（ClientProof を base64 で返す）
- [ ] `scram_client_final_message(client_final_bare: string, proof: string) -> string` を実装する
- [ ] `scram_verify_server_signature(server_final: string, server_key: list<number>) -> bool!` を実装する
- [ ] テスト: RFC 7677 の既知テストベクタで SCRAM 計算が正しいことを確認する

### C-1-D: conn.forge — 単一接続

- [ ] `packages/crucible/src/conn.forge` を作成する
- [ ] `Conn` 型を定義する（struct: tcp_conn, state, params フィールド）
- [ ] `connect(host: string, port: number, user: string, password: string, database: string) -> Conn!` を実装する
  - TCP 接続 → StartupMessage 送信 → 認証フロー（SCRAM-SHA-256）→ ReadyForQuery 待機
- [ ] `query_raw(conn: Conn, sql: string) -> list<map>!` を実装する（Simple Query → Row 一覧）
- [ ] `close(conn: Conn) -> unit` を実装する（Terminate 送信 → TCP close）
- [ ] テスト（統合）: `connect("localhost", 5432, "postgres", "test", "crucible_test")` が成功する

### C-1-E: マイルストーン確認

- [ ] E2E テスト: `connect` → `query_raw("SELECT 1 AS n")` → Row `{n: 1}` が返ることを確認する

---

## Phase C-2: クエリ実行・型マッピング

### C-2-A: types.forge — PostgreSQL 型マッピング

- [ ] `packages/crucible/src/types.forge` を作成する
- [ ] `pg_oid_to_type(oid: number) -> string` を実装する（主要 OID → 型名文字列）
- [ ] `pg_value_to_forge(pg_type: string, text: string) -> Value` を実装する
  - `int2/int4/int8` → number
  - `float4/float8/numeric` → number（parseFloat）
  - `varchar/text/char` → string
  - `bool` → bool（"t" / "f"）
  - `timestamp/timestamptz` → string（そのまま）
  - `json/jsonb` → map（JSON パース）
  - NULL → unit
- [ ] テスト: `pg_value_to_forge("int4", "42")` → `Value::Number(42.0)` を確認する
- [ ] テスト: `pg_value_to_forge("bool", "t")` → `Value::Bool(true)` を確認する
- [ ] テスト: NULL → `Value::Unit` を確認する

### C-2-B: query.forge — 高レベルクエリ API

- [ ] `packages/crucible/src/query.forge` を作成する
- [ ] `execute(conn: Conn, sql: string, params: list) -> list<map>!` を実装する（パラメータを埋め込んだ SQL を query_raw に渡す）
- [ ] `execute_one(conn: Conn, sql: string, params: list) -> map?!` を実装する（0件なら unit, 1件以上なら先頭）
- [ ] `execute_count(conn: Conn, sql: string, params: list) -> number!` を実装する
- [ ] パラメータのサニタイズ（SQL インジェクション対策: `$1`/`$2` 形式でバインド）
- [ ] テスト: `execute(conn, "SELECT $1::int AS n", [42])` → `[{n: 42}]` を確認する

---

## Phase C-3: ORM 層（モデル・QueryBuilder）

### C-3-A: model.forge — @model アノテーション

- [ ] `packages/crucible/src/model.forge` を作成する
- [ ] `@model_registry` グローバル map を定義する（モデル名 → テーブル名・フィールド一覧）
- [ ] `register_model(model_name: string, table: string, fields: list<string>) -> unit` を実装する
- [ ] `model_table(model_name: string) -> string` を実装する
- [ ] `model_fields(model_name: string) -> list<string>` を実装する
- [ ] `row_to_model(row: map, model_name: string) -> map` を実装する（フィールド名マッピング）
- [ ] テスト: `register_model("User", "users", ["id", "name", "email"])` → `model_table("User")` が `"users"` を返す

### C-3-B: builder.forge — QueryBuilder DSL

- [ ] `packages/crucible/src/builder.forge` を作成する
- [ ] `QueryBuilder` 型を定義する（struct: table, conditions, order, limit, offset, selects, joins フィールド）
- [ ] `query(model: string) -> QueryBuilder` を実装する
- [ ] `where_clause(builder: QueryBuilder, condition: string, params: list) -> QueryBuilder` を実装する
- [ ] `order_by(builder: QueryBuilder, column: string, asc: bool) -> QueryBuilder` を実装する
- [ ] `limit(builder: QueryBuilder, n: number) -> QueryBuilder` を実装する
- [ ] `offset(builder: QueryBuilder, n: number) -> QueryBuilder` を実装する
- [ ] `select_columns(builder: QueryBuilder, columns: list<string>) -> QueryBuilder` を実装する
- [ ] `join(builder: QueryBuilder, table: string, on: string) -> QueryBuilder` を実装する
- [ ] `build_sql(builder: QueryBuilder) -> { sql: string, params: list }` を実装する
- [ ] `all(builder: QueryBuilder, conn: Conn) -> list<map>!` を実装する
- [ ] `first(builder: QueryBuilder, conn: Conn) -> map?!` を実装する
- [ ] `count(builder: QueryBuilder, conn: Conn) -> number!` を実装する
- [ ] `exists(builder: QueryBuilder, conn: Conn) -> bool!` を実装する
- [ ] テスト: `build_sql` が正しい SQL 文字列を生成することを確認する（DB不要）
- [ ] テスト（統合）: `User::query() |> where_clause("active = $1", [true]) |> all(conn)` が動く

### C-3-C: mod.forge — 公開 CRUD API

- [ ] `packages/crucible/src/mod.forge` を本実装する（pub use + モデル API）
- [ ] `all(conn: Conn, model: string) -> list<map>!` を実装する
- [ ] `find(conn: Conn, model: string, id: number) -> map?!` を実装する
- [ ] `first_where(conn: Conn, model: string, condition: string, params: list) -> map?!` を実装する
- [ ] `where_many(conn: Conn, model: string, condition: string, params: list) -> list<map>!` を実装する
- [ ] `insert(conn: Conn, model: string, values: map) -> map!` を実装する（INSERT RETURNING *）
- [ ] `update(conn: Conn, model: string, values: map) -> map!` を実装する（UPDATE WHERE id=... RETURNING *）
- [ ] `delete_where(conn: Conn, model: string, condition: string, params: list) -> unit!` を実装する

---

## Phase C-4: マイグレーション + CLI 基本

### C-4-A: migration.forge — マイグレーション管理

- [ ] `packages/crucible/src/migration.forge` を作成する
- [ ] `Migration` 型を定義する（struct: version, filename, up_sql, down_sql）
- [ ] `parse_migration_file(path: string) -> Migration!` を実装する（`-- +migrate Up` / `-- +migrate Down` を分割）
- [ ] `read_migrations(dir: string) -> list<Migration>!` を実装する（バージョン順ソート）
- [ ] `ensure_migration_table(conn: Conn) -> unit!` を実装する（`_crucible_migrations` テーブル作成）
- [ ] `applied_versions(conn: Conn) -> list<string>!` を実装する
- [ ] `apply_migration(conn: Conn, migration: Migration) -> unit!` を実装する（Up SQL 実行 + 記録）
- [ ] `rollback_last(conn: Conn, migrations: list<Migration>) -> unit!` を実装する（Down SQL 実行 + 削除）
- [ ] `pending_migrations(all: list<Migration>, applied: list<string>) -> list<Migration>` を実装する
- [ ] テスト: `parse_migration_file` が Up/Down SQL を正しく分割することを確認する

### C-4-B: crucible-cli — 基本コマンド実装

- [ ] `crates/crucible-cli/src/main.rs` にコマンドルーティングを実装する（init / migrate / make:* / schema:*）
- [ ] `crates/crucible-cli/src/init.rs` を実装する
  - `crucible init --psql` で `crucible.toml` / `migrations/` / `001_create_users.sql` / `src/models/user.forge` を生成
- [ ] `crates/crucible-cli/src/migrate.rs` を実装する
  - `crucible migrate` — 未適用マイグレーションを順に適用
  - `crucible migrate:rollback` — 最後の1件を戻す
  - `crucible migrate:status` — 適用済み/未適用の一覧表示
- [ ] `crates/crucible-cli/src/make.rs` を実装する（make:migration / make:model / make:repo）
- [ ] `crates/crucible-cli/src/config.rs` を実装する（`crucible.toml` のパース・環境変数オーバーライド）
- [ ] テスト: `crucible init` で生成されるファイルの内容が仕様通りであることを確認する
- [ ] テスト（統合）: `crucible migrate` → `crucible migrate:status` → 全マイグレーションが applied 表示

---

## Phase C-5: スキーマ管理・接続プール・トランザクション

### C-5-A: schema.forge — スキーマ管理

- [ ] `packages/crucible/src/schema.forge` を作成する
- [ ] `read_schema(conn: Conn) -> map!` を実装する（`information_schema.columns` / `table_constraints` を参照）
- [ ] `generate_schema_forge(schema: map) -> string` を実装する（`src/schema.forge` のソースコードを文字列で生成）
- [ ] `diff_schema(models: map, db_schema: map) -> list<SchemaDiff>` を実装する（モデル定義とDBのズレを検出）
- [ ] `format_diff_report(diffs: list<SchemaDiff>) -> string` を実装する（人間が読めるレポート）
- [ ] `crates/crucible-cli/src/schema.rs` を実装する（`crucible schema:sync` / `schema:diff`）
- [ ] テスト（統合）: `schema:sync` が `src/schema.forge` を正しく生成することを確認する

### C-5-B: conn.forge 拡張 — 接続プール

- [ ] `ConnPool` 型を定義する（struct: config, connections: list<Conn>, available フラグ）
- [ ] `pool_connect(config: ConnPoolConfig) -> ConnPool!` を実装する（max_connections 本まで事前接続）
- [ ] `pool_acquire(pool: ConnPool) -> Conn!` を実装する（空き接続を取得。タイムアウト付き）
- [ ] `pool_release(pool: ConnPool, conn: Conn) -> unit` を実装する
- [ ] `ConnPoolConfig` の `connect_timeout_ms` を実装する

### C-5-C: query.forge 拡張 — トランザクション

- [ ] `transaction(conn: Conn, fn: fn() -> T!) -> T!` を実装する
  - BEGIN 送信 → fn 実行 → ok なら COMMIT / err なら ROLLBACK
- [ ] テスト（統合）: トランザクション中の INSERT が ROLLBACK で取り消されることを確認する

### C-5-D: make:repo 実装

- [ ] `crates/crucible-cli/src/make.rs` に `make:repo <Name>` を実装する（Repository trait + impl 生成）
- [ ] 生成される `user_repository.forge` の内容が仕様通りであることを確認する

---

## 進捗サマリ

| Phase | タスク数 | 完了数 | 進捗 |
|---|---:|---:|---:|
| C-0 基盤整備 | 15 | 0 | 0% |
| C-1 wire protocol + 認証 | 25 | 0 | 0% |
| C-2 クエリ + 型マッピング | 11 | 0 | 0% |
| C-3 ORM 層 | 24 | 0 | 0% |
| C-4 マイグレーション + CLI | 17 | 0 | 0% |
| C-5 スキーマ + プール + TX | 14 | 0 | 0% |
| **合計** | **106** | **0** | **0%** |
