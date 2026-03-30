# ForgeScript 実装計画

> 現行 MVP（crates/ 配下）からの路線変更計画
> 言語仕様: `forge/spec_v0.0.1.md`
> 設計方針: `dev/design-v2.md` / `dev/design-v3.md`

---

## 全体方針

### 現行コードとの関係

現行 `crates/` 配下のコードは**参照・流用可能**だが、
拡張子を `.forge` に変え、設計方針 v3 に基づいて再実装する。
`Nil` の存在など設計と矛盾する箇所は新実装では持ち込まない。

### 新ディレクトリ構成（5クレート）

```
（workspace root）
  Cargo.toml
  forge/                  ← 本ディレクトリ（仕様・計画・タスク）
  crates/                 ← 現行コード（参照用・段階的に廃止）
  src/                    ← 新実装（crates/ に移行するまでの暫定）
    forge-compiler/       フロントエンド（Lexer・Parser・AST）
    forge-vm/             RVM（バイトコード・コンパイラ・ランタイム）
    forge-stdlib/         標準ライブラリ（コレクション・イテレータ）
    forge-transpiler/     Rustコード生成（Phase 4以降）
    forge-cli/            forge コマンド群
```

---

## Phase 0：基盤整備

**目的**: 開発環境を整え、新実装の土台を作る

### 0-A: クレート再編

- 新ワークスペース設定（5クレート）
- `forge-compiler` クレート作成（空実装）
- `forge-vm` クレート作成（空実装）
- `forge-stdlib` クレート作成（空実装）
- `forge-cli` クレート作成（`forge run file.forge` の骨格）
- 現行 `crates/` の参照関係を整理

### 0-B: CI・テスト基盤

- `cargo test --workspace` が通る状態を維持
- `.forge` 拡張子のフィクスチャディレクトリを `fixtures/` に作成
- E2Eテスト用ランナーの骨格（`.forge` ファイルを実行して stdout を検証）

---

## Phase 1：字句解析・構文解析（Lexer / Parser / AST）

**目的**: `spec_v0.0.1.md` の全構文を解析できる状態

### 1-A: Lexer 拡張

現行 `fs-lexer` を `forge-compiler/src/lexer/` に移植しながら拡張。

追加するトークン:
```
// リテラル
Float(f64), Bool(bool)

// キーワード
State, Const, Fn, Return,
If, Else, For, In, While,
Match, True, False, None, Some, Ok, Err,

// 演算子・記号
Bang(!), And(&&), Or(||),
Arrow(=>), ThinArrow(->), Question(?),
Colon(:), DotDot(..), DotDotEq(..=),
EqEq(==), BangEq(!=), Lt(<), Gt(>), LtEq(<=), GtEq(>=),
Percent(%), Dot(.), LBracket([), RBracket(])
```

### 1-B: AST 定義

```rust
// 主要 AST ノード
enum Stmt {
    Let { name, type_ann, value },
    State { name, type_ann, value },
    Const { name, type_ann, value },
    Fn { name, params, return_type, body },
    Return(Option<Expr>),
    Expr(Expr),
}

enum Expr {
    Literal(Literal),
    Ident(String),
    BinOp { op, left, right },
    UnaryOp { op, operand },
    If { cond, then_block, else_block },
    While { cond, body },
    For { var, iter, body },
    Match { scrutinee, arms },
    Block(Vec<Stmt>, Option<Box<Expr>>),
    Call { callee, args },
    Index { object, index },
    Field { object, field },
    MethodCall { object, method, args },
    Closure { params, body },
    Interpolation { parts },   // 文字列補間
    Range { start, end, inclusive },
    List(Vec<Expr>),
    Question(Box<Expr>),       // ? 演算子
}
```

### 1-C: Parser 実装

再帰降下パーサー。優先順位:
```
最低: ||
      &&
      == !=
      < > <= >=
      + -
      * / %
      単項: ! -
      最高: . () [] ?
```

---

## Phase 2：RVM インタプリタ（forge run の実装）

**目的**: `spec_v0.0.1.md` のコードが `forge run file.forge` で実行できる

### 2-A: Value 型の再設計

```rust
enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Rc<RefCell<Vec<Value>>>),
    Option(Option<Box<Value>>),      // some / none
    Result(Result<Box<Value>, String>), // ok / err
    Closure { params, body, env },
    NativeFunction(NativeFn),
    Unit,                            // () – Nil の代替（表示しない）
}
```

