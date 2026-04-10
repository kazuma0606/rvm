# `forge/std` 標準ライブラリ仕様書 v1

> バージョン: 0.2.0
> 作成: 2026-04-08
> 対象: forge/std 第2層（実用ライン）+ 第3層（Rust にない高レベル抽象）

---

## モジュール一覧

| モジュール | 概要 |
|---|---|
| [`forge/std/env`](#forgestdenv) | 環境変数 + dotenv 自動ロード |
| [`forge/std/log`](#forgestdlog) | プラガブルロガー（Logger trait） |
| [`forge/std/process`](#forgestdprocess) | プロセス制御・シグナルハンドリング |
| [`forge/std/io`](#forgestdio) | 標準入出力・stderr |
| [`forge/std/json`](#forgestdjson) | JSON シリアライズ・デシリアライズ |
| [`forge/std/fs`](#forgestdfs) | ファイルシステム拡張 |
| [`forge/std/string`](#forgestdstring) | 文字列操作拡張 |
| [`forge/std/regex`](#forgestdregex) | 正規表現 |
| [`forge/std/uuid`](#forgestduuid) | UUID 生成 |
| [`forge/std/random`](#forgestdrandom) | 乱数生成 |
| [`forge/std/retry`](#forgestdretry) | リトライ・サーキットブレーカー（`@retry` デコレータ） |
| [`forge/std/cache`](#forgestdcache) | メモ化・TTL キャッシュ（`@memoize` / `@cache` デコレータ） |
| [`forge/std/metrics`](#forgestdmetrics) | プラガブルメトリクス（Counter / Gauge / Histogram） |
| [`forge/std/event`](#forgestdevent) | インプロセス Pub/Sub イベントバス |
| [`forge/std/config`](#forgestdconfig) | 多ソース統合型アプリケーション設定 |
| [`forge/std/pipeline`](#forgestdpipeline) | 宣言的データパイプライン（Source / Transform / Sink） |

---

## `forge/std/env`

環境変数の読み取りと `.env` ファイルの自動・手動ロードを提供する。
秘匿情報をコードに直書きせず、環境ごとの設定を切り替えるために使用する。

### dotenv 読み込み優先順位

```
1. システム環境変数（OS / シェルで export 済みのもの）  ← 最優先・上書き不可
2. .env.local                  ← ローカル秘匿情報（.gitignore 推奨）
3. .env.{FORGE_ENV}            ← 環境別（.env.development / .env.production 等）
4. .env                        ← ベースデフォルト（git 管理してよい）
```

**自動ロード**: `forge run` 起動時に上記順序で自動読み込み（明示的な呼び出し不要）。
`FORGE_ENV=production forge run` で環境を切り替え。

### API

```forge
use forge/std/env.*

let val    = env("DATABASE_URL")?              // string?
let val    = env_or("PORT", "3000")            // デフォルト付き
let port   = env_number("PORT")?               // string → number
let debug  = env_bool("DEBUG")?                // "true"/"1" → bool
let db_url = env_require("DATABASE_URL")       // T!（未設定なら実行時エラー）
load_env(".env.custom")                        // 手動ロード
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `env` | `(key: string) -> string?` | 環境変数を取得。未設定なら `none` |
| `env_or` | `(key: string, default: string) -> string` | 未設定時にデフォルト値を返す |
| `env_number` | `(key: string) -> number?` | 文字列を number に変換して返す |
| `env_bool` | `(key: string) -> bool?` | `"true"` / `"1"` / `"yes"` / `"on"` を `true` に変換 |
| `env_require` | `(key: string) -> string!` | 未設定なら実行時エラー（起動時チェック用） |
| `load_env` | `(path: string) -> unit!` | 指定パスの `.env` ファイルを手動ロード |

### Rust 変換

内部実装は `dotenvy` クレートを使用。
```rust
// env("KEY")        →  std::env::var("KEY").ok()
// env_require("KEY") →  std::env::var("KEY").expect("KEY is not set")
// load_env(".env")  →  dotenvy::from_path(".env").ok()
```

### 制約

- システム環境変数は `.env` で上書きできない（セキュリティ上の仕様）
- `load_env` は自動ロード後に呼び出すと追記ロードになる（既存値は上書きしない）

---

## `forge/std/log`

プラガブルなロガー抽象化ライブラリ。
`Logger` trait を介してバックエンドを自由に差し替えられる設計。

### Logger trait

```forge
trait Logger {
    fn info(self, msg: string, ctx: map<string, string>?)
    fn warn(self, msg: string, ctx: map<string, string>?)
    fn error(self, msg: string, ctx: map<string, string>?)
    fn debug(self, msg: string, ctx: map<string, string>?)
}
```

### 組み込み実装

| 実装 | 説明 | 用途 |
|---|---|---|
| `ConsoleLogger` | タイムスタンプ付きテキスト出力 | `forge run` デフォルト・開発時 |
| `JsonLogger` | 構造化 JSON 出力（1行1エントリ） | 本番・fluentd / Datadog 向け |
| `SilentLogger` | 何も出力しない | テスト・ベンチマーク用 |
| `FileLogger` | ローテーション付きファイル書き込み | ログ永続化 |
| `MultiLogger` | 複数バックエンドへの同時出力 | 複数先への同時送信 |

### サードパーティ実装（別パッケージ）

| パッケージ | 実装 | 説明 |
|---|---|---|
| `forge-prometheus` | `PrometheusLogger` | カウンタ/ゲージを Prometheus メトリクスとして公開 |
| `forge-mongo` | `MongoLogger` | ログを MongoDB コレクションに INSERT |

### 使用例

```forge
use forge/std/log.{ Logger, JsonLogger }

let logger = JsonLogger::new()
logger.info("サーバー起動", { port: "8080" })
logger.warn("接続リトライ中", { host: host, attempt: "3" })
logger.error("DB 接続失敗", { error: err })
// → {"level":"INFO","msg":"サーバー起動","port":"8080","ts":"2026-04-08T12:00:00Z"}
```

### `container {}` による注入パターン

```forge
use forge/std/log.{ Logger, JsonLogger, SilentLogger }
use forge_prometheus.{ PrometheusLogger }

container {
    bind Logger to match env_or("LOG_BACKEND", "json") {
        "prometheus" => PrometheusLogger::new(env_require("METRICS_PORT"))
        "silent"     => SilentLogger::new()
        _            => JsonLogger::new()
    }
}

@service
struct OrderService {
    logger: Logger   // 差し込まれた実装が使われる
}
```

バックエンドを変えても呼び出し側コードの変更は一切不要。

### ログレベル制御

`LOG_LEVEL` 環境変数（`debug` / `info` / `warn` / `error`）でフィルタ制御。

### Rust 変換方針

- `ConsoleLogger` → `tracing` / `env_logger`
- `JsonLogger` → `tracing-subscriber` JSON フォーマッタ
- `PrometheusLogger` → `prometheus` クレート
- `MongoLogger` → `mongodb` クレート

---

## `forge/std/process`

プロセス制御・コマンドライン引数・シグナルハンドリングを提供する。

### API

```forge
use forge/std/process.*

let argv = args()                             // list<string>
exit(0)                                       // プロセス終了
let out = run("git", ["status"])?             // string!（stdout）
on_signal("SIGTERM", () => { db.close(); exit(0) })
on_signal("SIGINT",  () => { exit(0) })
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `args` | `() -> list<string>` | コマンドライン引数の一覧（argv[0] はスクリプト名） |
| `exit` | `(code: number) -> unit` | プロセスを終了する |
| `run` | `(cmd: string, args: list<string>) -> string!` | 外部コマンドを実行し stdout を返す |
| `on_signal` | `(sig: string, handler: () -> unit) -> unit` | シグナルハンドラを登録する |

### サポートするシグナル

| シグナル | 説明 |
|---|---|
| `"SIGTERM"` | 終了要求（Docker / Kubernetes の停止シグナル） |
| `"SIGINT"` | Ctrl+C による中断 |
| `"SIGHUP"` | 設定再読み込み（Unix のみ） |

Windows では `SIGTERM` / `SIGINT` のみサポート。

### Rust 変換

```rust
// args()     →  std::env::args().collect::<Vec<_>>()
// exit(n)    →  std::process::exit(n)
// run(...)   →  std::process::Command::new(cmd).args(args).output()
// on_signal  →  tokio::signal / ctrlc クレート
```

---

## `forge/std/io`

標準入出力・stderr へのアクセスを提供する。

### API

```forge
use forge/std/io.*

let line = read_line()?       // string?（EOF なら none）
let all  = read_stdin()?      // string!（全入力を一度に読む）
eprintln("エラー: {msg}")     // stderr に改行付き出力
eprint("警告: ")              // stderr に改行なし出力
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `read_line` | `() -> string?` | 1行読み込む。EOF なら `none` |
| `read_stdin` | `() -> string!` | stdin 全体を読み込む |
| `eprintln` | `(msg: string) -> unit` | stderr に改行付き出力 |
| `eprint` | `(msg: string) -> unit` | stderr に改行なし出力 |

### Rust 変換

```rust
// read_line()  →  { let mut s = String::new(); std::io::stdin().read_line(&mut s).ok(); ... }
// read_stdin() →  { use std::io::Read; let mut s = String::new(); std::io::stdin().read_to_string(&mut s)?; s }
// eprintln!()  →  eprintln!("{}", msg)
```

---

## `forge/std/json`

JSON のシリアライズ・デシリアライズを提供する。

### API

```forge
use forge/std/json.*

let json_str = stringify(value)         // any -> string
let pretty   = stringify_pretty(value)  // インデント付き
let obj      = parse(json_str)?         // string -> map<string, any>!
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `stringify` | `(value: any) -> string` | 値を JSON 文字列に変換 |
| `stringify_pretty` | `(value: any) -> string` | インデント付き JSON 文字列に変換 |
| `parse` | `(json: string) -> map<string, any>!` | JSON 文字列をパース |

### 型マッピング

| ForgeScript 型 | JSON |
|---|---|
| `number` | `number` (integer) |
| `float` | `number` (floating point) |
| `bool` | `true` / `false` |
| `string` | `"string"` |
| `list<T>` | `[...]` |
| `map<string, T>` | `{...}` |
| `struct` | `{...}`（フィールド名をキーに） |
| `none` | `null` |

### Rust 変換

内部実装は `serde_json` クレートを使用。

---

## `forge/std/fs`

ファイルシステム操作の拡張 API。
基本実装（`read_file` / `write_file`）は実装済みのため、ここでは追加 API のみ記載する。

### API

```forge
use forge/std/fs.*

let entries = list_dir("./src")?                      // list<string>
make_dir("./output")?                                 // 再帰的にディレクトリ作成
delete_file("./tmp/work.txt")?

let full   = path_absolute("./src")?                  // string!
let joined = path_join("./src", "main.forge")         // string
let exists = path_exists("./forge.toml")              // bool
let is_dir = path_is_dir("./src")                     // bool
let ext    = path_ext("main.forge")                   // string?  → "forge"
let stem   = path_stem("main.forge")                  // string?  → "main"
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `list_dir` | `(path: string) -> list<string>!` | ディレクトリのエントリ名一覧 |
| `make_dir` | `(path: string) -> unit!` | ディレクトリを再帰的に作成 |
| `delete_file` | `(path: string) -> unit!` | ファイルを削除 |
| `path_absolute` | `(path: string) -> string!` | 絶対パスに変換 |
| `path_join` | `(base: string, ...parts: string) -> string` | パスを結合 |
| `path_exists` | `(path: string) -> bool` | パスが存在するか確認 |
| `path_is_dir` | `(path: string) -> bool` | ディレクトリかどうか確認 |
| `path_ext` | `(path: string) -> string?` | 拡張子を取得 |
| `path_stem` | `(path: string) -> string?` | 拡張子なしファイル名を取得 |

### Rust 変換

```rust
// list_dir    →  std::fs::read_dir(path)
// make_dir    →  std::fs::create_dir_all(path)
// delete_file →  std::fs::remove_file(path)
// path_join   →  std::path::Path::new(base).join(part)
// path_exists →  std::path::Path::new(path).exists()
```

---

## `forge/std/string`

文字列操作の拡張 API。
組み込み文字列関数（`len` / `string` / `number` 変換）は実装済みのため、ここでは追加 API のみ記載する。

### API

```forge
use forge/std/string.*

// トリム
let s = trim("  hello  ")                // "hello"
let s = trim_start("  hello  ")          // "hello  "
let s = trim_end("  hello  ")            // "  hello"

// 分割・結合
let parts = split("a,b,c", ",")          // ["a", "b", "c"]
let s     = join(["a", "b", "c"], ",")   // "a,b,c"

// 検索
let b = starts_with("hello", "he")       // true
let b = ends_with("hello", "lo")         // true
let b = contains("hello", "ell")         // true
let i = index_of("hello", "ll")          // number?

// 変換
let s = replace("hello", "l", "r")       // "herro"（全置換）
let s = replace_first("hello", "l", "r") // "herlo"
let s = to_upper("hello")                // "HELLO"
let s = to_lower("HELLO")                // "hello"
let s = repeat("ab", 3)                  // "ababab"

// パディング
let s = pad_left("42", 5, "0")           // "00042"
let s = pad_right("hi", 5, "-")          // "hi---"

// チェック
let b = is_empty("")                     // true
let n = char_count("hello")              // 5（Unicode 文字数）
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `trim` | `(s: string) -> string` | 両端の空白を除去 |
| `trim_start` | `(s: string) -> string` | 先頭の空白を除去 |
| `trim_end` | `(s: string) -> string` | 末尾の空白を除去 |
| `split` | `(s: string, sep: string) -> list<string>` | 区切り文字で分割 |
| `join` | `(list: list<string>, sep: string) -> string` | リストを結合 |
| `starts_with` | `(s: string, prefix: string) -> bool` | 前方一致 |
| `ends_with` | `(s: string, suffix: string) -> bool` | 後方一致 |
| `contains` | `(s: string, sub: string) -> bool` | 部分一致 |
| `index_of` | `(s: string, sub: string) -> number?` | 最初の出現位置 |
| `replace` | `(s: string, from: string, to: string) -> string` | 全置換 |
| `replace_first` | `(s: string, from: string, to: string) -> string` | 最初の1個を置換 |
| `to_upper` | `(s: string) -> string` | 大文字変換 |
| `to_lower` | `(s: string) -> string` | 小文字変換 |
| `repeat` | `(s: string, n: number) -> string` | n 回繰り返し |
| `pad_left` | `(s: string, width: number, pad: string) -> string` | 左パディング |
| `pad_right` | `(s: string, width: number, pad: string) -> string` | 右パディング |
| `is_empty` | `(s: string) -> bool` | 空文字列チェック |
| `char_count` | `(s: string) -> number` | Unicode 文字数（バイト数ではない） |

---

## `forge/std/regex`

正規表現によるマッチング・抽出・置換を提供する。

### API

```forge
use forge/std/regex.*

let b      = regex_match("^\\d{3}-\\d{4}$", "123-4567")         // bool
let m      = regex_find("\\d+", "abc123def")?                   // string?
let ms     = regex_find_all("\\d+", "a1b2c3")                   // list<string>
let groups = regex_capture("(\\d{4})-(\\d{2})-(\\d{2})", "2026-04-08")?
             // list<string>?  →  ["2026", "04", "08"]
let s      = regex_replace("\\s+", "hello   world", " ")        // "hello world"
let s      = regex_replace_first("\\d", "a1b2c3", "X")          // "aXb2c3"
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `regex_match` | `(pattern: string, input: string) -> bool` | パターンにマッチするか確認 |
| `regex_find` | `(pattern: string, input: string) -> string?` | 最初のマッチ文字列を返す |
| `regex_find_all` | `(pattern: string, input: string) -> list<string>` | 全マッチを返す |
| `regex_capture` | `(pattern: string, input: string) -> list<string>?` | キャプチャグループを返す |
| `regex_replace` | `(pattern: string, input: string, replacement: string) -> string` | 全マッチを置換 |
| `regex_replace_first` | `(pattern: string, input: string, replacement: string) -> string` | 最初のマッチのみ置換 |

### Rust 変換

内部実装は `regex` クレートを使用。
パターンは Rust `regex` クレートの構文に準拠（PCRE ではない）。

---

## `forge/std/uuid`

UUID 生成を提供する。

### API

```forge
use forge/std/uuid.*

let id = uuid_v4()    // "550e8400-e29b-41d4-a716-446655440000"
let b  = is_uuid(id)  // bool
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `uuid_v4` | `() -> string` | ランダム UUID v4 を生成 |
| `is_uuid` | `(s: string) -> bool` | UUID フォーマットか確認 |

### Rust 変換

内部実装は `uuid` クレートを使用。
```rust
// uuid_v4()  →  uuid::Uuid::new_v4().to_string()
```

---

## `forge/std/random`

乱数生成を提供する。

### API

```forge
use forge/std/random.*

let n = random_int(1, 100)        // number（1〜100・両端含む）
let f = random_float()            // float（0.0 以上 1.0 未満）
let x = random_choice([1, 2, 3]) // any（リストから1要素をランダム選択）
let s = shuffle([1, 2, 3, 4, 5]) // list<T>（シャッフルされた新しいリスト）
seed_random(42)                   // シード固定（テスト再現性）
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `random_int` | `(min: number, max: number) -> number` | 指定範囲の整数乱数（両端含む） |
| `random_float` | `() -> float` | 0.0 以上 1.0 未満の浮動小数点乱数 |
| `random_choice` | `(list: list<T>) -> T` | リストからランダムに1要素を選ぶ |
| `shuffle` | `(list: list<T>) -> list<T>` | リストをシャッフルした新しいリストを返す |
| `seed_random` | `(seed: number) -> unit` | 乱数シードを固定（テスト用） |

### Rust 変換

内部実装は `rand` クレートを使用。
```rust
// random_int(min, max)  →  rand::thread_rng().gen_range(min..=max)
// random_float()        →  rand::thread_rng().gen::<f64>()
// random_choice(list)   →  list.choose(&mut rand::thread_rng()).cloned()
// shuffle(list)         →  { let mut v = list.clone(); v.shuffle(&mut rand::thread_rng()); v }
```

---

## `forge/std/retry`

リトライ・指数バックオフ・サーキットブレーカーを提供する。
ネットワーク呼び出し・DB 接続など一時的な失敗を自動回復するために使用する。

### デコレータ API（推奨）

```forge
use forge/std/retry.{ retry, circuit_breaker }

// 最大3回・指数バックオフ・ジッター付き
@retry(max: 3, backoff: "exponential", base_ms: 100, jitter: true)
fn call_api(url: string) -> Response! { ... }

// サーキットブレーカー（5回失敗で open・30秒後に half-open）
@circuit_breaker(threshold: 5, timeout_ms: 30000)
fn query_db(sql: string) -> list<map<string, any>>! { ... }
```

### 関数 API

```forge
use forge/std/retry.{ retry, with_backoff, exponential, linear, constant }

// シンプルなリトライ
let res = retry(3, () => fetch(url))?

// バックオフ戦略を指定
let res = retry(5, with_backoff(exponential(100), jitter: true), () => fetch(url))?
let res = retry(5, with_backoff(linear(200)), () => fetch(url))?
let res = retry(3, with_backoff(constant(500)), () => fetch(url))?
```

### バックオフ戦略

| 戦略 | 説明 | 待機時間の例（base: 100ms） |
|---|---|---|
| `exponential(base_ms)` | 指数増加（2^n × base） | 100 → 200 → 400 → 800ms |
| `linear(step_ms)` | 線形増加 | 100 → 200 → 300 → 400ms |
| `constant(ms)` | 固定間隔 | 100 → 100 → 100ms |

`jitter: true` を付けると±25%のランダムゆらぎが加わり、スタンピーディング防止になる。

### サーキットブレーカー状態遷移

```
Closed（正常）
  ↓ 連続失敗が threshold を超える
Open（遮断）← 即座にエラーを返す
  ↓ timeout_ms 経過
HalfOpen（試験中）
  ↓ 成功 → Closed / 失敗 → Open
```

### Rust 変換

`tokio::time::sleep` + カスタム状態マシン。
`@retry` デコレータはトランスパイラが関数本体をクロージャでラップして生成する。

---

## `forge/std/cache`

メモ化・TTL キャッシュを提供する。
純粋関数の結果再利用・外部 API コールの削減に使用する。

### デコレータ API（推奨）

```forge
use forge/std/cache.{ memoize, cache }

// 引数をキーとして結果を永続キャッシュ（メモ化）
@memoize
fn fib(n: number) -> number {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

// TTL 付きキャッシュ（60秒・最大1000エントリ・LRU 退避）
@cache(ttl: 60, max: 1000)
fn fetch_user(id: string) -> User! { ... }

// キャッシュキーを明示（引数の一部だけをキーにしたい場合）
@cache(ttl: 300, key: id => id)
fn get_config(id: string, _request_id: string) -> Config! { ... }
```

### 関数 API

```forge
use forge/std/cache.{ Cache }

let c = Cache::new(ttl: 60, max: 500)
let user = c.get_or_set("user:{id}", () => fetch_user(id))?
c.invalidate("user:{id}")
c.clear()
```

### `Cache` メソッド一覧

| メソッド | シグネチャ | 説明 |
|---|---|---|
| `get_or_set` | `(key: string, fn: () -> T!) -> T!` | キャッシュにあれば返し、なければ fn を実行して格納 |
| `get` | `(key: string) -> T?` | キャッシュから取得（なければ `none`） |
| `set` | `(key: string, value: T, ttl: number?) -> unit` | 明示的に格納 |
| `invalidate` | `(key: string) -> unit` | 特定キーを無効化 |
| `clear` | `() -> unit` | 全エントリを削除 |

### Rust 変換

内部実装は `moka` クレート（高性能 LRU キャッシュ）を使用。
`@memoize` は引数の `Hash` 実装を利用した `HashMap` に変換される。

---

## `forge/std/metrics`

プラガブルなメトリクス抽象化ライブラリ。
`Logger` trait と同じ設計思想でバックエンドを差し替えられる。

### MetricsBackend trait

```forge
trait MetricsBackend {
    fn counter(self, name: string, value: number, labels: map<string, string>?)
    fn gauge(self, name: string, value: float, labels: map<string, string>?)
    fn histogram(self, name: string, value: float, labels: map<string, string>?)
}
```

### 組み込み実装

| 実装 | 説明 | 用途 |
|---|---|---|
| `PrometheusBackend` | `/metrics` エンドポイントで公開 | 本番・Grafana 連携 |
| `StatsdBackend` | UDP で StatsD サーバーへ送信 | Datadog / Telegraf 連携 |
| `InMemoryBackend` | メモリ内に保持 | テスト・ベンチマーク |
| `LogBackend` | Logger 経由でメトリクスを出力 | 開発時デバッグ |

### 使用例

```forge
use forge/std/metrics.{ MetricsBackend, PrometheusBackend }

container {
    bind MetricsBackend to PrometheusBackend::new(env_number_or("METRICS_PORT", 9090))
}

@service
struct OrderService {
    metrics: MetricsBackend
}

impl OrderService {
    fn create(self, order: Order) -> Order! {
        let saved = self.repo.save(order)?
        self.metrics.counter("orders_created_total", 1, { region: order.region })
        self.metrics.histogram("order_amount", order.amount, none)
        ok(saved)
    }
}
```

### `@timed` デコレータ

```forge
// メソッドの実行時間を histogram として自動記録
@timed(metric: "api_request_duration_ms")
fn handle_request(req: Request) -> Response! { ... }
```

### Rust 変換

- `PrometheusBackend` → `prometheus` クレート
- `StatsdBackend` → `cadence` クレート
- `@timed` → トランスパイラが `Instant::now()` + `.elapsed()` でラップ

---

## `forge/std/event`

**インプロセス・インメモリ** の Pub/Sub イベントバス。
同一プロセス内のモジュール間を疎結合にするためのもので、RabbitMQ / Kafka のような外部ブローカーとは異なる。

> **スコープの明確化**
>
> | | `forge/std/event` | 外部ブローカー系パッケージ |
> |---|---|---|
> | 動作範囲 | 同一プロセス内のみ | 別プロセス・別サービス間 |
> | 永続化 | なし（プロセス終了で消える） | あり（キュー・トピック） |
> | ACK / 再送 | なし | あり |
> | デッドレター | なし | あり |
> | 用途 | 層間の関心分離・DI との統合 | マイクロサービス間通信・非同期ジョブ |
>
> 外部ブローカー連携は別パッケージ（`forge-amqp` / `forge-kafka` / `forge-nats`）として提供する。
> 開発時は `forge/std/event`（インメモリ）、本番は `forge-amqp`（RabbitMQ）のように
> `container { bind EventBus to AmqpEventBus::new(...) }` で差し替えられる設計にする。

### イベント定義

```forge
// イベントは data 型として定義
data UserCreated  { user_id: string, email: string }
data OrderPlaced  { order_id: string, amount: float, user_id: string }
data PaymentFailed { order_id: string, reason: string }
```

### 発行・購読

```forge
use forge/std/event.{ emit, on, once, off }

// イベント発行（非同期・fire-and-forget）
emit(UserCreated { user_id: id, email: email })

// イベント購読（アプリ起動時に登録）
on(UserCreated,   e => send_welcome_email(e.email))
on(OrderPlaced,   e => update_inventory(e.order_id))
on(PaymentFailed, e => notify_admin(e.reason))

// 一度だけ受け取る
once(UserCreated, e => println("初回登録: {e.email}"))

// 購読解除
let handler = on(OrderPlaced, e => log(e))
off(handler)
```

### `@on` デコレータ（サービス層での宣言的購読）

```forge
@service
struct NotificationService {
    mailer: EmailService
}

impl NotificationService {
    @on(UserCreated)
    fn handle_user_created(self, e: UserCreated) -> unit! {
        self.mailer.send(e.email, "ようこそ", "登録ありがとうございます")
    }

    @on(PaymentFailed)
    fn handle_payment_failed(self, e: PaymentFailed) -> unit! {
        self.mailer.send_admin("決済失敗: {e.order_id}", e.reason)
    }
}
```

### 実行モード

| モード | 説明 | 設定 |
|---|---|---|
| `async`（デフォルト） | 非同期・fire-and-forget | `EventBus::new(mode: "async")` |
| `sync` | 発行スレッドで同期実行 | `EventBus::new(mode: "sync")` |
| `ordered` | 登録順に逐次実行（sync） | `EventBus::new(mode: "ordered")` |

### Rust 変換

`async` モードは `tokio::sync::broadcast` チャネルに変換。
`@on` デコレータはトランスパイラが `container` 初期化時に自動登録コードを生成する。

---

## `forge/std/config`

複数ソースを統合するアプリケーション設定ライブラリ。
`forge/std/env` の上位抽象として、型付き設定オブジェクトを提供する。

### 読み込み優先順位

```
1. 環境変数（OS / .env 系）           ← 最優先
2. config.toml の [env名] セクション  ← 環境別設定
3. config.toml の [default] セクション
4. data 型のフィールドデフォルト値     ← 最低優先
```

### 使用例

```forge
use forge/std/config.{ Config }

// 設定スキーマを data 型で定義
data AppConfig {
    port:       number = 8080
    db_url:     string = "sqlite://app.db"
    log_level:  string = "info"
    debug:      bool   = false
    max_conns:  number = 10
}

// ロード（優先順位に従って自動マージ）
let config = Config::load(AppConfig)?

// アクセス
println("ポート: {config.port}")
println("DB: {config.db_url}")
```

### `config.toml` の例

```toml
[default]
port = 8080
log_level = "info"

[development]
debug = true
log_level = "debug"

[production]
port = 80
max_conns = 100
```

`FORGE_ENV=production` なら `[production]` セクションが `[default]` を上書き。

### `container {}` との統合

```forge
container {
    bind AppConfig to Config::load(AppConfig)?
}

@service
struct ApiServer {
    config: AppConfig
}
```

### `Config` 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `Config::load` | `(T) -> T!` | data 型に従って設定をロード・バリデーション |
| `Config::load_from` | `(T, path: string) -> T!` | 指定パスの toml をロード |
| `Config::reload` | `(config: T) -> T!` | 設定を再読み込み（ホットリロード） |

### Rust 変換

内部実装は `config` クレートを使用。

---

## `forge/std/pipeline`

宣言的データパイプライン（ETL）抽象。
Source → Transform → Sink の構造でバッチ処理・データ変換を記述する。

### 基本構文

```forge
use forge/std/pipeline.{ pipeline, CsvSource, JsonSink, StdoutSink }

pipeline {
    source  CsvSource::new("input.csv")
    filter  row => row.amount > 0
    map     row => { ...row, amount: row.amount * 1.1 }
    sink    JsonSink::new("output.json")
}?
```

### 複数ステップ・集約

```forge
pipeline {
    source  CsvSource::new("sales.csv")
    filter  row => row.region == "JP"
    map     row => SaleRecord { id: row.id, amount: number(row.amount) }
    group   record => record.category
    map     group => {
        category: group.key,
        total: group.values.fold(0.0, (acc, r) => acc + r.amount)
    }
    sort    g => g.total   desc: true
    take    10
    sink    StdoutSink::new()
}?
```

### 組み込み Source / Sink

| 種別 | 型 | 説明 |
|---|---|---|
| Source | `CsvSource` | CSV ファイルを行単位で読み込む |
| Source | `JsonSource` | JSON Lines ファイルを読み込む |
| Source | `ListSource` | list をソースとして使う（テスト用） |
| Source | `DbSource` | forge-db クエリ結果をソースとして使う |
| Sink | `CsvSink` | CSV ファイルに書き出す |
| Sink | `JsonSink` | JSON Lines ファイルに書き出す |
| Sink | `StdoutSink` | 標準出力に出力 |
| Sink | `DbSink` | forge-db テーブルに INSERT |
| Sink | `CollectSink` | `list<T>` に収集して返す（テスト用） |

### パイプライン演算子一覧

| 演算子 | シグネチャ | 説明 |
|---|---|---|
| `source` | `Source<T>` | データソースを指定 |
| `filter` | `(T) -> bool` | 条件を満たす行のみ通す |
| `map` | `(T) -> U` | 変換関数を適用 |
| `flat_map` | `(T) -> list<U>` | 変換後に展開 |
| `group` | `(T) -> K` | キー関数でグルーピング |
| `sort` | `(T) -> K` | ソートキーを指定 |
| `take` | `number` | 先頭 N 件のみ通す |
| `skip` | `number` | 先頭 N 件をスキップ |
| `each` | `(T) -> unit` | 副作用（ログ出力等）を挟む |
| `sink` | `Sink<T>` | 出力先を指定 |

### 並列実行

```forge
pipeline {
    source CsvSource::new("large.csv")
    parallel 4   // 4並列で map/filter を実行
    map    row => heavy_transform(row)
    sink   JsonSink::new("output.json")
}?
```

### Rust 変換

`forge run` では逐次イテレータとして実行。
`forge build` では `rayon` 並列イテレータ（`parallel` 指定時）または `tokio` ストリームに変換。
