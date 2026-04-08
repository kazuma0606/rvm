# ForgeScript 言語拡張 実装計画

> 仕様: `lang/extends/spec.md`
> 前提: v0.1.0 コア言語・モジュールシステム・トランスパイラ B-0〜B-8 が完成済み

---

## フェーズ構成

```
Phase E-1: |> パイプ演算子
Phase E-2: ?. オプショナルチェーン / ?? null 合体
Phase E-3: 演算子オーバーロード
Phase E-4: 非同期クロージャ完成 / spawn
Phase E-5: const fn / コンパイル時定数
Phase E-6: ジェネレータ / yield       ← E-4 完成後
```

E-1〜E-5 は互いに独立しており並走可能。E-6 のみ E-4 に依存する。

---

## Phase E-1: `|>` パイプ演算子

### 目標

`list |> filter(...) |> map(...) |> fold(...)` がメソッドチェーンと等価に動くこと。

### 実装ステップ

1. **Lexer 拡張**
   - `|>` を `TokenKind::PipeArrow` として追加
   - `|` 単体は既存の `TokenKind::Pipe`（Pick/Omit 型操作）を維持

2. **AST 変更なし**
   - パース時点で `Expr::MethodCall` に変換するためAST 拡張不要

3. **パーサー拡張**
   - `parse_expr` の中置演算子処理に `PipeArrow` を追加
   - `lhs |> method(args)` → `Expr::MethodCall { object: lhs, method, args }` に変換
   - `lhs |> method`（引数なし）→ `Expr::MethodCall { object: lhs, method, args: [] }` に変換
   - 優先度: 比較演算子より低く、代入より高い

4. **インタープリタ変更なし**（AST 変換で吸収）

5. **トランスパイラ変更なし**（AST 変換で吸収）

6. **テスト**
   - E2E: パイプ演算子を使ったコレクション操作が正しく動作する
   - E2E: メソッドチェーンと `|>` の結果が一致する
   - Snapshot: `|>` 使用コードのトランスパイル結果がメソッドチェーンと同一

---

## Phase E-2: `?.` / `??`

### 目標

`T?` 型の値に対して `?.` でフィールドアクセス・メソッド呼び出しができ、
`??` でデフォルト値を提供できること。

### 実装ステップ

1. **Lexer 拡張**
   - `?.` を `TokenKind::QuestionDot` として追加
   - `??` を `TokenKind::QuestionQuestion` として追加
   - `?` 単体（既存の早期リターン演算子）は維持

2. **AST 拡張**
   ```rust
   Expr::OptionalChain {
       object: Box<Expr>,
       chain: ChainKind,
   }

   enum ChainKind {
       Field(String),
       Method { name: String, args: Vec<Expr> },
   }

   Expr::NullCoalesce {
       value:   Box<Expr>,
       default: Box<Expr>,
   }
   ```

3. **パーサー拡張**
   - `expr?.field` → `Expr::OptionalChain { chain: Field(...) }`
   - `expr?.method(args)` → `Expr::OptionalChain { chain: Method(...) }`
   - `expr ?? default` → `Expr::NullCoalesce`
   - `??` の優先度: `||` より低く、代入より高い

4. **インタープリタ拡張**
   - `OptionalChain`: `Value::Option(None)` → `Value::Option(None)` を返す
   - `OptionalChain`: `Value::Option(Some(v))` → `v.field` / `v.method()` の結果を `Some` で包む
   - `NullCoalesce`: `None` → default を評価して返す、`Some(v)` → `v` を返す

5. **トランスパイラ拡張**
   - `OptionalChain(Field)` → `.and_then(|v| Some(v.field))`
   - `OptionalChain(Method)` → `.and_then(|v| Some(v.method(args)))`
   - `NullCoalesce` → `.unwrap_or(default)`

6. **テスト**
   - `?.` が `none` を伝播する
   - `?.` が `some` の内側に正しくアクセスする
   - `??` がデフォルト値を返す
   - ネスト `user?.address?.city ?? "unknown"` が正しく動作する
   - Snapshot テスト

---

## Phase E-3: 演算子オーバーロード

### 目標

`impl` ブロック内で `operator +` を定義し、`v1 + v2` が呼び出せること。

### 実装ステップ

1. **Lexer 拡張**
   - `operator` を `TokenKind::Operator` キーワードとして追加

2. **AST 拡張**
   ```rust
   // ImplItem に追加
   ImplItem::OperatorDef {
       op:     OperatorKind,
       params: Vec<Param>,
       ret:    Option<TypeExpr>,
       body:   Block,
   }

   enum OperatorKind {
       Add, Sub, Mul, Div, Rem,
       Eq, Lt,
       Index,
       Neg,   // 単項マイナス
   }
   ```

3. **パーサー拡張**
   - `impl` ブロック内で `operator +(...) -> T { }` をパース
   - `operator unary-(self)` の単項形式もパース

4. **インタープリタ拡張**
   - 二項演算子評価時：左辺が struct 型の場合、impl から `operator <op>` を探す
   - 見つかれば呼び出す、なければ既存エラー処理

