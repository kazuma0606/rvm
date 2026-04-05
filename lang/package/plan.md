# forge new + forge.toml 実装計画

> `forge new` テンプレートエンジンと `forge.toml` パイプラインの実装プラン
> これが完成すると `forge new --template anvil` → `forge build` の接続が完成する

---

## 全体の依存関係

```
forge new <name> --template <t>
    ↓ forge.toml + src/*.forge を生成
forge build <dir>/
    ↓ forge.toml を読んでエントリポイント・依存解決
    ↓ .forge → .rs トランスパイル（既存 forge-transpiler）
    ↓ Cargo.toml を一時生成
    ↓ cargo build
target/<name> バイナリ完成
```

---

## 変更ファイル一覧

```
crates/forge-cli/
├── Cargo.toml                    ← P-1: toml クレートを追加
├── src/
│   ├── main.rs                   ← P-0: Some("new") ブランチ追加
│   │                               P-3: build コマンドに forge.toml 解決を追加
│   ├── new.rs          (新規)    ← P-1: forge new コマンド実装
│   ├── forge_toml.rs   (新規)    ← P-1: forge.toml パーサー・データ構造
│   └── templates.rs    (新規)    ← P-2: テンプレート埋め込み・展開ロジック
└── templates/          (新規)    ← P-2: テンプレートファイル群
    ├── script/
    │   ├── forge.toml
    │   └── src/main.forge
    ├── cli/
    │   ├── forge.toml
    │   └── src/main.forge
    ├── lib/
    │   ├── forge.toml
    │   └── src/lib.forge
    ├── data/
    │   ├── forge.toml
    │   └── src/main.forge
    └── anvil/
        ├── forge.toml
        ├── src/
        │   ├── main.forge
        │   ├── request.forge
        │   ├── response.forge
        │   ├── router.forge
        │   ├── middleware.forge
        │   ├── cors.forge
        │   └── auth.forge
        └── settings.json.example
```

---

## Phase ごとの実装詳細

### Phase P-0: CLI エントリ追加

`crates/forge-cli/src/main.rs` の match ブロックに `Some("new")` を追加する。

```rust
Some("new") => {
    let name = args.get(2).map(|s| s.as_str());
    let template = args.iter()
        .position(|a| a == "--template")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("script");
    new::run(name, template)?;
}
```

---

### Phase P-1: forge.toml パーサー

`crates/forge-cli/src/forge_toml.rs` に以下を実装する。

```rust
#[derive(Debug)]
pub struct ForgeToml {
    pub package: PackageSection,
    pub build:   Option<BuildSection>,
    pub deps:    IndexMap<String, DepValue>,
}

#[derive(Debug)]
pub struct PackageSection {
    pub name:    String,
    pub version: String,
    pub forge:   Option<String>,
    pub entry:   String,  // デフォルト: "src/main.forge"
}

#[derive(Debug)]
pub struct BuildSection {
    pub output:  Option<String>,  // デフォルト: "target/<name>"
    pub edition: String,          // デフォルト: "2021"
}
```

**TOML パース方法**: `toml` クレートを使用（`Cargo.toml` に追加）

```rust
impl ForgeToml {
    pub fn load(dir: &Path) -> Result<Self>  // forge.toml を読んでパース
    pub fn find(start: &Path) -> Option<PathBuf>  // 親ディレクトリを遡って探索
}
```

---

### Phase P-2: テンプレートエンジン

`crates/forge-cli/src/templates.rs` にテンプレート展開ロジックを実装する。

**テンプレートの埋め込み方法**: `include_str!` マクロでバイナリに埋め込む。

```rust
pub struct Template {
    pub name:  &'static str,
    pub files: &'static [(&'static str, &'static str)],  // (相対パス, 内容)
}

// テンプレート変数の置換: {{name}}, {{version}} など
pub fn render(content: &str, vars: &[(&str, &str)]) -> String
```

**テンプレート変数**:
- `{{name}}` — プロジェクト名
- `{{version}}` — 初期バージョン（`0.1.0`）
- `{{forge_version}}` — 現在の Forge バージョン

**テンプレート一覧**:

| テンプレート | 生成ファイル | 用途 |
|------------|------------|------|
| `script` | `forge.toml`, `src/main.forge` | デフォルト・スクリプト |
| `cli` | `forge.toml`, `src/main.forge` | CLIツール |
| `lib` | `forge.toml`, `src/lib.forge` | ライブラリ |
| `data` | `forge.toml`, `src/main.forge` | データ処理 |
| `anvil` | `forge.toml`, `src/*.forge` x7, `settings.json.example` | HTTP サーバ |

---

### Phase P-3: `forge build` への forge.toml 統合

`crates/forge-cli/src/main.rs` の `build` コマンドを更新する。

**現状の動作**:
```
forge build src/main.forge  →  単一ファイルをトランスパイル
```

**更新後の動作**:
```
forge build                 →  カレントディレクトリの forge.toml を読む
forge build packages/anvil/ →  指定ディレクトリの forge.toml を読む
forge build src/main.forge  →  従来通り単一ファイル（後方互換）
```

**forge.toml から Cargo.toml を生成するフロー**:
```rust
fn build_from_toml(forge_toml: &ForgeToml, project_dir: &Path) -> Result<()> {
    // 1. エントリポイントを解決
    let entry = project_dir.join(&forge_toml.package.entry);
    // 2. .forge → .rs トランスパイル（既存 transpiler 呼び出し）
    // 3. 一時ディレクトリに Cargo.toml を生成
    // 4. cargo build を実行
}
```

---

### Phase P-4: `forge run` / `forge test` への forge.toml 統合

`forge run` と `forge test` も同様に forge.toml を認識するよう更新する。

---

## Anvil テンプレートの内容（`templates/anvil/`）

`packages/anvil/` の実装ファイルをテンプレートとして埋め込む。
Anvil の Forge ソースが完成したタイミングでテンプレートに昇格させる。

```
templates/anvil/
├── forge.toml
│     name = "{{name}}"
│     entry = "src/main.forge"
├── src/
│   ├── main.forge       ← Anvil::new() + app.listen() の最小例
│   ├── request.forge    ← Request<T> data + impl
│   ├── response.forge   ← Response<T> data + impl + ErrorBody
│   ├── router.forge     ← Router + route registration
│   ├── middleware.forge ← logger / json_parser / require_role
│   ├── cors.forge       ← CorsOptions + cors()
│   └── auth.forge       ← AuthProvider trait + BearerAuthProvider
└── settings.json.example
```

---

## 依存クレートの追加

`crates/forge-cli/Cargo.toml` に追加：

```toml
[dependencies]
toml = "0.8"
```

> `toml` クレートは軽量（serde 依存のみ）で `std` 以外の大きな依存なし。

---

## テスト方針

- `forge new my-app` でディレクトリが正しく生成されることを確認（integration test）
- `forge new my-app --template anvil` で anvil テンプレートが展開されることを確認
- `forge.toml` のパーステスト（unit test in `forge_toml.rs`）
- `forge build packages/anvil/` が forge.toml を読んでビルドできることを確認

---

## 実装順序

```
P-0 (CLI エントリ) →
P-1 (forge.toml パーサー) →
P-2 (テンプレートエンジン + script/cli/lib/data テンプレート) →
P-2 (anvil テンプレート ← Anvil Forge ソース完成後に追加) →
P-3 (forge build への統合) →
P-4 (forge run / forge test への統合)
```
