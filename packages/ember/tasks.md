# Ember タスク一覧

> [ ] 未完了 / [x] 完了
>
> **ゴール**: ForgeScript で書いたブロック崩しが、ネイティブとブラウザ（WASM）の両方で動くこと。
>             E-4 完成でプレイ可能、E-5 完成で URL 共有できる。

---

## Phase E-0: ember-runtime クレート・ウィンドウ表示

### E-0-A: クレート新設

- [ ] `crates/ember-runtime/` ディレクトリと `Cargo.toml` を作成
- [ ] workspace の `Cargo.toml` に `ember-runtime` を追加
- [ ] 依存クレートを追加: `wgpu`, `winit`, `bytemuck`, `env_logger`
- [ ] feature flags 設定: `default = ["native"]`, `wasm = ["wasm-bindgen", "web-sys", "wasm-bindgen-futures", "js-sys"]`

### E-0-B: winit イベントループ

- [ ] `winit::EventLoop` + `winit::Window` でウィンドウを開く
- [ ] `WindowEvent::CloseRequested` / `Escape` キーで終了
- [ ] `WindowEvent::Resized` でビューポートを更新
- [ ] ウィンドウタイトル・サイズを `Ember::new()` から設定できる

### E-0-C: wgpu 初期化

- [ ] `wgpu::Instance` → `Surface` → `Adapter` → `Device` + `Queue` の初期化
- [ ] `SurfaceConfiguration` でスワップチェーンを設定（vsync あり）
- [ ] 背景色でクリアする `clear_pass` を実装
- [ ] フレームループ: `surface.get_current_texture()` → render → `present()`

### E-0-D: 動作確認

- [ ] ウィンドウが開き、指定背景色で塗りつぶされること
- [ ] ウィンドウを閉じるとプロセスが終了すること
- [ ] FPS が 60 前後で安定していること

---

## Phase E-1: ECS 基盤

### E-1-A: Entity / Component ストレージ

- [ ] `type EntityId = u64` を定義
- [ ] `struct World { entities: HashSet<u64>, components: HashMap<TypeId, HashMap<u64, Box<dyn Any + 'static>>> }` を実装
- [ ] `World::spawn()` → `EntityBuilder` を返す
- [ ] `EntityBuilder::with<C: Component>(component)` でコンポーネントを追加
- [ ] `EntityBuilder::build()` → `EntityId` を返す（`.with().with().build()` チェーン）
- [ ] `World::despawn(id)` — 全コンポーネントを削除
- [ ] `World::has_component<C>(id)` → `bool`

### E-1-B: クエリ

- [ ] `World::query<C>()` → `Iterator<(EntityId, &mut C)>` を実装（1コンポーネント）
- [ ] `World::query2<A, B>()` → `Iterator<(EntityId, &mut A, &mut B)>` を実装（2コンポーネント）
- [ ] `World::query3<A, B, C>()` → 3コンポーネントのクエリを実装
- [ ] `World::query_single<C>()` → 最初の1件（`Option<(EntityId, &mut C)>`）

### E-1-C: リソース（グローバル状態）

- [ ] `World::insert_resource<R>(r)` — 型ごとに1つのグローバル値を保存
- [ ] `World::resource<R>()` → `&R`
- [ ] `World::resource_mut<R>()` → `&mut R`

### E-1-D: System ループ

- [ ] `type System = Box<dyn Fn(&mut World)>` を定義
- [ ] `App { systems: Vec<System>, world: World }` を実装
- [ ] `App::add_system(fn)` でシステムを登録
- [ ] 毎フレーム全システムを順番に呼ぶ `App::tick()` を実装

### E-1-E: Time リソース

- [ ] `struct Time { delta: f32, elapsed: f32, fps: f32 }` を定義
- [ ] `World::insert_resource(Time::default())` で起動時に登録
- [ ] フレームごとに `time.delta` / `time.elapsed` / `time.fps` を更新
- [ ] `Time.delta()` としてグローバル関数風にアクセスできるよう ForgeScript から登録

