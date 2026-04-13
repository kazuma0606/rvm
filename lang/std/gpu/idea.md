# forge/std/gpu — GPU コンピュート

> 関連: `packages/ember/idea.md`（wgpu レンダリング統合）
> 関連: `lang/std/math/idea.md`（GPU 行列演算）
> 関連: `lang/std/plot/idea.md`（GPU 計算結果の可視化）
> 関連: `lang/std/ml/idea.md`（GPU 推論・学習）

---

## 動機

**「Rust でも GPU を普通に使いたい」**

Python + CUDA (PyTorch) が当たり前の世界に対して、
Rust から GPU を使うには wgpu / ash / cuda-rs などが必要で、
ハードルが高かった。

`forge/std/gpu` が目指すもの：
```
GPU ↔ CPU のデータ転送
カーネル（シェーダー）の記述
ディスパッチ・同期
```
この三つを ForgeScript で自然に書ける。

**同じコードがデスクトップとブラウザで動く：**
```
RTX 5070（Vulkan）     ─┐
Apple M4（Metal）      ─┤  wgpu → 共通コード
AMD RX 9070（Vulkan）  ─┤
Chrome（WebGPU）       ─┘
```

---

## バックエンド

| バックエンド | 動作環境 | RTX 5070 |
|---|---|---|
| Vulkan | Windows / Linux | ◎ 最速 |
| DirectX 12 | Windows | ◎ |
| Metal | macOS / iOS | — |
| WebGPU | Chrome / Firefox | △（ブラウザ経由） |
| OpenGL | レガシー | — |

RTX 5070（Blackwell GB205）は Vulkan 1.4 / DX12 Ultimate 対応。
wgpu は自動的に最適バックエンドを選択する。

---

## 三層 API 設計

```
高レベル API  ─ 行列演算・画像処理・ML カーネル（組み込み済み）
中レベル API  ─ カスタムコンピュートシェーダー（WGSL 記述）
低レベル API  ─ wgpu を直接操作（use raw {} ブロック）
```

---

## 高レベル API — GPU バッファと転送

### デバイス初期化

```forge
use forge/std/gpu.*

// GPU デバイスを取得（自動でベストバックエンドを選択）
let gpu = Gpu::init()?
display::text("GPU: {gpu.name()}")        // → "NVIDIA GeForce RTX 5070"
display::text("VRAM: {gpu.vram_mb()} MB") // → "12288 MB"
display::text("バックエンド: {gpu.backend()}")  // → "Vulkan"
```

### バッファ操作

```forge
// CPU → GPU バッファ転送
let data: list<f32> = (0..1024).map(|i| i as f32).collect()
let buf = gpu.buffer(data)                // GPU メモリに転送

// GPU → CPU 読み戻し
let result = buf.read()?                  // Vec<f32> として取得
display::text("合計: {result.sum()}")
```

---

## 高レベル API — 組み込みカーネル

### 行列演算（forge/std/math との統合）

```forge
use forge/std/gpu.*
use forge/std/math.*

// 行列積（GPU 上で実行）
let a = Matrix::from([[1.0, 2.0], [3.0, 4.0]])
let b = Matrix::from([[5.0, 6.0], [7.0, 8.0]])

let c = gpu.matmul(a, b)?
// → RTX 5070 のテンソルコアを使った高速行列積

// 大規模行列（ML 用途）
let w = Matrix::random(4096, 4096)   // 4096×4096
let x = Matrix::random(4096, 1)
let y = gpu.matmul(w, x)?            // CPU の数十倍速
```

### N体シミュレーション

```forge
// 重力 N体問題（全粒子ペア計算）
type Body = { pos: Vec3, vel: Vec3, mass: f32 }

let bodies: list<Body> = load_bodies("solarsystem.json")?

let sim = gpu.nbody(bodies,
    gravity:    6.674e-11,
    softening:  0.1,
    time_step:  0.01,
)

// 毎ステップ更新（60fps 相当）
loop {
    let frame = sim.step()?
    render(frame)
}
```

### 画像処理

```forge
use forge/std/gpu.*

let img = gpu.load_image("photo.jpg")?

// GPU 並列フィルタ
let blurred    = img.gaussian_blur(sigma: 2.0)?
let edges      = img.sobel_edge()?
let sharpened  = img.unsharp_mask(amount: 1.5)?

// カスタム畳み込み
let kernel = [[-1.0, -1.0, -1.0],
              [-1.0,  8.0, -1.0],
              [-1.0, -1.0, -1.0]]
let result = img.convolve(kernel)?

img.save("output.png")?
display::image(result)
```

