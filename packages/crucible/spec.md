# crucible 仕様書

> バージョン: v0.1.0（設計中）
> 最終更新: 2026-04-11

---

## 概要

crucible は ForgeScript で書かれた PostgreSQL ORM です。

**設計思想**

- **SQL が正規形**。マイグレーションは開発者が `.sql` で明示的に書く
- **sqlx 非依存**。PostgreSQL wire protocol を ForgeScript で直接実装
- **型は DB から導出**。`schema:sync` で生きている DB から型定義を生成
- **明示的・驚き最小**。自動的にスキーマを変更するコマンドは存在しない

---

## アーキテクチャ

```
┌─────────────────────────────────────────────────────┐
│  アプリケーション層                                    │
│  User::all() / User::where(...) / UserRepository     │
├─────────────────────────────────────────────────────┤
│  モデル層          @model("users") data User { ... } │
│  クエリビルダー層   QueryBuilder |> where |> limit    │
├─────────────────────────────────────────────────────┤
│  マイグレーション層  migrations/*.sql + 適用追跡       │
│  スキーマ層        schema.forge（DB から自動生成）     │
├─────────────────────────────────────────────────────┤
│  接続層           crucible/conn.forge（接続管理）      │
│  認証層           crucible/auth.forge（SCRAM-SHA-256）│
│  wire protocol    crucible/wire.forge（メッセージ）    │
│  TCP              forge/std/net                      │
└─────────────────────────────────────────────────────┘
```

---

## パッケージ構造

```
packages/crucible/
  forge.toml
  src/
    mod.forge          ← 公開 API
    wire.forge         ← PostgreSQL wire protocol（メッセージ encode/decode）
    auth.forge         ← SCRAM-SHA-256 認証
    conn.forge         ← 接続・接続プール管理
    query.forge        ← クエリ実行・結果変換
    builder.forge      ← QueryBuilder DSL
    migration.forge    ← マイグレーション適用・追跡
    schema.forge       ← schema:sync / schema:diff ロジック
    model.forge        ← @model アノテーション処理
    types.forge        ← PostgreSQL 型 ↔ ForgeScript Value 変換

crates/crucible-cli/   ← CLI（Rust クレート。forge-cli と同様の構造）
  src/
    main.rs
    init.rs            ← crucible init --psql
    migrate.rs         ← crucible migrate / rollback / status
    schema.rs          ← crucible schema:sync / schema:diff
    make.rs            ← crucible make:model / make:migration / make:repo
```

---

## 設定ファイル

### `crucible.toml`

```toml
[database]
driver   = "postgres"
host     = "localhost"
port     = 5432
name     = "myapp_dev"
user     = "postgres"
password = ""

[pool]
max_connections = 10
connect_timeout = 5000   # ms

[migrations]
directory = "migrations"
table     = "_crucible_migrations"
```

環境変数でオーバーライド可能：

```
CRUCIBLE_HOST     CRUCIBLE_PORT     CRUCIBLE_NAME
CRUCIBLE_USER     CRUCIBLE_PASSWORD
```

---

## CLI

### `crucible init --psql`

プロジェクトに crucible を統合する。

```bash
crucible init --psql
```

実行内容：

1. `crucible.toml` を生成
2. `migrations/` ディレクトリを作成
3. `migrations/001_create_users.sql` を生成（users テーブル雛形）
4. `src/models/user.forge` を生成（User モデル雛形）
5. プロジェクトの `forge.toml` に crucible を依存追記

生成される `migrations/001_create_users.sql`：

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

生成される `src/models/user.forge`：

```forge
use crucible.{ model }

@model("users")
data User {
    id:         number = 0
    name:       string = ""
    email:      string = ""
    password:   string = ""
    created_at: string = ""
    updated_at: string = ""
}
```

---

### `crucible migrate`

未適用のマイグレーションをバージョン順に適用する。

```bash
crucible migrate
```

```
  Applying 001_create_users.sql ... ok
  Applying 002_add_posts.sql    ... ok
  2 migrations applied.
```

内部動作：

1. `_crucible_migrations` テーブルが存在しない場合は作成
2. `migrations/` の `.sql` ファイルをバージョン順にソート
3. `_crucible_migrations` に記録されていないものを順に実行
4. `-- +migrate Up` セクションのみを適用
5. 適用済みを `_crucible_migrations` に記録

`_crucible_migrations` テーブル定義：

```sql
CREATE TABLE _crucible_migrations (
    id         SERIAL       PRIMARY KEY,
    version    VARCHAR(255) NOT NULL UNIQUE,
    filename   VARCHAR(255) NOT NULL,
    applied_at TIMESTAMP    NOT NULL DEFAULT now()
);
```

