# ForgeScript ジェネリクス タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: ジェネリクス・コレクション型・ユーティリティ型が
>             `forge run` で動作し、`forge build` でも Rust 変換できること

---

## Phase G-1: 型アノテーション拡張

### G-1-A: AST 拡張

- [x] `TypeAnn::Generic { name: String, args: Vec<TypeAnn> }` を追加（ユーザー定義ジェネリック型）
- [x] `TypeAnn::Map(Box<TypeAnn>, Box<TypeAnn>)` を追加（map<K, V>）
- [x] `TypeAnn::Set(Box<TypeAnn>)` を追加（set<T>）
- [x] `TypeAnn::OrderedMap(Box<TypeAnn>, Box<TypeAnn>)` を追加（ordered_map<K, V>）
- [x] `TypeAnn::OrderedSet(Box<TypeAnn>)` を追加（ordered_set<T>）
- [x] `TypeAnn::Unit` を追加（()）
- [x] `TypeAnn::Fn { params: Vec<TypeAnn>, return_type: Box<TypeAnn> }` を追加（T => U 型注釈）

### G-1-B: パーサー拡張

- [x] `map<K, V>` → `TypeAnn::Map` のパース
- [x] `set<T>` → `TypeAnn::Set` のパース
- [x] `ordered_map<K, V>` → `TypeAnn::OrderedMap` のパース
- [x] `ordered_set<T>` → `TypeAnn::OrderedSet` のパース
- [x] `()` → `TypeAnn::Unit` のパース
- [x] `Name<T>` → `TypeAnn::Generic` のパース（単一型引数）
- [x] `Name<T, U>` → `TypeAnn::Generic` のパース（複数型引数）
- [x] `T => U` 関数型注釈 → `TypeAnn::Fn` のパース（クロージャ引数型）

### G-1-C: テスト（forge-compiler パーサーテスト）

- [x] `test_parse_type_ann_map` — `map<string, number>` のパース
- [x] `test_parse_type_ann_set` — `set<string>` のパース
- [x] `test_parse_type_ann_ordered_map` — `ordered_map<string, number>` のパース
- [x] `test_parse_type_ann_ordered_set` — `ordered_set<string>` のパース
- [x] `test_parse_type_ann_unit` — `()` のパース
- [x] `test_parse_type_ann_generic_single` — `Response<string>` のパース
- [x] `test_parse_type_ann_generic_multi` — `Pair<string, number>` のパース
- [x] `test_parse_type_ann_generic_nested` — `Response<list<string>>` のパース
- [x] `test_parse_type_ann_fn` — `string => bool` のパース

---

## Phase G-2: ジェネリック定義構文

### G-2-A: AST 拡張

- [x] `Stmt::StructDef` に `generic_params: Vec<String>` フィールドを追加
- [x] `Stmt::EnumDef` に `generic_params: Vec<String>` フィールドを追加
- [x] `Stmt::ImplBlock` に `type_params: Vec<String>` および `target_type_args: Vec<TypeAnn>` を追加
- [x] `Stmt::Fn` に `type_params: Vec<String>` を追加
- [x] `FnDef` に `type_params: Vec<String>` を追加

### G-2-B: パーサー拡張

- [x] `struct Name<T> { ... }` のパース（StructDef + generic_params）
- [x] `struct Name<T, U> { ... }` のパース（複数型パラメータ）
- [x] `enum Name<T> { ... }` のパース（EnumDef + generic_params）
- [x] `fn name<T>(...) -> ...` のパース（Fn + type_params）
- [x] `impl<T> Name<T> { ... }` のパース（ImplBlock + type_params + target_type_args）
- [x] `impl<T, U> Name<T, U> { ... }` のパース（複数型パラメータ）

### G-2-C: テスト（forge-compiler パーサーテスト）

- [x] `test_parse_generic_struct_single` — `struct Response<T> { status: number, body: T }` のパース
- [x] `test_parse_generic_struct_multi` — `struct Pair<A, B> { first: A, second: B }` のパース
- [x] `test_parse_generic_enum` — `enum Either<L, R> { Left(L), Right(R) }` のパース
- [x] `test_parse_generic_fn` — `fn wrap<T>(value: T) -> Response<T>` のパース
- [x] `test_parse_generic_impl` — `impl<T> Response<T> { fn is_ok(self) -> bool }` のパース

---

## Phase G-3: コレクション型ランタイム

### G-3-A: Value 拡張

- [x] `Value::Map(Vec<(Value, Value)>)` を追加（内部表現: ordered key-value リスト）
- [x] `Value::Set(Vec<Value>)` を追加（内部表現: 重複なしリスト）

### G-3-B: パーサー拡張

- [x] `{}` リテラルのパース — 型注釈なしは空 Map
- [x] `{ "key": expr, ... }` → Map リテラルのパース
- [x] `{ expr, expr, ... }` → Set リテラルのパース（型注釈で set と判定）
- [x] `map[key]` インデックスアクセスのパース（既存の Index 式で対応）
- [x] `map[key] = value` インデックス代入のパース（IndexAssign 式を追加）

