---
name: release
description: 新しいリリースタグを切り、GitHub Actions のビルドをモニタリングして全ジョブ成功を確認する。引数でバージョンを指定（例: /release v0.2.0）。
---

`$ARGUMENTS` で指定したバージョンのリリースを作成してください。バージョン未指定の場合は現在の `Cargo.toml` バージョンを確認して提案する。

## 手順

### 1. リリース前チェック

```bash
# 現在のバージョンを確認
grep '^version' crates/forge-cli/Cargo.toml

# 未コミットの変更がないか確認
git status --short

# master が最新か確認
git log --oneline -5
```

### 2. バージョンを決める

- `$ARGUMENTS` が指定されている場合はそのバージョンを使う
- 未指定の場合は `vX.Y.Z` の形式で提案し、ユーザーの確認を求める

### 3. Cargo.toml のバージョンを更新する（必要な場合）

バージョン番号が `$ARGUMENTS` と異なる場合は、以下のファイルの `version = "..."` を更新する:
- `Cargo.toml`（workspace）
- `crates/forge-cli/Cargo.toml`

その後 `cargo generate-lockfile` を実行して Cargo.lock を更新する。

### 4. コミットしてタグを切る

```bash
git add Cargo.toml Cargo.lock crates/*/Cargo.toml
git commit -m "chore: bump version to $ARGUMENTS"
git push origin master
git tag $ARGUMENTS
git push origin $ARGUMENTS
```

### 5. CI をモニタリングする

```bash
gh run list --limit 3
gh run watch <run-id>
```

4ジョブ（x86_64-linux / aarch64-linux / x86_64-darwin / aarch64-darwin）が全て成功することを確認する。

### 6. リリース完了を報告する

- GitHub Release URL を表示する
- ビルド成功/失敗のサマリーを出力する
- FAIL がある場合は原因を分析して対処法を提示する

## 注意

- タグを切る前にユーザーに確認を求める
- `git push --force` は絶対に使わない
- ビルド失敗時は `gh run view <run-id> --log-failed` でエラーを確認する
