# ForgeScript validator タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `use forge/validator.{ Validator, Rule }` で高機能バリデーション DSL が使えること。
>             クロスフィールド・全エラー収集・条件付き・リスト・ネスト検証をサポートし、
>             `@validate` デコレータで Anvil ミドルウェアと統合できること。

---

## Phase VAL-1: コア型定義

### VAL-1-A: クレート新設

- [x] `crates/forge-validator/` ディレクトリと `Cargo.toml` を作成
- [x] workspace の `Cargo.toml` に `forge-validator` を追加
- [x] 依存クレートを追加: `forge-vm`、`regex`

### VAL-1-B: `ValidationError` 型

- [x] Rust 側に `struct ValidationError { fields: Vec<String>, message: String, value: Option<Value> }` を定義
- [x] `to_value()` で `Value::Struct` に変換するメソッドを実装
- [x] ForgeScript 側の `data ValidationError { ... }` をモジュール登録時に定義

### VAL-1-C: `RuleChain` 型（内部）

- [x] `type RuleCheck = Box<dyn Fn(&Value) -> Result<(), String>>` を定義
- [x] `struct RuleChain { checks: Vec<RuleCheck>, default_message: Option<String> }` を実装
- [x] `RuleChain::run(value)` — 全チェックを順番に評価して最初のエラーを返す

### VAL-1-D: `Rule` 型（名前付きルール用）

- [x] `struct Rule { chain: RuleChain }` を定義
- [x] `Rule::new()` コンストラクタをネイティブ関数として登録
- [x] `.message(str)` — デフォルトメッセージを設定するメソッドを実装
- [x] `Rule` を `Value` として保持できるよう `NativeObject` にラップ

### VAL-1-E: `Validator` 型

- [x] 各ルール種別を表す enum/struct を定義
  - `FieldRule { field: String, chain: RuleChain, message: String }`
  - `CrossRule { fields: Vec<String>, predicate: ClosureValue, message: String }`
  - `WhenRule { condition: ClosureValue, field: String, chain: RuleChain, message: String }`
  - `EachRule { field: String, chain: RuleChain, message: String }`
  - `NestedRule { field: String, validator: Arc<Validator> }`
- [x] `struct Validator { type_name: String, rules: Vec<ValidatorRule> }` を定義
- [x] `Validator` を `NativeObject` にラップして `Value` として保持できるよう実装

### VAL-1-F: モジュールエントリポイント

- [x] `pub fn register_validator_module(interp: &mut Interpreter)` を実装
- [x] `forge-vm` の `Interpreter::new()` 内で `register_validator_module()` を呼ぶ
- [x] `use forge/validator.{ Validator, Rule, ValidationError }` で解決できることを確認

---

## Phase VAL-2: ルールビルダー

### VAL-2-A: `RuleBuilder` オブジェクト

- [x] `struct RuleBuilder { chain: RuleChain }` を定義
- [x] `fn(r) { r.required() }` クロージャに渡せるよう `Value::NativeObject` として登録

### VAL-2-B: 基本制約メソッド

- [x] `r.required()` — `Value::Option(None)` および空文字列 `""` を拒否
- [x] `r.min_length(n)` — 文字列 / リストの長さが `n` 以上
- [x] `r.max_length(n)` — 文字列 / リストの長さが `n` 以下
- [x] `r.min(n)` — 数値が `n` 以上
- [x] `r.max(n)` — 数値が `n` 以下
- [x] `r.ok()` — 常に `Ok(())` （スキップ用）

### VAL-2-C: 正規表現・列挙・カスタム

- [x] `r.matches(pattern)` — `regex::Regex` でパターンマッチ
- [x] `r.one_of(list)` — 値がリストに含まれるか
- [x] `r.custom(fn)` — `(value) -> Result<(), string>` クロージャを呼び出す

### VAL-2-D: メソッドチェーン

- [x] 各メソッドが `self`（`RuleBuilder`）を返してチェーン可能にする
- [x] `Rule::new()` でも同じメソッドチェーンが使えるよう `Rule` と `RuleBuilder` を統合

---

## Phase VAL-3: Validator 構築 API

### VAL-3-A: `Validator::new(type_name)`

- [x] `Validator::new` をネイティブ関数として登録
- [x] 型名を文字列で受け取り空の `Validator` を返す

### VAL-3-B: `.rule(field, rule_or_fn, message)`

- [x] 第2引数が `fn(r) {...}` クロージャの場合: `RuleBuilder` を渡して評価し `FieldRule` を生成
- [x] 第2引数が `Rule` 値の場合: `Rule` の `RuleChain` をそのまま使う
- [x] `message` が省略された場合は `Rule::message()` のデフォルトを使う
- [x] `self`（`Validator`）を返してチェーン可能にする

### VAL-3-C: `.rule_cross(fn(f), fields, message)`

- [x] クロスフィールドクロージャ `fn(f) { f.a == f.b }` を `CrossRule` として登録
- [x] `fields: list<string>` をエラーの `fields` に使用
- [x] `self` を返す

### VAL-3-D: `.rule_when(condition_fn, field, rule_fn, message)`

- [x] 条件クロージャ `fn(f) { f.payment_method == "credit" }` を `WhenRule` に登録
- [x] 条件が false の場合はルールをスキップ
- [x] `self` を返す