### G-3-C: インタープリタ拡張（map）

- [x] `Value::Map` の表示（Display）
- [x] `Value::Map` のマップリテラル評価
- [x] `map.get(key)` → `Value?`
- [x] `map.insert(key, val)` → `()` (state 変更)
- [x] `map.contains_key(key)` → `bool`
- [x] `map.keys()` → `list<K>`
- [x] `map.values()` → `list<V>`
- [x] `map.len()` → `number`
- [x] `map.remove(key)` → `Value?`
- [x] `map[key]` インデックスアクセス → `Value`（存在しない場合は実行時エラー）
- [x] `map[key] = value` インデックス代入（state map のみ）

### G-3-D: インタープリタ拡張（set）

- [x] `Value::Set` の表示（Display）
- [x] `Value::Set` のセットリテラル評価
- [x] `set.contains(val)` → `bool`
- [x] `set.insert(val)` → `bool`
- [x] `set.union(other)` → `set<T>`
- [x] `set.intersect(other)` → `set<T>`
- [x] `set.difference(other)` → `set<T>`
- [x] `set.len()` → `number`
- [x] `set.to_list()` → `list<T>`

### G-3-E: テスト（forge-vm インタープリタテスト）

- [x] `test_map_literal_empty` — `let m: map<string, number> = {}` → 空マップ
- [x] `test_map_literal` — `{ "a": 1, "b": 2 }` → マップリテラル
- [x] `test_map_get` — `m.get("a")` → `some(1)`
- [x] `test_map_insert` — `m.insert("c", 3)` → マップが更新される
- [x] `test_map_contains_key` — `m.contains_key("a")` → `true`
- [x] `test_map_keys` — `m.keys()` → `["a", "b"]`
- [x] `test_map_values` — `m.values()` → `[1, 2]`
- [x] `test_map_len` — `m.len()` → `2`
- [x] `test_map_index_access` — `m["a"]` → `1`
- [x] `test_map_index_assign` — `m["c"] = 3` → マップが更新される
- [x] `test_set_literal` — `{"rust", "forge"}` → セットリテラル
- [x] `test_set_contains` — `s.contains("rust")` → `true`
- [x] `test_set_insert` — `s.insert("async")` → `true`
- [x] `test_set_union` — `s1.union(s2)` → 和集合
- [x] `test_set_intersect` — `s1.intersect(s2)` → 積集合
- [x] `test_set_difference` — `s1.difference(s2)` → 差集合
- [x] `test_set_len` — `s.len()` → `2`
- [x] `test_set_to_list` — `s.to_list()` → リスト

---

## Phase G-4: ジェネリック型インタープリタ

### G-4-A: インタープリタ拡張

- [x] `StructDef` の `generic_params` を無視してインスタンス生成（型消去）
- [x] `EnumDef` の `generic_params` を無視してバリアント生成（型消去）
- [x] `ImplBlock` の `type_params` を無視してメソッド登録（`impl<T> Name<T>` → `Name` に登録）
- [x] `Fn` の `type_params` を無視して関数定義
- [x] `StructInit` で `Name<T>` の型引数を無視（`Response::<string>` 構文は非サポート、`Response { ... }` で代用）
- [x] `TypeAnn::Generic` を型チェックなしで評価（forge run では型引数を無視）

### G-4-B: テスト（forge-vm インタープリタテスト）

- [x] `test_generic_struct_basic` — `struct Response<T> { body: T }` の定義と `Response { body: 42 }` のインスタンス化
- [x] `test_generic_struct_method` — `impl<T> Response<T> { fn is_ok(self) -> bool }` のメソッド呼び出し
- [x] `test_generic_fn_wrap` — `fn wrap<T>(v: T) -> Response<T>` の呼び出し
- [x] `test_generic_enum_either` — `enum Either<L, R> { Left(L), Right(R) }` のパターンマッチ

### G-4-C: E2E テスト（forge-cli）

- [x] `e2e_generics_basic` — ジェネリック struct/fn/enum の基本動作

---

## Phase G-5: ユーティリティ型

### G-5-A: インタープリタ拡張

- [x] `Partial<T>` — T の struct 定義を取得し、全フィールドを optional にした新 struct を動的生成
- [x] `Required<T>` — T の全フィールドの optional を除去した新 struct を動的生成
- [x] `Readonly<T>` — forge run では通常の T と同じ（型注釈のみ）
- [x] `Pick<T, Keys>` — T の指定フィールドのみの struct を動的生成
- [x] `Omit<T, Keys>` — T の指定フィールドを除いた struct を動的生成
- [x] `NonNullable<T?>` — T? を unwrap（None なら "NonNullable: value is None" でエラー）
- [x] `Record<K, V>` — `map<K, V>` と同じ（エイリアス）

