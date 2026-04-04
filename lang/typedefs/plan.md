# ForgeScript 型定義 実装計画

> 仕様: `forge/typedefs/spec.md`
> 前提: v0.0.1 の Lexer / Parser / Interpreter / 型チェッカーが完成済み

---

## フェーズ構成

```
Phase T-1: struct（基本・impl・@derive）
Phase T-2: enum（データなし・データあり・match 連携）
Phase T-3: trait / mixin / impl
Phase T-4: data キーワード（validate ブロック含む）
Phase T-5: typestate キーワード
```

---

## Phase T-1: struct

### 目標
`struct` 定義・インスタンス化・フィールドアクセス・`impl` メソッドが動作すること。
`@derive(Debug, Clone, Eq, Accessor, Singleton)` が機能すること。

### 実装ステップ

1. **Lexer 拡張**
   - `struct`, `impl`, `self`, `Self`, `for`, `mixin`, `trait`, `data`, `typestate` トークンを追加
   - `@` アノテーション構文のトークン追加

2. **AST 拡張**
   ```rust
   Stmt::StructDef { name, fields, derives }
   Stmt::ImplBlock { target, trait_name, methods }
   Expr::StructInit { name, fields }
   Expr::FieldAccess { object, field }
   ```

3. **パーサー拡張**
   - `struct Name { field: Type, ... }` のパース
   - `impl Name { fn ... }` のパース
   - `@derive(...)` アノテーションのパース
   - `Name { field: expr, ... }` インスタンス化のパース
   - `expr.field` フィールドアクセスのパース（既存の MethodCall と区別）

4. **インタープリタ拡張**
   - `Value::Struct { type_name, fields: HashMap<String, Value> }` を追加
   - `StructDef` → 型レジストリに登録
   - `StructInit` → `Value::Struct` を生成
   - `FieldAccess` → フィールド値を返す
   - `ImplBlock` → メソッドを型レジストリに登録
   - `self` → 現在のインスタンスを暗黙引数として束縛

5. **@derive 処理**
   - `Debug` → `display()` メソッド自動生成
   - `Clone` → `clone()` メソッド自動生成
   - `Eq` → `==` 演算子対応
   - `Accessor` → `get_<field>()` / `set_<field>(val)` 自動生成
   - `Singleton` → インスタンスキャッシュを型レジストリで管理

6. **テスト**

---

## Phase T-2: enum

### 目標
`enum` のデータなし・タプル・名前付きフィールドバリアントが定義・使用でき、
`match` でパターンマッチできること。

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::EnumDef { name, variants, derives }
   EnumVariant::Unit(name)
   EnumVariant::Tuple(name, Vec<TypeAnn>)
   EnumVariant::Struct(name, Vec<(String, TypeAnn)>)
   Expr::EnumInit { enum_name, variant, fields }
   ```

2. **パーサー拡張**
   - `enum Name { Variant, ... }` のパース
   - `Name::Variant` / `Name::Variant(expr)` / `Name::Variant { field: expr }` のパース

3. **インタープリタ拡張**
   - `Value::Enum { type_name, variant, data: EnumData }` を追加
   - `match` パターンに enum バリアントを追加

4. **テスト**

---

## Phase T-3: trait / mixin / impl

### 目標
`trait` 定義・`impl Trait for Type` による実装・
`mixin` のデフォルト実装が動作すること。

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::TraitDef { name, methods }        // 抽象メソッド + デフォルト実装
   Stmt::MixinDef { name, methods }        // デフォルト実装のみ
   Stmt::ImplTrait { trait_name, target, methods }
   TraitMethod::Abstract { name, params, return_type }
   TraitMethod::Default { name, params, return_type, body }
   ```

2. **パーサー拡張**
   - `trait Name { fn ... }` のパース
   - `mixin Name { fn ... }` のパース
   - `impl Trait for Type { fn ... }` のパース
   - `impl Mixin for Type` （本体なし）のパース

3. **インタープリタ拡張**
   - trait/mixin をレジストリに登録
   - `impl Trait for Type` でメソッドを型に紐付け
   - `impl Mixin for Type` でデフォルトメソッドを型に紐付け
   - メソッド解決順序: 直接 impl → trait デフォルト → mixin デフォルト

4. **テスト**

---

## Phase T-4: data キーワード

### 目標
`data` ブロックが `struct` + 全 derive 自動付与として動作し、
`validate` ブロックが書き込み前バリデーションとして機能すること。

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::DataDef { name, fields, validate_rules }
   ValidateRule { field, constraints: Vec<Constraint> }
   ```

2. **パーサー拡張**
   - `data Name { field: Type, ... } validate { ... }` のパース

3. **インタープリタ拡張**
   - `DataDef` → 全 derive を付与した `StructDef` として処理
   - `validate` ブロック → `.validate()` メソッドを自動生成
     - 成功: `ok(instance)` を返す
     - 失敗: `err("field: constraint violated")` を返す

4. **組み込みバリデーター**
   - `length(min..max)` / `length(min: n)` / `length(max: n)`
   - `alphanumeric` / `email_format` / `url_format`
   - `range(min..max)` / `range(min: n)` / `range(max: n)`
   - `contains_digit` / `contains_uppercase` / `contains_lowercase`
   - `matches(regex)` / `not_empty`

5. **テスト**

---

## Phase T-5: typestate

### 目標
`typestate` キーワードで状態機械を定義し、
状態遷移の正当性がインタープリタ実行時（v0.1.0）に検証されること。
（コンパイル時検証は forge build B-8 フェーズで対応）

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::TypestateDef { name, states, transitions }
   TypestateState { name, methods }
   ```

2. **パーサー拡張**
   - `typestate Name { states: [...], StateName { fn ... } }` のパース

3. **インタープリタ拡張**
   - `Value::Typestate { type_name, current_state, inner }` を追加
   - `Name::new<State>()` でインスタンス生成
   - メソッド呼び出し時に現在の状態を確認し、不正な遷移はランタイムエラー

4. **テスト**

---

## テスト方針

各フェーズ完了後に以下を実施：
- ユニットテスト（パーサー・インタープリタ各層）
- E2E テスト（`.forge` ファイルを `forge run` で実行して stdout 検証）
- ラウンドトリップテスト（`forge run` == `forge build + run`）は B-5 フェーズで対応
