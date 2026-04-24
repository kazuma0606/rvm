# `event` / `emit` 実装タスク

> 参照: `lang/event/spec.md`, `lang/event/plan.md`
> 進捗: 0/54

---

## EVT-1: 言語拡張

### EVT-1-A: レキサー (0/3)

- [ ] `event` キーワードトークンを追加
- [ ] `emit` キーワードトークンを追加
- [ ] レキサーテスト: `event`, `emit` が正しくトークン化される

### EVT-1-B: AST (0/5)

- [ ] `Stmt::Event { name, fields, span }` を定義（`data` と同じ Field 構造を流用）
- [ ] `Stmt::Emit { event_name, fields, span }` を定義
- [ ] `fields: Vec<(String, Box<Expr>)>` の型を定義
- [ ] AST の Display / Debug 実装
- [ ] AST テスト: ノード構造が正しく構築される

### EVT-1-C: パーサー (0/6)

- [ ] `event PascalName { field: Type, ... }` のパース
- [ ] `emit EventName { key: expr, ... }` のパース
- [ ] 末尾カンマの許容
- [ ] パーサーテスト: event 宣言の正常ケース
- [ ] パーサーテスト: emit 文の正常ケース
- [ ] パーサーテスト: エラーケース（フィールド不足等）

### EVT-1-D: 型チェッカー (0/6)

- [ ] `event` 宣言をスコープにイベント型として登録（`EventType` マーカー付き）
- [ ] `emit EventName { ... }` 時に EventName が存在することを検証
- [ ] emit のフィールドと event 宣言の過不足検証
- [ ] emit の各フィールドの型一致検証
- [ ] 型チェッカーテスト: フィールド不一致エラー
- [ ] 型チェッカーテスト: 存在しない event 名エラー

### EVT-1-E: インタープリター（emit 評価） (0/4)

- [ ] `emit EventName { ... }` でフィールドを `Value::Struct` に組み立てる
- [ ] `RunContext.event_queue.push((event_name, value))` で EventQueue に積む
- [ ] `emit` の戻り値は `Value::Unit`
- [ ] インタープリターテスト: emit 後に EventQueue に積まれることを確認

---

## EVT-2: EventQueue ランタイム

### EVT-2-A: RunContext への統合 (0/3)

- [ ] `RunContext.event_queue: Vec<(String, Value)>` フィールドを追加
- [ ] RunContext 初期化時に空の EventQueue をセット
- [ ] EventQueue 統合テスト

### EVT-2-B: フラッシュ処理 (0/5)

- [ ] `run { }` ブロック完了後に EventQueue をフラッシュする処理を追加
- [ ] 各イベントに対して `@on(EventName)` ハンドラーを container から検索
- [ ] ハンドラーを登録順に呼び出す
- [ ] ハンドラーが `err(...)` を返した場合はログに記録して継続
- [ ] フラッシュ処理テスト: emit → ハンドラー呼び出しの順序確認

---

## EVT-3: `@on(Event)` との統合

### EVT-3-A: container からのハンドラー検索 (0/4)

- [ ] 既存 DI container から `@on(EventName)` が付いたメソッドを収集
- [ ] `EventDispatcher { event_name → Vec<handler_fn> }` マップを実装
- [ ] job 実行前に container を初期化して EventDispatcher を構築
- [ ] dispatcher テスト: @on ハンドラーが正しく登録される

### EVT-3-B: ハンドラー呼び出し規約 (0/3)

- [ ] ハンドラーのシグネチャ `fn handle(self, e: EventType) -> unit!` を検証
- [ ] `event` 宣言の型と `@on` 引数の型が一致することを検証
- [ ] 複数ハンドラーが登録された場合に登録順で呼ばれることを確認

---

## EVT-4: `Run.events` API

### EVT-4-A: RunContext へのアクセサ (0/5)

- [ ] `Run` グローバルオブジェクトをインタープリターに登録
- [ ] `Run.events.count(EventType)` — EventType 名の件数を返す
- [ ] `Run.events.count_all()` — 全イベント件数を返す
- [ ] `Run.events.all()` — 全イベントのリスト（list<any>）を返す
- [ ] `Run.id` — run_id 文字列を返す
- [ ] `Run.started_at` — ISO 8601 形式の開始時刻を返す

### EVT-4-B: テスト (0/2)

- [ ] `Run.events.count(RowInvalid)` が emit 件数と一致することを確認
- [ ] `Run.id` / `Run.started_at` の値が正しいことを確認

---

## EVT-5: `events.jsonl` 記録

### EVT-5-A: 直列化 (0/4)

- [ ] 各イベントを JSON Lines 形式にシリアライズ
- [ ] フィールドは `event` 宣言の順序で出力
- [ ] `emit` した順番（時系列）で記録
- [ ] `runs/<run_id>/events.jsonl` に書き出す

### EVT-5-B: `result.json` へのイベント集計 (0/2)

- [ ] `result.json` の `events` フィールドにイベント名ごとの件数を追加
- [ ] 集計テスト: `{"events": {"RowInvalid": 8, "ImportFinished": 1}}` 形式

---

## EVT-6: テスト・サンプル

### EVT-6-A: ユニットテスト (0/5)

- [ ] `event` 宣言のパーステスト
- [ ] `emit` 文のパーステスト
- [ ] フィールド不一致エラーのテスト
- [ ] 存在しないイベント名へのエラーテスト
- [ ] EventQueue の蓄積・フラッシュテスト

### EVT-6-B: 統合テスト (0/3)

- [ ] `emit` → `@on` ハンドラーが呼ばれることを確認
- [ ] `Run.events.count()` が正確な件数を返すことを確認
- [ ] `events.jsonl` の内容が正しいことを確認

### EVT-6-C: サンプル (0/2)

- [ ] `examples/job-demo/events/row_invalid.forge`
- [ ] `examples/job-demo/handlers/report_errors.forge`
