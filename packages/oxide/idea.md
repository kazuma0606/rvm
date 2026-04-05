# oxide — ForgeScript製 Webフレームワーク構想

## コンセプト

oxide は ForgeScript で書かれた Laravel 風 Web フレームワーク。
`forge build` によって本物の Rust + Axum コードが生成され、ネイティブ性能で動作する。

> "Forge で書いた Web アプリが、そのまま Rust の速さで動く"

---

## ポジショニング

| フレームワーク | 対象 | 特徴 |
|--------------|------|------|
| **anvil**    | ForgeScript-native | 軽量・標準ライブラリ完結 |
| **oxide**    | ForgeScript → Rust | Laravel 風・Axum バックエンド・フルスタック |

anvil と oxide は競合しない。
oxide は「Forge が本番 Rust アプリを作れる」ことの証明であり、採用障壁を下げるキラーアプリ。

---

## アーキテクチャ

```
開発者 (Forge)          コンパイル後 (Rust)
┌──────────────────┐    ┌──────────────────────────────┐
│ oxide/*.forge    │ →  │ Axum + Clean Architecture    │
│ ~500 行          │    │ ~10,000 行相当               │
└──────────────────┘    └──────────────────────────────┘
```

### レイヤー構成（Clean Architecture）

```
oxide/
├── domain/          # エンティティ・値オブジェクト・ドメインイベント
├── application/     # ユースケース・コマンド・クエリ (CQRS)
├── infrastructure/  # DB・外部 API・リポジトリ実装
├── presentation/    # Router・ハンドラ・DTO
└── shared/          # エラー型・ユーティリティ
```

---

## ForgeScript での記述イメージ

### ドメインモデル

```forge
data User {
    id: string
    email: string
    role: "user" | "admin" | "superadmin"
    created_at: string
}
```

### ユースケース

```forge
fn create_user(input: CreateUserInput) -> User! {
    let hashed = hash(input.password)?
    User::create({ ...input, password: hashed })
}
```

### ルーター

```forge
use oxide::{ Router, middleware }
use oxide::auth::{ jwt, require_role }
use oxide::cors::CorsOptions

let app = Router::new()
    .post("/users",        create_user)
    .get("/users/{id}",    get_user)
    .delete("/users/{id}", delete_user |> require_role("admin"))
    .middleware(CorsOptions::any())
    .middleware(jwt::bearer())
```

### 認証プロバイダ

```forge
trait AuthProvider {
    fn verify(token: string) -> Claims!
    fn has_role(claims: Claims, role: string) -> bool
}

struct JwtAuthProvider {
    secret: string
}

impl AuthProvider for JwtAuthProvider {
    fn verify(token: string) -> Claims! {
        jwt::decode(token, self.secret)?
    }
    fn has_role(claims: Claims, role: string) -> bool {
        claims.role == role
    }
}
```

---

## 実装ロードマップ

### Phase OX-0: 基盤（Forge 側の対応待ち）

| 必要機能 | Forge 現状 | 対応 Phase |
|---------|-----------|-----------|
| `async fn` / `.await` | 未実装 | A-5 |
| `trait` / `impl Trait for` | 未実装 | T-1 |
| `Arc<dyn Trait>` DI | 未実装 | T-2 |
| 外部クレート `use` | 部分実装 | M-1 |
| カスタムエラー型 | `!` 型あり | E-1 |

### Phase OX-1: ドメイン層（Forge 実装開始）

- `data` 型でエンティティ定義
- バリデーション (`validate` アノテーション)
- ドメインイベント (`emit` キーワード)

### Phase OX-2: Application 層

- CQRS コマンド / クエリの Forge 構文
- `fn handle(cmd: CreateUserCommand) -> UserDto!` パターン

### Phase OX-3: Infrastructure 層

- DB アクセス (`oxide::db::query`)
- マイグレーション (`oxide migrate`)
- PostgreSQL / SQLite 対応

### Phase OX-4: Presentation 層

- `Router::new()` DSL
- ミドルウェアチェーン
- リクエスト / レスポンス型

### Phase OX-5: oxide-cli

- `forge new --template oxide` でプロジェクト生成
- `oxide make:controller`, `oxide make:usecase` 等のスキャフォールド
- `oxide migrate` でマイグレーション実行

---

## oxide-cli コマンド（構想）

```bash
forge new my-app --template oxide   # プロジェクト作成
oxide make:controller UserController # コントローラ生成
oxide make:usecase CreateUser        # ユースケース生成
oxide make:model User                # ドメインモデル生成
oxide migrate                        # マイグレーション実行
oxide serve                          # 開発サーバー起動
```

---

## 参照実装

既存の Rust 実装: `https://github.com/kazuma0606/oxide.git`

- Axum + Clean Architecture + JWT + CQRS の約 10,000 行
- この Rust コードが `forge build` の **生成物** となることがゴール
- 既存実装はリファレンスとして参照しつつ、Forge ソースから再生成する

---

## 成功指標

- [ ] oxide の Hello World が Forge で書ける
- [ ] CRUD API が Forge ~100 行で実装できる
- [ ] JWT 認証付き REST API が Forge ~200 行で実装できる
- [ ] 生成された Rust コードが `cargo build --release` でコンパイルできる
- [ ] ベンチマーク: actix-web 比 80% 以上の RPS を達成
