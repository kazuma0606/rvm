---
name: docker-smoke
description: docker compose run --rm forge-verify でスモークテストを実行し、結果を報告する。
---

Docker スモークテストを実行してください。

## 手順

### 1. イメージをビルドする

```bash
cd docker && docker compose build --no-cache
```

### 2. スモークテストを実行する

```bash
cd docker && docker compose run --rm forge-verify
```

### 3. 結果を報告する

- PASS / FAIL / SKIP の数を表示する
- FAIL がある場合は原因を分析して修正案を提示する

## FAIL 時の対応フロー

| FAIL 内容 | 対応 |
|---|---|
| `version` FAIL | `forge-cli/src/main.rs` の `--version` 実装を確認 |
| `hello world` FAIL | `forge run` のインタープリタを確認 |
| `build` FAIL | `cargo` が利用可能か、`forge build` の実装を確認 |
| `http get` FAIL | `forge/http` パッケージの実装、ネットワーク到達性を確認 |
| `mcp start/stop` FAIL | `crates/forge-mcp/src/daemon.rs` を確認 |

## 注意

- Docker が起動していない場合は Docker Desktop の起動を求める
- `docker compose build --no-cache` はキャッシュを使わずに再ビルドする
- GitHub Releases の最新バイナリが取得されることを確認する（`install.sh` 方式）
