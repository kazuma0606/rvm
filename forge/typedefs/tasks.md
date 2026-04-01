# ForgeScript 型定義 タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: struct / enum / trait / mixin / data / typestate が
>             `forge run` で動作し、`forge build` でも同等の出力を得られること

---

## Phase T-1: struct

### T-1-A: Lexer 拡張

- [x] `struct` キーワードトークンを追加
- [x] `impl` キーワードトークンを追加
- [x] `self` キーワードトークンを追加
- [x] `Self` キーワードトークンを追加
- [x] `trait` キーワードトークンを追加
- [x] `mixin` キーワードトークンを追加
- [x] `data` キーワードトークンを追加
- [x] `typestate` キーワードトークンを追加
- [x] `@` アノテーション構文のトークンを追加（`@derive` 等）

### T-1-B: AST 拡張

- [x] `Stmt::StructDef { name, fields, derives }` を追加
- [x] `Stmt::ImplBlock { target, trait_name, methods }` を追加
- [x] `Expr::StructInit { name, fields }` を追加
- [x] `Expr::FieldAccess { object, field }` を追加（MethodCall と区別）

### T-1-C: パーサー拡張

- [x] `struct Name { field: Type, ... }` のパース
- [x] `impl Name { fn ... }` のパース
- [x] `@derive(Debug, Clone, ...)` アノテーションのパース
- [x] `Name { field: expr, ... }` インスタンス化のパース
- [x] `expr.field` フィールドアクセスのパース

### T-1-D: インタープリタ拡張

- [x] `Value::Struct { type_name, fields: HashMap<String, Value> }` を追加
- [x] `StructDef` → 型レジストリへの登録
- [x] `StructInit` → `Value::Struct` の生成
- [x] `FieldAccess` → フィールド値の返却
- [x] `ImplBlock` → メソッドを型レジストリに登録
- [x] `self` → メソッド内の暗黙引数として束縛
- [x] `state self` → 可変メソッドの実装

### T-1-E: @derive 処理

- [x] `@derive(Debug)` → `display()` メソッド自動生成
- [x] `@derive(Clone)` → `clone()` メソッド自動生成
- [x] `@derive(Eq)` → `==` / `!=` 演算子の対応
- [x] `@derive(Hash)` → ハッシュ化対応
- [x] `@derive(Ord)` → 大小比較・`order_by` 対応
- [x] `@derive(Default)` → `Default::new()` 生成
- [x] `@derive(Accessor)` → `get_<field>()` / `set_<field>(val)` 自動生成
- [x] `@derive(Singleton)` → 型レジストリでインスタンスキャッシュ管理

### T-1-F: テスト

- [x] テスト: `test_struct_basic` — 定義・インスタンス化・フィールドアクセス
- [x] テスト: `test_struct_impl` — impl メソッドの呼び出し
- [x] テスト: `test_struct_self_mutation` — `state self` による値変更
- [x] テスト: `test_derive_debug` — `@derive(Debug)` の display
- [x] テスト: `test_derive_clone` — `@derive(Clone)` のディープコピー
- [x] テスト: `test_derive_eq` — `@derive(Eq)` の == 比較
- [x] テスト: `test_derive_accessor` — getter/setter の動作
- [x] テスト: `test_derive_singleton` — 同一インスタンスの返却
- [x] E2E テスト: `struct_basic.forge`
- [x] E2E テスト: `struct_methods.forge`
- [x] E2E テスト: `struct_derive.forge`

---

## Phase T-2: enum

### T-2-A: AST 拡張

- [x] `Stmt::EnumDef { name, variants, derives }` を追加
- [x] `EnumVariant::Unit(name)` を追加
- [x] `EnumVariant::Tuple(name, Vec<TypeAnn>)` を追加
- [x] `EnumVariant::Struct(name, Vec<(String, TypeAnn)>)` を追加
- [x] `Expr::EnumInit { enum_name, variant, data }` を追加

### T-2-B: パーサー拡張

- [x] `enum Name { Variant, ... }` のパース
- [x] `Name::Variant` のパース
- [x] `Name::Variant(expr, ...)` のパース
- [x] `Name::Variant { field: expr, ... }` のパース

### T-2-C: インタープリタ拡張

- [x] `Value::Enum { type_name, variant, data }` を追加
- [x] `EnumDef` → 型レジストリへの登録
- [x] `EnumInit` → `Value::Enum` の生成
- [x] `match` パターン: Unit バリアント対応
- [x] `match` パターン: Tuple バリアント対応（変数束縛）
- [x] `match` パターン: Struct バリアント対応（フィールド束縛）

### T-2-D: テスト

