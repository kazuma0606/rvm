# ForgeScript ジェネリクス 実装計画

> 仕様: `lang/generics/spec.md`
> 前提: T-1〜T-5（型定義）・M-0〜M-7（モジュール）・B-0〜B-8（トランスパイラ）完成済み

---

## フェーズ構成

```
Phase G-1: 型アノテーション拡張（Parser）
Phase G-2: ジェネリック定義構文（Parser）
Phase G-3: コレクション型ランタイム（map / set / ordered_map / ordered_set）
Phase G-4: ジェネリック型インタープリタ（forge run）
Phase G-5: ユーティリティ型（forge run）
Phase G-6: トランスパイラ拡張（forge build）
```

---

## 変更ファイル一覧

```
crates/
├── forge-compiler/
│   └── src/
│       ├── ast/mod.rs          ← G-1-A, G-2-A: TypeAnn・Stmt・FnDef に型パラメータ追加
│       └── parser/mod.rs       ← G-1-B, G-2-B: parse_type_ann・ジェネリック定義パース拡張
│
├── forge-vm/
│   └── src/
│       ├── value.rs            ← G-3-A: Value::Map / Value::Set 追加
│       └── interpreter.rs      ← G-3-C/D, G-4-A, G-5-A: Map/Set メソッド・ジェネリック型消去
│
├── forge-transpiler/
│   └── src/
│       └── codegen.rs          ← G-6-A/B/C/D: ジェネリック型の Rust 変換
│
└── forge-cli/
    └── tests/
        └── e2e/                ← G-4-C: E2E テスト追加
```

テストの配置：
```
crates/forge-compiler/src/parser/mod.rs  ← G-1-C, G-2-C のパーサーテスト（#[cfg(test)] に追記）
crates/forge-vm/src/interpreter.rs       ← G-3-E, G-4-B, G-5-C のインタープリタテスト（#[cfg(test)] に追記）
crates/forge-transpiler/src/             ← G-6-E スナップショットテスト（insta クレート）
```

---

## Phase G-1: 型アノテーション拡張

### 目標
型注釈で `Response<T>`, `map<K, V>`, `set<T>`, `()` などが
パースできること。インタープリタは型を消去して実行（型推論は将来）。

### 変更ファイル
- `crates/forge-compiler/src/ast/mod.rs`
- `crates/forge-compiler/src/parser/mod.rs`

### AST 変更（ast/mod.rs）
```rust
// TypeAnn に追加
TypeAnn::Generic { name: String, args: Vec<TypeAnn> }  // Response<T>, Pair<A, B>
TypeAnn::Map(Box<TypeAnn>, Box<TypeAnn>)                // map<K, V>
TypeAnn::Set(Box<TypeAnn>)                              // set<T>
TypeAnn::OrderedMap(Box<TypeAnn>, Box<TypeAnn>)         // ordered_map<K, V>
TypeAnn::OrderedSet(Box<TypeAnn>)                       // ordered_set<T>
TypeAnn::Unit                                           // ()
TypeAnn::Fn { params: Vec<TypeAnn>, return_type: Box<TypeAnn> }  // T => U
```

### パーサー変更（parser/mod.rs）
`parse_type_ann` を拡張：
- `map<K, V>` → `TypeAnn::Map`
- `set<T>` → `TypeAnn::Set`
- `ordered_map<K, V>` → `TypeAnn::OrderedMap`
- `ordered_set<T>` → `TypeAnn::OrderedSet`
- `()` → `TypeAnn::Unit`
- `Name<T>` → `TypeAnn::Generic`（`Named` の後に `<` が続く場合）
- `Name<T, U>` → `TypeAnn::Generic`（複数型引数）
- `T => U` → `TypeAnn::Fn`

### 影響を受ける他のコード
`TypeAnn` を match している箇所すべてに `_ => todo!("generics")` またはデフォルト処理を追加：
- `crates/forge-compiler/src/typechecker/` 内
- `crates/forge-transpiler/src/codegen.rs` 内
- `crates/forge-vm/src/interpreter.rs` 内

---

## Phase G-2: ジェネリック定義構文

### 目標
`struct Name<T>`, `fn name<T>(...)`, `impl<T> Name<T>`, `enum Name<T, U>` が
パースできること。型パラメータ名はAST上に保持（ランタイムでは消去）。

