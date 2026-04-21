# Ember — ForgeScript ゲームエンジン仕様書

> バージョン: 0.1.0
> 作成: 2026-04-21
> スコープ: E-0 〜 E-5（WASM ターゲットまで）
> 旗艦デモ: ブロック崩し（E-4 完成時点でプレイ可能・E-5 で URL 共有）

---

## 位置づけ

```
Anvil    → バックエンド（HTTP）
Crucible → データベース（PostgreSQL）
Bloom    → フロントエンド（Web UI）
Ember    → ゲーム・物理演算・グラフィックス  ← ここ
```

「Rust で出来ることは全部できる。もっと簡単に。」の最も分かりやすい証拠。
ForgeScript 製ブロック崩しを WASM でブラウザ上で動かし URL で共有できることがゴール。

---

## 技術スタック

```
ForgeScript（Ember API）
    ↓
crates/ember-runtime/（Rust）
    ├── wgpu     — 描画（WebGPU / Vulkan / Metal / DX12）
    ├── winit    — ウィンドウ・イベントループ
    ├── rapier2d — 2D 物理演算・衝突検出
    └── wasm-bindgen / web-sys  — WASM ターゲット時
```

### ビルドターゲット

```bash
# ネイティブデスクトップ（開発・テスト用）
forge build --game

# ブラウザ（WASM + WebGPU）← v0.1 最終ゴール
forge build --game --wasm
```

---

## アーキテクチャ：ECS（Entity Component System）

```
Entity    → ゲームオブジェクトの ID（u64）
Component → データのみ（Position・Velocity・Sprite 等）
System    → 毎フレームの処理ロジック（Component の組を受け取る関数）
World     → Entity・Component の保持と System の実行を管理
```

### ForgeScript ECS 構文

`system` キーワードは ECS クエリのシンタックスシュガー。
引数の型がそのままクエリ条件になる。

```forge
use ember.*

// コンポーネント定義（通常の data 型）
data Position { x: float, y: float }
data Velocity { dx: float, dy: float }
data Sprite   { texture: string, w: float, h: float }

// system キーワード：該当コンポーネントを持つ全 Entity に対して毎フレーム実行
system move_system(pos: Position, vel: Velocity) {
    pos.x += vel.dx * Time.delta()
    pos.y += vel.dy * Time.delta()
}

// ゲーム起動
let game = Ember::new()
    .title("My Game")
    .window(800, 600)
    .on_start(setup)
    .system(move_system)
    .run()
```

`system` の展開イメージ（コンパイラが生成）：

```forge
fn move_system(__world: World) {
    for (__pos, __vel) in __world.query(Position, Velocity) {
        let pos = __pos
        let vel = __vel
        pos.x += vel.dx * Time.delta()
        pos.y += vel.dy * Time.delta()
    }
}
```

---

## World API

```forge
// エンティティ生成
fn setup(world: World) {
    let ball = world.spawn()
        .with(Position { x: 400.0, y: 300.0 })
        .with(Velocity { dx: 200.0, dy: -300.0 })
        .with(Circle  { radius: 10.0, color: Color::white() })
        .with(Ball    {})

    let paddle = world.spawn()
        .with(Position { x: 360.0, y: 560.0 })
        .with(Rect    { w: 80.0, h: 12.0, color: Color::cyan() })
        .with(Paddle  {})
}

// クエリ（system 外で使う場合）
for (pos, vel) in world.query(Position, Velocity) {
    // ...
}

// エンティティ削除
world.despawn(entity_id)

// リソース（グローバル状態）
world.insert_resource(Score { value: 0 })
let score = world.resource(Score)
```

---

## 描画 API

wgpu を内部で使用。ForgeScript からは高レベル API のみ公開する。

### 矩形・円・テキスト

```forge
use ember/render.*

system draw_rects(pos: Position, rect: Rect) {
    Renderer::draw_rect(pos.x, pos.y, rect.w, rect.h, rect.color)
}

system draw_circles(pos: Position, circle: Circle) {
    Renderer::draw_circle(pos.x, pos.y, circle.radius, circle.color)
}

// テキスト描画（スコア表示等）
Renderer::draw_text("Score: {score}", 10.0, 10.0, 24.0, Color::white())
```

