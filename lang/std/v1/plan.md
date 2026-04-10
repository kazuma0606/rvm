# `forge/std` 標準ライブラリ v1 実装計画

> 仕様: `lang/std/v1/spec.md`
> 前提: コア言語・モジュールシステム・トランスパイラ B-0〜B-8・E-1〜E-6 が完成済み

---

## フェーズ構成

```
Phase S-1: 環境・プロセス・IO（依存なし・最優先）
Phase S-2: データ処理（依存なし・並走可能）
Phase S-3: ログ・設定（trait システム使用）
Phase S-4: 高レベル抽象（デコレータ拡張）  ← S-3 完成後
Phase S-5: イベント・パイプライン（DSL拡張）← S-4 完成後
```

S-1・S-2 は互いに独立して並走可能。
S-4 は `@retry` / `@memoize` 等のデコレータ処理をトランスパイラに追加するため S-3 の後。
S-5 の `pipeline { }` ブロックはパーサー拡張が必要なため S-4 の後。

---

## Phase S-1: 環境・プロセス・IO

### 目標

CLI・サーバーアプリの最低限のインフラが揃うこと。
`env("KEY")` / `args()` / `read_line()` が `forge run` / `forge build` 両方で動くこと。

### 実装ステップ

1. **`forge/std/env`**
   - `forge-stdlib` クレートに `env` モジュールを追加
   - `dotenvy` クレートを依存に追加
   - `forge run` 起動時に `.env` 系ファイルを自動ロードするフック（`forge-cli` 側）
   - 関数: `env` / `env_or` / `env_number` / `env_bool` / `env_require` / `load_env`
   - インタープリタ: `Value::String` / `Value::Bool` / `Value::Number` への変換
   - トランスパイラ: `std::env::var` / `dotenvy` への変換

2. **`forge/std/process`**
   - 関数: `args` / `exit` / `run` / `on_signal`
   - インタープリタ: `std::env::args` / `std::process::exit` / `Command::new` を直接呼ぶ
   - トランスパイラ: そのまま同等の Rust 標準関数に変換
   - `on_signal` は `forge run` ではシグナル登録のみ（`ctrlc` クレート）

3. **`forge/std/io`**
   - 関数: `read_line` / `read_stdin` / `eprintln` / `eprint`
   - インタープリタ: `std::io::stdin().read_line` / `eprintln!`
   - トランスパイラ: 同等の Rust マクロ・関数に変換

### テスト方針

- `test_env_or_returns_default`: `env_or` のデフォルト値確認
- `test_env_require_missing_errors`: 未設定キーのエラー確認
- `test_args_returns_list`: `args()` が `list<string>` を返すこと
- `test_read_line_from_stdin`: stdin モック経由の読み込み
- `test_eprintln_outputs_stderr`: stderr への出力確認
- Snapshot: `env_require` / `load_env` のトランスパイル結果

---

## Phase S-2: データ処理

### 目標

文字列・ファイル・JSON・正規表現・UUID・乱数が使えるようになること。

### 実装ステップ

1. **`forge/std/json`**
   - `serde_json` クレートを依存に追加
   - `stringify` / `stringify_pretty` / `parse`
   - `Value` → `serde_json::Value` 相互変換

2. **`forge/std/string`（拡張）**
   - 既存の組み込み文字列関数を補完する形で追加
   - `trim` / `trim_start` / `trim_end` / `split` / `join` / `starts_with` / `ends_with`
   - `contains` / `index_of` / `replace` / `replace_first` / `to_upper` / `to_lower`
   - `repeat` / `pad_left` / `pad_right` / `is_empty` / `char_count`

3. **`forge/std/fs`（拡張）**
   - 既存の `read_file` / `write_file` に追加
   - `list_dir` / `make_dir` / `delete_file`
   - `path_absolute` / `path_join` / `path_exists` / `path_is_dir` / `path_ext` / `path_stem`

4. **`forge/std/regex`**
   - `regex` クレートを依存に追加
   - `regex_match` / `regex_find` / `regex_find_all` / `regex_capture`
   - `regex_replace` / `regex_replace_first`

5. **`forge/std/uuid`**
   - `uuid` クレートを依存に追加（`v4` feature）
   - `uuid_v4` / `is_uuid`

6. **`forge/std/random`**
   - `rand` クレートを依存に追加
   - `random_int` / `random_float` / `random_choice` / `shuffle` / `seed_random`

### テスト方針

各モジュール最低3本のユニットテスト + 1本の Snapshot テスト。

---

## Phase S-3: ログ・設定

### 目標

`Logger` trait と `Config::load` が動き、`container {}` と統合できること。

### 実装ステップ

1. **`forge/std/log`**
   - `Logger` trait を ForgeScript の trait として定義（`forge-stdlib` に組み込み）
   - `ConsoleLogger` / `JsonLogger` / `SilentLogger` 実装
   - `FileLogger` / `MultiLogger` 実装
   - `LOG_LEVEL` 環境変数によるフィルタ
   - トランスパイラ: `tracing` / `tracing-subscriber` への変換

2. **`forge/std/config`**
   - `Config::load(T)` 関数の実装
   - toml パースに `toml` クレートを使用
   - `FORGE_ENV` に応じたセクション選択
   - `data` 型のデフォルト値との優先順位マージ
   - トランスパイラ: `config` クレートへの変換

### テスト方針

