# ForgeScript validator 実装計画

> 仕様: `lang/validator/spec.md`
> 前提: `forge/std/regex`（`r.matches()` で使用）、Anvil ミドルウェアチェーンが実装済み

---

## フェーズ構成

```
Phase VAL-1: コア型定義
Phase VAL-2: ルールビルダー
Phase VAL-3: Validator 構築 API
Phase VAL-4: バリデーション実行エンジン
Phase VAL-5: Anvil ミドルウェア統合
Phase VAL-6: テスト・ドキュメント
```

---

## Phase VAL-1: コア型定義

### 目標

`ValidationError`・`Rule`・`Validator` の Rust ネイティブ実装を `crates/forge-validator` クレートとして作成し、インタープリタから `use forge/validator.{...}` でロードできること。

### 実装ステップ

1. **`crates/forge-validator` クレート新設**
   - `Cargo.toml` に `forge-validator` を workspace へ追加
   - 依存: `forge-vm`（`Value`・`NativeFn` 型）、`regex`（`matches()` 実装用）

2. **`ValidationError` 型**
   - ForgeScript 側: `data ValidationError { fields: list<string>, message: string, value: any? }`
   - Rust 側: `struct ValidationError { fields: Vec<String>, message: String, value: Option<Value> }`
   - `Value::Struct` として返せるよう `to_value()` を実装

3. **`RuleChain` 型（内部）**
   - 複数の制約クロージャを保持する `Vec<Box<dyn Fn(&Value) -> RuleResult>>`
   - `RuleResult = Result<(), String>`（エラーメッセージを返す）

4. **`Rule` 型**
   - ForgeScript から `Rule::new()` で構築できるビルダー
   - 内部に `RuleChain` とオプションのデフォルトメッセージを持つ
   - `Validator::rule()` に渡せるよう `Value` として保持できること

5. **`Validator` 型**
   - `struct Validator { rules: Vec<FieldRule>, cross_rules: Vec<CrossRule>, ... }`
   - 各ルール種別（単フィールド・クロス・条件付き・リスト・ネスト）を統一的に保持
   - `ForgeScript Value` として不透明オブジェクト（`NativeObject`）に格納

6. **モジュールエントリポイント**
   - `fn register_validator_module(interp: &mut Interpreter)` を公開
   - `forge-vm` の `Interpreter::register_module("forge/validator", ...)` で呼び出す

---

## Phase VAL-2: ルールビルダー

### 目標

`fn(r) { r.required().min_length(8) }` 形式のルール定義クロージャが正しく評価され、各制約が値を検証できること。

### 実装ステップ

1. **`RuleBuilder` オブジェクト**
   - `on_statement` フック内でクロージャに渡される一時オブジェクト
   - メソッドチェーンを Rust 側でネイティブ実装

2. **各メソッドの実装**

   | メソッド | 検証ロジック |
   |---|---|
   | `r.required()` | `Value::Option(None)` または空文字列を拒否 |
   | `r.min_length(n)` | `value.len() >= n`（string / list 共通） |
   | `r.max_length(n)` | `value.len() <= n` |
   | `r.min(n)` | 数値 `>= n` |
   | `r.max(n)` | 数値 `<= n` |
   | `r.matches(pattern)` | `regex::Regex::new(pattern)?.is_match(value)` |
   | `r.one_of(list)` | `list.contains(value)` |
   | `r.custom(fn)` | ForgeScript クロージャ `(value) -> Result<(), string>` を呼び出す |
   | `r.ok()` | 常に `Ok(())` |

3. **`Rule::new()` + `.message()` の実装**
   - 定数ルールとして使い回せる `Rule` 値
   - `.message(str)` でデフォルトメッセージを設定

4. **メソッドチェーンの返り値**
   - 各メソッドは `self`（`RuleBuilder`）を返すことでチェーンを実現

---

## Phase VAL-3: Validator 構築 API

### 目標

`Validator::new(Type).rule(...).rule_cross(...)` の流暢なビルダー API が ForgeScript から使えること。

### 実装ステップ

1. **`Validator::new(type_name)`**
   - 対象型の名前を文字列として受け取る（型チェックは将来拡張）
   - 空の `Validator` インスタンスを返す

2. **`.rule(field, rule_or_fn, message)`**
   - 引数が `Rule` 値または `fn(r) {...}` クロージャのどちらでも受け付ける
   - クロージャの場合は `RuleBuilder` を引数として呼び出し、結果を `FieldRule` に変換

3. **`.rule_cross(fn(f), fields, message)`**
   - `fn(f) { f.password == f.confirm_password }` 形式のクロージャを受け取る
   - `f` にはバリデーション対象のフォームオブジェクトを渡す
   - `fields: list<string>` でエラーに関連付けるフィールド名を指定

4. **`.rule_when(condition_fn, field, rule_fn, message)`**
   - `condition_fn(form) -> bool` が true の場合のみ `rule_fn` を適用
   - false の場合は `r.ok()` と等価（スキップ）

5. **`.rule_each(field, rule_fn, message)`**
   - フィールドの値が `list` の場合、各要素に `rule_fn` を適用
   - エラーの `fields` は `["tags[0]"]` 形式（インデックス付き）

