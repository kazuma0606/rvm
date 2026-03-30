---
name: forge-spec-check
description: forge/spec_v0.0.1.mdの全項目を実装コードと照合し、未実装・仕様乖離を✅/⚠️/❌で報告する。
---

`forge/spec_v0.0.1.md` の全項目を実装コードと照合してください。

## チェック手順

### 1. Nil の廃止確認（最重要）
```
grep -r "Nil" src/ crates/
```
`Value::Nil` が残っていれば ❌ として報告する。

### 2. Lexer の確認（forge-compiler/src/lexer/）
spec_v0.0.1.md の「1. 字句規則」と照合：
- 全リテラル型（Int / Float / Str / Bool）
- 全キーワード（let / state / const / fn / if / else / for / in / while / match / return / true / false / none / some / ok / err）
- 全演算子・記号

### 3. AST の確認（forge-compiler/src/ast/）
spec_v0.0.1.md の「4. 式」「5. 関数」「6. match式」と照合：
- 全 Stmt バリアント
- 全 Expr バリアント（特に Closure が `=>` 前提か）
- Pattern バリアント（some / none / ok / err / _ / 範囲）

### 4. Parser の確認（forge-compiler/src/parser/）
- 全構文がパースできるか
- `x => expr` クロージャ記法が対応しているか

### 5. Value / Interpreter の確認（forge-vm/）
- Value::Unit が存在するか
- Value::Option / Value::Result が存在するか
- `?` 演算子の伝播が実装されているか

### 6. 組み込み関数の確認
spec の「9. 組み込み関数」と照合：
print / println / string / number / float / len / type_of

## 出力形式

```
## 仕様適合レポート（実行日時）

### Lexer
✅ Int リテラル
✅ Float リテラル
❌ 文字列補間（未実装）
...

### AST / Parser
...

### Interpreter / Value
...

### 組み込み関数
...

### サマリー
✅ XX 項目 / ⚠️ XX 項目 / ❌ XX 項目
仕様適合率: XX%

### 要対応（❌ 項目一覧）
1. ...
```