### コンポーネント型

```forge
data Rect   { w: float, h: float, color: Color }
data Circle { radius: float, color: Color }
data Color  { r: float, g: float, b: float, a: float }
```

`Color` のショートハンド：

```forge
Color::white()   // (1,1,1,1)
Color::black()   // (0,0,0,1)
Color::red()     // (1,0,0,1)
Color::green()   // (0,1,0,1)
Color::cyan()    // (0,1,1,1)
Color::rgb(r, g, b)      // a=1
Color::rgba(r, g, b, a)
```

### カメラ

```forge
let camera = Camera2D::default()  // 原点左上、Y軸下向き
```

v0.1 はカメラ移動なし（固定座標系）。

---

## 物理 API

Rapier2D を内部で使用。**重力は設定可能**（ブロック崩しでは `0.0`）。

```forge
use ember/physics.*

// 物理ワールド設定（Ember::new() に渡す）
let physics = PhysicsWorld::new()
    .gravity(0.0, 0.0)   // ブロック崩し：重力なし
    // .gravity(0.0, -9.8)  // プラットフォーマー：重力あり

// 動的剛体（ボール）
world.spawn()
    .with(Position  { x: 400.0, y: 300.0 })
    .with(Circle    { radius: 10.0, color: Color::white() })
    .with(DynamicBody { velocity: Vec2 { x: 200.0, y: -300.0 },
                        restitution: 1.0, friction: 0.0 })
    .with(Collider::circle(10.0))
    .with(Ball {})

// 静的剛体（壁・ブロック）
world.spawn()
    .with(Position  { x: 0.0, y: 300.0 })
    .with(StaticBody {})
    .with(Collider::rect(10.0, 600.0))
```

### 衝突イベント

```forge
system on_collision(event: CollisionEvent, world: World) {
    let (a, b) = event.entities()
    // ブロックを消す
    if world.has_component(b, Block) {
        world.despawn(b)
        let score = world.resource_mut(Score)
        score.value += 10
    }
}
```

---

## 入力 API

```forge
use ember/input.*

system paddle_input(pos: Position, paddle: Paddle) {
    let speed = 400.0
    if Input::key_held(Key::Left)  { pos.x -= speed * Time.delta() }
    if Input::key_held(Key::Right) { pos.x += speed * Time.delta() }
}

// マウス入力（将来）
Input::mouse_pos()            // Vec2
Input::mouse_pressed(MouseButton::Left)
```

### 主要キー定数

```forge
Key::Left / Key::Right / Key::Up / Key::Down
Key::Space
Key::Enter
Key::Escape
Key::A / Key::D / Key::W / Key::S
```

---

## Time API

```forge
Time.delta()        // 前フレームからの経過時間（秒、f32）
Time.elapsed()      // 起動からの経過時間（秒）
Time.fps()          // 現在の FPS
```

---

## 旗艦デモ：ブロック崩し

### コンポーネント構成

```forge
data Position  { x: float, y: float }
data Velocity  { dx: float, dy: float }
data Rect      { w: float, h: float, color: Color }
data Circle    { radius: float, color: Color }
data Paddle    {}
data Ball      {}
data Block     { hp: number }
data Wall      {}
data GameState { score: number, lives: number, cleared: bool }
```

### システム構成

```forge
// 入力
system paddle_move(pos: Position, _: Paddle)

// 物理（Rapier2D が担当）

// ブロック消去 + スコア加算
system block_hit(event: CollisionEvent, world: World)

// 画面外判定（ボールが底を抜けた）
system ball_out(pos: Position, _: Ball, world: World)

// 全ブロック消去 = クリア判定
system check_clear(world: World)

// 描画
system draw_rects(pos: Position, rect: Rect)
system draw_circles(pos: Position, circle: Circle)
system draw_hud(world: World)   // スコア・残機テキスト
```

### ゲームのセットアップ

