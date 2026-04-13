# Ember — ForgeScript ゲームエンジン構想

> パッケージ名: `ember`
> 命名: 鍛冶炉の残り火（Forge テーマ統一）
> 関連: `packages/bloom/`（ゲーム内 UI）、`forge/std/wasm`（WASM 実行）

---

## 位置づけ

ForgeScript エコシステムの「何でも作れる」を証明するデモパッケージ。

```
Anvil   → バックエンド（HTTP）
Crucible → データベース（PostgreSQL）
Bloom   → フロントエンド（Web UI）
Ember   → ゲーム・物理演算・グラフィックス  ← ここ
```

「Rust で出来ることは全部できる。もっと簡単に。」の最も分かりやすい証拠。

---

## コンセプト

### なぜ ForgeScript でゲームエンジンか

既存の選択肢:

| | Bevy | Unity | Godot | **Ember** |
|---|---|---|---|---|
| 言語 | Rust（難） | C# | GDScript | ForgeScript |
| 学習コスト | 高 | 中 | 低〜中 | 低 |
| WASM対応 | ○（experimental） | △ | ○ | **◎（ネイティブ設計）** |
| Web/Desktop 同一コード | △ | × | △ | **◎** |
| カスタム物理 | ○ | × | △ | **◎（use raw{}）** |

「Bevy より書きやすく、ブラウザで当たり前に動く」が差別化軸。

---

## 技術スタック

```
ForgeScript（Ember API）
    ↓
crates/ember-runtime/（Rust）
    ├── wgpu     — 描画（WebGPU / Vulkan / Metal / DX12）
    ├── rapier2d — 2D 物理演算
    ├── rapier3d — 3D 物理演算（将来）
    └── winit    — ウィンドウ・入力管理
```

### wgpu とは

プラットフォームを問わず GPU を叩ける Rust の統一グラフィックス API。

```
forge build --game         → ネイティブ（DirectX12 / Metal / Vulkan）
forge build --game --wasm  → ブラウザ（WebGPU）
```

同じ Ember のコードが Desktop と Browser 両方で動く。

### Rapier とは

Rust 製の 2D/3D 物理演算ライブラリ。WASM 対応済み。

- 剛体・コライダー（球・矩形・多角形・メッシュ）
- 重力・摩擦・反発係数
- 関節（ヒンジ・バネ・固定）
- 連続衝突検出（CCD）

---

## アーキテクチャ：ECS（Entity Component System）

Bevy と同様に ECS を採用するが、ForgeScript らしいシンプルな API に包む。

### ECS とは

```
Entity    → ゲームオブジェクトの ID（数値）
Component → データのみ（位置・速度・スプライト等）
System    → 毎フレームの処理ロジック
```

Rust の所有権モデルと相性が良く、並列処理も自然に書ける。

### ForgeScript API 設計

```forge
use ember.*

// コンポーネント定義
data Position { x: float, y: float }
data Velocity { dx: float, dy: float }
data Sprite   { texture: string, width: float, height: float }
data RigidBody { mass: float, restitution: float }

// システム（毎フレーム実行される処理）
system move_system(pos: Position, vel: Velocity) {
    pos.x += vel.dx * Time.delta()
    pos.y += vel.dy * Time.delta()
}

system bounce_system(pos: Position, vel: Velocity) {
    if pos.y > Screen.height() {
        vel.dy = -vel.dy * 0.8
    }
}

// ゲーム定義
let game = Ember::new()
    .title("My Game")
    .window(800, 600)
    .system(move_system)
    .system(bounce_system)
    .run()
```

### エンティティの生成

```forge
// スポーン
fn setup(world: World) {
    // ボールを10個生成
    for i in 0..10 {
        world.spawn()
            .with(Position { x: random_float(0.0, 800.0), y: 0.0 })
            .with(Velocity { dx: random_float(-100.0, 100.0), dy: 0.0 })
            .with(Sprite { texture: "ball.png", width: 32.0, height: 32.0 })
            .with(RigidBody { mass: 1.0, restitution: 0.7 })
    }
}

let game = Ember::new()
    .on_start(setup)
    .system(move_system)
    .run()
```

---

## 物理演算 API

```forge
use ember/physics.*

// 物理ワールドの設定
let physics = PhysicsWorld::new()
    .gravity(0.0, -9.8)

// 地面（静的剛体）
world.spawn()
    .with(Position { x: 400.0, y: 580.0 })
    .with(Collider::rect(800.0, 20.0))
    .with(StaticBody)

// ボール（動的剛体）
world.spawn()
    .with(Position { x: 400.0, y: 100.0 })
    .with(Collider::circle(16.0))
    .with(DynamicBody { mass: 1.0, restitution: 0.8, friction: 0.5 })
    .with(Sprite { texture: "ball.png", width: 32.0, height: 32.0 })
```

