# ForgeScript アーキテクチャ設計方針

> 作成: 2026-04-08
> 関連: `lang/extend_idea.md`（デコレータ拡張 §8）、`lang/extends/spec.md`（E-3 operator / E-5 const fn）
> 前提: `extend_idea.md` §9（`forge build` 時の Rust コード保存）の実装後に効果を検証できる

---

## 背景と問題意識

Java のDIコンテナ（Spring）はリフレクションで依存を解決し、逆依存・循環依存をランタイムまたはコンテナ起動時に検出する。
Rust では所有権・ライフタイムの制約からランタイム DIコンテナが普及せず、`Arc<dyn Trait>` の手動配線が主流になっている。

手動DIの問題点：
- 「誰でもどこからでも依存を引っ張れる」ため逆依存の防止がコードレビュー頼みになる
- 依存方向のルールがコードに現れず、アーキテクチャが腐敗しやすい

ForgeScript は Rust の上に乗りつつ、**言語レベルで依存方向を設計できる**立場にある。
Java ほど重くなく、手動 DI より体系的な「中間的アプローチ」を採用する。

---

## 採用方針：A + B + C のハイブリッド

### アプローチ A：モジュール依存方向チェック（コンパイル時・`forge check` 強化）

`forge.toml` でアーキテクチャ層を宣言し、`forge check` が逆依存・循環依存をエラーにする。

```toml
# forge.toml
[architecture]
layers = [
    "src/domain",
    "src/usecase",
    "src/interface",
    "src/infrastructure",
]
# 上位（左）は下位（右）に依存できない
# domain は何にも依存しない
# usecase は domain のみ依存可
# interface は usecase / domain に依存可
# infrastructure は全層に依存可（実装側）
```

`forge check` の出力例：

```
エラー: 依存方向違反
  src/domain/user.forge が src/usecase/user_usecase.forge に依存しています
  domain → usecase の方向は禁止されています
  ルール: layers = [domain, usecase, interface, infrastructure]
```

**実装コスト**: 低（`forge check` の静的解析拡張）
**効果**: 依存方向の強制がコードレビュー不要になる

---

### アプローチ B：`@inject` デコレータ + `container {}` ブロック

デコレータ（`extend_idea.md` §8）を活用した宣言的 DI。
トランスパイル時に `Arc<dyn Trait>` の配線コードを自動生成する。

```forge
// ── Domain 層 ──────────────────────────────────────────
// trait のみ定義（実装は Infrastructure 層）
trait UserRepository {
    fn find_by_id(id: number) -> User?
    fn save(user: User) -> User!
}

trait EmailService {
    fn send(to: string, subject: string, body: string) -> unit!
}

// ── UseCase 層 ──────────────────────────────────────────
@service
struct RegisterUserUseCase {
    repo:  UserRepository   // trait に依存（実装に依存しない）
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

// ── Infrastructure 層 ───────────────────────────────────
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

// ── コンテナ設定（main.forge または di.forge）────────────
container {
    bind UserRepository  to PostgresUserRepository
    bind EmailService    to SmtpEmailService
}
```

**Rust 変換イメージ**:

```rust
// container { } ブロックが生成するコード
struct Container {
    user_repository: Arc<dyn UserRepository>,
    email_service:   Arc<dyn EmailService>,
}

impl Container {
    fn new(db: DbConnection, smtp_host: String, smtp_port: i64) -> Self {
        Self {
            user_repository: Arc::new(PostgresUserRepository { db }),
            email_service:   Arc::new(SmtpEmailService { host: smtp_host, port: smtp_port }),
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

**実装コスト**: 中（デコレータ実装後）
**効果**: Java の `@Autowired` 相当の体験。ボイラープレート削減

---

### アプローチ C：`typestate` による起動保証

「コンテナが未設定のままサーバーを起動できない」ことをコンパイル時に保証する。

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
    .configure(container { bind UserRepository to PostgresUserRepository })
    .start()?

// App::new().start() はコンパイルエラー（Unconfigured に start() は存在しない）
```