### 変更ファイル
- `crates/forge-compiler/src/ast/mod.rs`
- `crates/forge-compiler/src/parser/mod.rs`

### AST 変更（ast/mod.rs）
```rust
// 型パラメータを持つ定義に generic_params / type_params を追加
Stmt::StructDef { ..., generic_params: Vec<String> }
Stmt::EnumDef   { ..., generic_params: Vec<String> }
Stmt::ImplBlock { ..., type_params: Vec<String>, target_type_args: Vec<TypeAnn> }
Stmt::Fn        { ..., type_params: Vec<String> }
FnDef           { ..., type_params: Vec<String> }
```

### パーサー変更（parser/mod.rs）
- `struct Name<T> { ... }` のパース → `parse_struct_def` に `<T>` 読み取りを追加
- `enum Name<T, U> { ... }` のパース → `parse_enum_def_body` に追加
- `fn name<T>(...) -> ...` のパース → `parse_fn` に追加
- `impl<T> Name<T> { ... }` のパース → `parse_impl_or_impl_trait` に追加

### 型パラメータパース ヘルパー
```rust
// <T> / <T, U> / <A, B, C> を Vec<String> としてパース
fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError>

// <TypeAnn> / <TypeAnn, TypeAnn> を Vec<TypeAnn> としてパース（impl<T> Name<T> の右辺）
fn parse_type_args(&mut self) -> Result<Vec<TypeAnn>, ParseError>
```

---

## Phase G-3: コレクション型ランタイム

### 目標
`map<K, V>` / `set<T>` / `ordered_map<K, V>` / `ordered_set<T>` が
`forge run` で動作すること。

### 変更ファイル
- `crates/forge-vm/src/value.rs`
- `crates/forge-vm/src/interpreter.rs`
- `crates/forge-compiler/src/ast/mod.rs`（IndexAssign 式追加）
- `crates/forge-compiler/src/parser/mod.rs`（Map/Set リテラル・IndexAssign パース）

### Value 変更（value.rs）
```rust
// 追加
Value::Map(Vec<(Value, Value)>)  // 順序付き key-value ペアリスト
Value::Set(Vec<Value>)           // 重複なしリスト
```
`forge run` ではシンプルな `Vec` ベースの O(n) 実装。
`ordered_map` / `ordered_set` も同じ内部表現（ソートは keys()/entries() 時に実施）。

### AST 変更（ast/mod.rs）
```rust
// Expr に追加
Expr::MapLiteral { pairs: Vec<(Expr, Expr)>, span: Span }   // { "a": 1, "b": 2 }
Expr::SetLiteral { items: Vec<Expr>, span: Span }            // {"a", "b"}
Expr::IndexAssign {                                          // m["key"] = val
    object: Box<Expr>,
    index: Box<Expr>,
    value: Box<Expr>,
    span: Span,
}
```

**注意**: `{}` が Map なのか Set なのかは以下の規則で判断する：
- `{ "key": expr, ... }` → MapLiteral（コロンあり）
- `{ expr, expr, ... }` → SetLiteral（コロンなし）
- `{}` → 空 MapLiteral（型注釈で map か set かを区別するが、デフォルトは Map）

### 組み込みメソッド

**map メソッド**（interpreter.rs の MethodCall 処理に追加）
| メソッド | 戻り値 |
|---|---|
| `get(key)` | `Value?` |
| `insert(key, val)` | `()` |
| `contains_key(key)` | `bool` |
| `keys()` | `list<K>` |
| `values()` | `list<V>` |
| `entries()` | `list<[K, V]>` |
| `len()` | `number` |
| `remove(key)` | `Value?` |

**set メソッド**（interpreter.rs の MethodCall 処理に追加）
| メソッド | 戻り値 |
|---|---|
| `contains(val)` | `bool` |
| `insert(val)` | `bool` |
| `union(other)` | `set<T>` |
| `intersect(other)` | `set<T>` |
| `difference(other)` | `set<T>` |
| `len()` | `number` |
| `to_list()` | `list<T>` |

---

## Phase G-4: ジェネリック型インタープリタ

### 目標
ジェネリック struct / fn / enum が `forge run` で動作すること。
型パラメータはランタイムで消去（型チェックはしない）。

### 変更ファイル
- `crates/forge-vm/src/interpreter.rs`

