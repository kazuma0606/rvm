# Ember — 実装計画

> 仕様: `packages/ember/spec.md`
> スコープ: E-0 〜 E-5（WASM ターゲットまで）
> 旗艦デモ: ブロック崩し（E-4 でプレイ可能・E-5 で URL 共有）

---

## フェーズ構成

```
Phase E-0: ember-runtime クレート・ウィンドウ表示
Phase E-1: ECS 基盤（Entity / Component / System / World）
Phase E-2: 2D 描画（矩形・円・テキスト）
Phase E-3: 物理演算（Rapier2D 統合）
Phase E-4: 入力 + ブロック崩しデモ  ← ★プレイ可能マイルストーン
Phase E-5: WASM ビルド対応           ← ★URL 共有マイルストーン
```

---

## Phase E-0: ember-runtime クレート・ウィンドウ表示

### 目標

`crates/ember-runtime/` を新設し、空のウィンドウが開いて背景色で塗りつぶされること。

### 実装ステップ

1. **クレート新設**
   - `crates/ember-runtime/Cargo.toml` を作成
   - workspace に追加
   - 依存: `wgpu`, `winit`, `bytemuck`, `env_logger`
   - feature flags: `default = ["native"]` / `wasm = [...]`

2. **winit イベントループ**
   - `EventLoop::new()` + `WindowBuilder` でウィンドウを生成
   - `Event::WindowEvent::CloseRequested` / Escape で終了
   - `Event::MainEventsCleared` をフレームトリガーとして使用
   - `WindowEvent::Resized` で surface を再設定

3. **wgpu 初期化シーケンス**
   ```
   Instance → Surface → Adapter（prefer HighPerformance）
   → Device + Queue → SurfaceConfiguration（Bgra8UnormSrgb, Fifo）
   ```
   - `surface.configure(&device, &config)` でスワップチェーンを設定

4. **クリアパス**
   - `device.create_command_encoder()` → `begin_render_pass()` → 背景色クリア
   - `queue.submit()` → `surface_texture.present()`

5. **`Ember::new()` ビルダー骨格**
   - `.title()` / `.window()` / `.background()` のオプション保持
   - `.run()` でイベントループ開始（ブロッキング）

### アーキテクチャメモ

- `GpuContext { device, queue, surface, config }` を一つの struct にまとめる
- `App { gpu: GpuContext, world: World, systems: Vec<System> }` がトップレベル

---

## Phase E-1: ECS 基盤

### 目標

`World::spawn().with(Position{...}).with(Velocity{...})` でエンティティを生成し、
`World::query2(Position, Velocity)` でコンポーネントを取得・更新できること。

### 実装ステップ

1. **Entity ストレージ**
   ```rust
   struct World {
       next_id: u64,
       entities: HashSet<u64>,
       components: HashMap<TypeId, HashMap<u64, Box<dyn Any + 'static>>>,
       resources: HashMap<TypeId, Box<dyn Any + 'static>>,
   }
   ```

2. **`EntityBuilder` パターン**
   ```rust
   impl World {
       fn spawn(&mut self) -> EntityBuilder<'_> { ... }
   }
   struct EntityBuilder<'w> {
       id: u64,
       world: &'w mut World,
   }
   impl EntityBuilder<'_> {
       fn with<C: 'static>(self, c: C) -> Self { ... }
       fn build(self) -> EntityId { self.id }
   }
   ```
   `.with()` がチェーン後に `build()` を省略できるよう `Drop` でも ID を確定させる。

3. **クエリ実装**
   - `query<A>()`: `TypeId::of::<A>()` でマップを引いてイテレータを返す
   - `query2<A, B>()`: A を基準に走査し、B も持つ Entity だけを返す
   - 安全性: クエリ中に `despawn` は呼ばない（フレーム末にまとめて処理）

4. **リソース**
   ```rust
   fn insert_resource<R: 'static>(&mut self, r: R)
   fn resource<R: 'static>(&self) -> &R
   fn resource_mut<R: 'static>(&mut self) -> &mut R
   ```

5. **System ループ**
   ```rust
   type System = Box<dyn Fn(&mut World)>;
   ```
   `App::tick()` で `systems` を順番に呼び出す。
   毎フレーム最初に `Time` リソースを更新してから実行。

6. **`Time` リソース**
   ```rust
   struct Time { pub delta: f32, pub elapsed: f32, pub fps: f32 }
   ```
   `Instant::now()` で前フレームからの差分を計算。

---

## Phase E-2: 2D 描画

### 目標

矩形・円・テキストを画面に描画できること。ブロック崩しの全ビジュアルが揃う。

### 実装ステップ

1. **`Renderer2D` 設計**
   - 毎フレーム頂点バッファを CPU 側で構築し `queue.write_buffer()` で転送
   - 描画コマンドを `DrawCommand` のリストとして蓄積し、最後にまとめて `draw_indexed()` する
   - ソートなし（登録順に描画）

