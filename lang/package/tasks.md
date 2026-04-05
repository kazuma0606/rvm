# forge new + forge.toml タスク
> plan.md に沿った実装タスク一覧

---

## Phase P-0: CLI エントリ追加

- [x] `crates/forge-cli/src/main.rs` の match ブロックに `Some("new")` ブランチを追加する
- [x] `--template <name>` オプションを手動でパースする処理を実装する
- [x] テンプレート名が未指定の場合は `"script"` をデフォルトにする
- [x] `forge new --help` でコマンド説明が表示されることを確認する

---

## Phase P-1: forge.toml パーサー

### データ構造

- [x] `crates/forge-cli/src/forge_toml.rs` を新規追加する
- [x] `ForgeToml` 構造体を実装する (`package` / `build` / `dependencies`)
- [x] `PackageSection` 構造体を実装する (`name` / `version` / `forge` / `entry`)
- [x] `BuildSection` 構造体を実装する (`output` / `edition`)
- [x] `entry` のデフォルト値を `"src/main.forge"` にする
- [x] `edition` のデフォルト値を `"2021"` にする

### パース実装

- [x] `crates/forge-cli/Cargo.toml` に `toml = "0.8"` を追加する
- [x] `ForgeToml::load(dir: &Path) -> Result<Self>` を実装する
- [x] `ForgeToml::find(start: &Path) -> Option<PathBuf>` を実装する
- [x] `[package]` セクションの必須フィールド (`name` / `version`) がない場合にエラーを返す
- [x] `[dependencies]` に文字列版 (`"1.0"`) とテーブル版 (`{ version = "1", features = [...] }`) の両方を受け付ける

### テスト

- [x] `forge_toml.rs` に最小 forge.toml のパーステストを書く
- [x] `entry` 省略時にデフォルト値 `"src/main.forge"` が使われることのテストを書く
- [x] `[dependencies]` の文字列版パーステストを書く
- [x] `[dependencies]` のテーブル版パーステストを書く
- [x] 不正な TOML でエラーになることのテストを書く

---

## Phase P-2: テンプレートエンジン

### テンプレートエンジン本体

- [x] `crates/forge-cli/src/templates.rs` を新規追加する
- [x] `Template` 構造体を実装する (`name: &str`, `files: &[(&str, &str)]`)
- [x] `render(content: &str, vars: &[(&str, &str)]) -> String` を実装する (`{{name}}` 置換)
- [x] `write_template(dest: &Path, template: &Template, vars: &[(&str, &str)]) -> Result<()>` を実装する
- [x] 既存ディレクトリがある場合はエラーにする
- [x] 中間ディレクトリ (`src/` など) を自動作成する
- [x] テンプレート変数 `{{name}}` / `{{version}}` / `{{forge_version}}` を実装する

### `script` テンプレート

- [x] `crates/forge-cli/templates/script/forge.toml` を追加する
- [x] `crates/forge-cli/templates/script/src/main.forge` を追加する (`fn main() { println("Hello, {{name}}!") }`)
- [x] テンプレートを `include_str!` でバイナリに埋め込む

### `cli` テンプレート

- [x] `crates/forge-cli/templates/cli/forge.toml` を追加する
- [x] `crates/forge-cli/templates/cli/src/main.forge` を追加する (CLI 雛形)
- [x] テンプレートを `include_str!` でバイナリに埋め込む

### `lib` テンプレート

- [x] `crates/forge-cli/templates/lib/forge.toml` を追加する (`entry = "src/lib.forge"`)
- [x] `crates/forge-cli/templates/lib/src/lib.forge` を追加する
- [x] テンプレートを `include_str!` でバイナリに埋め込む

### `data` テンプレート

- [x] `crates/forge-cli/templates/data/forge.toml` を追加する
- [x] `crates/forge-cli/templates/data/src/main.forge` を追加する (data 使用例)
- [x] テンプレートを `include_str!` でバイナリに埋め込む

### `anvil` テンプレート

- [x] `crates/forge-cli/templates/anvil/forge.toml` を追加する
- [x] `crates/forge-cli/templates/anvil/src/main.forge` を追加する (`Anvil::new()` 相当の最小雛形)
- [x] `crates/forge-cli/templates/anvil/src/request.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/src/response.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/src/router.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/src/middleware.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/src/cors.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/src/auth.forge` を追加する
- [x] `crates/forge-cli/templates/anvil/settings.json.example` を追加する
- [x] テンプレートを `include_str!` でバイナリに埋め込む

### `forge new` コマンド実装

- [x] `crates/forge-cli/src/new.rs` を本実装する
- [x] `run(name: Option<&str>, template: &str) -> Result<()>` を実装する
- [x] `name` が `None` の場合にインタラクティブプロンプトで入力を求める
- [x] テンプレート名が不正な場合にエラーメッセージと利用可能テンプレート一覧を表示する
- [x] 既存ディレクトリが対象にある場合にエラーを返す
- [x] 生成後に `Created <name>/` と `cd <name> && forge run src/main.forge` を表示する
- [x] `--git` オプションで `git init` を実行する (デフォルトは実行しない)

### テスト

- [x] `templates.rs` に `render()` のテストを書く (変数置換が正しく効くこと)
- [x] `forge new my-test-app` でファイルが正しく生成される integration test を書く
- [x] `forge new my-test-app --template cli` で cli テンプレートが展開されるテストを書く
- [x] 既存ディレクトリへの生成でエラーになることのテストを書く

---

## Phase P-3: `forge build` との forge.toml 統合

- [x] `main.rs` の `build` コマンドで引数がディレクトリパスの場合に `forge.toml` を探す分岐を追加する
- [x] 引数なしの場合はカレントディレクトリの `forge.toml` を探す分岐を追加する
- [x] `forge.toml` が見つかった場合に `package.entry` をエントリポイントとして使う
- [x] `forge.toml` の `[dependencies]` から `Cargo.toml` の `[dependencies]` セクションを構成する
- [x] `build.output` に従って出力先を決定する (デフォルト `target/<name>`)
- [x] 単一ファイル指定 (`forge build src/main.forge`) の既存挙動を維持する
- [x] `forge build packages/anvil/` で `forge.toml` を読んで正しく動くことを確認する

---

## Phase P-4: `forge run` / `forge test` との forge.toml 統合

- [x] `forge run` がディレクトリ指定時に `forge.toml` を読んでエントリポイントを解決する
- [x] `forge test` がディレクトリ指定時に `forge.toml` を読んで `tests/*.test.forge` を探索する
- [x] 単一ファイル指定の既存挙動を維持する

---

## 進捗サマリ

| Phase | タスク数 | 完了数 | 進捗 |
|-------|---------:|-------:|-----:|
| P-0   | 4        | 4      | 100% |
| P-1   | 16       | 16     | 100% |
| P-2   | 33       | 33     | 100% |
| P-3   | 7        | 7      | 100% |
| P-4   | 3        | 3      | 100% |
| **合計** | **63** | **63** | **100%** |