### 変更内容
- `StructDef` の `generic_params` を無視してインスタンス生成（既存コードに `generic_params` を無視するだけ）
- `EnumDef` の `generic_params` を無視
- `ImplBlock` の `type_params` / `target_type_args` を無視して `target` 名でメソッド登録
- `Fn` の `type_params` を無視して関数定義
- `TypeAnn::Generic` を型チェックなしで評価（型引数は無視）

---

## Phase G-5: ユーティリティ型

### 目標
`Partial<T>`, `Required<T>`, `Readonly<T>`, `Pick<T, Keys>`,
`Omit<T, Keys>`, `NonNullable<T>`, `Record<K, V>` が
`forge run` で動作すること。

### 変更ファイル
- `crates/forge-vm/src/interpreter.rs`

### 動作仕様（forge run）
ユーティリティ型は `TypeAnn::Generic { name, args }` としてパースされる。
`forge run` では型注釈を評価しないため、**主に `Partial::from()` / `Pick::from()` 等の
関数呼び出しとして実装**する。

| 関数呼び出し | 動作 |
|---|---|
| `Partial::from(instance)` | 全フィールドを `some(v)` でラップ |
| `Required::from(instance)` | 全フィールドの `some(v)` を `v` に unwrap |
| `Pick::from(instance, ["id", "name"])` | 指定フィールドのみ抽出 |
| `Omit::from(instance, ["password"])` | 指定フィールドを除いて抽出 |
| `NonNullable::from(optional)` | `some(v)` → `v` / `none` → 実行時エラー |
| `Record::new()` | 空 Map を返す |

これらは型定義なしで interpreter.rs の組み込み関数として実装する。

---

## Phase G-6: トランスパイラ拡張

### 目標
ジェネリック定義・コレクション型・ユーティリティ型が
`forge build` で正しい Rust コードに変換されること。

### 変更ファイル
- `crates/forge-transpiler/src/codegen.rs`

### 変換規則

**型注釈**
| ForgeScript | Rust |
|---|---|
| `TypeAnn::Generic { name, args }` | `Name<A, B>` |
| `TypeAnn::Map(K, V)` | `std::collections::HashMap<K, V>` |
| `TypeAnn::Set(T)` | `std::collections::HashSet<T>` |
| `TypeAnn::OrderedMap(K, V)` | `std::collections::BTreeMap<K, V>` |
| `TypeAnn::OrderedSet(T)` | `std::collections::BTreeSet<T>` |
| `TypeAnn::Unit` | `()` |
| `TypeAnn::Fn { params, return_type }` | `impl Fn(T) -> U` |

**定義**
| ForgeScript | Rust |
|---|---|
| `struct Foo<T> { ... }` | `struct Foo<T> { ... }` |
| `enum Bar<T, U> { ... }` | `enum Bar<T, U> { ... }` |
| `fn f<T>(...) -> ...` | `fn f<T>(...) -> ...` |
| `impl<T> Foo<T> { ... }` | `impl<T> Foo<T> { ... }` |

**コレクションリテラル**
| ForgeScript | Rust |
|---|---|
| `{ "a": 1, "b": 2 }` (map) | `HashMap::from([("a".to_string(), 1_i64), ...])` |
| `{"a", "b"}` (set) | `HashSet::from(["a".to_string(), ...])` |
| `m["key"]` | `m[&"key".to_string()]` |
| `m.insert(k, v)` | `m.insert(k, v)` |

**use 文の自動挿入**
`HashMap` / `HashSet` を使う場合、`use std::collections::{HashMap, HashSet};` を自動挿入する。

**ユーティリティ型**（`forge build` 時にコンパイラが struct を自動生成）

| ForgeScript | 生成 Rust 型名 |
|---|---|
| `Partial<User>` | `PartialUser` |
| `Required<Config>` | `RequiredConfig` |
| `Pick<User, "id" \| "name">` | `UserPick_id_name` |
| `Omit<User, "password">` | `UserOmit_password` |

---

## テスト方針

各フェーズ完了後に以下を実施：
- ユニットテスト（パーサー層: `crates/forge-compiler/src/parser/mod.rs` の `#[cfg(test)]`）
- インタープリタテスト（`crates/forge-vm/src/interpreter.rs` の `#[cfg(test)]`）
- E2E テスト（`crates/forge-cli/tests/e2e/` に `.forge` ファイルを追加）
- スナップショットテスト（`crates/forge-transpiler/` に insta テストを追加）