**実装コスト**: 低（`typestate` は実装済み・`container` との統合のみ）
**効果**: 依存未解決での起動をコンパイル時にブロック

---

## 命名規則の強制

アプローチ A の `forge check` 拡張として、層ごとの命名規則を検証できる。

```toml
# forge.toml
[architecture.naming]
"src/domain"         = { suffix = [] }                          # 自由（エンティティ・値オブジェクト）
"src/usecase"        = { suffix = ["UseCase", "Service"] }
"src/interface"      = { suffix = ["Controller", "Handler", "Presenter"] }
"src/infrastructure" = { suffix = ["Repository", "Gateway", "Client", "Adapter"] }
```

違反例：

```
警告: 命名規則違反
  src/usecase/user_db.forge: "UserDb" は UseCase / Service で終わっていません
  ヒント: UseCase 層の型名は UseCase または Service で終わることを推奨します
```

命名規則は**警告止まり**（エラーにするかはオプション設定）とし、既存プロジェクトへの強制適用を避ける。

---

## `forge new` テンプレート拡張

CLI でクリーンアーキテクチャの雛形を生成できるようにする。

```bash
# 基本プロジェクト（現在）
forge new my-app

# クリーンアーキテクチャ版
forge new my-app --template clean-arch

# Anvil マイクロサービス版
forge new my-service --template anvil-clean
```

### `--template clean-arch` 生成物

```
my-app/
├── forge.toml              ← [architecture] 設定入り
├── src/
│   ├── main.forge          ← container 設定 + App 起動
│   ├── domain/
│   │   ├── mod.forge
│   │   └── user.forge      ← struct User / trait UserRepository / trait EmailService
│   ├── usecase/
│   │   ├── mod.forge
│   │   └── register_user_usecase.forge
│   ├── interface/
│   │   ├── mod.forge
│   │   └── user_controller.forge
│   └── infrastructure/
│       ├── mod.forge
│       ├── postgres_user_repository.forge
│       └── smtp_email_service.forge
└── tests/
    └── register_user_test.forge
```

### `--template anvil-clean` 生成物

`clean-arch` に Anvil HTTP フレームワークを統合した版。
`interface/` 層が `AnvilRouter` を保持し、`infrastructure/` 層に HTTP クライアント・DB 接続が入る。

---

## Rust コード保存との関係

`extend_idea.md` §9 で提案した `target/forge_rs/` への自動保存が実装されると、
**生成された Rust コードがクリーンアーキテクチャになっているか目視確認できる**。

確認すべきポイント：

1. `container {}` が生成する `Arc<dyn Trait>` 配線が正しいか
2. 層ごとにモジュールが分割されているか（`mod domain;` / `mod usecase;` 等）
3. 依存方向が Rust コードレベルでも守られているか（`use` 文の方向）

**実装順序の推奨**:

```
[1] extend_idea.md §9: forge build 時の Rust コード保存（target/forge_rs/）
      ↓ 生成コードを目で確認できるようになる
[2] アプローチ A: forge check での依存方向チェック
      ↓ 逆依存がエラーになる
[3] forge new --template clean-arch
      ↓ 雛形が使えるようになる
[4] アプローチ B: @inject / container {}
      ↓ DIの自動配線
[5] アプローチ C: typestate との統合
      ↓ 起動保証
```

---

## 優先度まとめ

| 機能 | 実装コスト | 依存 | 優先度 |
|---|---|---|---|
| `forge build` Rust コード保存 | 低 | なし | **最優先** |
| 依存方向チェック（A） | 低 | なし | **高** |
| 命名規則チェック（A 拡張） | 低 | A | **高** |
| `forge new --template clean-arch` | 中 | A | **高** |
| `@inject` / `container {}` (B) | 中 | デコレータ拡張 | 中 |
| `typestate` 起動保証（C） | 低 | B | 中 |
| `forge new --template anvil-clean` | 低 | clean-arch テンプレート | 低 |
