# 構想メモ — 未着手アイデア集

> 実装フェーズに入る前の構想置き場。
> 詳細設計が必要になったら `lang/std/*/idea.md` や `packages/*/idea.md` に昇格させる。

---

## システム系

### forge/std/embed — 組み込み / IoT
Rust のもう一つの本拠地。ESP32 / Raspberry Pi Pico で ForgeScript が動く。

- `forge build --target thumbv7em-none-eabi`（ARM Cortex-M）
- `@no_std @embedded` アトリビュートでベアメタルモード
- GPIO / UART / SPI / I2C / BLE の抽象化
- センサー・モーター・ディスプレイ制御
- カーネル（forge-kernel）の no_std 対応が先行条件
- **「アプリからマイコンまで同じ言語」の証明になる**

```forge
@no_std @embedded
fn main() -> ! {
    let led = gpio::pin(25).output()
    loop { led.toggle(); sleep(500ms) }
}
```

---

### forge/std/consensus — 分散合意（Raft）
Actor / MQ の上に乗る分散調整レイヤー。Anvil クラスタリングの基盤。

- Raft リーダー選出・ログ複製・スナップショット
- `forge/std/mq` の WebSocket ブリッジと統合
- Anvil の水平スケールアウトに直結
- miniMQ のネットワーク版ブローカーとしても使える

```forge
let cluster = Raft::new(["node1:7878", "node2:7878", "node3:7878"])
cluster.propose(Command::Set { key: "x", value: "42" })?
```

---

### forge/std/proto — シリアライゼーション
Protocol Buffers / MessagePack / Cap'n Proto の ForgeScript 統合。

- `@proto` アトリビュートで型定義 → エンコーダ/デコーダ自動生成
- MQ・Actor・ネットワーク通信の共通フォーマット
- ゼロコピーデシリアライズ（Cap'n Proto）
- GPU バッファ転送形式としても活用可能

```forge
@proto
type Packet = { id: u32, payload: bytes, timestamp: u64 }
let buf = packet.encode()
let pkt = Packet::decode(buf)?
```

---

## 数理・科学系

### forge/std/quantum — 量子回路シミュレーション
`forge/std/math` と最も相性が良い領域。ノートブックで量子アルゴリズムを可視化。

- 量子ビット・ゲート（H / CNOT / Toffoli / QFT）
- 状態ベクトルシミュレーション（密度行列も）
- Grover 探索・Shor 因数分解・量子フーリエ変換
- 確率分布を `display::plot` で可視化
- GPU（行列積）と統合 → 大規模回路シミュレーション

```forge
let qc = Circuit::new(qubits: 4)
qc.h(0..4)
qc.oracle(target: 6)
qc.grover_diffusion()
let result = qc.simulate(shots: 1000)
display::bar(result.probabilities())
```

---

### forge/std/sim — エージェントベースシミュレーション
個体から積み上げる社会・生態・物理シミュレーション。

- `agent` キーワードで個体の状態と行動を定義
- `World` が全エージェントを管理・ステップ実行
- GPU と統合 → 数万エージェントがリアルタイムで動く
- Ember と統合 → シミュレーションを 3D 可視化
- 応用: SIR（感染症）・群衆行動・経済・生態系・交通

```forge
agent Person {
    state { status: SirStatus, x: f64, y: f64 }
    tick(dt) { /* 移動・感染判定 */ }
}
let world = World::new(agents: 10_000)
world.run(steps: 365)
plot().line(world.history("SIR")).show()
```

---

### forge/std/zk — ゼロ知識証明
証明したい事実だけを証明し、それ以外は何も漏らさない暗号技術。

- Schnorr 証明・Groth16・PLONK などのプロトコル
- `forge/std/math` の有限体・楕円曲線と直結
- 認証・プライバシー保護・信頼性証明
- ブロックチェーン不要の「検証可能な計算」
- **数学ライブラリが揃ったあとの自然な発展先**

```forge
let prover = ZkProver::new(secret: 42)
let proof  = prover.prove(|x| x * x == 1764)?
let valid  = ZkVerifier::verify(proof, |x| x * x == 1764)
// → true（x=42 を知っていることが証明された、ただし 42 は漏れない）
```

---

## クリエイティブ系

### forge/std/canvas — クリエイティブコーディング
p5.js / Processing の ForgeScript 版。ノートブックで動くインタラクティブキャンバス。

- `Canvas::new()` → `draw()` ループ
- 図形・色・変換・アニメーション
- `display::canvas` でノートブックにインライン表示
- 数学可視化ツールとしても機能（リサージュ・フラクタル・カオス）
- WebGL / wgpu バックエンドで GPU 描画も可能

```forge
let c = Canvas::new(800, 600)
c.draw(|frame| {
    frame.background(Color::black())
    for i in 0..200 {
        let t = frame.time() + i as f64 * 0.1
        let x = 400.0 + (t * 1.3).sin() * 200.0
        let y = 300.0 + (t * 0.7).cos() * 150.0
        frame.circle(x, y, radius: 3.0, color: Color::hsl(i * 2, 100, 60))
    }
})
c.run()
```

---

### forge/std/audio — 音声合成・ライブコーディング音楽
cpal + rodio 基盤。ノートブックで TidalCycles 的なライブコーディング。

- オシレーター（sine / saw / square / noise）
- フィルター（lowpass / highpass / bandpass）
- エンベロープ（ADSR）・エフェクト（reverb / delay / chorus）
- リズムパターン DSL
- FFT → `display::plot` でスペクトル可視化
- `forge/std/math` とのパイプライン（信号処理・フーリエ解析）

```forge
let synth = osc::saw(220.0)
    |> filter::lowpass(cutoff: 800.hz(), resonance: 0.7)
    |> envelope::adsr(0.01s, 0.1s, 0.6, 0.3s)
    |> reverb::hall(mix: 0.3)
    |> audio::out()

let beat = pattern!(kick: "x...x...x...x...", bpm: 120)
beat.play()
```

---

## メタ系

### forge/std/macro — マクロシステム
Rust の手続きマクロ相当を ForgeScript で実現。コンパイル時コード生成。

- `@derive(Debug, Serialize)` でボイラープレート自動生成
- `@sql` `@html` `@proto` 等の DSL マクロ
- `forge/std/crucible` の型安全クエリビルダに活用
- Bloom のテンプレートエンジン基盤にもなる

```forge
@derive(Debug, Serialize, Deserialize)
type User = { name: str, age: i32 }

@sql
let users = SELECT * FROM users WHERE age > 18
```

---

## 優先度メモ

着手するなら以下の順が自然：

```
forge/std/proto   ← MQ・Actor がすでにあるので通信フォーマットが欲しくなる
forge/std/embed   ← forge-kernel の no_std が整ったら自然な次のステップ
forge/std/canvas  ← ノートブックと数学ライブラリが揃ったら映えるデモになる
forge/std/audio   ← canvas と同時期、ノートブック統合で個性が出る
forge/std/quantum ← 数学ライブラリ（M-3 以降）が先行条件
forge/std/sim     ← GPU + Actor が整ったら面白くなる
forge/std/zk      ← 数学ライブラリの有限体・楕円曲線が先行条件
forge/std/consensus ← MQ のネットワーク版が先行条件
```
