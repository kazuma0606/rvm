# forge/std/actor — アクターモデル

> 関連: `lang/std/mq/idea.md`（forge-mq がメッセージ基盤）
> 関連: `packages/anvil/`（サーバー分散処理）
> 関連: `packages/ember/idea.md`（ゲームエンティティ管理）

---

## 動機

Erlang/Elixir の OTP、Akka（JVM）が提供するアクターモデルを
ForgeScript ネイティブで実現する。

**アクターモデルの本質：**
```
状態はアクターの内部にだけある
アクター間の通信はメッセージのみ
→ 共有状態なし → ロックなし → スケールしやすい
```

**Rust における現状：**
- `actix` — 高機能だが重い、Web フレームワークと一体化
- `bastion` — 開発停滞
- `kameo` — 新しいが ForgeScript との統合がない

`forge/std/actor` は `forge/std/mq` の上に薄く乗せる設計。

---

## 設計思想

1. **MQ ベース** — `forge/std/mq` のキューがアクターの受信箱
2. **軽量** — Erlang のような数百万アクターを目指す（tokio task per actor）
3. **スーパーバイザーツリー** — 障害隔離と自動再起動
4. **ローカル ↔ リモート透過** — 同じ API でプロセス内/ネットワーク越しを切替
5. **型安全** — メッセージ型はコンパイル時に検証

---

## コアコンセプト

```
Actor
  ├── Address（他のアクターへの参照、send のみ可能）
  ├── Mailbox（MQ の Queue）
  ├── State（アクター内部の状態）
  ├── handle()（メッセージ処理）
  └── Supervisor（障害時の再起動ポリシー）
```

---

## ForgeScript API

### アクターの定義

```forge
use forge/std/actor.*

// メッセージ型定義
type Deposit  = { amount: i32 }
type Withdraw = { amount: i32 }
type Balance  = {}    // 残高照会リクエスト

// アクター定義
actor BankAccount {
    state {
        balance: i32 = 0,
        owner:   str,
    }

    // メッセージハンドラ
    on Deposit(msg) {
        self.balance += msg.amount
        log::info("{self.owner}: 入金 ¥{msg.amount} → 残高 ¥{self.balance}")
    }

    on Withdraw(msg) {
        if self.balance >= msg.amount {
            self.balance -= msg.amount
        } else {
            self.send_error(InsufficientFunds { balance: self.balance })
        }
    }

    // リクエスト/レスポンス
    ask Balance -> i32 {
        self.balance
    }
}
```

### アクターの起動・通信

```forge
// システム起動
let system = ActorSystem::new()

// アクター生成
let account = system.spawn(BankAccount {
    owner: "田中太郎"
})

// 非同期メッセージ送信（fire and forget）
account.send(Deposit { amount: 10000 })
account.send(Withdraw { amount: 3000 })

// 結果を待つ（ask パターン）
let balance = account.ask(Balance {}).await?
display::text("残高: ¥{balance}")   // → ¥7000

// アクターを停止
account.stop()
```

### スーパーバイザーツリー

```forge
// 障害時の再起動ポリシー
let supervisor = Supervisor::new()
    .strategy(RestartStrategy::OneForOne)   // 失敗したアクターだけ再起動
    // .strategy(RestartStrategy::AllForOne) // 全アクターを再起動
    // .strategy(RestartStrategy::RestForOne) // 失敗以降のアクターを再起動
    .max_restarts(3, within: 10s)           // 10秒以内に3回失敗したら諦める

// 子アクターを監視下に置く
let worker1 = supervisor.spawn(WorkerActor { id: 1 })
let worker2 = supervisor.spawn(WorkerActor { id: 2 })
let worker3 = supervisor.spawn(WorkerActor { id: 3 })

// ツリー構造
//   supervisor
//     ├── worker1
//     ├── worker2
//     └── worker3
// worker2 がクラッシュ → worker2 だけ自動再起動
```

### ステートマシンとしてのアクター

```forge
// 状態遷移を型で表現
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { session_id: str },
    Error(str),
}

actor Connection {
    state { inner: ConnectionState = ConnectionState::Disconnected }

    on Connect(msg: Connect) {
        match self.inner {
            Disconnected => {
                self.inner = Connecting
                self.send_self(DoConnect { url: msg.url })
            }
            _ => log::warn("既に接続中")
        }
    }

    on DoConnect(msg) {
        match tcp::connect(msg.url) {
            Ok(session) => self.inner = Connected { session_id: session.id },
            Err(e)      => self.inner = Error(e.to_str()),
        }
    }

    on Disconnect(_) {
        self.inner = Disconnected
    }
}
```