---

### `crucible migrate:rollback`

最後に適用したマイグレーションを1件戻す。

```bash
crucible migrate:rollback
```

```
  Rolling back 002_add_posts.sql ... ok
```

`-- +migrate Down` セクションを実行し、`_crucible_migrations` から削除する。

---

### `crucible migrate:status`

適用済み・未適用のマイグレーション一覧を表示する。

```bash
crucible migrate:status
```

```
  ✅ 001_create_users.sql   applied at 2026-04-11 10:23:04
  ✅ 002_add_posts.sql      applied at 2026-04-11 10:23:04
  ⬜ 003_add_comments.sql   pending
```

---

### `crucible make:migration <name>`

空の `.sql` マイグレーションファイルを生成する。バージョン番号は自動採番。

```bash
crucible make:migration add_avatar_url_to_users
```

```
  Created: migrations/003_add_avatar_url_to_users.sql
```

生成されるファイル：

```sql
-- +migrate Up
-- TODO: ここに SQL を書く

-- +migrate Down
-- TODO: ここに SQL を書く
```

---

### `crucible make:model <Name>`

モデルファイルを生成する。対応するマイグレーションも同時に生成する。

```bash
crucible make:model Post
```

```
  Created: src/models/post.forge
  Created: migrations/003_create_posts.sql
```

`src/models/post.forge`：

```forge
use crucible.{ model }

@model("posts")
data Post {
    id:         number = 0
    created_at: string = ""
    updated_at: string = ""
}
```

`migrations/003_create_posts.sql`：

```sql
-- +migrate Up
CREATE TABLE posts (
    id         SERIAL    PRIMARY KEY,
    created_at TIMESTAMP NOT NULL DEFAULT now(),
    updated_at TIMESTAMP NOT NULL DEFAULT now()
);

-- +migrate Down
DROP TABLE posts;
```

---

### `crucible make:repo <Name>`

Repository trait と標準実装を生成する。

```bash
crucible make:repo User
```

```
  Created: src/repositories/user_repository.forge
```

```forge
use crucible.{ model }
use ./models/user.{ User }

trait UserRepository {
    fn find_by_id(id: number) -> User?!
    fn find_all() -> list<User>!
    fn find_by_email(email: string) -> User?!
    fn save(user: User) -> User!
    fn delete(id: number) -> unit!
}

impl UserRepository {
    fn find_by_id(id: number) -> User?! {
        User::where("id = $1", [id]).first().await
    }

    fn find_all() -> list<User>! {
        User::all().await
    }

    fn find_by_email(email: string) -> User?! {
        User::where("email = $1", [email]).first().await
    }

    fn save(user: User) -> User! {
        if user.id == 0 {
            User::insert(user).await
        } else {
            User::update(user).await
        }
    }

    fn delete(id: number) -> unit! {
        User::delete_where("id = $1", [id]).await
    }
}
```

---

### `crucible schema:sync`

生きている DB のスキーマを読み取り、`src/schema.forge` を生成する。

```bash
crucible schema:sync
```

```
  Connected to postgres://localhost/myapp_dev
  Reading schema from information_schema ...
  Generated: src/schema.forge
```

生成される `src/schema.forge`（触らない）：

```forge
// AUTO-GENERATED by crucible schema:sync
// Do not edit manually. Run `crucible schema:sync` to update.

pub let schema = {
    users: {
        id:         { pg_type: "int4",        nullable: false, primary_key: true  },
        name:       { pg_type: "varchar",     nullable: false },
        email:      { pg_type: "varchar",     nullable: false, unique: true },
        password:   { pg_type: "varchar",     nullable: false },
        created_at: { pg_type: "timestamptz", nullable: false },
        updated_at: { pg_type: "timestamptz", nullable: false },
    },
    posts: {
        id:         { pg_type: "int4",        nullable: false, primary_key: true  },
        user_id:    { pg_type: "int4",        nullable: false, references: "users.id" },
        body:       { pg_type: "text",        nullable: false },
        created_at: { pg_type: "timestamptz", nullable: false },
    },
}
```

内部では `information_schema.columns` / `information_schema.table_constraints` を使用する。

---

### `crucible schema:diff`

`@model` 定義と実際の DB スキーマのズレを検出し、次のマイグレーションを提案する。
**自動変更は行わない。提案のみ。**

```bash
crucible schema:diff
```