### VAL-3-E: `.rule_each(field, rule_fn, message)`

- [x] リストフィールドの各要素に `rule_fn` を適用する `EachRule` を登録
- [x] エラーの `fields` は `["tags[0]"]` 形式
- [x] `self` を返す

### VAL-3-F: `.rule_nested(field, nested_validator)`

- [x] ネスト `Validator` を `NestedRule` として登録
- [x] エラーの `fields` はプレフィックスを付けて `["address.zip"]` 形式に変換
- [x] `self` を返す

---

## Phase VAL-4: バリデーション実行エンジン

### VAL-4-A: フィールド値の取り出し

- [x] `Value::Struct { fields }` から名前でフィールドを取得するヘルパーを実装
- [x] フィールドが存在しない場合は `Value::Option(None)` として扱う

### VAL-4-B: `validate(form)` — fail-fast

- [x] 単フィールドルール → クロスルール → 条件付きルール → リストルール → ネストルールの順に評価
- [x] 最初のエラーで `Err(ValidationError)` を返す
- [x] 全通過で `Ok(())` を返す
- [x] `validate` をネイティブメソッドとして `Validator` に登録

### VAL-4-C: `validate_all(form)` — 全収集

- [x] 全ルールを評価してエラーを `Vec<ValidationError>` に収集
- [x] 1件以上のエラーがあれば `Err(list<ValidationError>)` を返す
- [x] `validate_all` をネイティブメソッドとして登録

### VAL-4-D: エラーの `value` フィールド付与

- [x] 各エラーにデバッグ用の入力値 `value: any?` を付与
- [x] `validate()` では単一エラー、`validate_all()` では全エラーに付与

### VAL-4-E: `data validate` との共存

- [x] `validate()` / `validate_all()` 実行前に `data validate` ブロックが存在する場合はそちらを先に実行
- [x] `data validate` で失敗した場合は `Validator` のルールを実行しない

---

## Phase VAL-5: Anvil ミドルウェア統合

### VAL-5-A: `@validate` デコレータのパーサー対応

- [x] `forge-compiler` のレキサーに `@validate` トークンを追加
- [x] `@validate(Type, using: expr)` 構文をパーサーで解析
- [x] ハンドラ関数の `Stmt::Fn` の `decorators` フィールドに格納

### VAL-5-B: ミドルウェア展開（コード生成）

- [x] `@validate` デコレータをミドルウェアラップに変換するコード生成を実装
- [x] 展開後: `Validator::validate_middleware(validator, handler)` として扱う

### VAL-5-C: `validate_middleware` ネイティブ関数

- [x] `validate_middleware(validator, handler)` をネイティブ関数として実装
- [x] リクエストボディを対象型にデシリアライズ
- [x] `validate_all()` を実行し、失敗時は HTTP 422 を返す
- [x] 成功時は元のハンドラを呼び出す

### VAL-5-D: HTTP 422 レスポンス

- [x] `ValidationError` リストを `{ "errors": [...] }` JSON にシリアライズ
- [x] `Response::json(...).status(422)` 相当の `Value` を構築

---

## Phase VAL-6: テスト

### VAL-6-A: ルール単体テスト

- [x] テスト: `test_required` — `none` / 空文字列を拒否、値があれば通過
- [x] テスト: `test_min_length` — 境界値（n-1/n/n+1）の正確な判定
- [x] テスト: `test_max_length` — 境界値の正確な判定
- [x] テスト: `test_min_max_number` — 数値の境界値判定
- [x] テスト: `test_matches` — 有効・無効なメールアドレスパターン
- [x] テスト: `test_one_of` — リスト内外の値
- [x] テスト: `test_custom` — `ok(())` / `err(msg)` を返すカスタム関数

### VAL-6-B: Validator 統合テスト

- [x] テスト: `test_validate_single_field` — 単フィールドルール + `validate()`
- [x] テスト: `test_validate_all_collects_errors` — 複数違反の全収集
- [x] テスト: `test_validate_cross_field` — パスワード一致チェック
- [x] テスト: `test_validate_when` — 条件 true / false の両方
- [x] テスト: `test_validate_each` — リスト要素の検証・エラーインデックス
- [x] テスト: `test_validate_nested` — ネスト struct のドット区切りエラー
- [x] テスト: `test_named_rule_reuse` — `Rule` 定数を複数 `Validator` で共有

### VAL-6-C: E2E テスト

- [x] E2E テスト: `RegistrationForm` + `registration_validator` の完全バリデーション（spec 完全版サンプル）
- [x] E2E テスト: `@validate` デコレータで HTTP 422 が返ること
- [x] E2E テスト: `validate_all()` 手動バリデーションで全エラーが収集されること

### VAL-6-D: サンプルファイル

- [x] `examples/validator/src/main.forge` — 仕様書の完全版サンプルを動作させる
- [x] `examples/validator/src/anvil_handler.forge` — Anvil + `@validate` 統合サンプル

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| VAL-1: コア型定義 | 13 / 13 |
| VAL-2: ルールビルダー | 11 / 11 |
| VAL-3: Validator 構築 API | 14 / 14 |
| VAL-4: バリデーション実行エンジン | 13 / 13 |
| VAL-5: Anvil ミドルウェア統合 | 11 / 11 |
| VAL-6: テスト | 19 / 19 |
| **合計** | **81 / 81** |