### G-5-B: ユーティリティ型 構文対応

- [x] `Partial<T>` を型注釈として使用（fn 引数型など）
- [x] `Partial::from(instance)` — 既存インスタンスから Partial を生成（全フィールドを some() でラップ）
- [x] `Pick::from(instance)` — 指定フィールドのみ抽出
- [x] `Omit::from(instance)` — 指定フィールドを除いて抽出
- [x] `NonNullable` の実行時 unwrap

### G-5-C: テスト（forge-vm）

- [x] `test_partial_type` — `Partial<User>` でフィールドが optional になること
- [x] `test_partial_from` — `Partial::from(user)` で変換できること
- [x] `test_required_type` — `Required<Config>` でフィールドが非 optional になること
- [x] `test_pick_type` — `Pick<User, "id" | "name">` で指定フィールドのみ取得
- [x] `test_omit_type` — `Omit<User, "password">` で指定フィールドを除いた型
- [x] `test_nonnullable` — `NonNullable<string?>` で Some("hello") が "hello" になる
- [x] `test_nonnullable_none_error` — None を NonNullable に渡すとエラー
- [x] `test_record_alias` — `Record<string, number>` が `map<string, number>` と同等

---

## Phase G-6: トランスパイラ拡張

### G-6-A: 型注釈の Rust 変換

- [x] `TypeAnn::Generic` → `Name<T>` Rust 変換
- [x] `TypeAnn::Map` → `std::collections::HashMap<K, V>` Rust 変換
- [x] `TypeAnn::Set` → `std::collections::HashSet<T>` Rust 変換
- [x] `TypeAnn::OrderedMap` → `std::collections::BTreeMap<K, V>` Rust 変換
- [x] `TypeAnn::OrderedSet` → `std::collections::BTreeSet<T>` Rust 変換
- [x] `TypeAnn::Unit` → `()` Rust 変換
- [x] `TypeAnn::Fn` → `impl Fn(T) -> U` Rust 変換

### G-6-B: ジェネリック定義の Rust 変換

- [x] `struct Name<T> { ... }` → `struct Name<T> { ... }` Rust 変換
- [x] `enum Name<T, U> { ... }` → `enum Name<T, U> { ... }` Rust 変換
- [x] `fn name<T>(...) -> ...` → `fn name<T>(...) -> ...` Rust 変換
- [x] `impl<T> Name<T> { ... }` → `impl<T> Name<T> { ... }` Rust 変換

### G-6-C: コレクション型の Rust 変換

- [x] `map<K, V>` リテラル `{ "a": 1 }` → `HashMap::from([(...)])` Rust 変換
- [x] `set<T>` リテラル `{"a", "b"}` → `HashSet::from([...])` Rust 変換
- [x] `map.get(k)` → `.get(&k)` Rust 変換
- [x] `map.insert(k, v)` → `.insert(k, v)` Rust 変換
- [x] `map[k]` → `map[&k]` Rust 変換
- [x] HashMap / HashSet の `use` 文自動挿入

### G-6-D: ユーティリティ型の Rust 変換

- [x] `Partial<User>` → `PartialUser` struct 自動生成（forge build 時）
- [x] `Required<Config>` → `RequiredConfig` struct 自動生成
- [x] `Pick<User, "id" | "name">` → `UserPick_id_name` struct 自動生成
- [x] `Omit<User, "password">` → `UserOmit_password` struct 自動生成
- [x] `Readonly<T>` → `&T` 参照への変換
- [x] 同一型引数への重複生成防止

### G-6-E: スナップショットテスト（forge-transpiler）

- [x] `snapshot_generic_struct` — ジェネリック struct の Rust 変換
- [x] `snapshot_generic_fn` — ジェネリック fn の Rust 変換
- [x] `snapshot_generic_impl` — ジェネリック impl の Rust 変換
- [x] `snapshot_generic_enum` — ジェネリック enum の Rust 変換
- [x] `snapshot_map_type` — `map<K, V>` の Rust 変換
- [x] `snapshot_set_type` — `set<T>` の Rust 変換
- [x] `snapshot_partial_type` — `Partial<T>` の Rust 変換
- [x] `snapshot_pick_type` — `Pick<T, Keys>` の Rust 変換
- [x] `snapshot_omit_type` — `Omit<T, Keys>` の Rust 変換

---

## 進捗サマリー

| Phase | タスク数 | 完了 |
|---|---|---|
| G-1: 型アノテーション拡張 | 17 | 0 |
| G-2: ジェネリック定義構文 | 13 | 0 |
| G-3: コレクション型ランタイム | 26 | 0 |
| G-4: ジェネリック型インタープリタ | 8 | 0 |
| G-5: ユーティリティ型 | 15 | 0 |
| G-6: トランスパイラ拡張 | 24 | 0 |
| **合計** | **103** | **0** |
