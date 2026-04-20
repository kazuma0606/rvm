# ForgeScript DI 仕様

> バージョン対象: v0.2.0 以降
> 関連ファイル:
>   - `lang/typedefs/spec.md` — trait / mixin / typestate（前提・実装済み）
>   - `lang/architecture.md` — 設計方針の原案（本ファイルに統合）
>   - `lang/std/v1/spec.md` — Logger / MetricsBackend / EventBus との統合例
>   - `lang/extend_idea.md §5` — ユーザー定義デコレータの背景

---

## 1. 設計方針

### 背景

Java の DI コンテナ（Spring）はリフレクションで依存を解決し、逆依存・循環依存をランタイムまたはコンテナ起動時に検出する。Rust では所有権・ライフタイムの制約からランタイム DI コンテナが普及せず、`Arc<dyn Trait>` の手動配線が主流になっている。

手動 DI の問題点：

- 「誰でもどこからでも依存を引っ張れる」ため逆依存の防止がコードレビュー頼みになる
- 依存方向のルールがコードに現れず、アーキテクチャが腐敗しやすい

ForgeScript は Rust の上に乗りつつ、**言語レベルで依存方向を設計できる**立場にある。Java ほど重くなく、手動 DI より体系的な「中間的アプローチ」を採用する。

### 採用方針: A + B + C のハイブリッド

| アプローチ | 内容 | 実装コスト |
|---|---|---|
| A | `forge check` による依存方向チェック | 低 |
| B | `@service` / `@repository` + `container {}` による宣言的 DI | 中 |
| C | `typestate` との統合による起動保証 | 低（typestate は実装済み） |

---

## 2. デコレータシステム（アプローチ B の前提）

`@derive` は既に実装済みだが、DI では **組み込みデコレータ**としてコンパイラが特別扱いするものを追加定義する。ユーザー定義デコレータ（`extend_idea.md §5`）とは別枠で、まずこの組み込みセットを先行実装する。

### 2-1. `@service`

UseCase・サービス層の struct に付与する。`container {}` が配線対象として認識する。

```forge
@service
struct RegisterUserUseCase {
    repo:  UserRepository   // trait に依存
    email: EmailService
}
```

トランスパイル後: `container` が `Arc<dyn UserRepository>` と `Arc<dyn EmailService>` を自動で差し込む。

### 2-2. `@repository`

Infrastructure 層のリポジトリ実装に付与する。`container {}` が `bind` の右辺候補として認識する。

```forge
@repository
struct PostgresUserRepository {
    db: DbConnection
}

impl UserRepository for PostgresUserRepository { ... }
```

### 2-3. `@on`（イベント購読）

`forge/std/event` と連携するサービス層のイベントハンドラに付与する。`container` 初期化時にトランスパイラが自動登録コードを生成する。

```forge
@service
struct NotificationService {
    mailer: EmailService
}

impl NotificationService {
    @on(UserCreated)
    fn handle_user_created(self, event: UserCreated) -> unit! {
        self.mailer.send(event.email, "ようこそ", "登録完了メール本文")?
    }
}
```

### 2-4. `@timed`（メトリクス計測）

メソッドの実行時間を `forge/std/metrics` の `MetricsBackend` に自動記録する。

```forge
@timed(metric: "api_request_duration_ms")
fn handle_request(req: Request) -> Response! { ... }
```

トランスパイル後: `Instant::now()` + `.elapsed()` でラップし、`metrics.histogram(...)` を呼ぶコードを生成。

---

## 3. `container {}` ブロック

### 3-1. 基本構文

```forge
container {
    bind TraitName to ConcreteImplementation
}
```

`bind` は trait → 具体実装の対応を宣言する。`container` ブロックは `main.forge` または専用の `di.forge` に置く。

### 3-2. 完全な使用例