2. **矩形パイプライン**
   ```wgsl
   // vertex.wgsl
   struct VertexInput { @location(0) pos: vec2<f32>, @location(1) color: vec4<f32> }
   @vertex fn vs_main(in: VertexInput) -> ...
   @fragment fn fs_main(...) -> @location(0) vec4<f32> { return in.color; }
   ```
   - 矩形を 2 枚の三角形（6 頂点 or インデックスバッファ）で描画

3. **円パイプライン**
   - 分割数 32 の扇形三角形で近似（中心 + 32 辺頂点 → 32 三角形）
   - 矩形と同じシェーダーを流用

4. **テキスト描画**
   - `fontdue` クレートでビットマップラスタライズ（依存追加）
   - グリフをアトラステクスチャに焼き、文字ごとに矩形で描画
   - `draw_text(text, x, y, size, color)` の高レベル API

5. **座標変換**
   - ForgeScript 側: 左上原点・ピクセル単位・Y 軸下向き
   - wgpu 側: NDC（-1〜1）に変換する `uniforms` バッファ（`window_size` のみ）
   - `transform_uniform = [[2/w, 0, -1], [0, -2/h, 1]]`

6. **ForgeScript バインド**
   - `Renderer::draw_rect(x, y, w, h, color)` をネイティブ関数として登録
   - `Renderer::draw_circle(x, y, radius, color)` を登録
   - `Renderer::draw_text(text, x, y, size, color)` を登録
   - `draw_rects` / `draw_circles` システムをデフォルトで提供

---

## Phase E-3: 物理演算（Rapier2D）

### 目標

ボールが壁・パドル・ブロックと正しく衝突反射し、衝突イベントを ECS で受け取れること。

### 実装ステップ

1. **Rapier2D 統合**
   - `rapier2d` を依存に追加
   - `PhysicsWorld` が `RigidBodySet`, `ColliderSet`, `PhysicsPipeline` 等を保持
   - 毎フレーム `physics_world.step(dt)` を呼ぶ（System ループの前に実行）

2. **Entity ↔ Rapier のマッピング**
   - `EntityPhysicsMap: HashMap<EntityId, (RigidBodyHandle, ColliderHandle)>` を管理
   - `DynamicBody` / `StaticBody` + `Collider` コンポーネントが追加されたとき自動登録
   - 登録タイミング: `on_start` 完了後・フレーム開始前に `sync_new_bodies()` を実行

3. **位置同期（双方向）**
   - Rapier → ECS: `DynamicBody` の剛体位置を `Position` に書き戻す
   - ECS → Rapier: `StaticBody`（パドル）の `Position` 変更を Rapier のキネマティック剛体に書き込む

4. **反射設定**
   - ボールは `restitution = 1.0`, `friction = 0.0` で完全反射
   - 壁・ブロックは `restitution = 1.0`, `friction = 0.0`
   - Rapier の `CoefficientCombineRule::Max` で合成

5. **衝突イベント**
   - `rapier2d::pipeline::EventHandler` を実装して衝突ペアを取得
   - ECS の `EventQueue<CollisionEvent>` リソースに積む
   - System 側: `world.resource_mut(EventQueue<CollisionEvent>).drain()` で取得

---

## Phase E-4: 入力 + ブロック崩し

### 目標

キーボードでパドルを操作し、ブロック崩しが完全にプレイできること。

### 実装ステップ

1. **InputState リソース**
   ```rust
   struct InputState {
       keys_held:     HashSet<KeyCode>,
       keys_pressed:  HashSet<KeyCode>,
       keys_released: HashSet<KeyCode>,
   }
   ```
   - `winit::WindowEvent::KeyboardInput` を受け取ってステートを更新
   - フレーム末に `keys_pressed` / `keys_released` をクリア

2. **ForgeScript バインド**
   - `Input::key_held(Key::Left)` → `InputState.keys_held.contains(KeyCode::Left)`
   - `Input::key_pressed(Key::Space)` → `keys_pressed.contains(...)`
   - `Key::Left/Right/Up/Down/Space/Enter/Escape/A/D/W/S` 定数を登録

3. **`system` キーワードのコンパイラ対応**
   - `forge-compiler` のレキサーに `system` トークンを追加
   - `system name(c1: T1, c2: T2) { ... }` → `Stmt::System { name, params, body }` にパース
   - `Stmt::System` → `fn name(__world: World) { for (__c1, __c2) in __world.query2(T1, T2) { ... } }` に展開
   - パラメータが 1 個のとき `query<T1>()`, 3 個のとき `query3<T1, T2, T3>()` を使う

4. **ブロック崩し実装**
   - `examples/breakout/src/main.forge` を spec の完全版サンプルに従って作成
   - 8×6 のブロックグリッドを生成（行ごとに色を変える）
   - ゲーム状態: `score`, `lives`, `cleared` をリソースで管理
   - クリア・ゲームオーバー時に `draw_hud` でメッセージ表示