```
  Comparing models to DB schema ...

  ⚠  User.avatar_url  モデルにあるが DB に存在しない
     → crucible make:migration add_avatar_url_to_users を実行してください
     → SQL 例: ALTER TABLE users ADD COLUMN avatar_url VARCHAR(255);

  ⚠  Post.title  モデルにあるが DB に存在しない
     → crucible make:migration add_title_to_posts を実行してください
     → SQL 例: ALTER TABLE posts ADD COLUMN title VARCHAR(255) NOT NULL DEFAULT '';

  ✅ users  その他のカラムは一致
  ✅ posts  その他のカラムは一致
```

---

## モデル定義

### `@model` アノテーション

```forge
@model("users")
data User {
    id:         number = 0
    name:       string = ""
    email:      string = ""
    password:   string = ""
    created_at: string = ""
    updated_at: string = ""
}
```

`@model("table_name")` はテーブル名とモデルを対応付けるだけ。
スキーマ変更は行わない。

---

## クエリ API

### 基本 CRUD

```forge
// 全件取得
let users = User::all().await?

// 主キー検索
let user = User::find(1).await?          // User?!

// 条件検索（最初の1件）
let user = User::first_where("email = $1", [email]).await?

// 条件検索（複数件）
let users = User::where("active = $1", [true]).await?

// INSERT
let new_user = User::insert(User {
    name:     "Alice"
    email:    "alice@example.com"
    password: hashed
}).await?

// UPDATE
let updated = User::update(User { ..user, name: "Alice Smith" }).await?

// DELETE
User::delete_where("id = $1", [user.id]).await?
```

### QueryBuilder（`|>` で繋ぐ）

```forge
let users = User::query()
    |> where("active = $1", [true])
    |> where("created_at > $1", [since])
    |> order_by("name", asc: true)
    |> limit(20)
    |> offset(40)
    |> all().await?
```

### SELECT カラム指定

```forge
let names = User::query()
    |> select(["id", "name"])
    |> all().await?
```

### JOIN

```forge
let rows = User::query()
    |> join("posts", "posts.user_id = users.id")
    |> select(["users.name", "posts.body"])
    |> all().await?
```

### COUNT / EXISTS

```forge
let count = User::query()
    |> where("active = $1", [true])
    |> count().await?

let exists = User::query()
    |> where("email = $1", [email])
    |> exists().await?
```

### トランザクション

```forge
use crucible.{ transaction }

transaction(|| {
    let user = User::insert(new_user).await?
    Post::insert(Post { user_id: user.id, body: "hello" }).await?
    ok(user)
}).await?
```

---

## wire protocol 実装

### メッセージフォーマット

PostgreSQL Frontend/Backend Protocol v3 を ForgeScript で実装する。

```
┌────────────┬──────────────────────────────┐
│ 1 byte     │ メッセージ種別（'Q', 'D' 等）  │
├────────────┼──────────────────────────────┤
│ 4 bytes    │ メッセージ長（自身を含む）     │
├────────────┴──────────────────────────────┤
│ N bytes    │ ペイロード                    │
└────────────────────────────────────────────┘
```

### 実装するメッセージ

| 種別 | 方向 | 内容 |
|---|---|---|
| StartupMessage | Client → Server | 接続開始 |
| AuthenticationSASL | Server → Client | SCRAM-SHA-256 開始 |
| SASLInitialResponse | Client → Server | SCRAM client-first |
| AuthenticationSASLContinue | Server → Client | サーバーチャレンジ |
| SASLResponse | Client → Server | SCRAM client-final |
| AuthenticationSASLFinal | Server → Client | サーバー検証 |
| AuthenticationOk | Server → Client | 認証完了 |
| ReadyForQuery | Server → Client | クエリ受付可能 |
| Query | Client → Server | Simple Query |
| RowDescription | Server → Client | 結果カラム定義 |
| DataRow | Server → Client | 結果行データ |
| CommandComplete | Server → Client | コマンド完了 |
| ErrorResponse | Server → Client | エラー |
| Terminate | Client → Server | 接続終了 |

### 認証：SCRAM-SHA-256

```
Client                              Server
  │── StartupMessage ─────────────▶ │
  │◀── AuthenticationSASL ──────────  │  "SCRAM-SHA-256"
  │── SASLInitialResponse ─────────▶ │  client-first-message
  │◀── AuthenticationSASLContinue ──  │  server-first-message（nonce + salt + iterations）
  │── SASLResponse ────────────────▶ │  client-final-message（ClientProof）
  │◀── AuthenticationSASLFinal ─────  │  ServerSignature 検証
  │◀── AuthenticationOk ────────────  │
  │◀── ReadyForQuery ───────────────  │
```

