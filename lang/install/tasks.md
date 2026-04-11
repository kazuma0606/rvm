# forge インストール・MCP サーバ タスク一覧

> 仕様: `lang/install/spec.md`
> 計画: `lang/install/plan.md`

---

## Phase I-1: Docker 検証環境

### I-1-A: Dockerfile 作成
- [x] `docker/Dockerfile` を作成
  - ベース: `ubuntu:22.04`
  - `apt-get`: curl / ca-certificates / build-essential
  - rustup 非対話インストール（stable・`--no-modify-path`）
  - `ENV PATH="/root/.cargo/bin:${PATH}"` を設定
  - `cargo install --git https://github.com/kazuma0606/rvm.git --bin forge-new --locked`
  - `forge-new` → `forge` シンボリックリンク作成（`/usr/local/bin/forge`）

### I-1-B: docker-compose.yml 作成
- [x] `docker/docker-compose.yml` を作成
  - service: `forge-verify`
  - `smoke_test.sh` を read-only volume でマウント
  - `command: bash /workspace/smoke_test.sh`

### I-1-C: smoke_test.sh 作成
- [ ] `docker/smoke_test.sh` を作成
  - `forge --version` — exit 0・"forge" 文字列を含む
  - `forge run /tmp/hello.fg` — "Hello, ForgeScript!" を出力
  - `forge build /tmp/build_test.fg -o /tmp/out && /tmp/out` — "42" を出力
  - `forge run /tmp/http_test.fg` — "200" を出力（インターネット到達可能時のみ・`SKIP` 対応）
  - 最終的に `FAIL > 0` なら exit 1

### I-1-D: 動作確認
- [ ] `docker compose run --rm forge-verify` が正常完了する
- [ ] スモークテスト全項目 PASS する

---

## Phase I-2: install.sh + GitHub Releases バイナリ

### I-2-A: GitHub Actions リリースジョブ
- [ ] `.github/workflows/release.yml` を作成
  - trigger: `push tags v*`
  - matrix: `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu` / `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - `cargo build --release --target` でクロスコンパイル
  - バイナリを GitHub Release にアップロード

### I-2-B: install.sh 作成
- [ ] `install.sh` を作成（リポジトリルート）
  - OS / ARCH 判定（uname）
  - GitHub Releases からバイナリ取得
  - `/usr/local/bin/forge` に配置・実行権付与
  - `forge --version` で確認

### I-2-C: Dockerfile 切り替え
- [ ] `docker/Dockerfile` の `cargo install --git` を `install.sh` 方式に変更
- [ ] `docker compose run --rm forge-verify` でスモークテスト再 PASS 確認

---

## Phase M-1: forge-mcp Rust 実装

### M-1-A: クレート準備
- [ ] `crates/forge-mcp/Cargo.toml` を作成
  - deps: `serde` / `serde_json` / `forge-parser` / `forge-vm`
- [ ] `crates/forge-mcp/src/lib.rs` を作成（`pub fn run_stdio()` スタブ）
- [ ] `Cargo.toml`（workspace）に `forge-mcp` を追加
- [ ] `forge-cli/Cargo.toml` に `forge-mcp` 依存を追加

### M-1-B: stdio モード実装
- [ ] stdin から JSON-RPC リクエストを読む処理を実装
- [ ] stdout に JSON-RPC レスポンスを書く処理を実装
- [ ] MCP initialize / tools/list ハンドラを実装

### M-1-C: MCP ツール実装
- [ ] `parse_file(path)` — ForgeScript をパースしてエラー一覧を返す
- [ ] `type_check(path)` — 型エラー・未使用変数を返す
- [ ] `run_snippet(code)` — インタープリタで実行し結果を返す
- [ ] `search_symbol(name, kind?)` — 定義箇所を返す
- [ ] `get_spec_section(section)` — 仕様書セクションを返す

### M-1-D: daemon モード実装
- [ ] `forge mcp start` — バックグラウンドプロセスを起動・PID ファイル書き込み（`~/.forge/mcp/forge-mcp.pid`）
- [ ] `forge mcp stop` — PID ファイル読み込み・プロセス終了（Unix: SIGTERM / Windows: TerminateProcess）
- [ ] `forge mcp restart` — stop → start
- [ ] `forge mcp status` — PID 生死確認・統計表示
- [ ] `forge mcp connect` — 起動中 daemon に MCP クライアントとして接続

### M-1-E: forge-cli 統合
- [ ] `forge-cli/src/main.rs` に `mcp` サブコマンドを追加
  - `forge mcp` → `forge_mcp::run_stdio()`
  - `forge mcp start/stop/restart/status/connect` → 各 daemon 関数

### M-1-F: クロスプラットフォーム
- [ ] `#[cfg(unix)]` / `#[cfg(windows)]` でプロセス起動・終了・存在確認を分岐実装
- [ ] Windows で `forge mcp start` が DETACHED_PROCESS フラグで起動する

---

## Phase M-2: ログ・状態管理

### M-2-A: JSON Lines ロギング
- [ ] `crates/forge-mcp/src/log.rs` を作成
  - `~/.forge/mcp/forge-mcp.log` に JSON Lines 形式で書き込む
  - フィールド: `ts` / `level` / `tool` / `req_id` / `elapsed_ms` / `msg` / `detail`

### M-2-B: ローリングログ
- [ ] 書き込み前にファイルサイズを確認し 10 MB 超でローテート
- [ ] `.log` → `.log.1` → `.log.2` → `.log.3`（3 世代で古いものを削除）

### M-2-C: セッション状態
- [ ] `crates/forge-mcp/src/state.rs` を作成（`McpSessionState` 構造体）
  - `started_at` / `request_count` / `error_count` / `last_error` / `tool_counts`
- [ ] リクエスト処理のたびにカウントを更新

### M-2-D: logs サブコマンド
- [ ] `forge mcp logs` — ログファイルの末尾 50 行を表示
- [ ] `forge mcp logs -f` — tail -f 相当（新規書き込みを追跡表示）
- [ ] `forge mcp logs --errors` — level=ERROR のみフィルタして表示
- [ ] `forge mcp logs --clear` — ログファイルを削除

---

## Phase I-3: MCP 動作確認（Docker）

### I-3-A: smoke_test.sh に MCP テスト追加
- [ ] `forge mcp start` が成功する（exit 0）
- [ ] `forge mcp status` が "running" を出力する
- [ ] `forge mcp stop` が成功する（exit 0）
- [ ] `docker compose run --rm forge-verify` でスモークテスト全 PASS 確認

---

## 進捗サマリ

| Phase | タスク数 | 完了 |
|---|---|---|
| I-1 | 6 | 2 |
| I-2 | 4 | 0 |
| M-1 | 15 | 0 |
| M-2 | 7 | 0 |
| I-3 | 4 | 0 |
| **合計** | **36** | **2** |
