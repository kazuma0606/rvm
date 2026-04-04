# ForgeScript テストシステム仕様

> バージョン対象: v0.2.0
> 関連ファイル: `forge/future_task_20260330.md`, `forge/modules/spec.md`

---

## 1. 設計方針

- **A: インラインブロック** — `test "..." { }` を本番コードと同じファイルに書く（Zig スタイル）
- **B: コンパニオンファイル** — `*.test.forge` を別ファイルに書く（Go スタイル）
- まず A を実装し、後から B に拡張する
- `forge run` → test ブロックをスキップ
- `forge test` → test ブロックのみ収集して実行
- `forge build` → `#[cfg(test)]` ブロックに変換（将来対応）

---

## 2. test ブロック構文

### 2-A: インラインブロック

```forge
fn add(a: number, b: number) -> number {
    a + b
}

test "add: 基本" {
    assert_eq(add(1, 2), 3)
    assert_eq(add(0, 0), 0)
    assert_eq(add(-1, 1), 0)
}

test "add: 大きな数" {
    assert(add(100, 200) == 300)
}
```

### 2-B: when test ブロックとの関係

`test "..." { }` は内部的に `when test` の下に置かれたものとして扱う。

```forge
// これは
test "add works" { assert_eq(add(1, 2), 3) }

// 内部的にはこれと同等
when test {
    test "add works" { assert_eq(add(1, 2), 3) }
}
```

### 2-C: コンパニオンファイル（Phase FT-2）

```
src/
  math/
    basic.forge       ← 本番コード
    basic.test.forge  ← テストコード
```

```forge
// basic.test.forge
use ./basic.{add, multiply}

test "add" {
    assert_eq(add(1, 2), 3)
}

test "multiply" {
    assert_eq(multiply(3, 4), 12)
}
```

---

## 3. アサーション関数

| 関数 | 動作 | 失敗時メッセージ |
|---|---|---|
| `assert(expr)` | `expr` が `true` でなければ失敗 | `assertion failed: <expr>` |
| `assert_eq(a, b)` | `a == b` でなければ失敗 | `assertion failed: expected <b>, got <a>` |
| `assert_ne(a, b)` | `a != b` でなければ失敗 | `assertion failed: expected not <b>, got <a>` |
| `assert_err(result)` | `result` が `err(...)` でなければ失敗 | `assertion failed: expected Err, got Ok` |
| `assert_ok(result)` | `result` が `ok(...)` でなければ失敗 | `assertion failed: expected Ok, got Err` |

アサーション失敗時はテストを中断し、次のテストへ進む（パニックではない）。

---

## 4. forge test コマンド

### 4-A: 単一ファイル

```bash
forge test src/math/basic.forge
```

- ファイル内の全 `test "..." { }` ブロックを収集して実行
- `when test` ブロック内のコードも実行

### 4-B: ディレクトリ走査（Phase FT-2）

```bash
forge test src/
forge test          # カレントディレクトリを走査
```

- `src/` 配下の全 `.forge` ファイルのインラインテストを実行
- `*.test.forge` ファイルも自動収集

### 4-C: フィルタ

```bash
forge test src/math.forge --filter "add"
```

- テスト名に "add" を含むテストのみ実行

---

## 5. 出力フォーマット

```
forge test src/math/basic.forge

running 3 tests
  ✅ add: 基本
  ✅ add: 大きな数
  ❌ add: 負の数
       assertion failed: expected 0, got 1
         --> src/math/basic.forge:15

test result: FAILED. 2 passed; 1 failed
```

成功時:
```
test result: ok. 3 passed; 0 failed
```

---

## 6. テストの実行モデル

- 各 `test` ブロックは独立したスコープで実行（前のテストの状態を引き継がない）
- ブロック外のトップレベルコード（`fn` / `const` / `struct` 等）は共有
- `state` 変数はテストごとにリセット
- テストは宣言順に実行

---

## 7. forge build との関係

```forge
test "add" { assert_eq(add(1, 2), 3) }
```

トランスパイル後:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(1, 2), 3);
    }
}
```

→ トランスパイラ対応は B-8 フェーズで実施。

---

## 8. 制約・未サポート（v0.2.0）

- テストのタイムアウト指定は未サポート
- 並列実行は未サポート（順次実行のみ）
- モック・スタブは未サポート（標準ライブラリ不在）
- `forge test --watch` は未サポート（将来対応）