---

## Phase E-2: 描画（矩形・円・テキスト）

### E-2-A: 2D レンダラー基盤

- [ ] `struct Renderer2D` を実装（wgpu パイプラインを保持）
- [ ] 頂点バッファ・インデックスバッファのダイナミック更新（毎フレーム構築）
- [ ] 座標系: 左上原点、Y 軸下向き、ピクセル単位

### E-2-B: 矩形描画

- [ ] 塗りつぶし矩形のシェーダー（WGSL）を実装
- [ ] `Renderer::draw_rect(x, y, w, h, color)` ネイティブ関数を登録
- [ ] `Rect { w, h, color }` コンポーネントを持つ Entity を自動描画する `draw_rects` システムを提供

### E-2-C: 円描画

- [ ] 円を三角形扇で近似（分割数 32）するシェーダーを実装
- [ ] `Renderer::draw_circle(x, y, radius, color)` を登録
- [ ] `Circle { radius, color }` コンポーネントを持つ Entity を自動描画する `draw_circles` システムを提供

### E-2-D: テキスト描画

- [ ] `ab_glyph` / `fontdue` クレートでビットマップフォントをラスタライズ
- [ ] `Renderer::draw_text(text, x, y, size, color)` を登録
- [ ] 文字列を矩形テクスチャに焼いて描画する

### E-2-E: `Color` 型

- [ ] `Color { r, g, b, a: f32 }` を定義
- [ ] `Color::white/black/red/green/cyan/yellow` ショートハンドを登録
- [ ] `Color::rgb(r, g, b)` / `Color::rgba(r, g, b, a)` を登録

---

## Phase E-3: 物理演算（Rapier2D）

### E-3-A: Rapier2D 統合

- [ ] `rapier2d` を依存クレートに追加
- [ ] `struct PhysicsWorld { gravity: Vec2, rigid_body_set, collider_set, ... }` を実装
- [ ] `PhysicsWorld::new().gravity(x, y)` ビルダーを実装
- [ ] 毎フレーム `PhysicsWorld::step(dt)` を呼ぶ

### E-3-B: 剛体・コライダー

- [ ] `DynamicBody { velocity: Vec2, restitution: f32, friction: f32 }` コンポーネントを実装
- [ ] `StaticBody {}` コンポーネントを実装（キネマティック剛体：パドル用）
- [ ] `Collider::rect(w, h)` を実装
- [ ] `Collider::circle(radius)` を実装
- [ ] Entity に `DynamicBody` / `StaticBody` + `Collider` が付いたとき、自動で Rapier に登録

### E-3-C: 位置同期

- [ ] Rapier の剛体位置を毎フレーム ECS の `Position` コンポーネントに書き戻す
- [ ] パドル（キネマティック）は ECS の `Position` 変更を Rapier に書き込む

### E-3-D: 衝突イベント

- [ ] `struct CollisionEvent { entity_a: EntityId, entity_b: EntityId }` を定義
- [ ] Rapier の衝突コールバックを ECS イベントキューに流す
- [ ] `system on_collision(event: CollisionEvent, world: World)` がイベントを受け取れる

---

## Phase E-4: 入力 + ブロック崩しデモ ★

### E-4-A: キーボード入力

- [ ] `struct InputState { keys_held: HashSet<Key>, keys_pressed: HashSet<Key>, keys_released: HashSet<Key> }` を実装
- [ ] `winit::WindowEvent::KeyboardInput` でステートを更新
- [ ] `Input::key_held(Key::Left)` / `Input::key_pressed(Key::Space)` を ForgeScript から呼べるよう登録
- [ ] 主要キー定数を登録: `Key::Left/Right/Up/Down/Space/Enter/Escape/A/D/W/S`

### E-4-B: `system` キーワードのコンパイラ対応

