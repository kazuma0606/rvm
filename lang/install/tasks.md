# forge インストール タスク一覧

> 仕様: `lang/install/spec.md`
> 計画: `lang/install/plan.md`

---

## Phase I-1: Vagrant 検証環境

### I-1-A: Vagrantfile 作成
- [ ] `vagrant/Vagrantfile` を作成
  - box: `ubuntu/jammy64`
  - CPU: 1、メモリ: 2048 MB
  - GUI: false
  - synced_folder: disabled
  - provision: `provision.sh`

### I-1-B: provision.sh 作成
- [ ] `vagrant/provision.sh` を作成
  - `apt-get update -qq`
  - rustup 非対話インストール（`-y --default-toolchain stable --no-modify-path`）
  - `/home/vagrant/.bashrc` に PATH 追加
  - `rustc --version` / `cargo --version` で確認
  - `cargo install --git <repo> --bin forge --locked`
  - `forge --version` で確認

### I-1-C: スモークテストスクリプト作成
- [ ] `vagrant/smoke_test.sh` を作成
  - `forge --version` — exit 0・バージョン文字列出力
  - `forge run hello.fg` — `Hello, ForgeScript!` 出力
  - `forge run http_test.fg` — status 200・ok true 出力
  - `forge build build_test.fg -o /tmp/out && /tmp/out` — `42` 出力

### I-1-D: 動作確認
- [ ] `vagrant up` が正常完了する（provision.sh エラーなし）
- [ ] `vagrant ssh -c "forge --version"` が成功する
- [ ] `vagrant ssh -c "/vagrant/smoke_test.sh"` が全テスト合格する

---

## Phase I-2: install.sh + GitHub Releases バイナリ

### I-2-A: CI クロスコンパイル
- [ ] `.github/workflows/release.yml` を作成
  - trigger: `git push --tags`
  - matrix: `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu` / `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - artifacts を GitHub Release にアップロード

### I-2-B: install.sh 作成
- [ ] `install.sh` を作成（リポジトリルート）
  - OS / ARCH 判定
  - GitHub Releases からバイナリ取得
  - `/usr/local/bin/forge` に配置
  - `forge --version` で確認

### I-2-C: provision.sh 切り替え
- [ ] `vagrant/provision.sh` の `cargo install --git` を `install.sh` 方式に切り替え
- [ ] `vagrant up` + `smoke_test.sh` で再検証

---

## Phase I-3: MCP サーバ動作確認

> forge-mcp 実装後に着手

### I-3-A: MCP スモークテスト追加
- [ ] `vagrant/smoke_test.sh` に以下を追加
  - `forge mcp start` が成功する
  - `forge mcp status` が `running` を出力する
  - `forge mcp stop` が成功する
- [ ] `vagrant ssh -c "/vagrant/smoke_test.sh"` が全テスト合格する

---

## 進捗サマリ

| Phase | タスク数 | 完了 |
|---|---|---|
| I-1 | 8 | 0 |
| I-2 | 5 | 0 |
| I-3 | 2 | 0 |
| **合計** | **15** | **0** |
