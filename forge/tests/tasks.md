# ForgeScript テストシステム タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `test "..." { }` ブロックが書け、`forge test <file>` で実行・結果表示されること

---

## Phase FT-1: インラインテスト

### FT-1-A: Lexer 拡張

- [x] `test` キーワードトークンを追加

### FT-1-B: AST 拡張

- [x] `Stmt::TestBlock { name: String, body: Vec<Stmt> }` を追加

### FT-1-C: パーサー拡張

- [x] `test "テスト名" { ... }` のパース
- [x] `forge run` / `forge build` でも構文エラーにならないこと（スキップ対象）

### FT-1-D: インタープリタ拡張

- [x] `forge run` では `TestBlock` をスキップ（`is_test_mode = false`）
- [x] `forge test` では `is_test_mode = true` で実行
- [x] `TestBlock` の収集・順次実行
- [x] 各テストを独立したスコープで実行（前テストの `state` 変化を引き継がない）
- [x] アサーション失敗を `TestFailure { test_name, message }` として捕捉し次テストへ進む

### FT-1-E: アサーション組み込み関数

- [x] `assert(expr)` — `false` なら失敗: `"assertion failed"`
- [x] `assert_eq(a, b)` — `a != b` なら失敗: `"assertion failed: expected <b>, got <a>"`
- [x] `assert_ne(a, b)` — `a == b` なら失敗: `"assertion failed: expected not <b>, got <a>"`
- [x] `assert_ok(result)` — `Err` なら失敗: `"assertion failed: expected Ok, got Err(<msg>)"`
- [x] `assert_err(result)` — `Ok` なら失敗: `"assertion failed: expected Err, got Ok"`

### FT-1-F: forge test CLI コマンド

- [x] `forge test <file>` サブコマンドを `forge-cli/src/main.rs` に追加
- [x] `--filter <pattern>` オプション（テスト名の部分一致）
- [x] 実行前に `running N tests` を表示
- [x] 成功テスト: `  ✅ <name>` を表示
- [x] 失敗テスト: `  ❌ <name>` + インデントされたメッセージを表示
- [x] 最終行に `test result: ok. N passed; 0 failed` または `FAILED. N passed; M failed` を表示
- [x] 1件以上失敗した場合はプロセスを exit code 1 で終了

### FT-1-G: テスト

- [x] テスト: `test_parse_test_block` — `test "名前" { }` のパース確認
- [x] テスト: `test_assert_eq_pass` — `assert_eq(1+1, 2)` が通過
- [x] テスト: `test_assert_eq_fail` — `assert_eq(1, 2)` が失敗として捕捉される
- [x] テスト: `test_assert_pass` — `assert(true)` が通過
- [x] テスト: `test_assert_fail` — `assert(false)` が失敗として捕捉される
- [x] テスト: `test_assert_ok` — `assert_ok(ok(1))` が通過
- [x] テスト: `test_assert_err` — `assert_err(err("msg"))` が通過
- [x] テスト: `test_test_scope_isolation` — テスト間で state が引き継がれない
- [x] テスト: `test_run_skips_test_blocks` — `forge run` では test ブロックがスキップされる
- [x] E2E テスト: `e2e_forge_test_pass` — 全テスト成功のファイルで exit 0
- [x] E2E テスト: `e2e_forge_test_fail` — 失敗テストありのファイルで exit 1
- [x] E2E テスト: `e2e_forge_test_filter` — `--filter` で対象テストを絞り込み

---

## Phase FT-2: コンパニオンファイル（将来）

### FT-2-A: ディレクトリ走査

- [ ] `forge test src/` で `.forge` ファイルの全インラインテストを収集
- [ ] `*.test.forge` ファイルも自動収集

### FT-2-B: コンパニオンファイル解決

- [ ] `basic.test.forge` が `basic.forge` の pub シンボルに `use` なしでアクセス可能
- [ ] テスト実行時に本番ファイルと同じスコープで評価

### FT-2-C: テスト

- [ ] E2E テスト: `forge test` でディレクトリ走査
- [ ] E2E テスト: `*.test.forge` のコンパニオンテスト実行