5. **動作確認チェックリスト**
   - パドルが左右に動く（画面端でクランプ）
   - ボールが壁・パドルで正しく反射する
   - ブロックに当たったら消えてスコアが増える
   - 全ブロック消去でクリアメッセージ
   - ボールが底を抜けたら残機減少・ボールリセット・残機 0 でゲームオーバー

---

## Phase E-5: WASM ビルド対応

### 目標

`forge build --game --wasm examples/breakout/` で生成した HTML をブラウザで開いてプレイできること。

### 実装ステップ

1. **feature フラグ分岐**
   ```toml
   [features]
   default = ["native"]
   native = []
   wasm = ["wasm-bindgen", "web-sys", "wasm-bindgen-futures", "js-sys"]

   [dependencies]
   wgpu = { features = ["webgpu"] }
   winit = { features = ["web-sys"] }
   ```

2. **WASM エントリポイント**
   ```rust
   #[cfg(target_arch = "wasm32")]
   #[wasm_bindgen(start)]
   pub fn wasm_main() {
       console_error_panic_hook::set_once();
       wasm_bindgen_futures::spawn_local(async {
           run_app().await;
       });
   }
   ```

3. **Surface の分岐**
   ```rust
   #[cfg(target_arch = "wasm32")]
   let surface = {
       let canvas = web_sys::window()
           .and_then(|w| w.document())
           .and_then(|d| d.get_element_by_id("ember"))
           .and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok())
           .expect("canvas#ember not found");
       instance.create_surface_from_canvas(canvas)
   };
   ```

4. **イベントループの WASM 対応**
   - `winit` の WASM ターゲットは `EventLoop::run()` が `!` を返さない
   - `event_loop.spawn()` を使ってブラウザのイベントループに乗る

5. **`forge build --game --wasm` コマンド**
   - `forge-cli` に `--game` / `--game --wasm` フラグを追加
   - `--game --wasm` 時: `wasm-pack build --target web --out-dir target/game-wasm/pkg`
   - `index.html` テンプレートをコピー + WASM モジュール名を埋め込む

6. **index.html テンプレート**
   ```html
   <!DOCTYPE html>
   <html>
   <head><title>{{title}}</title></head>
   <body style="margin:0;background:#000;">
     <canvas id="ember" width="{{width}}" height="{{height}}"></canvas>
     <script type="module">
       import init from './pkg/{{name}}.js';
       init();
     </script>
   </body>
   </html>
   ```

---

## 実装優先順位と依存関係

```
E-0 (ウィンドウ)
  └─ E-1 (ECS)
       ├─ E-2 (描画)          ← E-1 完了後に並行可能
       └─ E-3 (物理)          ← E-1 完了後に並行可能
            └─ E-4 (入力 + ブロック崩し)  ← E-2・E-3 完了後
                 └─ E-5 (WASM)            ← E-4 完了後
```

E-2 と E-3 は独立しているため並行開発できる。

---

## アーキテクチャ全体図

```
ForgeScript (examples/breakout/src/main.forge)
    │  use ember.*
    ▼
forge-vm (Interpreter)
    │  ネイティブ関数として登録
    ▼
crates/ember-runtime/
    ├── app.rs         — Ember::new() ビルダー・App 構造体
    ├── ecs/
    │   ├── world.rs   — World・EntityBuilder・query
    │   ├── system.rs  — System 型・実行ループ
    │   └── time.rs    — Time リソース
    ├── render/
    │   ├── gpu.rs     — GpuContext（wgpu 初期化）
    │   ├── renderer.rs — Renderer2D・DrawCommand
    │   └── text.rs    — fontdue テキスト描画
    ├── physics/
    │   ├── world.rs   — PhysicsWorld（Rapier2D ラッパー）
    │   └── events.rs  — CollisionEvent・EventQueue
    ├── input/
    │   └── state.rs   — InputState・Key 定数
    └── wasm/
        └── entry.rs   — #[wasm_bindgen(start) エントリポイント
```

---

## 技術的なリスクと対策

| リスク | 対策 |
|---|---|
| WebGPU のブラウザサポート不足 | Chrome 113+ を動作確認対象とし、未対応ブラウザには「Chrome を使ってください」を表示 |
| wgpu の WASM ビルドエラー | wgpu 0.20+ の `webgpu` feature を使用。WebGL フォールバック（`webgl` feature）を予備として保持 |
| winit の WASM イベントループ仕様変更 | winit 0.29+ の `web-sys` feature を追跡 |
| Rapier2D の WASM 対応 | rapier2d は公式に WASM サポート済み。WASM ターゲットでも `rapier2d` の同一 crate を使う |
| `fontdue` のビットマップ品質 | 日本語不要・ASCII のみなら fontdue で十分。必要なら `ab_glyph` に差し替え |
