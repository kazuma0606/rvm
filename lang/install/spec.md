# forge インストール仕様

> ROADMAP [11] DX強化 — Linux インストール対応

---

## 概要

ForgeScript を Linux 環境にインストールし動作確認するための仕様。
Vagrant による最小構成の仮想サーバを使い、Rust のインストールから
`forge` コマンドの動作確認まで一貫して検証する。

---

## Phase I-1: Vagrant 検証環境

### 目的

- Windows ホストから Linux 環境を即座に再現・破棄できる
- CI 相当の「クリーンな Linux」で forge のインストールを検証する
- 将来の `install.sh` / GitHub Releases バイナリのテスト基盤にする

### Vagrant VM 最小スペック

| 項目 | 値 | 備考 |
|---|---|---|
| Box | `ubuntu/jammy64` | Ubuntu 22.04 LTS |
| CPU | 1 vCPU | |
| Memory | 2048 MB | Rust コンパイルに必要な最低ライン |
| Disk | デフォルト（20 GB） | |
| ネットワーク | NAT（デフォルト） | `vagrant ssh` で接続 |
| GUI | なし | headless |
| Synced folder | 無効（`disabled: true`） | 検証は VM 内で完結 |

### Vagrantfile

```ruby
Vagrant.configure("2") do |config|
  config.vm.box = "ubuntu/jammy64"

  config.vm.provider "virtualbox" do |vb|
    vb.name   = "forge-verify"
    vb.cpus   = 1
    vb.memory = 2048
    vb.gui    = false
  end

  # synced folder 無効（プロジェクトファイルは VM 内に持ち込まない）
  config.vm.synced_folder ".", "/vagrant", disabled: true

  config.vm.provision "shell", path: "provision.sh"
end
```

### provision.sh（プロビジョニングスクリプト）

VM 初回起動時に自動実行。

```bash
#!/usr/bin/env bash
set -euo pipefail

# --- パッケージ更新 ---
apt-get update -qq

# --- Rust インストール（rustup） ---
# 非対話モードで stable toolchain をインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --default-toolchain stable --no-modify-path

# PATH を通す（このスクリプト内 + vagrant ユーザー用）
export PATH="$HOME/.cargo/bin:$PATH"
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> /home/vagrant/.bashrc

# Rust バージョン確認
rustc --version
cargo --version

# --- forge インストール ---
# TODO: Phase I-2 で GitHub Releases バイナリに切り替え
# 現状は cargo install --git でビルドインストール
cargo install --git https://github.com/<owner>/rvm --bin forge --locked

# forge バージョン確認
forge --version
```

> **注意**: `cargo install --git` は初回ビルドに 5〜10 分かかる。
> Phase I-2 でプリビルドバイナリ配布に切り替えることで解消する。

---

## Phase I-2: インストール方式

### フェーズ別インストール方式

| フェーズ | 方式 | 状態 |
|---|---|---|
| I-1（現在） | `cargo install --git` | ソースからビルド。検証用 |
| I-2 | `install.sh` + GitHub Releases バイナリ | ビルド不要。本番向け |

### install.sh（I-2 向け設計）

```bash
#!/usr/bin/env bash
# curl -sSf https://install.forgescript.dev | sh

set -euo pipefail

VERSION="${FORGE_VERSION:-latest}"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"   # linux / darwin
ARCH="$(uname -m)"                               # x86_64 / aarch64

# GitHub Releases から対応バイナリを取得
BASE_URL="https://github.com/<owner>/rvm/releases/download/${VERSION}"
BINARY="forge-${OS}-${ARCH}"

curl -sSfL "${BASE_URL}/${BINARY}" -o /tmp/forge
chmod +x /tmp/forge
mv /tmp/forge /usr/local/bin/forge

forge --version
echo "ForgeScript installed successfully."
```

対応プラットフォーム（リリースバイナリ）：

| OS | アーキテクチャ | バイナリ名 |
|---|---|---|
| Linux | x86_64 | `forge-linux-x86_64` |
| Linux | aarch64 | `forge-linux-aarch64` |
| macOS | x86_64 | `forge-darwin-x86_64` |
| macOS | arm64 | `forge-darwin-aarch64` |

> Windows は MSI インストーラまたは `winget` を別途検討。

---

## Phase I-3: 動作確認（スモークテスト）

プロビジョニング完了後または手動で実行。

### テスト手順

#### 1. バージョン確認

```bash
forge --version
# 期待出力例: forge 0.1.0
```

#### 2. Hello World（`forge run`）

```bash
cat <<'EOF' > /tmp/hello.fg
fn main() {
    println("Hello, ForgeScript!")
}
EOF

forge run /tmp/hello.fg
# 期待出力: Hello, ForgeScript!
```

#### 3. HTTP リクエスト（`forge run` + `forge/http`）

```bash
cat <<'EOF' > /tmp/http_test.fg
use forge/http.{ get }

fn main() {
    let res = get("https://httpbin.org/get").send()
    println(res.status)
    println(res.ok)
}
EOF

forge run /tmp/http_test.fg
# 期待出力:
# 200
# true
```

#### 4. ビルド確認（`forge build`）

```bash
cat <<'EOF' > /tmp/build_test.fg
fn main() {
    let x = 42
    println(x)
}
EOF

forge build /tmp/build_test.fg -o /tmp/build_out
/tmp/build_out
# 期待出力: 42
```

#### 5. MCP サーバ起動確認（forge-mcp 実装後）

```bash
forge mcp start
forge mcp status
# 期待出力: forge-mcp: running (pid XXXX)
forge mcp stop
```

### 合否判定

| テスト | 合格条件 |
|---|---|
| バージョン確認 | exit code 0、バージョン文字列が出力される |
| Hello World | 標準出力に `Hello, ForgeScript!` が含まれる |
| HTTP リクエスト | `200` と `true` が出力される |
| ビルド確認 | バイナリが生成され、実行結果が `42` |
| MCP 起動確認 | status が `running` になる |

---

## ファイル構成

```
lang/install/
  spec.md         ← 本ファイル
  plan.md         ← 実装計画（未作成）
  tasks.md        ← タスク一覧（未作成）

vagrant/          ← リポジトリルートに配置
  Vagrantfile
  provision.sh
```

---

## 前提条件（ホスト側）

- VirtualBox インストール済み
- Vagrant インストール済み
- インターネット接続あり（box ダウンロード・cargo install 用）

---

## 制約・注意事項

- `ubuntu/jammy64` box の初回ダウンロードは約 500 MB
- `cargo install --git`（I-1）は初回ビルドに 5〜10 分かかる
- VM メモリ 2048 MB 未満だとリンク時に OOM になる可能性がある
- `provision.sh` は root で実行されるため、`forge` は `/root/.cargo/bin/` にインストールされる
  → `vagrant ssh` 後は `vagrant` ユーザーの PATH に追加が必要（`provision.sh` で対応）
