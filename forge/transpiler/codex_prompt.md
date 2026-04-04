# Codex への実装依頼: B-3残タスク・B-7・B-8

## プロジェクト概要

ForgeScript トランスパイラ（`forge-transpiler` クレート）の実装を依頼します。
ForgeScript は Rust にトランスパイルされる独自言語です。

仕様・計画・タスクは以下を参照してください：
- 仕様: `forge/transpiler/spec.md`
- 計画: `forge/transpiler/plan.md`
- タスク: `forge/transpiler/tasks.md`

---

## 実装対象

以下の3フェーズを**この順番**で実装してください。

### Phase B-3 残タスク: FnMut / FnOnce クロージャ推論

`forge/transpiler/tasks.md` の B-3 に未完了（`[ ]`）のタスクが2件あります。

**実装箇所**: `forge-transpiler/src/codegen.rs` の `gen_closure()` 関数に TODO コメントあり。

- `state` 変数をクロージャ本体内で**変更**している → `FnMut`（`move |x| ...`）
- 変数をクロージャ本体内で**消費**している → `FnOnce`（`move |x| ...`）

判定方法:
1. クロージャのキャプチャ変数を収集する（既存実装あり）
2. キャプチャ変数のうち `state` として宣言されたものが代入されているか走査
3. 代入あり → `FnMut` + `move`、なし → 現状の `Fn` のまま
4. FnOnce は現時点で `spawn` 等が未実装のため、実際に発生するケースはない。
   テストコードで「消費パターン」を人工的に作って検証すること。

---

### Phase B-7: async / await

仕様: `forge/transpiler/spec.md` セクション 13

タスク: `forge/transpiler/tasks.md` の Phase B-7（B-7-A〜B-7-I）

**重要な実装ポイント**:

1. **async 伝播（B-7-A）**: 1パスでは不足。呼び出しグラフを構築し、固定点に達するまで繰り返す。
   - 関数 A が `async fn B` を `.await` で呼ぶ → A も `async fn` に昇格
   - これを全関数が確定するまでループ

2. **Cargo.toml 自動更新（B-7-C）**: `.await` が存在するプロジェクトに以下を追加：
   ```toml
   tokio = { version = "1", features = ["full"] }
   ```

3. **async 再帰（B-7-E）**: 直接再帰する `async fn` は Rust でコンパイルエラーになる。
   自動で `Box::pin(async move { ... })` に変換すること。

4. **クロージャ内 await の禁止（B-7-G）**: クロージャ本体内に `.await` を発見したら
   `TranspileError` を返す。Rust の async closure は nightly 機能のため未サポート。

5. **forge run フォールバック（B-7-H）**: インタープリタ（`forge-vm/src/interpreter.rs`）で
   `Expr::Await { expr }` を `expr` の評価結果をそのまま返す no-op として実装。

6. **test ブロック（B-7-F）**: `.await` を含む test ブロックは `#[tokio::test] async fn` に変換。

---

### Phase B-8: typestate 変換

仕様: `forge/transpiler/spec.md` セクション 14

タスク: `forge/transpiler/tasks.md` の Phase B-8（B-8-A〜B-8-F）

**制約条件（最重要・必ず実装すること）**:

| 制約 | 対応 |
|---|---|
| 状態は Unit 型のみ | 違反したらコンパイルエラー |
| ジェネリクス付き typestate 禁止 | 違反したらコンパイルエラー |
| `@derive` on typestate 禁止 | 違反したらコンパイルエラー |
| `any {}` ブロックは1つのみ | 違反したらコンパイルエラー |

制約チェック（B-8-A）を**必ず最初**に実装し、制約違反時は `TranspileError` を返すこと。

**生成パターン**（`forge/transpiler/spec.md` セクション14-2 参照）:
- `states: [A, B, C]` → `struct A; struct B; struct C;`
- `typestate Name { ... }` → `struct Name<S> { ..., _state: PhantomData<S> }`
- `use std::marker::PhantomData;` を自動挿入
- 遷移メソッド（戻り値が別状態）: `self` を消費
- 参照メソッド（戻り値がプリミティブ等）: `&self`
- `any { }` ブロック → 全状態に同一 impl を展開

---

## 実装ルール（厳守）

1. **`unwrap()` を使わない**。`?` 演算子か `unwrap_or_else(|e| panic!("... {}", e))` を使う
2. **`Value::Nil` を使わない**
3. **テスト名は `tasks.md` の定義と完全一致**させること
4. **フェーズ完了ごとに `cargo test --workspace` を実行**し、全テスト通過を確認
5. **フェーズ完了ごとに `tasks.md` の該当チェックボックスを `[x]` に更新**してコミット
6. コミットメッセージ形式: `feat(transpiler): implement Phase B-X ...`

---

## 成果物の確認方法

実装完了後、以下を確認してください：

```bash
cargo test --workspace
```

全テスト通過であること。失敗した場合は修正してから次のフェーズに進むこと。

---

## 参考: 既存の実装パターン

- `forge-transpiler/src/codegen.rs`: メインのコード生成器（`CodeGenerator` 構造体）
- `forge-transpiler/src/builtin.rs`: 組み込み関数の変換テーブル
- `forge-compiler/src/parser/mod.rs`: AST の定義（`Expr` / `Stmt` 等）
- `forge-vm/src/interpreter.rs`: インタープリタ（B-7-H の変更対象）
- `forge-cli/tests/e2e.rs`: E2E テスト（ラウンドトリップテストのパターン参照）

スナップショットテストは `insta` クレートを使わず `assert!(out.contains(...))` 形式で
既存テストに合わせること。

---

## 注意事項

- B-7 の実装時、`forge-compiler` の AST に `Expr::Await` が存在しない場合は
  パーサーへの追加も含めて実装すること
- B-8 の `typestate` AST ノードが未実装の場合も同様にパーサーから実装すること
- 既存テストを壊さないこと。`cargo test` が通らない状態でコミットしないこと