6. **`.rule_nested(field, nested_validator)`**
   - 別の `Validator` インスタンスをネストして適用
   - エラーの `fields` は `["address.zip"]` 形式（ドット区切り）

---

## Phase VAL-4: バリデーション実行エンジン

### 目標

`v.validate(form)` / `v.validate_all(form)` が仕様通り `Result` を返すこと。

### 実装ステップ

1. **`validate(form)` — fail-fast**
   - 全ルール（単フィールド → クロス → 条件付き → リスト → ネスト）を順番に評価
   - 最初のエラーで `Err(ValidationError)` を返す
   - 成功時は `Ok(())` を返す

2. **`validate_all(form)` — 全収集**
   - 全ルールを評価してエラーを `Vec<ValidationError>` に収集
   - 少なくとも 1 件のエラーがあれば `Err(errors)` を返す
   - 成功時は `Ok(())` を返す

3. **フィールド値の取り出し**
   - `form` は `Value::Struct` または `Value::Map` を想定
   - フィールド名でインデックスして値を取得
   - フィールドが存在しない場合は `Value::Option(None)` と同等に扱う

4. **`data validate` との共存**
   - `data` の `validate` ブロックが存在する型への適用時は、`data validate` を先に実行
   - `data validate` 通過後のみ `Validator` のルールを適用（将来拡張: 現フェーズでは `Validator` 単体動作を優先）

5. **エラーの `value` フィールド**
   - 各エラーにデバッグ用の入力値を付与（`any?` 型）
   - `validate_all` 時は全エラーに値を格納

---

## Phase VAL-5: Anvil ミドルウェア統合

### 目標

`@validate(Form, using: validator)` デコレータがハンドラに適用され、バリデーション失敗時に自動で HTTP 422 を返すこと。

### 実装ステップ

1. **`@validate` デコレータのパーサー対応**
   - `forge-compiler` のパーサーに `@validate(Type, using: expr)` 構文を追加
   - ハンドラ関数の `Stmt::Fn` に `decorators` フィールドとして格納

2. **コード生成（ミドルウェア展開）**
   - `@validate` が付いたハンドラを Anvil のミドルウェアチェーンにラップするコードを生成
   - 展開後のイメージ:
     ```rust
     // @validate(RegistrationForm, using: registration_validator) を展開
     let __wrapped = anvil::validate_middleware(registration_validator, register_handler);
     ```

3. **`validate_middleware` ネイティブ関数**
   - `forge-vm` の Anvil 統合層に `validate_middleware(validator, handler)` を追加
   - リクエストボディをデシリアライズ → `validate_all()` → 失敗時 422 レスポンス
   - 422 レスポンスボディ: `{ "errors": [...] }` JSON 形式

4. **HTTP 422 レスポンスの構築**
   - `ValidationError` リストを JSON にシリアライズ
   - `Response::json({ errors: errors }).status(422)` と等価な Value を構築

5. **手動バリデーション（`@validate` なし）**
   - `validate_all()` の戻り値を `match` で分岐する使い方もサポート（VAL-4 で実装済み）

---

## Phase VAL-6: テスト・ドキュメント

### 目標

各フェーズのユニットテスト・統合テスト・E2E テストが揃い、spec の全サンプルが動作すること。

### 実装ステップ

1. **ユニットテスト（ルール単体）**
   - `required`・`min_length`・`max_length`・`min`・`max` の境界値テスト
   - `matches` の正規表現テスト（有効・無効パターン）
   - `one_of` のリストテスト
   - `custom` のカスタム関数テスト

2. **統合テスト（Validator フロー）**
   - 単フィールドルール + `validate()` / `validate_all()`
   - クロスフィールドルール + 複数エラー収集
   - 条件付きルール（条件 true / false の両方）
   - リスト要素バリデーション
   - ネスト struct バリデーション（ドット区切りフィールド名）
   - 名前付き `Rule` 定数の再利用

3. **E2E テスト（spec の完全版サンプル）**
   - `RegistrationForm` + `registration_validator` の完全バリデーション
   - Anvil ハンドラに `@validate` を付けて HTTP 422 が返ること
   - `validate_all()` 手動バリデーションの動作確認

4. **ForgeScript サンプルファイル**
   - `examples/validator/src/main.forge` — spec 完全版サンプル
   - `examples/validator/src/anvil_handler.forge` — Anvil 統合サンプル

---

## 実装優先順位と依存関係

```
VAL-1 (コア型)
  └─ VAL-2 (ルールビルダー)
       └─ VAL-3 (Validator 構築 API)
            └─ VAL-4 (実行エンジン)
                 ├─ VAL-5 (Anvil 統合) ← VAL-4 完了後
                 └─ VAL-6 (テスト)    ← 各フェーズと並行
```

## アーキテクチャメモ

- **クレート**: `crates/forge-validator/` を新設（`forge-vm` に依存）
- **モジュール登録**: `forge-vm` の `Interpreter::new()` 内で `register_validator_module()` を呼ぶ
- **`regex` クレート**: `crates/forge-validator/Cargo.toml` に追加
- **不透明オブジェクト**: `Validator` / `Rule` は `Value::NativeObject(Arc<dyn Any>)` として保持（`Rc` ではなく `Arc` で `Send` を確保）
- **エラーの `value` フィールド**: `any?` は ForgeScript の `Value::Option(Some(v))` にマップ
