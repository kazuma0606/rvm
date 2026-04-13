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

## エコシステム系

### Forge Package Registry
`npm` / `crates.io` の ForgeScript 版。**言語が本当に普及するかどうかはここで決まる。**

- `forge add` / `forge publish` でパッケージ管理
- セマンティックバージョニング・依存解決
- GitHub の星より「誰かがパッケージを公開し始めた」が普及の証拠
- forge.toml でバージョン固定

```bash
forge add forge/std/math        # 公式パッケージ
forge add kazuma/my-lib@1.2.0   # コミュニティパッケージ
forge publish                   # 自分のパッケージを公開
```

---

### Forge Playground（Web REPL）
ブラウザで即試せる環境。**布教ツールとして最強。**

- URL を開くだけで ForgeScript が動く（WASM で実現可能）
- コードを書いてリンクをシェアできる
- README のサンプルコードが全部「実行可能」になる
- Rust Playground / Kotlin Playground と同じ戦略
- **技術的には今の forge-vm を wasm32 にコンパイルするだけで作れる**

---

## データ処理系

### forge/std/data — DataFrame / データパイプライン
Python の pandas 相当。**実務ユーザーが一番欲しがる領域。**

- CSV / JSON / Parquet / Excel 読み書き
- filter / group_by / agg / sort / join
- GPU と統合 → 大規模データ処理が高速
- ノートブックとの相性が抜群
- `display::table` で即可視化

```forge
let df = DataFrame::read_csv("sales.csv")?

df.filter(|row| row["amount"] > 1000)
  .group_by("region")
  .agg(["amount": Agg::Sum, "count": Agg::Count])
  .sort_by("amount", desc: true)
  |> display::table
```

---

### forge/std/search — 組み込みフルテキスト検索
[Tantivy](https://github.com/quickwit-oss/tantivy)（Rust 製 Lucene）のラッパー。

- SQLite に検索機能を足したいときに即使える
- Anvil に組み込めばサイト内検索が即実装できる
- ElasticSearch 不要で全文検索が動く
- 日本語形態素解析（lindera）との統合も視野

```forge
let index = SearchIndex::new("./my_index")?
index.add({ id: 1, title: "ForgeScript入門", body: "..." })?

let results = index.search("ForgeScript GPU")?
for hit in results {
    display::text("{hit.score:.2} — {hit.title}")
}
```

---

## 「流れ」を扱う系

### forge/std/reactive — リアクティブストリーム / シグナル
Bloom の内部モデルの基盤にもなる。SolidJS のシグナルに近い概念。

- シグナル（状態の最小単位）・derived（派生値）・effect（副作用）
- `forge/std/actor` と組み合わせると分散リアクティブシステム
- Bloom の UI 更新ロジックをこの上に乗せられる
- RxJS / Solid / Svelte の「なぜ動くか」を ForgeScript で学べる

```forge
let count  = signal(0)
let double = derived(|| count.get() * 2)

effect(|| {
    display::text("count={count.get()}, double={double.get()}")
})

count.set(5)   // → double が自動再計算、effect が自動再実行
```

---

### forge/std/shell — シェルスクリプト置き換え
bash スクリプトを型安全な ForgeScript で書く。

- パイプでデータを型付きで渡せる（bash との最大の差別化）
- `forge run deploy.forge` で実行
- **自分たちのリリーススクリプトを ForgeScript で書くと説得力がある**
- git / ssh / fs / http を標準で統合

```forge
// deploy.forge — bash の代わりに
let branch = git::current_branch()?
if branch != "main" { exit(1) }

sh("cargo build --release")?
ssh::copy("target/release/forge", "deploy@server:/usr/local/bin/forge")?
ssh::run("deploy@server", "systemctl restart forge")?
display::text("デプロイ完了 ✓")
```

---

## ビジュアル系

### forge/std/flow — ノードベースビジュアルプログラミング
TouchDesigner / Node-RED の ForgeScript 版。ノートブック内で動く。

- ノードをつなぐだけでデータパイプラインが作れる
- 非プログラマーへの入口になる
- 内部は普通の ForgeScript コードに変換される
- `forge/std/data` と統合 → ノーコードデータ分析

```
[CSV読み込み] → [フィルタ] → [グループ集計] → [グラフ表示]
     ↓               ↓              ↓               ↓
  DataFrame      DataFrame      DataFrame        Plot
```

---

### forge/std/graph — グラフ理論・ネットワーク分析
[petgraph](https://github.com/petgraph/petgraph) ラッパー＋可視化。

- Dijkstra / BFS / DFS / トポロジカルソート / 最小全域木
- 中心性分析（媒介・近接・固有ベクトル）
- `display::plot_network` でグラフを可視化
- `forge/std/sim` と組み合わせるとネットワーク上のシミュレーション
- 応用: SNS・依存関係・ルート探索・サプライチェーン・知識グラフ

```forge
let g = Graph::new()
let a = g.node("Tokyo")
let b = g.node("Osaka")
g.edge(a, b, weight: 513.0)

let path = g.dijkstra(a, b)?
display::text("最短距離: {path.cost} km")
display::plot_network(g, node_size: g.betweenness_centrality())
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

### 短期で「使える・刺さる」もの
```
Forge Playground    ← forge-vm を wasm32 化するだけ。マーケティング効果が即効
forge/std/shell     ← 自分たちのデプロイスクリプトを今すぐ ForgeScript で書ける
forge/std/data      ← 実務ユーザーが一番欲しい。pandas の代替
forge/std/search    ← Anvil サイト内検索に即使える
```

### 中期：基盤が揃ったら自然につながるもの
```
forge/std/proto     ← MQ・Actor がすでにあるので通信フォーマットが欲しくなる
forge/std/reactive  ← Bloom の内部モデル基盤
forge/std/graph     ← data + sim と組み合わせると強い
forge/std/embed     ← forge-kernel の no_std が整ったら自然な次のステップ
forge/std/canvas    ← ノートブック + 数学ライブラリが揃ったら映えるデモになる
forge/std/audio     ← canvas と同時期、ノートブック統合で個性が出る
forge/std/flow      ← data + canvas が揃ったらノーコード入口になる
```

### 長期：数学ライブラリが先行条件
```
forge/std/quantum   ← math M-3（行列・線形代数）以降
forge/std/sim       ← GPU + Actor が整ったら
forge/std/zk        ← math の有限体・楕円曲線が先行条件
forge/std/consensus ← MQ のネットワーク版が先行条件
```

### エコシステム：どこかのタイミングで必須
```
Forge Package Registry  ← 言語の普及はここで決まる
forge/std/macro         ← 言語が安定してから
```
