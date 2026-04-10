# `forge/std` 標準ライブラリ v1 進捗

> 参照: `lang/std/v1/spec.md`
>
> 目標: Phase S-1〜S-5 を順に完了させて標準ライブラリ全体を整備する。

---

## Phase S-1: 環境変数 / プロセス / IO

### S-1-A: `forge/std/env`
- [x] `dotenvy` を使って `.env` 系ファイルを自動読み込みし、`env`, `env_or`, `env_number`, `env_bool`, `env_require`, `load_env` をエクスポート。
- [x] `load_env` でカスタムパスを扱い、標準入出力を通じて設定を確認。
- [x] `test_env_or_returns_default`
- [x] `test_env_require_missing_errors`
- [x] `test_env_bool_recognizes_variants`
- [x] `test_load_env_custom_path`
- [x] Snapshot: `test_transpile_env_require`

### S-1-B: `forge/std/process`
- [x] `args`, `exit`, `run` を提供し、`on_signal` は `signal-hook` を使って `SIGINT`/`SIGTERM`/`SIGHUP` を処理。
- [x] `test_args_returns_list`
- [x] `test_run_echo_command`
- [x] `test_on_signal_sigint`
- [x] Snapshot: `test_transpile_process_run`

### S-1-C: `forge/std/io`
- [x] `read_line`, `read_stdin`, `eprint`, `eprintln` で標準入出力をラップ。
- [x] `test_read_line_returns_none_on_eof`
- [x] `test_eprintln_writes_to_stderr`
- [x] Snapshot: `test_transpile_io_read_line`

---

## Phase S-2: JSON / 文字列 / ファイル / Regex / UUID / 乱数

### S-2-A: `forge/std/json`
- [x] `stringify`, `stringify_pretty`, `parse` で `serde_json` を利用。
- [x] `test_stringify_struct`
- [x] `test_stringify_list`
- [x] `test_parse_json_object`
- [x] `test_parse_invalid_json_errors`
- [x] Snapshot: `test_transpile_json_stringify`

### S-2-B: `forge/std/string`
- [x] `trim`, `trim_start`, `trim_end`
- [x] `split`, `join`, `starts_with`, `ends_with`, `contains`
- [x] `index_of`, `replace`, `replace_first`
- [x] `to_upper`, `to_lower`, `repeat`
- [x] `pad_left`, `pad_right`, `is_empty`, `char_count`
- [x] Unicode 対応と日本語文字列を想定したテスト

### S-2-C: `forge/std/fs`
- [x] `list_dir`, `make_dir`, `delete_file`
- [x] `path_absolute`, `path_join`, `path_exists`, `path_is_dir`, `path_ext`, `path_stem`
- [x] `test_list_dir_returns_entries`
- [x] `test_make_dir_recursive`
- [x] `test_path_join_combines`
- [x] `test_path_ext_and_stem`

### S-2-D: `forge/std/regex`
- [x] `regex_match`, `regex_find`, `regex_find_all`, `regex_capture`
- [x] `regex_replace`, `regex_replace_first`
- [x] `test_regex_match_digit_pattern`
- [x] `test_regex_find_all_returns_all`
- [x] `test_regex_capture_groups`
- [x] `test_regex_replace_all`
- [x] Snapshot: `test_transpile_regex_match`

### S-2-E: `forge/std/uuid`
- [x] `uuid_v4()` で v4 UUID を生成
- [x] `is_uuid(s)` でフォーマット検証
- [x] `test_uuid_v4_format`
- [x] `test_is_uuid_valid_and_invalid`

### S-2-F: `forge/std/random`
- [x] `random_int(min, max)`
- [x] `random_float()`
- [x] `random_choice(list)`
- [x] `shuffle(list)`
- [x] `seed_random(seed)`
- [x] `test_random_int_in_range`
- [x] `test_random_choice_from_list`
- [x] `test_shuffle_preserves_elements`
- [x] `test_seed_random_reproducible`

---

## Phase S-3: ログ / 設定

### S-3-A: `forge/std/log`
- [x] `Logger` trait と `ConsoleLogger`, `JsonLogger`, `SilentLogger`, `FileLogger`, `MultiLogger` を実装。
- [x] `LOG_LEVEL` によるフィルタリング。
- [x] `test_console_logger_outputs_with_level`
- [x] `test_json_logger_outputs_valid_json`
- [x] `test_silent_logger_outputs_nothing`
- [x] `test_log_level_filter_debug`
- [x] `test_multi_logger_calls_all_backends`
- [x] Snapshot: `test_transpile_logger_json`

