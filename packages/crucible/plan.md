# crucible 実装計画

> 仕様: `packages/crucible/spec.md`
> タスク: `packages/crucible/tasks.md`

---

## 前提知識

crucible は2層で構成される。

| 層 | 言語 | 場所 |
|---|---|---|
| ForgeScript ライブラリ | ForgeScript | `packages/crucible/src/*.forge` |
| CLI ツール | Rust | `crates/crucible-cli/src/` |

ForgeScript 層は forge-vm のインタープリタ上で動く。
TCP 接続・バイト I/O などシステム操作は `forge-stdlib`（Rust）に組み込み関数として追加し、ForgeScript から呼ぶ。

---

## 依存関係グラフ

```
forge/std/net (TCP client 追加)
  └── wire.forge (PostgreSQL メッセージ encode/decode)
        ├── auth.forge (SCRAM-SHA-256)
        └── conn.forge (接続管理)
              └── query.forge (クエリ送受信)
                    ├── types.forge (型マッピング)
                    ├── builder.forge (QueryBuilder DSL)
                    └── model.forge (@model アノテーション)
                          └── migration.forge (マイグレーション)
                                └── schema.forge (schema:sync/diff)

crates/crucible-cli/  (各 Phase で並行して追加)
```

---

## Phase C-0: 基盤整備（TCP クライアント追加）

### 目的

`forge/std/net` に TCP クライアント関数を追加し、ForgeScript から PostgreSQL に TCP 接続できるようにする。

### 追加する Rust 組み込み関数（`crates/forge-stdlib/src/net.rs`）

| 関数名 | シグネチャ | 説明 |
|---|---|---|
| `tcp_connect` | `(host: string, port: number) -> TcpConn!` | TCP 接続を確立 |
| `tcp_write` | `(conn: TcpConn, data: list<number>) -> unit!` | バイト列を送信 |
| `tcp_read` | `(conn: TcpConn, n: number) -> list<number>!` | n バイトを受信 |
| `tcp_read_exact` | `(conn: TcpConn, n: number) -> list<number>!` | n バイトを必ず受信 |
| `tcp_close` | `(conn: TcpConn) -> unit` | 接続を閉じる |

`TcpConn` は Value::Native として管理する。

### パッケージ・クレートのセットアップ

- `packages/crucible/` ディレクトリと `forge.toml` を作成
- `crates/crucible-cli/` Rust クレートを新規作成
- `Cargo.toml`（workspace）に `crucible-cli` を追加

---

## Phase C-1: wire protocol + 認証

### 目的

PostgreSQL wire protocol v3 を ForgeScript で実装し、単一 TCP 接続で `SELECT 1` が返せる状態にする。

### 実装ファイル

**`packages/crucible/src/wire.forge`**

PostgreSQL メッセージのバイト列 encode/decode。

```
encode_startup_message(user, database) -> list<number>
encode_query(sql) -> list<number>
encode_sasl_initial_response(mechanism, payload) -> list<number>
encode_sasl_response(payload) -> list<number>
encode_terminate() -> list<number>

decode_backend_message(bytes) -> BackendMessage
```

`BackendMessage` は match で分岐できる型（data または enum ライクな struct）。

**`packages/crucible/src/auth.forge`**

SCRAM-SHA-256 認証フロー（ForgeScript 部分）。
HMAC・PBKDF2 の計算は `use raw {}` で sha2/base64 を使う。

```
scram_client_first(username) -> { message: string, nonce: string }
scram_client_final(server_first, client_first, nonce, password) -> { message: string, proof: string }
scram_verify_server_final(server_final, server_key) -> bool!
```

**`packages/crucible/src/conn.forge`**

単一 TCP 接続の確立・送受信・接続状態管理。

```
connect(host, port, user, password, database) -> Conn!
query_raw(conn, sql) -> list<Row>!
close(conn) -> unit
```

### マイルストーン

`connect("localhost", 5432, "postgres", "", "myapp") |> query_raw("SELECT 1")` が Row を返す。

---

## Phase C-2: クエリ実行・型マッピング

### 目的

任意の SQL を実行し、ForgeScript の Value に変換できるようにする。

### 実装ファイル

**`packages/crucible/src/types.forge`**

PostgreSQL OID → ForgeScript Value 変換テーブル。