### 並列リダクション

```forge
// GPU で大規模データの集計
let data = gpu.buffer((0..10_000_000).map(|i| i as f32))

let sum  = gpu.reduce(data, op: Reduce::Sum)?
let max  = gpu.reduce(data, op: Reduce::Max)?
let mean = sum / 10_000_000.0

display::text("合計: {sum}, 最大: {max}, 平均: {mean}")
```

---

## 中レベル API — カスタムコンピュートシェーダー

WGSL（WebGPU Shading Language）でカーネルを記述する。

```forge
use forge/std/gpu.*

// WGSL シェーダーをインライン定義
@compute(workgroup: [256, 1, 1])
shader fn vector_add(
    @group(0) @binding(0) a:      Buffer<f32, Read>,
    @group(0) @binding(1) b:      Buffer<f32, Read>,
    @group(0) @binding(2) output: Buffer<f32, Write>,
    @builtin(global_invocation_id) id: vec3<u32>,
) {
    let i = id.x
    output[i] = a[i] + b[i]
}

// ディスパッチ
let n = 1_000_000
let a = gpu.buffer(vec![1.0_f32; n])
let b = gpu.buffer(vec![2.0_f32; n])
let c = gpu.buffer_empty::<f32>(n)

gpu.dispatch(vector_add, bindings: [a, b, c], size: n)?

let result = c.read()?
display::text("result[0] = {result[0]}")  // → 3.0
```

### フィジックスシミュレーション（カスタムカーネル）

```forge
@compute(workgroup: [64, 1, 1])
shader fn sph_density(
    @group(0) @binding(0) positions: Buffer<vec3<f32>, Read>,
    @group(0) @binding(1) densities: Buffer<f32, Write>,
    @uniform mass: f32,
    @uniform h: f32,           // smoothing length
) {
    let i = global_id.x
    var density = 0.0_f32

    for j in 0..positions.len() {
        let r = length(positions[i] - positions[j])
        density += mass * poly6_kernel(r, h)
    }

    densities[i] = density
}

// 流体シミュレーション（SPH法）
let particles = gpu.buffer(initial_positions)
let densities = gpu.buffer_empty::<f32>(N)

for step in 0..STEPS {
    gpu.dispatch(sph_density,
        bindings: [particles, densities],
        uniforms: [mass, smoothing_h],
        size: N,
    )?
}
```

---

## 低レベル API — `use raw {}`

wgpu を直接操作したい場合の脱出ハッチ。

```forge
use raw {
    use wgpu::*

    // wgpu の Device / Queue を直接取得
    let (device, queue) = forge_gpu::raw_device();

    // カスタムパイプライン構築
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("my_pipeline"),
        layout: None,
        module: &shader_module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    // コマンドバッファ送信
    let mut encoder = device.create_command_encoder(&Default::default());
    // ... （wgpu raw API）
    queue.submit([encoder.finish()]);
}
```

---

## Ember との統合

レンダリング（描画）と コンピュート（物理・AI）が同一 GPU 上で動く。

```
┌─────────────────────────────────────────────────┐
│  RTX 5070（wgpu Vulkan バックエンド）            │
│                                                 │
│  Render Pass          Compute Pass              │
│  ├── Vertex Shader    ├── Physics（rapier GPU） │
│  ├── Fragment Shader  ├── Particle System        │
│  └── 描画結果         └── Enemy AI（推論）      │
└─────────────────────────────────────────────────┘
```

```forge
// Ember ゲームループでの統合
system PhysicsOnGpu {
    query(body: Body, transform: Transform) {
        // GPU コンピュートで N 体物理を一括処理
        let forces = gpu.nbody(bodies.positions(), G: 9.8)?
        for (body, force) in zip(bodies, forces) {
            body.vel += force * dt
        }
    }
}

system ParticleSystem {
    // パーティクル更新を GPU で並列実行
    gpu.dispatch(update_particles,
        bindings: [particle_buf],
        size: MAX_PARTICLES,
    )?
}
```

---

## ノートブック統合