### アクターのパイプライン（ストリーム処理）

```forge
// データパイプライン
let pipeline = system.pipeline([
    FetchActor   { source: "api.example.com" },
    ParseActor   {},
    FilterActor  { predicate: |item| item.score > 0.8 },
    StoreActor   { db: crucible::connect()? },
])

pipeline.start()
pipeline.send(FetchRequest { query: "machine learning" })
```

---

## ローカル ↔ リモート透過

```forge
// ローカル（同プロセス内）
let system = ActorSystem::new()
let actor  = system.spawn(MyActor {})
actor.send(MyMessage { data: 42 })   // ← インメモリ

// リモート（ネットワーク越し）
let system = ActorSystem::new()
    .with_remote(RemoteConfig {
        bind: "0.0.0.0:7879",
        cluster: ["node2:7879", "node3:7879"],
    })

let remote_actor = system.lookup("node2/MyActor")?
remote_actor.send(MyMessage { data: 42 })   // ← TCP/WebSocket（同じ API）
```

---

## MQ との関係

```
forge/std/actor
       │ 受信箱 = MQ Queue
       │ 送信   = MQ publish
       ▼
forge/std/mq（TopicRouter + Queue + Broker）
       │
       ├── ネイティブ: tokio::sync::mpsc
       └── ブラウザ:  BroadcastChannel
```

`actor.send(msg)` の内部は `mq.publish(actor_id, msg)` と等価。
MQ 直接利用と Actor 利用は自由に混在できる。

---

## Anvil / Ember との統合

### Anvil（Web サーバー）

```forge
// リクエストを Worker アクターに分散
actor RequestHandler {
    on HttpRequest(req) {
        match req.path {
            "/api/orders" => order_worker.send(ProcessOrder { req }),
            "/api/users"  => user_worker.send(LookupUser { req }),
        }
    }
}

// バックグラウンドジョブ
actor EmailWorker {
    on SendEmail(msg) {
        smtp::send(msg.to, msg.subject, msg.body)?
    }
}
```

### Ember（ゲームエンジン）

```forge
// ゲームエンティティを Actor として扱う
actor EnemyAI {
    state { pos: Vec2, hp: i32, target: Address }

    on Tick(dt) {
        let dir = (self.target_pos() - self.pos).normalize()
        self.pos += dir * SPEED * dt
    }

    on TakeDamage(msg) {
        self.hp -= msg.damage
        if self.hp <= 0 { self.stop() }
    }
}
```

---

## ブラウザ（Bloom）との統合

```forge
// Island ごとにアクターを割り当てる
// Island A: ショッピングカート
actor CartActor {
    state { items: list<CartItem> = [] }

    on AddItem(msg) {
        self.items.push(msg.item)
        // MQ 経由で他 Island に通知
        mq.publish("cart.updated", CartUpdated { count: self.items.len() })
    }
}

// Island B: ヘッダーバッジ（別 Island、別 WASM）
mq.subscribe<CartUpdated>("cart.updated", |e| {
    badge_count = e.count
})
```

---

## 実装フェーズ

| フェーズ | 内容 |
|---|---|
| **A-0** | アクター基盤：`spawn` / `send` / `ask`（同期版、MQ Q-0 依存） |
| **A-1** | tokio 非同期化（MQ Q-1 依存） |
| **A-2** | スーパーバイザーツリー（OneForOne / AllForOne） |
| **A-3** | ステートマシン型安全化（型状態遷移の検証） |
| **A-4** | リモートアクター（TCP / WebSocket） |
| **A-5** | Bloom Island 統合（WASM アクター） |
| **A-6** | Ember ECS との統合（エンティティ = アクター） |
| **A-7** | クラスタリング（Raft ベース分散調整） |

**A-2 完成 = Anvil の本番バックグラウンドワーカーに使える**
**A-5 完成 = Bloom Islands 間の型安全通信が実現**

---

## 参考

- [Erlang OTP](https://www.erlang.org/doc/design_principles/des_princ) — スーパーバイザーツリーの元祖
- [Akka](https://akka.io/) — JVM 版アクターモデルの完成形
- [kameo](https://github.com/tqwewe/kameo) — Rust 製軽量アクター（参考実装）
- [NATS JetStream](https://docs.nats.io/nats-concepts/jetstream) — 軽量 MQ の本番実装参考