SCRAM-SHA-256 の計算（`use raw {}` で実装）：

```forge
use raw {
    use sha2::{Sha256, Hmac, Mac};
    use base64::{Engine, engine::general_purpose::STANDARD as B64};

    fn scram_hi(password: &[u8], salt: &[u8], iterations: u32) -> Vec<u8> {
        // PBKDF2-HMAC-SHA256
    }
    fn scram_hmac(key: &[u8], msg: &[u8]) -> Vec<u8> { ... }
    fn scram_h(data: &[u8]) -> Vec<u8> { ... }
}
```

SCRAM のロジック自体（client-first / server-first / client-final の組み立て）は
ForgeScript で実装する。

---

## PostgreSQL 型マッピング

| PostgreSQL 型 | ForgeScript 型 |
|---|---|
| `int2` / `int4` / `int8` | `number` |
| `float4` / `float8` / `numeric` | `number` |
| `varchar` / `text` / `char` | `string` |
| `bool` | `bool` |
| `timestamp` / `timestamptz` | `string`（ISO 8601）|
| `date` | `string`（YYYY-MM-DD）|
| `json` / `jsonb` | `map` |
| `array` | `list` |
| `NULL` | `unit` |

---

## 内部ファイル構成（`packages/crucible/src/`）

| ファイル | 責務 |
|---|---|
| `wire.forge` | バイトメッセージの encode / decode |
| `auth.forge` | SCRAM-SHA-256 認証フロー |
| `conn.forge` | TCP 接続・接続プール |
| `query.forge` | クエリ送信・結果受信・行変換 |
| `builder.forge` | QueryBuilder（where / order_by / limit 等） |
| `migration.forge` | SQL ファイル読み込み・適用・追跡 |
| `schema.forge` | information_schema 読み取り・schema.forge 生成 |
| `model.forge` | @model アノテーション・フィールドマッピング |
| `types.forge` | PostgreSQL 型 ↔ Value 変換 |
| `mod.forge` | 公開 API（pub use） |

---

## 実装フェーズ

### Phase 1：wire protocol + 認証
- `wire.forge`：StartupMessage / Query / RowDescription / DataRow / CommandComplete
- `auth.forge`：SCRAM-SHA-256
- `conn.forge`：単一接続
- 目標：`query(sql)` で SELECT 結果が返る

### Phase 2：ORM 基礎
- `model.forge`：`@model` アノテーション処理
- `builder.forge`：QueryBuilder（where / order_by / limit / all / first）
- `types.forge`：型マッピング
- 目標：`User::all()` / `User::find(id)` / `User::where(...)` が動く

### Phase 3：マイグレーション
- `migration.forge`：SQL ファイル適用・`_crucible_migrations` 管理
- `crates/crucible-cli`：`crucible init` / `migrate` / `make:migration`
- 目標：`crucible migrate` で SQL が適用される

### Phase 4：スキーマ管理
- `schema.forge`：`information_schema` 読み取り・`schema.forge` 生成
- `crates/crucible-cli`：`schema:sync` / `schema:diff`
- 目標：`crucible schema:sync` / `schema:diff` が動く

### Phase 5：接続プール・トランザクション・Repository
- `conn.forge`：接続プール
- `query.forge`：トランザクション
- `crates/crucible-cli`：`make:model` / `make:repo`
- 目標：本番利用可能な水準

---

## 依存関係

```toml
# packages/crucible/forge.toml
[package]
name    = "crucible"
version = "0.1.0"

[dependencies]
forge-std = { path = "../../crates/forge-stdlib" }

# SCRAM-SHA-256 の計算のみ use raw {} で使用
[raw-dependencies]
sha2   = "0.10"
base64 = "0.22"
```

---

## 使用例（全体）

```forge
use crucible.{ connect, transaction }
use ./models/user.{ User }
use ./repositories/user_repository.{ UserRepository }

fn main() -> unit {
    connect("postgres://localhost/myapp_dev").await?

    // クエリビルダー
    let users = User::query()
        |> where("active = $1", [true])
        |> order_by("name", asc: true)
        |> limit(10)
        |> all().await?

    for user in users {
        println("${user.name} <${user.email}>")
    }

    // トランザクション
    transaction(|| {
        let user = User::insert(User {
            name:     "Bob"
            email:    "bob@example.com"
            password: hash("secret")
        }).await?
        println("created: ${user.id}")
        ok(unit)
    }).await?
}
```