- `test_console_logger_outputs_level`: レベル付き出力の確認
- `test_json_logger_outputs_json`: JSON 形式の確認
- `test_silent_logger_outputs_nothing`: 出力なしの確認
- `test_log_level_filter`: `LOG_LEVEL` によるフィルタ確認
- `test_config_load_defaults`: フィールドデフォルト値の適用
- `test_config_load_env_overrides`: 環境変数の優先順位確認
- `test_config_load_toml_section`: `FORGE_ENV` によるセクション選択

---

## Phase S-4: 高レベル抽象（デコレータ拡張）

### 目標

`@retry` / `@memoize` / `@cache` / `@timed` デコレータが `forge build` で動くこと。
`forge run` では関数ラッパーとして逐次実行。

### 前提

- S-3（Logger trait）完成済み
- トランスパイラのデコレータ処理フレームワーク（`@derive` を拡張）

### 実装ステップ

1. **`forge/std/retry`**
   - `retry(n, strategy, fn)` 関数の実装（インタープリタ）
   - バックオフ戦略: `exponential` / `linear` / `constant`
   - サーキットブレーカー状態マシン（Closed / Open / HalfOpen）
   - `@retry(max, backoff, base_ms, jitter)` デコレータ
   - トランスパイラ: `tokio::time::sleep` + ループに展開

2. **`forge/std/cache`**
   - `Cache::new(ttl, max)` 型の実装（インタープリタは `HashMap`）
   - `get_or_set` / `get` / `set` / `invalidate` / `clear`
   - `@memoize` デコレータ（引数をキーにした `HashMap` ラッパー）
   - `@cache(ttl, max, key)` デコレータ
   - トランスパイラ: `moka` クレートへの変換

3. **`forge/std/metrics`**
   - `MetricsBackend` trait の実装
   - `InMemoryBackend`（テスト用）/ `LogBackend`（開発用）
   - `PrometheusBackend` / `StatsdBackend`（別クレートとして）
   - `@timed(metric)` デコレータ
   - トランスパイラ: `Instant::now()` + `.elapsed()` ラッパーに展開

### テスト方針

- `test_retry_succeeds_after_failures`: 失敗後のリトライ成功
- `test_retry_exhausted_returns_error`: 全リトライ消費でエラー
- `test_circuit_breaker_opens_after_threshold`: しきい値超えで Open
- `test_memoize_caches_result`: 同じ引数で関数を再呼び出ししないこと
- `test_cache_ttl_expires`: TTL 後にキャッシュが無効化されること
- `test_cache_invalidate`: `invalidate` で特定エントリ削除
- `test_metrics_counter_increments`: カウンタのインクリメント確認
- Snapshot: `@retry` / `@memoize` のトランスパイル展開結果

---

## Phase S-5: イベント・パイプライン

### 目標

`emit` / `on` / `@on` が動き、`pipeline { }` ブロックが宣言的に書けること。

### 前提

- S-4 完成済み
- `pipeline { }` ブロックのパーサー拡張

### 実装ステップ

1. **`forge/std/event`**
   - `EventBus` 型の実装（インタープリタは `HashMap<TypeId, Vec<Fn>>` ベース）
   - `emit(event)` / `on(EventType, handler)` / `once` / `off`
   - `@on(EventType)` デコレータ（`container` 初期化時に自動登録）
   - 実行モード: `async`（デフォルト）/ `sync` / `ordered`
   - トランスパイラ: `tokio::sync::broadcast` チャネルへの変換

2. **`forge/std/pipeline`**
   - **パーサー拡張**: `pipeline { }` ブロックを `Expr::Pipeline` として追加
   - **AST 拡張**: `PipelineStep`（Source / Filter / Map / FlatMap / Group / Sort / Take / Skip / Each / Sink / Parallel）
   - インタープリタ: 逐次イテレータとして実行
   - 組み込み Source: `ListSource` / `CsvSource` / `JsonSource`
   - 組み込み Sink: `CollectSink` / `StdoutSink` / `CsvSink` / `JsonSink`
   - `parallel N` 指定時: `forge run` では逐次、`forge build` では `rayon` に変換
   - トランスパイラ: `Iterator` チェーン / `rayon::ParallelIterator` に変換

### テスト方針

- `test_event_on_emit_calls_handler`: emit で on ハンドラが呼ばれること
- `test_event_once_fires_once`: `once` が1回だけ実行されること
- `test_event_off_unsubscribes`: `off` で購読解除されること
- `test_pipeline_filter_map_collect`: filter + map + CollectSink の結果確認
- `test_pipeline_group_aggregation`: group によるグルーピング確認
- `test_pipeline_take_limits`: `take N` で件数制限の確認
- Snapshot: `pipeline { }` のトランスパイル結果（Iterator チェーン）
- Snapshot: `parallel` 付き pipeline の `rayon` 変換結果

---

## 実装順序の推奨

```
┌──────────────────────────────────────┐
│ 並走可能                              │
│   S-1  env / process / io            │ ← 最優先・CLI すぐ使える
│   S-2  json / string / fs / regex    │ ← S-1 と並走可
│        uuid / random                 │
└──────────────────────────────────────┘
              ↓
┌──────────────────────────────────────┐
│   S-3  log / config                  │
└──────────────────────────────────────┘
              ↓
┌──────────────────────────────────────┐
│   S-4  retry / cache / metrics       │
└──────────────────────────────────────┘
              ↓
┌──────────────────────────────────────┐
│   S-5  event / pipeline              │
└──────────────────────────────────────┘
```