`Nil` は廃止。`Unit` は内部的な「戻り値なし」の表現で、ユーザーには見せない。

### 2-B: 環境（スコープ）管理

```rust
struct Env {
    values: HashMap<String, Value>,
    parent: Option<Rc<RefCell<Env>>>,
}
```

ツリーウォーキングインタプリタでスコープチェーンを実装。

### 2-C: インタプリタ本体

AST を直接評価するツリーウォーカー。

```rust
fn eval_expr(expr: &Expr, env: &Env) -> Result<Value, RuntimeError>
fn eval_stmt(stmt: &Stmt, env: &Env) -> Result<Option<Value>, RuntimeError>
```

対応する機能:
- 算術・比較・論理演算
- `let` / `state` / `const` バインディング
- `if` / `while` / `for` 制御フロー
- 関数定義・呼び出し・クロージャ
- `match` パターンマッチ（リテラル・`some`/`none`/`ok`/`err`・ワイルドカード）
- `?` 演算子（Result 伝播）
- 文字列補間
- リストリテラル・範囲リテラル
- メソッド呼び出し（`.map()` `.filter()` 等）

### 2-D: 標準ライブラリ（ネイティブ関数）

```forge
print / println / string / number / float / len / type_of
```

### 2-E: forge-cli 実装

```bash
forge run file.forge     # ファイル実行
forge repl               # 対話型 REPL
forge help               # ヘルプ
```

---

## Phase 3：コレクション API（forge-stdlib）

**目的**: `list<T>` のイテレータメソッドをネイティブで実装する

### 3-A: 変換系

```forge
.map(f)  .filter(f)  .flat_map(f)  .filter_map(f)
.take(n)  .skip(n)  .take_while(f)  .skip_while(f)
.enumerate()  .zip(other)  .flatten()
```

### 3-B: 集計系

```forge
.sum()  .count()  .fold(seed, f)
.any(f)  .all(f)  .none(f)
.first()  .last()  .nth(n)
.min()  .max()  .min_by(f)  .max_by(f)
```

### 3-C: ソート・変形系

```forge
.order_by(f)  .order_by_descending(f)
.then_by(f)  .then_by_descending(f)
.reverse()  .distinct()
.take(n)  .skip(n)
```

### 3-D: 収集系

```forge
.collect()  .to_hashmap(key_f, val_f)
```

---

## Phase 4：型システム基盤

**目的**: 型注釈の解析・基本的な型チェックを実装する

### 4-A: 型チェッカー（forge-compiler/src/typechecker/）

- 型注釈のパース（`number` / `float` / `string` / `bool` / `T?` / `T!` / `list<T>`）
- 基本的な型推論（リテラルの型確定）
- 関数の型シグネチャ検査
- `T?` / `T!` の match 網羅性チェック

### 4-B: forge check コマンド

```bash
forge check file.forge   # 型チェックのみ実行（インタプリタは走らせない）
```

---

## Phase 5：struct / enum / trait（型定義）

**目的**: ユーザー定義型を実装する（spec は別途 spec_v0.1.0.md を作成）

- `struct` 定義・フィールドアクセス・メソッド（`impl` ブロック）
- `enum` 定義（データあり・なし）・パターンマッチ
- `trait` 定義・`impl Trait for Type`
- `derive` キーワード（`debug` / `clone` / `eq`）
- `typestate` キーワード（Phase 5 後半）

---

## Phase 6：Rustトランスパイラ（forge build）

**目的**: ForgeScript → Rust コード生成

- AST → Rust コード文字列の生成
- `forge build file.forge` → `rustc` 呼び出し → バイナリ生成
- `forge transpile file.forge` → Rust コードのみ出力
- クロージャキャプチャ推論（`Fn` / `FnMut` / `FnOnce`）
- `async/await` 自動昇格（`.await` 検出 → `#[tokio::main]`）

---

## バージョン対応表

| バージョン | Phase | 主な機能 |
|---|---|---|
| v0.0.1 | 0〜2 | let/state/const・if/for/while・fn・クロージャ・match・T?/T! |
| v0.0.2 | 3 | list\<T\>・イテレータメソッド・文字列補間完全対応 |
| v0.0.3 | 4 | 型チェッカー・forge check |
| v0.1.0 | 5 | struct/enum/trait/typestate |
| v0.2.0 | 6 | forge build（Rustトランスパイラ） |
