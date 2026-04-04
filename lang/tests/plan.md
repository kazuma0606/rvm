# ForgeScript テストシステム 実装計画

> 仕様: `forge/tests/spec.md`
> 前提: モジュールシステム（M-0〜M-7）・when キーワード（M-5）が完成済み

---

## フェーズ構成

```
Phase FT-1: インラインテスト（test "..." { } + forge test <file>）
Phase FT-2: コンパニオンファイル（*.test.forge + ディレクトリ走査）
```

---

## Phase FT-1: インラインテスト

### 目標
`test "..." { }` ブロックが定義でき、`forge test <file>` で収集・実行されること。
アサーション関数（`assert` / `assert_eq` / `assert_ne` / `assert_ok` / `assert_err`）が使えること。

### 実装ステップ

1. **Lexer 拡張**
   - `test` キーワードトークンを追加

2. **AST 拡張**
   ```rust
   Stmt::TestBlock {
       name: String,
       body: Vec<Stmt>,
   }
   ```

3. **パーサー拡張**
   - `test "テスト名" { ... }` のパース
   - ブロック内は通常の文（Stmt）のリスト

4. **インタープリタ拡張**
   - `forge run` では `TestBlock` をスキップ（`is_test_mode = false`）
   - `forge test` では `is_test_mode = true` でインタープリタを起動
   - `TestBlock` の収集・順次実行
   - 各テストは独立したスコープで実行
   - アサーション失敗は `TestFailure { test_name, message, line }` として捕捉

5. **アサーション組み込み関数**
   - `assert(expr)` — false なら TestFailure
   - `assert_eq(a, b)` — a != b なら TestFailure
   - `assert_ne(a, b)` — a == b なら TestFailure
   - `assert_ok(result)` — Err なら TestFailure
   - `assert_err(result)` — Ok なら TestFailure

6. **forge test CLI コマンド**
   - `forge-cli/src/main.rs` に `forge test <file>` サブコマンドを追加
   - `--filter <pattern>` オプション（テスト名の部分一致フィルタ）
   - 実行結果を標準出力に表示

7. **出力フォーマット**
   ```
   running N tests
     ✅ テスト名
     ❌ テスト名
          assertion failed: expected X, got Y
            --> file.forge:line

   test result: ok. N passed; 0 failed
   ```

8. **テスト**

---

## Phase FT-2: コンパニオンファイル（将来）

### 目標
`*.test.forge` ファイルにテストを分離でき、`forge test src/` でディレクトリを走査して自動収集されること。

### 実装ステップ

1. **ディレクトリ走査**
   - `forge test src/` → `src/` 以下の全 `.forge` のインラインテストを収集
   - `*.test.forge` も自動収集

2. **コンパニオンファイルの解決**
   - `basic.test.forge` は `basic.forge` と同じモジュールスコープで実行
   - `use` なしで `basic.forge` の pub シンボルにアクセス可能

3. **テスト**

---

## テスト方針

### ユニットテスト
- パーサー: `test "..." { }` のパース確認
- インタープリタ: アサーション成功・失敗の動作確認

### E2E テスト
- `forge test fixtures/test_*.forge` の出力を検証
- 成功ケース・失敗ケース・フィルタのそれぞれで確認