```forge
use forge/std/gpu.*
use forge/std/plot.*
use forge/std/math.*

// 1. GPU で大規模行列の固有値計算
let matrix = Matrix::random(2048, 2048)
let eigenvalues = gpu.eigen(matrix)?

// 2. 結果をプロット
plot()
    .histogram(eigenvalues.real_parts(), bins: 50)
    .title("Marchenko-Pastur 分布（ランダム行列の固有値）")
    .show()

// 3. GPU での Monte Carlo π 計算
let n = 100_000_000    // 1億点

@compute(workgroup: [256, 1, 1])
shader fn monte_carlo_pi(
    @group(0) @binding(0) results: Buffer<u32, Write>,
    @uniform seed: u32,
) {
    let i = global_id.x
    let x = rand_f32(seed + i * 2)
    let y = rand_f32(seed + i * 2 + 1)
    results[i] = select(0u, 1u, x*x + y*y <= 1.0)
}

let hits = gpu.buffer_empty::<u32>(n)
gpu.dispatch(monte_carlo_pi, bindings: [hits], uniforms: [42u32], size: n)?
let pi_estimate = 4.0 * hits.read()?.sum() as f64 / n as f64
display::text("π ≈ {pi_estimate}")   // → π ≈ 3.14159...
```

---

## forge/std/ml との統合

```forge
use forge/std/gpu.*
use forge/std/ml.*

// GPU 上でニューラルネットワーク推論
let model = ml::load_model("model.onnx")?
let input = gpu.buffer(image_data)

// バッチ推論（GPU 並列）
let outputs = gpu.infer(model, input)?

// GPU 上での PCA（大規模データ）
let data = Matrix::random(100_000, 512)   // 10万件 × 512次元
let pca  = gpu.pca(data, components: 2)? // GPU で SVD

plot()
    .scatter(pca.col(0), pca.col(1), color: labels)
    .title("GPU PCA（10万点）")
    .show()
```

---

## 開発者体験

```toml
# forge.toml
[gpu]
backend = "auto"          # auto / vulkan / dx12 / metal / webgpu
prefer_dedicated = true   # 内蔵 GPU より外部 GPU を優先
```

```forge
// GPU 情報の確認
let info = gpu::info()
display::table([
    ["GPU",      info.name],           // NVIDIA GeForce RTX 5070
    ["VRAM",     "{info.vram_mb} MB"], // 12288 MB
    ["Backend",  info.backend],        // Vulkan
    ["Compute",  info.compute_units],  // 48 SM
    ["WGSL",     info.wgsl_version],   // 1.0
])
```

---

## Rust クレート設計

```
crates/
  forge-gpu/              ← コア（wgpu ラッパー）
    src/
      device.rs           ← Gpu::init(), バックエンド選択
      buffer.rs           ← GpuBuffer<T>, CPU↔GPU 転送
      compute.rs          ← dispatch(), shader! マクロ
      kernels/
        matmul.rs         ← 行列積（WGSL 組み込みカーネル）
        reduce.rs         ← リダクション（sum/max/min）
        image.rs          ← 画像処理カーネル群
        nbody.rs          ← N体シミュレーション
      wgsl/               ← 組み込み WGSL シェーダーファイル
```

---

## 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **G-0** | `Gpu::init()` + バッファ転送（CPU ↔ GPU）+ デバイス情報 |
| **G-1** | カスタムコンピュートシェーダー dispatch（vector_add デモ） |
| **G-2** | 組み込みカーネル：リダクション（sum/max/min） |
| **G-3** | 組み込みカーネル：行列積（テンソルコア活用） |
| **G-4** | N体シミュレーション カーネル |
| **G-5** | 画像処理カーネル群（blur / edge / convolve） |
| **G-6** | Ember レンダリングパイプラインとの統合 |
| **G-7** | WebGPU（ブラウザ）対応（WASM + WebGPU バックエンド） |
| **G-8** | forge/std/ml との統合（ONNX 推論、GPU PCA） |

**G-1 完成 = RTX 5070 で最初の自作カーネルが動く**
**G-3 完成 = Python + NumPy に相当する GPU 行列演算が揃う**
**G-6 完成 = Ember の物理・パーティクルが GPU 並列化される**

---

## 参考

- [wgpu](https://wgpu.rs/) — Rust 製クロスプラットフォーム GPU API
- [WGSL Spec](https://www.w3.org/TR/WGSL/) — WebGPU Shading Language 仕様
- [Blackwell Architecture](https://www.nvidia.com/en-us/geforce/graphics-cards/50-series/) — RTX 5000 系アーキテクチャ
- [learn-wgpu](https://sotrh.github.io/learn-wgpu/) — wgpu チュートリアル
- [wgpu-compute-tutorial](https://github.com/gfx-rs/wgpu/tree/trunk/examples) — wgpu 公式サンプル