5. **トランスパイラ拡張**
   - `operator +` → `impl std::ops::Add for Type { type Output = ...; fn add(...) }`
   - `operator ==` → `impl PartialEq for Type { fn eq(...) }`
   - `operator <` → `impl PartialOrd for Type` + `impl Ord for Type`
   - `operator []` → `impl std::ops::Index for Type`
   - `operator unary-` → `impl std::ops::Neg for Type`

6. **テスト**
   - `+` / `*` / `==` / `[]` / `unary-` の各演算子が正しく動作する
   - `@derive(Eq)` との競合エラーが出る
   - Snapshot テスト

---

## Phase E-4: 非同期クロージャ完成 / `spawn`

### 目標

`await` を含むクロージャが自動的に `async` クロージャに昇格し、
`spawn { }` で非同期タスクを起動できること。

### 実装ステップ

1. **Lexer 拡張**
   - `spawn` を `TokenKind::Spawn` キーワードとして追加

2. **AST 拡張**
   ```rust
   Expr::Spawn { body: Block }
   ```

3. **パーサー拡張**
   - `spawn { ... }` → `Expr::Spawn`

4. **インタープリタ拡張**
   - `spawn { }` をシングルスレッド逐次実行として処理（`forge run` 用）
   - 戻り値は `Value::Option(Some(result))` として扱う（簡易 handle）

5. **トランスパイラ拡張**
   - クロージャ本体に `await` が含まれる場合 → `|args| async move { ... }` を生成
   - `spawn { body }` → `tokio::spawn(async move { body })`
   - handle の `.await?` → `handle.await?`

6. **テスト**
   - 非同期クロージャが `forge run` で正しく動作する
   - `spawn { }` が逐次実行される
   - Snapshot: 非同期クロージャの Rust 変換が正しい

---

## Phase E-5: `const fn` / コンパイル時定数

### 目標

`const fn` が定義でき、`const` 式内で呼び出せること。
`forge run` では通常の関数として実行し、`forge build` では `const fn` に変換する。

### 実装ステップ

1. **Lexer 変更なし**（`const` は既存トークン）

2. **AST 拡張**
   ```rust
   // FnDecl に is_const: bool フラグを追加
   Stmt::FnDecl {
       name:     String,
       params:   Vec<Param>,
       ret:      Option<TypeExpr>,
       body:     Block,
       is_async: bool,
       is_const: bool,   // 追加
   }
   ```

3. **パーサー拡張**
   - `const fn name(...)` → `FnDecl { is_const: true, ... }`

4. **インタープリタ変更なし**（`const fn` を通常の関数として実行）

5. **トランスパイラ拡張**
   - `is_const: true` の関数 → `const fn name(...)` を生成
   - `const` 変数の初期化式に `const fn` 呼び出しが含まれる場合 → `const VAR: T = fn_call(...)` を生成

6. **テスト**
   - `const fn` が `forge run` で通常の関数として動作する
   - Snapshot: `const fn` が Rust の `const fn` に変換される
   - Snapshot: `const VAR = const_fn(...)` が定数式に変換される

---

## Phase E-6: ジェネレータ / `yield`

### 目標

`generate<T>` 型の関数が定義でき、コレクション API と接続できること。

### 前提

E-4（`spawn` 完成）後に着手。

### 実装ステップ

1. **Lexer 拡張**
   - `yield` を `TokenKind::Yield` キーワードとして追加
   - `generate` を型名として認識（`TokenKind::Ident` のまま、型システムで解釈）

2. **AST 拡張**
   ```rust
   Stmt::Yield { value: Box<Expr> }

   // FnDecl の return type に GenerateType を追加
   TypeExpr::Generate(Box<TypeExpr>)
   ```

3. **パーサー拡張**
   - `yield expr` → `Stmt::Yield`
   - `-> generate<T>` → `TypeExpr::Generate(T)`

4. **インタープリタ拡張**
   - `generate<T>` 関数は内部的に `Vec<T>` に `yield` された値を蓄積して返す（簡易実装）
   - 戻り値は `Value::List` として扱い、既存コレクション API と接続

5. **トランスパイラ拡張**
   - `generate<T>` 関数 → `impl Iterator<Item = T>` + `std::iter::from_fn` クロージャに変換
   - `yield val` → クロージャ内で `Some(val)` を返す形式に変換（状態は `move` キャプチャ）

6. **テスト**
   - 有限ジェネレータ（`yield` が終了する）が正しく動作する
   - 無限ジェネレータ + `take(n)` が正しく動作する
   - `|> filter` / `|> map` / `|> fold` との接続
   - Snapshot テスト

---

## 実装順序の推奨

```
┌─────────────────────────────────────────┐
│ 並走可能（依存なし）                     │
│   E-1  |> パイプ演算子                  │ ← 最優先・Lexer+Parser のみ
│   E-2  ?. / ??                          │ ← 優先度高・使用頻度高
│   E-3  演算子オーバーロード              │ ← 優先度高
│   E-5  const fn                         │ ← 実装コスト低
└─────────────────────────────────────────┘
           ↓
┌─────────────────────────────────────────┐
│ E-4  非同期クロージャ / spawn            │
└─────────────────────────────────────────┘
           ↓
┌─────────────────────────────────────────┐
│ E-6  ジェネレータ / yield               │
└─────────────────────────────────────────┘
```