- [ ] `forge-compiler` のレキサーに `system` トークンを追加
- [ ] `system name(comp1: Type1, comp2: Type2) { ... }` をパースして `Stmt::System` に変換
- [ ] `Stmt::System` → `fn name(world: World) { for (comp1, comp2) in world.query2(Type1, Type2) { ... } }` に展開

### E-4-C: ブロック崩し実装

- [ ] `examples/breakout/src/main.forge` を作成
- [ ] コンポーネント定義: `Position, Rect, Circle, Paddle, Ball, Block, Wall, GameState`
- [ ] `setup(world)`: パドル・ボール・ブロック(8×6)・壁を生成
- [ ] `paddle_move`: Left/Right キーでパドル移動（画面端でクランプ）
- [ ] `block_hit`: ボール↔ブロック衝突でブロック削除・スコア加算
- [ ] `ball_out`: ボールが画面下端を抜けたら残機減少・ボールリセット
- [ ] `check_clear`: ブロックが0個でクリア判定
- [ ] `draw_rects` / `draw_circles` / `draw_hud` で描画
- [ ] `Ember::new()` にシステムを登録して `.run()`

### E-4-D: ゲームロジック完成確認

- [ ] パドルが左右に動くこと
- [ ] ボールが壁・パドルで正しく反射すること
- [ ] ブロックに当たったら消えてスコアが増えること
- [ ] 全ブロック消去でクリアメッセージが出ること
- [ ] ボールが底を抜けたら残機が減り、0でゲームオーバーになること

---

## Phase E-5: WASM ビルド対応

### E-5-A: WASM ターゲット設定

- [ ] `wasm-bindgen`, `web-sys`, `wasm-bindgen-futures`, `js-sys` を依存に追加（feature `wasm`）
- [ ] `wgpu` の WASM 設定: `features = ["webgpu"]`（または `webgl`）を追加
- [ ] `winit` の WASM 設定: `features = ["web-sys"]` を追加
- [ ] `#[cfg(target_arch = "wasm32")]` で native / WASM の初期化コードを分岐

### E-5-B: WASM エントリポイント

- [ ] `#[wasm_bindgen(start)]` pub fn を実装
- [ ] `web_sys::HtmlCanvasElement` を取得して wgpu Surface に接続
- [ ] `wasm_bindgen_futures::spawn_local` でイベントループを非同期実行
- [ ] `console_error_panic_hook` でパニック時のスタックトレースを devtools に出力

### E-5-C: `forge build --game --wasm` コマンド

- [ ] `forge-cli` に `--game` フラグを追加
- [ ] `--game` のみ: `cargo build --release` + バイナリをコピー
- [ ] `--game --wasm`: `wasm-pack build --target web` を呼び出す
- [ ] 出力先: `target/game-wasm/` に `index.html`, `*.wasm`, `*.js` を生成
- [ ] `index.html` テンプレートを作成（`<canvas id="ember">` を含む）

### E-5-D: ブロック崩し WASM 動作確認

- [ ] `forge build --game --wasm examples/breakout/` が成功すること
- [ ] `python -m http.server` で `target/game-wasm/` を配信してブラウザで動作確認
- [ ] キーボード入力・物理・描画が native と同等に動作すること
- [ ] 60 FPS で安定すること（Chrome DevTools で確認）

---

## 進捗サマリー

| フェーズ | 完了 / 全体 |
|---|---|
| E-0: クレート・ウィンドウ | 0 / 11 |
| E-1: ECS 基盤 | 0 / 16 |
| E-2: 描画 | 0 / 14 |
| E-3: 物理演算 | 0 / 12 |
| E-4: 入力 + ブロック崩し | 0 / 14 |
| E-5: WASM ビルド | 0 / 12 |
| **合計** | **0 / 79** |

> E-4 完成 = ブロック崩しがネイティブでプレイ可能
> E-5 完成 = ブラウザで URL 共有できる 🎮
