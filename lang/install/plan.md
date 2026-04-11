# forge インストール実装計画

> 仕様: `lang/install/spec.md`

---

## Phase I-1: Vagrant 検証環境

**目標**: `vagrant up` 1コマンドで Rust + forge がインストールされ、スモークテストが通る状態を作る。

### 手順

1. `vagrant/Vagrantfile` を作成（ubuntu/jammy64・1 CPU・2 GB）
2. `vagrant/provision.sh` を作成
   - apt-get update
   - rustup で stable インストール
   - `cargo install --git` で forge をインストール
   - PATH 設定
3. `vagrant up` で動作確認
4. スモークテスト（`forge --version` / `forge run hello.fg`）を手動実行

**完了条件**: `vagrant up && vagrant ssh -c "forge --version"` が成功する

---

## Phase I-2: install.sh + GitHub Releases バイナリ

**目標**: ソースビルド不要の 1 行インストールを実現する。

### 手順

1. CI（GitHub Actions）でクロスコンパイルジョブを追加
   - `ubuntu-latest` で `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`
   - `macos-latest` で `x86_64-apple-darwin` / `aarch64-apple-darwin`
2. `git tag v0.x.x` → GitHub Release に自動アップロード
3. `install.sh` を作成（OS/ARCH 判定 → バイナリダウンロード → `/usr/local/bin/`）
4. `vagrant/provision.sh` の `cargo install --git` を `install.sh` 方式に切り替えて再検証

**完了条件**: `curl ... | sh` でインストールでき、スモークテストが通る

---

## Phase I-3: MCP サーバ動作確認（forge-mcp 実装後）

**目標**: `forge mcp start/status/stop` が Linux で動作する。

### 手順

1. `vagrant/smoke_test.sh` に MCP テストを追加
2. `forge mcp start` → `forge mcp status` → `forge mcp stop` の順で確認

---

## 依存関係

```
I-1 ──→ I-2 ──→ I-3
         ↑
    GitHub Releases CI
    （別途 CI 設定が必要）
```

I-1 は独立して着手可能。I-2 は GitHub リポジトリ公開後。
