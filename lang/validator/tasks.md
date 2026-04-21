# ForgeScript validator タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: `use forge/validator.{ Validator, Rule }` で高機能バリデーション DSL が使えること。
>             クロスフィールド・全エラー収集・条件付き・リスト・ネスト検証をサポートし、
>             `@validate` デコレータで Anvil ミドルウェアと統合できること。

---

## Phase VAL-1: コア型定義

### VAL-1-A: クレート新設

- [ ] `crates/forge-validator/` ディレクトリと `Cargo.toml` を作成
- [ ] workspace の `Cargo.toml` に `forge-validator` を追加
- [ ] 依存クレートを追加: `forge-vm`、`regex`

### VAL-1-B: `ValidationError` 型

- [ ] Rust 側に `struct ValidationError { fields: Vec<String>, message: String, value: Option<Value> }` を定義
- [ ] `to_value()` で `Value::Struct` に変換するメソッドを実装
- [ ] ForgeScript 側の `data ValidationError { ... }` をモジュール登録時に定義

### VAL-1-C: `RuleChain` 型（内部）

- [ ] `type RuleCheck = Box<dyn Fn(&Value) -> Result<(), String>>` を定義
- [ ] `struct RuleChain { checks: Vec<RuleCheck>, default_message: Option<String> }` を実装
- [ ] `RuleChain::run(value)` — 全チェックを順番に評価して最初のエラーを返す

### VAL-1-D: `Rule` 型（名前付きルール用）

- [ ] `struct Rule { chain: RuleChain }` を定義
- [ ] `Rule::new()` コンストラクタをネイティブ関数として登録
- [ ] `.message(str)` — デフォルトメッセージを設定するメソッドを実装
- [ ] `Rule` を `Value` として保持できるよう `NativeObject` にラップ

### VAL-1-E: `Validator` 型

- [ ] 各ルール種別を表す enum/struct を定義
  - `FieldRule { field: String, chain: RuleChain, message: String }`
  - `CrossRule { fields: Vec<String>, predicate: ClosureValue, message: String }`
  - `WhenRule { condition: ClosureValue, field: String, chain: RuleChain, message: String }`
  - `EachRule { field: String, chain: RuleChain, message: String }`
  - `NestedRule { field: String, validator: Arc<Validator> }`
- [ ] `struct Validator { type_name: String, rules: Vec<ValidatorRule> }` を定義
- [ ] `Validator` を `NativeObject` にラップして `Value` として保持できるよう実装

### VAL-1-F: モジュールエントリポイント

- [ ] `pub fn register_validator_module(interp: &mut Interpreter)` を実装
- [ ] `forge-vm` の `Interpreter::new()` 内で `register_validator_module()` を呼ぶ
- [ ] `use forge/validator.{ Validator, Rule, ValidationError }` で解決できることを確認

---

## Phase VAL-2: ルールビルダー

### VAL-2-A: `RuleBuilder` オブジェクト

- [ ] `struct RuleBuilder { chain: RuleChain }` を定義
- [ ] `fn(r) { r.required() }` クロージャに渡せるよう `Value::NativeObject` として登録

### VAL-2-B: 基本制約メソッド

- [ ] `r.required()` — `Value::Option(None)` および空文字列 `""` を拒否
- [ ] `r.min_length(n)` — 文字列 / リストの長さが `n` 以上
- [ ] `r.max_length(n)` — 文字列 / リストの長さが `n` 以下
- [ ] `r.min(n)` — 数値が `n` 以上
- [ ] `r.max(n)` — 数値が `n` 以下
- [ ] `r.ok()` — 常に `Ok(())` （スキップ用）

### VAL-2-C: 正規表現・列挙・カスタム

- [ ] `r.matches(pattern)` — `regex::Regex` でパターンマッチ
- [ ] `r.one_of(list)` — 値がリストに含まれるか
- [ ] `r.custom(fn)` — `(value) -> Result<(), string>` クロージャを呼び出す

### VAL-2-D: メソッドチェーン

- [ ] 各メソッドが `self`（`RuleBuilder`）を返してチェーン可能にする
- [ ] `Rule::new()` でも同じメソッドチェーンが使えるよう `Rule` と `RuleBuilder` を統合

---

## Phase VAL-3: Validator 構築 API

### VAL-3-A: `Validator::new(type_name)`

- [ ] `Validator::new` をネイティブ関数として登録
- [ ] 型名を文字列で受け取り空の `Validator` を返す

### VAL-3-B: `.rule(field, rule_or_fn, message)`

- [ ] 第2引数が `fn(r) {...}` クロージャの場合: `RuleBuilder` を渡して評価し `FieldRule` を生成
- [ ] 第2引数が `Rule` 値の場合: `Rule` の `RuleChain` をそのまま使う
- [ ] `message` が省略された場合は `Rule::message()` のデフォルトを使う
- [ ] `self`（`Validator`）を返してチェーン可能にする

### VAL-3-C: `.rule_cross(fn(f), fields, message)`

- [ ] クロスフィールドクロージャ `fn(f) { f.a == f.b }` を `CrossRule` として登録
- [ ] `fields: list<string>` をエラーの `fields` に使用
- [ ] `self` を返す

### VAL-3-D: `.rule_when(condition_fn, field, rule_fn, message)`

- [ ] 条件クロージャ `fn(f) { f.payment_method == "credit" }` を `WhenRule` に登録
- [ ] 条件が false の場合はルールをスキップ
- [ ] `self` を返す