```forge
// ── Domain 層（src/domain/user.forge）────────────────────
trait UserRepository {
    fn find_by_id(id: number) -> User?
    fn save(user: User) -> User!
}

trait EmailService {
    fn send(to: string, subject: string, body: string) -> unit!
}

// ── UseCase 層（src/usecase/register_user_usecase.forge）──
@service
struct RegisterUserUseCase {
    repo:  UserRepository
    email: EmailService
}

impl RegisterUserUseCase {
    fn execute(self, name: string, email_addr: string) -> User! {
        let user = User { name: name, email: email_addr }
        let saved = self.repo.save(user)?
        self.email.send(saved.email, "登録完了", "ようこそ")?
        ok(saved)
    }
}

// ── Infrastructure 層（src/infrastructure/）───────────────
@repository
struct PostgresUserRepository {
    db: DbConnection
}

impl UserRepository for PostgresUserRepository {
    fn find_by_id(id: number) -> User? { /* ... */ }
    fn save(user: User) -> User! { /* ... */ }
}

@service
struct SmtpEmailService {
    host: string
    port: number
}

impl EmailService for SmtpEmailService {
    fn send(to: string, subject: string, body: string) -> unit! { /* ... */ }
}

// ── コンテナ設定（src/main.forge）────────────────────────
container {
    bind UserRepository to PostgresUserRepository
    bind EmailService   to SmtpEmailService
}
```

### 3-3. 環境による切り替え

`bind` の右辺に `match` 式を使える。

```forge
container {
    bind Logger to match env_or("LOG_BACKEND", "json") {
        "prometheus" => PrometheusLogger::new(env_require("METRICS_PORT"))
        "silent"     => SilentLogger::new()
        _            => JsonLogger::new()
    }
    bind MetricsBackend to PrometheusBackend::new(env_number_or("METRICS_PORT", 9090))
    bind EventBus       to match env_or("ENV", "development") {
        "production" => AmqpEventBus::new(env_require("AMQP_URL"))
        _            => InMemoryEventBus::new()
    }
}
```

### 3-4. Rust トランスパイル結果

`container {}` ブロックは以下の Rust コードに変換される。

```rust
struct Container {
    user_repository: Arc<dyn UserRepository>,
    email_service:   Arc<dyn EmailService>,
}

impl Container {
    fn new(db: DbConnection, smtp_host: String, smtp_port: i64) -> Self {
        Self {
            user_repository: Arc::new(PostgresUserRepository { db }),
            email_service:   Arc::new(SmtpEmailService {
                host: smtp_host,
                port: smtp_port,
            }),
        }
    }

    fn register_user_use_case(&self) -> RegisterUserUseCase {
        RegisterUserUseCase {
            repo:  Arc::clone(&self.user_repository),
            email: Arc::clone(&self.email_service),
        }
    }
}
```

---

## 4. 依存方向チェック（アプローチ A）

### 4-1. `forge.toml` でのレイヤー宣言

```toml
[architecture]
layers = [
    "src/domain",
    "src/usecase",
    "src/interface",
    "src/infrastructure",
]
```

リスト上位の層ほど内側。下位の層が上位の層に依存することは禁止。

- `domain` は何にも依存しない
- `usecase` は `domain` のみ依存可
- `interface` は `usecase` / `domain` に依存可
- `infrastructure` は全層に依存可（実装側）

### 4-2. `forge check` の出力例

```
エラー: 依存方向違反
  src/domain/user.forge が src/usecase/user_usecase.forge に依存しています
  domain → usecase の方向は禁止されています
  ルール: layers = [domain, usecase, interface, infrastructure]
```

### 4-3. 命名規則チェック（オプション）

`forge.toml` に `[architecture.naming]` を追加することで、層ごとの命名規則を警告できる。

```toml
[architecture.naming]
"src/domain"         = { suffix = [] }
"src/usecase"        = { suffix = ["UseCase", "Service"] }
"src/interface"      = { suffix = ["Controller", "Handler", "Presenter"] }
"src/infrastructure" = { suffix = ["Repository", "Gateway", "Client", "Adapter"] }
```

命名規則違反は**警告止まり**（エラーにするかはオプション設定）。既存プロジェクトへの強制適用を避けるため。