```forge
fn setup(world: World) {
    world.insert_resource(GameState { score: 0, lives: 3, cleared: false })

    // パドル
    world.spawn()
        .with(Position { x: 360.0, y: 560.0 })
        .with(Rect     { w: 80.0, h: 12.0, color: Color::cyan() })
        .with(Collider::rect(80.0, 12.0))
        .with(StaticBody {})   // Rapier: キネマティック剛体
        .with(Paddle {})

    // ボール
    world.spawn()
        .with(Position { x: 400.0, y: 300.0 })
        .with(Circle   { radius: 8.0, color: Color::white() })
        .with(Collider::circle(8.0))
        .with(DynamicBody { velocity: Vec2 { x: 180.0, y: -260.0 },
                            restitution: 1.0, friction: 0.0 })
        .with(Ball {})

    // ブロック（8列 × 6行）
    for row in 0..6 {
        for col in 0..8 {
            let color = block_color(row)
            world.spawn()
                .with(Position { x: 60.0 + col * 88.0, y: 60.0 + row * 32.0 })
                .with(Rect     { w: 76.0, h: 24.0, color: color })
                .with(Collider::rect(76.0, 24.0))
                .with(StaticBody {})
                .with(Block { hp: 1 })
        }
    }

    // 壁（左・右・上）
    world.spawn().with(Position { x: -5.0,  y: 300.0 }).with(Collider::rect(10.0, 600.0)).with(StaticBody {}).with(Wall {})
    world.spawn().with(Position { x: 805.0, y: 300.0 }).with(Collider::rect(10.0, 600.0)).with(StaticBody {}).with(Wall {})
    world.spawn().with(Position { x: 400.0, y: -5.0  }).with(Collider::rect(800.0, 10.0)).with(StaticBody {}).with(Wall {})
}

let game = Ember::new()
    .title("Breakout — Ember Demo")
    .window(800, 600)
    .physics(PhysicsWorld::new().gravity(0.0, 0.0))
    .on_start(setup)
    .system(paddle_move)
    .system(block_hit)
    .system(ball_out)
    .system(check_clear)
    .system(draw_rects)
    .system(draw_circles)
    .system(draw_hud)
    .run()
```

---

## Ember::new() ビルダー

```forge
Ember::new()
    .title(str)                     // ウィンドウタイトル
    .window(width, height)          // ウィンドウサイズ（pixels）
    .physics(PhysicsWorld)          // 物理ワールド設定（省略時は物理なし）
    .on_start(fn(world: World))     // 初期化コールバック
    .system(fn)                     // System 登録（複数可）
    .background(Color)              // 背景色（省略時 Color::black()）
    .run()                          // イベントループ開始（ブロッキング）
```

---

## forge build --game / --game --wasm

### native ビルド

```bash
forge build --game [path]
# → target/game/<name>  （ELF / EXE / MachO）
```

### WASM ビルド

```bash
forge build --game --wasm [path]
# → target/game-wasm/
#     ├── index.html
#     ├── <name>.wasm
#     └── <name>.js
```

生成される `index.html` は `<canvas id="ember">` を持ち、`wasm-bindgen` で生成した JS で WASM をロードする。

---

## 将来の拡張（v0.2 以降）

| 機能 | フェーズ |
|---|---|
| Bloom HUD 統合（canvas + DOM オーバーレイ） | E-6 |
| Anvil WebSocket マルチプレイヤー | E-7 |
| スプライト・テクスチャロード | E-2.5 |
| オーディオ（rodio / web-audio） | E-8 |
| 3D / rapier3d | E-9 |
| `forge new --template game` | E-10 |

---

## Rust 実装方針

- `crates/ember-runtime/` を新設（workspace に追加）
- Feature flags: `default = ["native"]`、`wasm = ["wasm-bindgen", "web-sys"]`
- wgpu の surface は `winit::Window`（native）/ `web_sys::HtmlCanvasElement`（WASM）に接続
- ECS は外部クレートに依存せず自前実装（シンプルな `HashMap<TypeId, HashMap<u64, Box<dyn Any>>>`）
- Rapier2D の `RigidBodySet`・`ColliderSet` は `PhysicsWorld` が保持し、毎フレーム `step()` を呼ぶ
- ForgeScript から Ember API は `Interpreter::register_module("ember", ...)` で登録
- `system` キーワードは `forge-compiler` のパーサーが `fn(world: World)` に展開