- [x] テスト: `test_enum_unit` — データなしバリアントの定義と match
- [x] テスト: `test_enum_tuple` — タプルバリアントの束縛
- [x] テスト: `test_enum_struct_variant` — 名前付きフィールドバリアント
- [x] テスト: `test_enum_derive` — `@derive(Debug, Clone, Eq)` の動作
- [x] E2E テスト: `enum_basic.forge`
- [x] E2E テスト: `enum_match.forge`

---

## Phase T-3: trait / mixin / impl

### T-3-A: AST 拡張

- [ ] `Stmt::TraitDef { name, methods }` を追加
- [ ] `Stmt::MixinDef { name, methods }` を追加
- [ ] `Stmt::ImplTrait { trait_name, target, methods }` を追加
- [ ] `TraitMethod::Abstract { name, params, return_type }` を追加
- [ ] `TraitMethod::Default { name, params, return_type, body }` を追加

### T-3-B: パーサー拡張

- [ ] `trait Name { fn ... }` のパース（デフォルト実装あり・なし混在）
- [ ] `mixin Name { fn ... }` のパース
- [ ] `impl Trait for Type { fn ... }` のパース
- [ ] `impl Mixin for Type` （本体なし）のパース

### T-3-C: インタープリタ拡張

- [ ] `TraitDef` / `MixinDef` → trait レジストリへの登録
- [ ] `ImplTrait` → メソッドを型に紐付け
- [ ] `impl Mixin for Type`（本体なし）→ デフォルトメソッドをそのまま紐付け
- [ ] メソッド解決順序: 直接 impl → trait デフォルト → mixin デフォルト
- [ ] mixin メソッド名衝突 → コンパイルエラー

### T-3-D: テスト

- [ ] テスト: `test_trait_impl` — 基本的な trait の定義と実装
- [ ] テスト: `test_trait_default` — デフォルト実装の継承と上書き
- [ ] テスト: `test_mixin_basic` — mixin のデフォルト実装
- [ ] テスト: `test_mixin_multi` — 複数 mixin の組み合わせ
- [ ] テスト: `test_mixin_conflict` — メソッド名衝突のエラー検出
- [ ] E2E テスト: `trait_basic.forge`
- [ ] E2E テスト: `mixin_basic.forge`

---

## Phase T-4: data キーワード

### T-4-A: AST 拡張

- [ ] `Stmt::DataDef { name, fields, validate_rules }` を追加
- [ ] `ValidateRule { field, constraints }` を追加

### T-4-B: パーサー拡張

- [ ] `data Name { field: Type, ... }` のパース
- [ ] `data Name { ... } validate { field: constraint, ... }` のパース
- [ ] バリデーター構文のパース（`length(3..20)`, `email_format` 等）

### T-4-C: インタープリタ拡張

- [ ] `DataDef` → 全 derive を付与した StructDef として処理
- [ ] `validate` ブロック → `.validate()` メソッドの自動生成
- [ ] 組み込みバリデーター: `length`, `alphanumeric`, `email_format`
- [ ] 組み込みバリデーター: `range`, `contains_digit`, `contains_uppercase`
- [ ] 組み込みバリデーター: `not_empty`, `matches`

### T-4-D: テスト

- [ ] テスト: `test_data_basic` — 定義・インスタンス化・自動 derive 確認
- [ ] テスト: `test_data_validate_ok` — バリデーション成功
- [ ] テスト: `test_data_validate_err` — バリデーション失敗とエラーメッセージ
- [ ] E2E テスト: `data_basic.forge`
- [ ] E2E テスト: `data_validate.forge`

---

## Phase T-5: typestate

### T-5-A: AST 拡張

- [ ] `Stmt::TypestateDef { name, states, transitions }` を追加
- [ ] `TypestateState { name, methods }` を追加

### T-5-B: パーサー拡張

- [ ] `typestate Name { states: [...], StateName { fn ... } }` のパース
- [ ] `Name::new<StateName>()` のパース

### T-5-C: インタープリタ拡張

- [ ] `Value::Typestate { type_name, current_state, inner }` を追加
- [ ] `Name::new<State>()` でインスタンス生成
- [ ] メソッド呼び出し時の状態チェック（不正な遷移はランタイムエラー）
- [ ] 状態遷移後に新しい `Value::Typestate` を返す

### T-5-D: テスト

- [ ] テスト: `test_typestate_basic` — 正常な状態遷移
- [ ] テスト: `test_typestate_invalid` — 不正な状態でのメソッド呼び出しエラー
- [ ] E2E テスト: `typestate_connection.forge`
- [ ] E2E テスト: `typestate_door.forge`
