# forge/std/mq — インメモリメッセージキュー

> 関連: `lang/std/actor/idea.md`（Actor モデル）
> 関連: `packages/anvil/`（サーバーサイド Pub/Sub）
> 関連: `web-ui/idea.md`（Bloom Islands 間通信）

---

## 動機

RabbitMQ / Kafka は本番では強力だが：
- Erlang VM / JVM が必要 → ローカル開発が重い
- 設定ファイルが多い → 試すまでのコストが高い
- ブラウザでは動かない

**miniMQ の位置づけ：**

```
開発・テスト・小規模本番 → forge/std/mq（インメモリ）
本番スケール            → RabbitMQ / Kafka へ差し替え
ブラウザ内              → forge/std/mq（WASM + JS BroadcastChannel）
```

API は同じ。バックエンドだけ切り替わる。

---

## 設計思想

1. **インメモリ完結** — 外部プロセス不要、`MQ::new()` 一行で起動
2. **トピック木構造** — `orders.*` `payments.#` のようなワイルドカードルーティング
3. **WASM 対応** — ルーティングコアは pure Rust、I/O は JS シムに委譲
4. **型安全メッセージ** — `publish<T>` / `subscribe<T>` で型が合わないとコンパイルエラー
5. **Actor の基盤** — `forge/std/actor` はこの上に乗る

---

## コアアーキテクチャ

```
┌──────────────────────────────────────────────────────┐
│  forge-mq（Rust クレート、wasm32 対応）               │
│                                                      │
│  TopicRouter                                         │
│    ├── "orders.created"   → [Queue A, Queue B]       │
│    ├── "orders.cancelled" → [Queue A]                │
│    ├── "payments.*"       → [Queue C]                │
│    └── "#"                → [Dead Letter Queue]      │
│                                                      │
│  Queue（per subscriber）                             │
│    ├── priority: VecDeque<Message>                   │
│    ├── capacity: Option<usize>                       │
│    └── on_full: Block | Drop | DeadLetter            │
│                                                      │
│  Broker                                              │
│    ├── publish(topic, msg) → route → enqueue         │
│    ├── subscribe(pattern, handler)                   │
│    └── tick() → flush pending（WASM 用）             │
└──────────────────────────────────────────────────────┘

ネイティブ: tokio::sync::mpsc でリアルタイム配送
WASM:      JS BroadcastChannel / MessageChannel に委譲
```

---

## ForgeScript API

### 基本 Pub/Sub

```forge
use forge/std/mq.*

// ブローカー生成
let mq = MQ::new()

// 型付きメッセージ定義
type OrderCreated = { id: i64, amount: i32, item: str }
type OrderCancelled = { id: i64, reason: str }

// Publisher
mq.publish("orders.created", OrderCreated {
    id: 42, amount: 1000, item: "Widget"
})

// Subscriber（完全一致）
mq.subscribe<OrderCreated>("orders.created", |msg| {
    display::text("注文受信: #{msg.id} — ¥{msg.amount}")
})

// ワイルドカード（* = 一階層、# = 複数階層）
mq.subscribe<OrderCreated>("orders.*", |msg| {
    log::info("全注文イベント: {msg}")
})

mq.subscribe("payments.#", |msg: RawMessage| {
    log::debug("決済系イベント: {msg.topic}")
})
```

### 優先度キュー

```forge
// 高優先度メッセージ（先に処理される）
mq.publish("alerts.critical", alert, priority: Priority::High)
mq.publish("alerts.info",     notice, priority: Priority::Low)

// キュー設定
mq.queue("alerts.critical")
    .capacity(100)
    .on_full(OnFull::Block)       // 満杯時にブロック
    .priority(true)               // 優先度キュー有効
```

### Dead Letter Queue

```forge
// 処理失敗 or タイムアウトしたメッセージを DLQ へ
mq.subscribe<OrderCreated>("orders.created", |msg| {
    if !process_order(msg) {
        return Err("処理失敗")    // → DLQ へ自動転送
    }
})

// DLQ の監視
mq.subscribe_dlq(|msg: DeadLetter| {
    log::error("配送失敗 [{msg.topic}]: {msg.reason}")
    alert::notify("dead_letter", msg)
})
```

### リクエスト/レスポンスパターン

```forge
// RPC スタイル（reply-to パターン）
let result = mq.request<CalcRequest, CalcResponse>(
    topic: "calc.add",
    body:  CalcRequest { a: 10, b: 20 },
    timeout: 5s,
)?
display::text("結果: {result.value}")   // → 30

// レスポンダー側
mq.respond<CalcRequest, CalcResponse>("calc.add", |req| {
    CalcResponse { value: req.a + req.b }
})
```

### ファンアウト / ブロードキャスト

