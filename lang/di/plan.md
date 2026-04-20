# ForgeScript DI 実装計画

> 仕様: `lang/di/spec.md`
> 前提: `lang/typedefs`（trait / mixin / typestate）が実装済み

---

## フェーズ構成

```
Phase DI-1: 依存方向チェック（forge check 拡張）
Phase DI-2: forge new --template clean-arch
Phase DI-3: @service / @repository デコレータ
Phase DI-4: container {} パーサー + トランスパイル
Phase DI-5: typestate との統合
Phase DI-6: @on / @timed デコレータ
```

---

## Phase DI-1: 依存方向チェック

### 目標

`forge.toml` に `[architecture]` セクションを宣言することで、`forge check` が逆依存・循環依存をエラーとして報告すること。命名規則違反を警告として報告すること。

### 実装ステップ

1. **`forge.toml` パーサー拡張**
   - `[architecture]` セクションの読み取り
   - `layers = [...]` の順序付きリストとして解析
   - `[architecture.naming]` セクションの読み取り

2. **依存グラフ構築**
   - 対象プロジェクトの `.forge` ファイルから `use` 宣言を収集
   - ファイルパスをレイヤーに分類（`forge.toml` の `layers` に基づく）
   - 有向グラフとして依存関係を構築

3. **逆依存・循環依存の検出**
   - `layers` の順序に従い、上位層から下位層への依存を検出（違反）
   - 循環依存の検出（Tarjan の強連結成分アルゴリズムで実装）
   - エラーメッセージ: ファイルパス・依存先・違反ルールを明示

4. **命名規則チェック**
   - `[architecture.naming]` の `suffix` リストを各ファイルの型名に照合
   - 違反は warning 扱い（エラーにするかはオプション設定 `naming_rules = "warn" | "error"`）

5. **`forge check` コマンドへの統合**
   - 既存の型チェックに加えてアーキテクチャチェックを実行
   - `--arch-only` フラグで依存チェックのみ実行できるオプション

6. **テスト**

---

## Phase DI-2: `forge new --template clean-arch`

### 目標

`forge new <name> --template clean-arch` でクリーンアーキテクチャの雛形プロジェクトが生成されること。

### 生成物

```
<name>/
├── forge.toml              ← [architecture] 設定入り
├── src/
│   ├── main.forge
│   ├── domain/
│   │   ├── mod.forge
│   │   └── user.forge
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

### 実装ステップ

1. **テンプレートファイルの作成**（`crates/forge-cli/templates/clean-arch/`）
   - 各ファイルの雛形（プレースホルダー `{{name}}` を含む）
   - `forge.toml` に `[architecture]` セクション入り

2. **`forge-cli` の `new` サブコマンド拡張**
   - `--template` フラグの追加
   - テンプレートディレクトリのコピーとプレースホルダー置換

3. **`--template anvil-clean` の追加**（DI-2 の延長）
   - `interface/` が `AnvilRouter` を使う版

4. **テスト**

---

## Phase DI-3: `@service` / `@repository` デコレータ

### 目標

`@service` / `@repository` がパーサーで認識され、インタープリタが struct にメタデータとして付与できること。`container {}` の DI-4 実装のための前提となる。

### 実装ステップ

1. **Lexer 拡張**
   - `@service` / `@repository` トークンの追加
   - 既存の `@derive` トークンと統一的に扱えるよう整理

2. **AST 拡張**
   ```rust
   Decorator::Service
   Decorator::Repository
   Decorator::On { event_type: String }
   Decorator::Timed { metric: String }
   ```
   - `Stmt::StructDef` の `derives` を `decorators: Vec<Decorator>` に拡張

3. **パーサー拡張**
   - `@service struct Name { ... }` のパース
   - `@repository struct Name { ... }` のパース
   - 複数デコレータの組み合わせ（例: `@service @derive(Debug)`）

4. **インタープリタ拡張**
   - 型レジストリに `is_service: bool` / `is_repository: bool` フラグを追加
   - デコレータ情報をメタデータとして保持（トランスパイル時に参照）

5. **テスト**

---

## Phase DI-4: `container {}` パーサー + トランスパイル

### 目標

`container { bind Trait to Implementation }` が動作し、`@service` の struct に依存が自動配線されること。

### 実装ステップ

1. **Lexer 拡張**
   - `container` / `bind` / `to` トークンの追加

2. **AST 拡張**
   ```rust
   Stmt::ContainerDef { bindings: Vec<Binding> }
   Binding { trait_name: String, implementation: Expr }
   ```
   - `implementation` は `Expr`（`match` 式による条件バインドに対応）

3. **パーサー拡張**
   - `container { bind Trait to Impl }` のパース
   - `bind Trait to match expr { ... }` のパース（環境切り替え）

4. **インタープリタ拡張**
   - `ContainerDef` → コンテナレジストリへの登録
   - `@service` の struct インスタンス化時に、フィールドの trait 型をコンテナから解決して自動注入

5. **Rust トランスパイル（`crates/forge-compiler`）**
   - `container {}` → `struct Container { ... }` + `impl Container::new()` の生成
   - `@service` struct → `Arc<dyn Trait>` フィールドに変換
   - `Container::register_<service>()` メソッドの自動生成

6. **テスト**

---

## Phase DI-5: `typestate` との統合

### 目標

`typestate App { Unconfigured -> Configured -> Running }` パターンで、`container` が未設定のまま起動できないことをコンパイル時に保証できること。

### 実装ステップ

1. **`container` を値として渡せるよう拡張**
   - `container { ... }` をインライン式として書けるようにする
   - `App::new().configure(container { bind ... })` の構文対応

2. **typestate の `configure(self, c: container)` シグネチャ対応**
   - `container` 型を型アノテーションとして認識

3. **Rust トランスパイル**
   - `configure` メソッドが `Container` struct を受け取るコードを生成

4. **テスト**

---

## Phase DI-6: `@on` / `@timed` デコレータ

### 目標

`@on(EventType)` で `forge/std/event` のイベント購読を宣言的に記述できること。`@timed` でメトリクス計測を自動化できること。

### 実装ステップ

1. **`@on` デコレータ**
   - パーサーで `@on(EventType)` を認識
   - `container` 初期化時にトランスパイラが自動登録コード（`event_bus.subscribe(...)`)を生成

2. **`@timed` デコレータ**
   - パーサーで `@timed(metric: "name")` を認識
   - トランスパイラが `Instant::now()` + `.elapsed()` + `metrics.histogram(...)` でラップするコードを生成

3. **テスト**

---

## テスト方針

各フェーズ完了後に以下を実施：

- ユニットテスト（パーサー・インタープリタ各層）
- E2E テスト（`forge run` で動作確認）
- DI-4 以降は `forge build` のトランスパイル結果（`target/forge_rs/`）も確認