### S-3-B: `forge/std/config`
- [x] `Config::load(T)` / `Config::load_from(path)` で TOML を読み込み。
- [x] `FORGE_ENV` を使って `.env.{FORGE_ENV}` などを参照。
- [x] `Config::reload` でホットリロード。
- [x] `test_config_load_uses_field_defaults`
- [x] `test_config_load_toml_overrides_defaults`
- [x] `test_config_load_env_overrides_toml`
- [x] `test_config_load_forge_env_section`
- [x] Snapshot: `test_transpile_config_load`

---

## Phase S-4: `retry`, `cache`, `metrics`

### S-4-A: `forge/std/retry`
- [x] `retry`, `with_backoff`, `CircuitBreaker` を提供。
- [x] `exponential`, `linear`, `constant` バックオフ。
- [x] `@retry`, `@circuit_breaker` デコレータ。
- [x] `test_retry_succeeds_after_failures`
- [x] `test_retry_exhausted_returns_last_error`
- [x] `test_exponential_backoff_timing`
- [x] `test_circuit_breaker_opens_after_threshold`
- [x] `test_circuit_breaker_half_open_recovery`
- [x] Snapshot: `test_transpile_retry_decorator`
- [x] Snapshot: `test_transpile_circuit_breaker_decorator`

### S-4-B: `forge/std/cache`
- [x] `Cache::new`, `get_or_set`, `get`, `set`, `invalidate`, `clear`
- [x] TTL 制御と `@memoize` / `@cache` デコレータ
- [x] `test_cache_get_or_set_caches_result`
- [x] `test_cache_ttl_expires_entry`
- [x] `test_cache_invalidate_removes_entry`
- [x] `test_memoize_does_not_call_fn_twice`
- [x] `test_cache_key_fn_custom_key`
- [x] Snapshot: `test_transpile_memoize_decorator`
- [x] Snapshot: `test_transpile_cache_ttl_decorator`

### S-4-C: `forge/std/metrics`
- [x] `MetricsBackend` trait + `InMemoryBackend`, `LogBackend`, `PrometheusBackend`
- [x] `@timed(metric)` デコレータ
- [x] `test_in_memory_counter_increments`
- [x] `test_in_memory_gauge_updates`
- [x] `test_log_backend_outputs_metric`
- [x] Snapshot: `test_transpile_timed_decorator`

---

## Phase S-5: イベント / パイプライン

### S-5-A: `forge/std/event`
- [x] `EventBus` で `emit`, `on`, `once`, `off`、非同期/同期/順序付きモードと `@on`。
- [x] `test_on_emit_calls_handler`
- [x] `test_once_fires_only_once`
- [x] `test_off_stops_handler`
- [x] `test_multiple_handlers_for_same_event`
- [x] `test_event_does_not_cross_types`
- [x] Snapshot: `test_transpile_emit_async`
- [x] Snapshot: `test_transpile_on_decorator`

### S-5-B: `forge/std/pipeline`
- [x] `pipeline {}` ブロックと `source`/`filter`/`map`/`flat_map`/`group`/`sort`/`take`/`skip`/`each`/`sink`/`parallel` ステップ。
- [x] `CsvSource`, `JsonSource`, `ListSource` などの Source、`StdoutSink`, `CsvSink`, `JsonSink`, `CollectSink` などの Sink、`parallel N` 構文。
- [x] `test_parse_pipeline_block`
- [x] `test_pipeline_filter_map_collect`
- [x] `test_pipeline_group_aggregation`
- [x] `test_pipeline_take_limits_output`
- [x] `test_pipeline_csv_source_stdout_sink`
- [x] Snapshot: `test_transpile_pipeline_iterator_chain`
- [x] Snapshot: `test_transpile_pipeline_parallel_rayon`

---

## 進捗一覧

| Phase | 内容 | 完了 / 合計 |
|---|---|---|
| S-1 | 環境変数 / プロセス / IO | 22 / 22 |
| S-2 | JSON / 文字列 / ファイル / Regex / UUID / 乱数 | 44 / 44 |
| S-3 | ログ / 設定 | 24 / 24 |
| S-4 | リトライ / キャッシュ / メトリクス | 24 / 38 |
| S-5 | イベント / パイプライン | 0 / 34 |
| **合計** | | **66 / 162** |