| PostgreSQL | ForgeScript |
|---|---|
| int2/int4/int8 | number |
| float4/float8/numeric | number |
| varchar/text/char | string |
| bool | bool |
| timestamp/timestamptz | string (ISO 8601) |
| json/jsonb | map |
| array | list |
| NULL | unit |

**`packages/crucible/src/query.forge`**

高レベルクエリ API。

```
execute(conn, sql, params) -> list<Row>!
execute_one(conn, sql, params) -> Row?!
count(conn, sql, params) -> number!
```

`Row` は `map<string, Value>`（カラム名をキーにした map）。

---

## Phase C-3: ORM 層（モデル・QueryBuilder）

### 目的

`@model` アノテーションと QueryBuilder DSL で型安全なクエリが書けるようにする。

### 実装ファイル

**`packages/crucible/src/model.forge`**

`@model("table_name")` アノテーション処理。
モデル構造体のフィールド名 → カラム名マッピング。

```
model_table(model_name) -> string         // "User" -> "users"
model_fields(model_name) -> list<string>  // フィールド名一覧
row_to_model(row, model_name) -> T        // Row -> モデル値
model_to_row(value, model_name) -> Row    // モデル値 -> Row
```

**`packages/crucible/src/builder.forge`**

QueryBuilder DSL（パイプ演算子対応）。

```
query(model) -> QueryBuilder
where(builder, condition, params) -> QueryBuilder
order_by(builder, column, asc) -> QueryBuilder
limit(builder, n) -> QueryBuilder
offset(builder, n) -> QueryBuilder
select(builder, columns) -> QueryBuilder
join(builder, table, on) -> QueryBuilder
all(builder) -> list<T>!
first(builder) -> T?!
count(builder) -> number!
exists(builder) -> bool!
```

**`packages/crucible/src/mod.forge`**

公開 API（User::all / User::find / User::insert / User::update / User::delete_where 等）。

---

## Phase C-4: マイグレーション + CLI（基本）

### 目的

`crucible migrate` / `crucible make:migration` が動くようにする。

### 実装ファイル

**`packages/crucible/src/migration.forge`**

```
read_migrations(dir) -> list<Migration>!
applied_versions(conn) -> list<string>!
apply_migration(conn, migration) -> unit!
rollback_migration(conn, migration) -> unit!
ensure_migration_table(conn) -> unit!
```

**`crates/crucible-cli/src/`**

Rust CLI クレート。forge-vm を使って `.forge` コードを実行する。

- `main.rs` — `crucible init / migrate / make:*` コマンドルーティング
- `init.rs` — `crucible init --psql`（ファイル生成）
- `migrate.rs` — `crucible migrate / migrate:rollback / migrate:status`
- `make.rs` — `crucible make:migration / make:model / make:repo`

---

## Phase C-5: スキーマ管理・接続プール・トランザクション

### 目的

本番利用可能な水準にする。

### 実装ファイル

**`packages/crucible/src/schema.forge`**

`information_schema` からスキーマを読んで `src/schema.forge` を生成する。

**`packages/crucible/src/conn.forge`（拡張）**

接続プール（最大接続数・タイムアウト設定）。

**`packages/crucible/src/query.forge`（拡張）**

トランザクション API。

```
transaction(conn, fn() -> T!) -> T!
```

**`crates/crucible-cli/src/schema.rs`**

`crucible schema:sync / schema:diff`。

---

## テスト戦略

### 単体テスト（PostgreSQL 不要）

- wire protocol の encode/decode（バイト列の検証）
- SCRAM-SHA-256 の計算ロジック
- QueryBuilder の SQL 生成
- 型マッピング（OID → Value 変換）

### 統合テスト（PostgreSQL 必要）

Docker Compose で PostgreSQL を起動してテスト。

```yaml
# docker/docker-compose.test.yml
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: test
      POSTGRES_DB: crucible_test
    ports:
      - "5432:5432"
```

テスト実行前に `docker compose -f docker/docker-compose.test.yml up -d` を実行。

---

## 実装順序まとめ

```
C-0  TCP クライアント追加（forge-stdlib Rust）
  └─ C-1  wire protocol + SCRAM 認証
      └─ C-2  クエリ実行 + 型マッピング
          └─ C-3  @model + QueryBuilder
              └─ C-4  マイグレーション + CLI 基本
                  └─ C-5  スキーマ管理 + 接続プール + TX
```
