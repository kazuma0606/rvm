# forge インストール・MCP サーバ仕様

> ROADMAP [11] Linux インストール対応 / [15] forge-mcp（MCP サーバ）

---

## 概要

1. **Docker 検証環境**（Phase I-1）: Linux クリーン環境で Rust + forge のインストールをローカルで検証する
2. **install.sh + GitHub Releases**（Phase I-2）: ビルド不要の 1 行インストール
3. **forge-mcp 実装**（Phase M-1〜M-2）: `forge mcp` サブコマンド群。`forge` バイナリに同梱。Windows / Linux 両対応

---

## Phase I-1: Docker 検証環境

### 目的

- Windows / Linux どちらのホストでも同じ手順で検証できる
- VirtualBox 不要（Docker Desktop のみ）
- レイヤーキャッシュで 2 回目以降は高速
- 将来 CI（GitHub Actions）にそのまま流用できる

### Dockerfile

```dockerfile
# docker/Dockerfile
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/root/.cargo/bin:${PATH}"

RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential && \
    rm -rf /var/lib/apt/lists/*

# Rust インストール（non-interactive）
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --no-modify-path

# forge インストール（cargo install --git）
# Phase I-2 で install.sh + リリースバイナリに切り替え
RUN cargo install --git https://github.com/<owner>/rvm --bin forge --locked

WORKDIR /workspace
CMD ["bash"]
```

### docker-compose.yml

```yaml
# docker/docker-compose.yml
services:
  forge-verify:
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - ./smoke_test.sh:/workspace/smoke_test.sh:ro
    command: bash /workspace/smoke_test.sh
```

### smoke_test.sh

```bash
#!/usr/bin/env bash
set -euo pipefail

PASS=0; FAIL=0

check() {
    local name="$1"; local cmd="$2"; local expect="$3"
    actual=$(eval "$cmd" 2>&1) || true
    if echo "$actual" | grep -q "$expect"; then
        echo "  [PASS] $name"; PASS=$((PASS+1))
    else
        echo "  [FAIL] $name (expected: '$expect', got: '$actual')"; FAIL=$((FAIL+1))
    fi
}

echo "=== forge smoke test ==="

# 1. バージョン確認
check "version" "forge --version" "forge"

# 2. Hello World
cat > /tmp/hello.fg <<'EOF'
fn main() { println("Hello, ForgeScript!") }
EOF
check "hello world" "forge run /tmp/hello.fg" "Hello, ForgeScript!"

# 3. forge build
cat > /tmp/build_test.fg <<'EOF'
fn main() { let x = 42; println(x) }
EOF
forge build /tmp/build_test.fg -o /tmp/forge_out 2>/dev/null
check "build" "/tmp/forge_out" "42"

# 4. HTTP（httpbin.org が到達可能な場合のみ）
if curl -sf --max-time 3 https://httpbin.org/get > /dev/null 2>&1; then
    cat > /tmp/http_test.fg <<'EOF'
use forge/http.{ get }
fn main() {
    let res = get("https://httpbin.org/get").send()
    println(res.status)
    println(res.ok)
}
EOF
    check "http get" "forge run /tmp/http_test.fg" "200"
else
    echo "  [SKIP] http get (no internet)"
fi

echo ""
echo "=== result: ${PASS} passed, ${FAIL} failed ==="
[ "$FAIL" -eq 0 ]
```

### 実行方法

```bash
# ビルド＆スモークテスト
cd docker
docker compose run --rm forge-verify

# 対話シェル
docker compose run --rm forge-verify bash
```

---

## Phase I-2: install.sh + GitHub Releases バイナリ

### install.sh

```bash
#!/usr/bin/env bash
# curl -sSf https://install.forgescript.dev | sh

set -euo pipefail

VERSION="${FORGE_VERSION:-latest}"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

BASE="https://github.com/<owner>/rvm/releases/download/${VERSION}"
BINARY="forge-${OS}-${ARCH}"

curl -sSfL "${BASE}/${BINARY}" -o /tmp/forge
chmod +x /tmp/forge
mv /tmp/forge /usr/local/bin/forge
forge --version
echo "ForgeScript installed."
```

### 対応プラットフォーム

| OS | アーキテクチャ | バイナリ名 |
|---|---|---|
| Linux | x86_64 | `forge-linux-x86_64` |
| Linux | aarch64 | `forge-linux-aarch64` |
| macOS | x86_64 | `forge-darwin-x86_64` |
| macOS | arm64 | `forge-darwin-aarch64` |

> Windows は MSI / winget を別途検討。

### GitHub Actions（クロスコンパイル）

```yaml
# .github/workflows/release.yml（概略）
on:
  push:
    tags: ["v*"]
jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest; target: x86_64-unknown-linux-gnu
          - os: ubuntu-latest; target: aarch64-unknown-linux-gnu
          - os: macos-latest;  target: x86_64-apple-darwin
          - os: macos-latest;  target: aarch64-apple-darwin
    steps:
      - cargo build --release --target ${{ matrix.target }}
      - upload-artifact → GitHub Release
```