```
警告: 命名規則違反
  src/usecase/user_db.forge: "UserDb" は UseCase / Service で終わっていません
  ヒント: UseCase 層の型名は UseCase または Service で終わることを推奨します
```

---

## 5. `typestate` との統合（アプローチ C）

「コンテナが未設定のままサーバーを起動できない」ことをコンパイル時に保証する。`typestate` は実装済みのため、`container` との統合のみが新規実装対象となる。

```forge
typestate App {
    Unconfigured -> Configured -> Running -> Stopped

    Unconfigured {
        fn configure(self, c: container) -> Configured { ... }
    }
    Configured {
        fn start(self) -> Running! { ... }
    }
    Running {
        fn stop(self) -> Stopped { ... }
    }
}

// 使う側
let app = App::new()
    .configure(container {
        bind UserRepository to PostgresUserRepository
        bind EmailService   to SmtpEmailService
    })
    .start()?

// App::new().start() はコンパイルエラー
// （Unconfigured に start() は存在しない）
```

---

## 6. 標準ライブラリとの統合

`container {}` は以下の標準ライブラリ trait と組み合わせて使う設計になっている。各 trait の詳細は `lang/std/v1/spec.md` を参照。

| trait | バインド例 | 用途 |
|---|---|---|
| `Logger` | `bind Logger to JsonLogger::new()` | 構造化ログ |
| `MetricsBackend` | `bind MetricsBackend to PrometheusBackend::new(...)` | メトリクス |
| `EventBus` | `bind EventBus to AmqpEventBus::new(...)` | イベント Pub/Sub |
| `AppConfig` | `bind AppConfig to Config::load(AppConfig)?` | 設定管理 |

バックエンドを変えても呼び出し側（`@service` の struct）のコードは一切変更不要。

---

## 7. `forge new` テンプレート

### `--template clean-arch`

```bash
forge new my-app --template clean-arch
```

生成物：

```
my-app/
├── forge.toml              ← [architecture] 設定入り
├── src/
│   ├── main.forge          ← container 設定 + App 起動
│   ├── domain/
│   │   ├── mod.forge
│   │   └── user.forge      ← data User / trait UserRepository / trait EmailService
│   ├── usecase/
│   │   ├── mod.forge
│   │   └── register_user_usecase.forge
│   ├── interface/
│   │   ├── mod.forge
│   │   └── user_handler.forge
│   └── infrastructure/
│       ├── mod.forge
│       ├── postgres_user_repository.forge
│       └── smtp_email_service.forge
└── tests/
    └── register_user_test.forge
```

### `--template anvil-clean`

`clean-arch` に Anvil HTTP フレームワークを統合した版。`interface/` 層が `AnvilRouter` を保持し、`infrastructure/` 層に HTTP クライアント・DB 接続が入る。

---

## 8. 実装ロードマップ

```
[1] forge check の依存方向チェック（アプローチ A）
      ↓ 逆依存がコンパイル時にエラーになる
[2] forge new --template clean-arch
      ↓ 雛形で正しい構造が手に入る
[3] @service / @repository デコレータのパーサー実装
      ↓ コンパイラがバインド対象を認識できる
[4] container {} 構文のパーサー + トランスパイル
      ↓ Arc<dyn Trait> の配線コードを自動生成
[5] typestate との統合（アプローチ C）
      ↓ 依存未解決での起動をコンパイル時にブロック
[6] @on / @timed デコレータ
      ↓ イベント購読・メトリクス計測の宣言的記述
```

| 機能 | 実装コスト | 依存 | 優先度 |
|---|---|---|---|
| 依存方向チェック（A） | 低 | なし | 高 |
| 命名規則チェック（A 拡張） | 低 | A | 高 |
| `forge new --template clean-arch` | 中 | A | 高 |
| `@service` / `@repository` パーサー | 低 | なし | 中 |
| `container {}` パーサー + トランスパイル | 中 | `@service` | 中 |
| `typestate` との統合（C） | 低 | `container {}` | 中 |
| `@on` / `@timed` デコレータ | 中 | `container {}` | 低 |
| `forge new --template anvil-clean` | 低 | clean-arch テンプレート | 低 |