---

## 描画 API

```forge
use ember/render.*

// スプライト描画
system draw_sprites(pos: Position, sprite: Sprite) {
    Renderer::draw_sprite(sprite.texture, pos.x, pos.y, sprite.width, sprite.height)
}

// 図形描画（デバッグ用）
system draw_debug(pos: Position, col: Collider) {
    Renderer::draw_rect_outline(pos.x, pos.y, col.width, col.height, Color::green())
}

// カメラ
let camera = Camera2D::new()
    .follow(player_entity)
    .zoom(1.5)
```

---

## 入力 API

```forge
use ember/input.*

system player_input(vel: Velocity) {
    if Input::key_held(Key::Left)  { vel.dx = -200.0 }
    if Input::key_held(Key::Right) { vel.dx =  200.0 }
    if Input::key_pressed(Key::Space) { vel.dy = -400.0 }  // ジャンプ
    if Input::mouse_pressed(MouseButton::Left) {
        let pos = Input::mouse_pos()
        // クリック位置に弾を発射
    }
}
```

---

## `use raw {}` との連携

ForgeScript で書けない高度な処理は Rust に委譲できる。

```forge
// カスタムシェーダー
fn render_water(world: World) {
    use raw {
        // wgpu で直接 WGSL シェーダーを書く
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(include_str!("water.wgsl").into()),
        });
        // ...
    }
}

// カスタム物理（布シミュレーション等）
fn simulate_cloth(vertices: list<Position>) {
    use raw {
        // rapier の高度な API を直接使う
    }
}
```

---

## Bloom との統合（ゲーム内 UI）

```forge
use ember.*
use bloom.*

// ゲーム内 HUD（Bloom コンポーネントをゲーム画面に重ねる）
@island
component HUD {
    <div class="absolute top-4 left-4 text-white text-2xl font-bold">
        Score: {score}
    </div>
    <div class="absolute top-4 right-4">
        HP: {hp} / 100
    </div>

    <script>
        state score: number = 0
        state hp: number = 100
    </script>
}

let game = Ember::new()
    .window(800, 600, "My Game")
    .ui(HUD)          // Bloom コンポーネントを HUD としてオーバーレイ
    .system(game_logic)
    .run()
```

---

## Anvil との統合（マルチプレイヤー）

```forge
use ember.*
use anvil/websocket.*

// サーバーから位置同期
system sync_remote_players(pos: Position, network_id: NetworkId) {
    let state = ws.receive()?
    pos.x = state["x"]
    pos.y = state["y"]
}
```

---

## ビルドターゲット

```bash
# ネイティブデスクトップ
forge build --game

# ブラウザ（WASM + WebGPU）
forge build --game --wasm

# 将来: モバイル（Tauri Mobile 経由）
forge build --game --mobile
```

---

## デモシナリオ（GitHubスター獲得用）

優先度順:

| デモ | 内容 | 難度 | インパクト |
|---|---|---|---|
| **物理ボール** | 重力・反発するボールを100個 | 低 | ◎（動画映え） |
| **簡易プラットフォーマー** | 左右移動・ジャンプ・地形衝突 | 中 | ◎ |
| **パーティクルシステム** | 爆発・火花エフェクト | 中 | ○ |
| **ピンボール** | flipper・スコア・物理 | 中 | ◎ |
| **簡易シューティング** | 弾・敵・当たり判定 | 中 | ○ |

**最初のデモは「物理ボール」が最適。**
コードが短く、動画で「ぬるぬる動く物理演算」が一目で伝わる。

---

## 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **E-0** | `crates/ember-runtime/` 作成・wgpu ウィンドウ表示・wgpu 三角形描画 |
| **E-1** | ECS 基盤（Entity / Component / System ループ） |
| **E-2** | スプライト描画・テクスチャロード |
| **E-3** | Rapier2D 統合・重力・衝突検出 |
| **E-4** | 入力（キーボード・マウス） |
| **E-5** | WASM ビルド対応（`forge build --game --wasm`） |
| **E-6** | Bloom HUD 統合 |
| **E-7** | Anvil WebSocket 統合（マルチプレイヤー基盤） |

**E-3 完成 = 物理ボールデモが動く = 最初の公開チャンス**

---

## 参考

- [wgpu](https://wgpu.rs/) — クロスプラットフォーム GPU API
- [Rapier](https://rapier.rs/) — Rust 物理演算
- [Bevy](https://bevyengine.org/) — Rust ゲームエンジン（参考・競合ではない）
- [winit](https://github.com/rust-windowing/winit) — ウィンドウ管理