```forge
// 全サブスクライバーに同じメッセージを送る
mq.fanout("system.shutdown", ShutdownSignal { graceful: true })

// ラウンドロビン（ロードバランシング）
mq.publish("workers.task", task, delivery: Delivery::RoundRobin)
```

---

## WASM ブラウザ統合

### アーキテクチャ

```
┌──────────────────────────────────────────┐
│  forge-mq.wasm（ルーティングコア）        │
│  ・トピックマッチング                     │
│  ・優先度ソート                          │
│  ・フィルタリング                        │
│  ・シリアライズ / デシリアライズ          │
└──────────────┬───────────────────────────┘
               │ メッセージ配送のみ JS に委譲
┌──────────────▼───────────────────────────┐
│  forge-mq-browser.js（~80行のシム）      │
│                                          │
│  タブ間通信    → BroadcastChannel        │
│  Worker通信   → MessageChannel           │
│  常駐ブローカー → SharedWorker           │
│  同一ページ内  → queueMicrotask          │
└──────────────────────────────────────────┘
```

### ブラウザでの使用例

```forge
// Bloom Island A（カート）
use forge/std/mq.*

let mq = MQ::browser()    // BroadcastChannel バックエンド

@click
fn add_to_cart(item: Item) {
    mq.publish("cart.item_added", CartEvent { item })
}

// Bloom Island B（ヘッダーのカートアイコン）
mq.subscribe<CartEvent>("cart.*", |e| {
    cart_count += 1
    badge.update(cart_count)
})
```

### ユースケース

| シナリオ | 仕組み |
|---|---|
| Bloom Islands 間通信 | BroadcastChannel（同一オリジン） |
| Web Worker への指示 | MessageChannel |
| タブ間カート同期 | BroadcastChannel |
| オフラインキュー | IndexedDB に退避 → 復帰時 flush |
| Ember ゲームイベント | queueMicrotask（同一フレーム内） |

---

## ネットワーク越し MQ（オプション）

```
ネイティブ forge-mq
    ↓ オプションで TCP / WebSocket ブリッジを有効化
forge-mq-net（tokio + tungstenite）
    ↓
複数プロセス間 / サーバー ↔ ブラウザ 通信
```

```forge
// サーバー側（Anvil）
let mq = MQ::new()
    .with_websocket_bridge(port: 7878)   // ブラウザからも接続可

// ブラウザ側（Bloom）
let mq = MQ::connect("ws://localhost:7878")
mq.subscribe<PriceUpdate>("stock.*", |msg| { ... })
```

プロセス内 MQ → ネットワーク MQ が **コード変更なし** で切り替わる。

---

## 設定（forge.toml）

```toml
[mq]
backend = "memory"          # memory / websocket / rabbitmq
capacity = 10000            # デフォルトキューサイズ
dlq_enabled = true
dlq_ttl = "24h"

# バックエンド切り替え（本番）
# backend = "rabbitmq"
# url = "amqp://localhost:5672"
```

---

## Rust クレート設計

```
crates/
  forge-mq/           ← コア（wasm32 対応、I/O なし）
    src/
      broker.rs       ← Broker / TopicRouter
      queue.rs        ← Queue（優先度・DLQ）
      pattern.rs      ← トピックパターンマッチング
      message.rs      ← Message / DeadLetter 型
  forge-mq-async/     ← tokio バックエンド（native）
  forge-mq-browser/   ← WASM + JS BroadcastChannel シム
  forge-mq-net/       ← WebSocket ブリッジ（オプション）
```

`forge-mq` コアは `no_std` 互換を目指す（`alloc` のみ依存）。

---

## 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **Q-0** | `forge-mq` コア：TopicRouter + Queue + Broker（同期版） |
| **Q-1** | tokio 非同期バックエンド（`forge-mq-async`） |
| **Q-2** | 優先度キュー・Dead Letter Queue |
| **Q-3** | Request/Response パターン（RPC スタイル） |
| **Q-4** | WASM + BroadcastChannel シム（`forge-mq-browser`） |
| **Q-5** | WebSocket ブリッジ（`forge-mq-net`）|
| **Q-6** | RabbitMQ / Redis Streams アダプター（本番移行パス） |

**Q-1 完成 = Actor モデルの基盤が整う**
**Q-4 完成 = Bloom Islands 間通信が動く**

---

## 参考

- [tokio::sync::broadcast](https://docs.rs/tokio/latest/tokio/sync/broadcast/) — tokio のチャネル
- [BroadcastChannel API](https://developer.mozilla.org/en-US/docs/Web/API/BroadcastChannel) — ブラウザタブ間通信
- [AMQP トピック交換](https://www.rabbitmq.com/tutorials/tutorial-five-python) — ワイルドカードルーティング仕様
- [NATS](https://nats.io/) — 軽量 MQ の参考実装（Go 製）
