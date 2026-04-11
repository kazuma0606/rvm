---
name: forge-spec-check
description: 仕様ファイルを実装コードと照合し、未実装・仕様乖離を✅/⚠️/❌で報告する。引数でエリア指定可（例: /forge-spec-check lang/syntax）。
---

`$ARGUMENTS` で指定した仕様、または未指定なら `lang/v0.1.0/spec_v0.0.1.md` をベースに実装コードと照合してください。

## 対象仕様ファイルの決定

- 引数なし → `lang/v0.1.0/spec_v0.0.1.md`（コア言語仕様）
- `lang/syntax` → `lang/syntax/spec.md`
- `lang/tests` → `lang/tests/spec.md`
- `lang/transpiler` → `lang/transpiler/spec.md`
- `packages/anvil` → `packages/anvil/spec.md`
- `lang/packages/http` → `lang/packages/http/spec.md`
- `lang/install` → `lang/install/spec.md`

## チェック手順

### 1. 共通チェック（必ず実施）

```bash
grep -r "Value::Nil" crates/
```
`Value::Nil` が残っていれば ❌ として報告する。

### 2. 仕様ファイルを全文読む

各セクションを順番に確認し、対応する実装コードを読む。

### 3. 実装コードと照合する

対象クレート（`crates/forge-compiler/`, `crates/forge-vm/`, `crates/forge-stdlib/`, `crates/forge-mcp/` 等）のソースを読み、仕様の各項目が実装されているか確認する。

### 4. 出力形式

```
## 仕様適合レポート（対象: <仕様ファイル名>）

### <セクション名>
✅ 項目名 — 実装済み
⚠️ 項目名 — 部分実装（詳細）
❌ 項目名 — 未実装

### サマリー
✅ XX 項目 / ⚠️ XX 項目 / ❌ XX 項目
仕様適合率: XX%

### 要対応（❌ / ⚠️ 項目一覧）
1. ❌ ...
2. ⚠️ ...
```