---

## Phase M-1: forge-mcp 実装

### 概要

`forge mcp` は `forge` バイナリのサブコマンドとして実装し、インストール時に一緒に使えるようになる。**別パッケージ・別インストール不要**。

### 2 つの動作モード

| モード | コマンド | 用途 |
|---|---|---|
| stdio | `forge mcp` | Claude Code が直接 spawn。JSON-RPC over stdin/stdout |
| daemon | `forge mcp start` | バックグラウンド常駐。HTTP/SSE で接続 |

### Claude Code 設定例

```json
// stdio モード（シンプル）
{ "command": "forge", "args": ["mcp"] }

// daemon モード（ログ・状態管理付き）
{ "command": "forge", "args": ["mcp", "connect"] }
```

### MCP ツール一覧

| ツール | 引数 | 概要 |
|---|---|---|
| `parse_file` | `path: string` | ForgeScript ファイルを AST にパース。構文エラーを返す |
| `type_check` | `path: string` | 型エラー・未使用変数を報告 |
| `run_snippet` | `code: string` | コードスニペットをインタープリタで実行し結果を返す |
| `search_symbol` | `name: string, kind?: string` | 関数・型の定義を探す |
| `get_spec_section` | `section: string` | 仕様書の該当セクションを返す |

### 管理コマンド

```
forge mcp             # stdio モードで起動（Claude Code 用）
forge mcp start       # デーモン起動
forge mcp stop        # デーモン停止
forge mcp restart     # stop → start
forge mcp status      # 生死・統計表示
forge mcp connect     # 起動中デーモンへ接続（MCP クライアント向け）
forge mcp logs        # ログ表示
forge mcp logs -f     # tail -f 相当
forge mcp logs --errors   # エラーのみ
forge mcp logs --clear    # ログ削除
```

### Rust モジュール構成

```
crates/forge-mcp/
  src/
    lib.rs            # pub fn run_stdio() / pub fn run_daemon()
    server.rs         # MCP JSON-RPC ハンドラ
    tools/
      mod.rs
      parse_file.rs
      type_check.rs
      run_snippet.rs
      search_symbol.rs
      get_spec_section.rs
    daemon.rs         # start/stop/restart/status
    log.rs            # JSON Lines ロギング・ローリング
    state.rs          # McpSessionState（揮発・メモリ内）
```

`forge-cli` の `main.rs` から呼び出す：

```rust
match subcommand {
    "mcp" => forge_mcp::run_stdio(),
    "mcp" if args.contains("start") => forge_mcp::daemon::start(),
    "mcp" if args.contains("stop")  => forge_mcp::daemon::stop(),
    // ...
}
```

---

## Phase M-2: ログ・状態管理

### ファイル構成

```
~/.forge/mcp/
  forge-mcp.pid       # 実行中 PID（daemon モード）
  forge-mcp.log       # ローリングログ（JSON Lines）
  forge-mcp.log.1     # ローテート済み（最大 3 世代）
```

### ログエントリ形式（JSON Lines）

```json
{"ts":"2026-04-11T10:00:00Z","level":"INFO","tool":"run_snippet","req_id":"abc123","elapsed_ms":12}
{"ts":"2026-04-11T10:00:01Z","level":"ERROR","tool":"run_snippet","req_id":"def456","msg":"parse error","detail":"line 3: unexpected '}'"}
```

### ローリングログ設定

| 項目 | 値 |
|---|---|
| 上限サイズ | 10 MB（設定可能） |
| 世代数 | 3 |
| ローテートタイミング | 書き込み時にサイズ超過を検知 |

### セッション状態（メモリ内・stop で消える）

```rust
struct McpSessionState {
    started_at: Instant,
    request_count: u64,
    error_count: u64,
    last_error: Option<String>,
    tool_counts: HashMap<String, u64>,
}
```

`forge mcp status` 表示例：

```
forge-mcp: running (pid 12345)
uptime:    2h 14m
requests:  342  (errors: 3)
last error: [10:45:12] run_snippet — parse error at line 3
log:       ~/.forge/mcp/forge-mcp.log (1.2 MB)
```

### クロスプラットフォーム対応

```rust
#[cfg(unix)]
fn spawn_daemon() { /* fork/detach */ }

#[cfg(windows)]
fn spawn_daemon() {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    Command::new("forge").args(["mcp", "--daemon-inner"])
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(unix)]
fn is_running(pid: u32) -> bool { unsafe { libc::kill(pid as i32, 0) == 0 } }

#[cfg(windows)]
fn is_running(pid: u32) -> bool { /* OpenProcess で確認 */ }
```

---

## ファイル構成

```
lang/install/
  spec.md             ← 本ファイル（install + forge-mcp 統合）
  plan.md
  tasks.md

docker/               ← リポジトリルートに配置
  Dockerfile
  docker-compose.yml
  smoke_test.sh

install.sh            ← リポジトリルートに配置（Phase I-2）

crates/forge-mcp/     ← forge-mcp Rust クレート（Phase M-1）
  Cargo.toml
  src/...
```
