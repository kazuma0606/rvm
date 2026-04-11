# forge インストール・MCP サーバ 実装計画

> 仕様: `lang/install/spec.md`

---

## Phase I-1: Docker 検証環境

**目標**: `docker compose run` 1コマンドでスモークテストが通る。

1. `docker/Dockerfile` 作成（Ubuntu 22.04・rustup・cargo install --git）
2. `docker/docker-compose.yml` 作成
3. `docker/smoke_test.sh` 作成（version / hello / build / http）
4. ローカルで `docker compose run --rm forge-verify` を実行して全 PASS 確認

**完了条件**: スモークテストが全 PASS する

---

## Phase I-2: install.sh + GitHub Releases バイナリ

**目標**: `cargo install --git` に依存しない 1 行インストールを実現する。

1. GitHub Actions でクロスコンパイルジョブ追加（Linux x86_64/arm64・macOS x86_64/arm64）
2. `install.sh` 作成（OS/ARCH 判定・バイナリダウンロード・配置）
3. `docker/Dockerfile` を `install.sh` 方式に切り替えて再検証

**完了条件**: `install.sh` 経由でインストールしたバイナリでスモークテストが通る

---

## Phase M-1: forge-mcp Rust 実装

**目標**: `forge mcp`（stdio）と `forge mcp start/stop/...`（daemon）が動く。

1. `crates/forge-mcp/` クレート新規作成
2. stdio モード実装（JSON-RPC over stdin/stdout）
3. MCP ツール実装（parse_file / type_check / run_snippet / search_symbol / get_spec_section）
4. daemon モード実装（start/stop/restart/status）
5. `forge-cli` に `mcp` サブコマンド追加

**完了条件**: `forge mcp`・`forge mcp start/stop/status` が Windows / Linux で動作する

---

## Phase M-2: ログ・状態管理

**目標**: エラー発生時にログで追跡できる。

1. JSON Lines ロギング実装（`~/.forge/mcp/forge-mcp.log`）
2. ローリングログ実装（10 MB・3 世代）
3. `McpSessionState`（揮発・メモリ内）実装
4. `forge mcp status` に統計表示追加
5. `forge mcp logs / logs -f / logs --errors / logs --clear` 実装

**完了条件**: エラー発生後に `forge mcp logs --errors` でエラーが確認できる

---

## Phase I-3: MCP 動作確認（Docker）

**目標**: Linux Docker 環境で MCP が正常に動作する。

1. `docker/smoke_test.sh` に MCP テスト追加
   - `forge mcp start` → `forge mcp status` → `forge mcp stop`
2. Docker コンテナ内でスモークテスト全 PASS 確認

---

## 依存関係

```
I-1 ─────────────────→ I-2
                         ↓（バイナリ方式に切り替え）
M-1 → M-2 → I-3（Docker で MCP 確認）
```

I-1 と M-1 は並列着手可能。I-3 は M-1・M-2 完了後。