### VAL-3-E: `.rule_each(field, rule_fn, message)`

- [ ] リストフィールドの各要素に `rule_fn` を適用する `EachRule` を登録
- [ ] エラーの `fields` は `["tags[0]"]` 形式
- [ ] `self` を返す

### VAL-3-F: `.rule_nested(field, nested_validator)`

- [ ] ネスト `Validator` を `NestedRule` として登録
- [ ] エラーの `fields` はプレフィックスを付けて `["address.zip"]` 形式に変換
- [ ] `self` を返す

---

## Phase VAL-4: バリデーション実行エンジン

### VAL-4-A: フィールド値の取り出し

- [ ] `Value::Struct { fields }` から名前でフィールドを取得するヘルパーを実装
- [ ] フィールドが存在しない場合は `Value::Option(None)` として扱う

### VAL-4-B: `validate(form)` — fail-fast

- [ ] 単フィールドルール → クロスルール → 条件付きルール → リストルール → ネストルールの順に評価
- [ ] 最初のエラーで `Err(ValidationError)` を返す
- [ ] 全通過で `Ok(())` を返す
- [ ] `validate` をネイティブメソッドとして `Validator` に登録

### VAL-4-C: `validate_all(form)` — 全収集

- [ ] 全ルールを評価してエラーを `Vec<ValidationError>` に収集
- [ ] 1件以上のエラーがあれば `Err(list<ValidationError>)` を返す
- [ ] `validate_all` をネイティブメソッドとして登録

### VAL-4-D: エラーの `value` フィールド付与

- [ ] 各エラーにデバッグ用の入力値 `value: any?` を付与
- [ ] `validate()` では単一エラー、`validate_all()` では全エラーに付与

### VAL-4-E: `data validate` との共存

- [ ] `validate()` / `validate_all()` 実行前に `data validate` ブロックが存在する場合はそちらを先に実行
- [ ] `data validate` で失敗した場合は `Validator` のルールを実行しない

---

## Phase VAL-5: Anvil ミドルウェア統合

### VAL-5-A: `@validate` デコレータのパーサー対応

- [ ] `forge-compiler` のレキサーに `@validate` トークンを追加
- [ ] `@validate(Type, using: expr)` 構文をパーサーで解析
- [ ] ハンドラ関数の `Stmt::Fn` の `decorators` フィールドに格納

### VAL-5-B: ミドルウェア展開（コード生成）

- [ ] `@validate` デコレータをミドルウェアラップに変換するコード生成を実装
- [ ] 展開後: `Validator::validate_middleware(validator, handler)` として扱う

### VAL-5-C: `validate_middleware` ネイティブ関数

- [ ] `validate_middleware(validator, handler)` をネイティブ関数として実装
- [ ] リクエストボディを対象型にデシリアライズ
- [ ] `validate_all()` を実行し、失敗時は HTTP 422 を返す
- [ ] 成功時は元のハンドラを呼び出す

### VAL-5-D: HTTP 422 レスポンス

- [ ] `ValidationError` リストを `{ "errors": [...] }` JSON にシリアライズ
- [ ] `Response::json(...).status(422)` 相当の `Value` を構築

---

## Phase VAL-6: テスト

### VAL-6-A: ルール単体テスト

- [ ] テスト: `test_required` — `none` / 空文字列を拒否、値があれば通過
- [ ] テスト: `test_min_length` — 境界値（n-1/n/n+1）の正確な判定
- [ ] テスト: `test_max_length` — 境界値の正確な判定
- [ ] テスト: `test_min_max_number` — 数値の境界値判定
- [ ] テスト: `test_matches` — 有効・無効なメールアドレスパターン
- [ ] テスト: `test_one_of` — リスト内外の値
- [ ] テスト: `test_custom` — `ok(())` / `err(msg)` を返すカスタム関数

### VAL-6-B: Validator 統合テスト

- [ ] テスト: `test_validate_single_field` — 単フィールドルール + `validate()`
- [ ] テスト: `test_validate_all_collects_errors` — 複数違反の全収集
- [ ] テスト: `test_validate_cross_field` — パスワード一致チェック
- [ ] テスト: `test_validate_when` — 条件 true / false の両方
- [ ] テスト: `test_validate_each` — リスト要素の検証・エラーインデックス
- [ ] テスト: `test_validate_nested` — ネスト struct のドット区切りエラー
- [ ] テスト: `test_named_rule_reuse` — `Rule` 定数を複数 `Validator` で共有

### VAL-6-C: E2E テスト

- [ ] E2E テスト: `RegistrationForm` + `registration_validator` の完全バリデーション（spec 完全版サンプル）
- [ ] E2E テスト: `@validate` デコレータで HTTP 422 が返ること
- [ ] E2E テスト: `validate_all()` 手動バリデーションで全エラーが収集されること

### VAL-6-D: サンプルファイル

- [ ] `examples/validator/src/main.forge` — 仕様書の完全版サンプルを動作させる
- [ ] `examples/validator/src/anvil_handler.forge` — Anvil + `@validate` 統合サンプル

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| VAL-1: コア型定義 | 0 / 13 |
| VAL-2: ルールビルダー | 0 / 11 |
| VAL-3: Validator 構築 API | 0 / 14 |
| VAL-4: バリデーション実行エンジン | 0 / 9 |
| VAL-5: Anvil ミドルウェア統合 | 0 / 8 |
| VAL-6: テスト | 0 / 17 |
| **合計** | **0 / 72** |
