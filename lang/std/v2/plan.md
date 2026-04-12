# `forge/std` 標準ライブラリ v2 実装計画

> 仕様: `lang/std/v2/spec.md`
> 前提: forge/std v1 全Phase完了・forge-stdlib クレート存在済み

---

## フェーズ構成

```
Phase W-0: Wasm 基本（load / call）
Phase W-1: WasmOptions + プリセット
Phase W-2: セキュリティサンドボックス（fuel / memory / timeout）
Phase W-3: from_bytes（動的ロード）
Phase C-0: crypto ハッシュ・HMAC
Phase C-1: crypto 署名・検証
Phase Z-0: compress gzip
Phase Z-1: compress brotli
```

W-0〜W-3 は順番に実施。C・Z は W-0 完了後に並走可能。

---

## Phase W-0: Wasm 基本

### 目標

`Wasm.load(path)` / `Wasm.call(fn_name, input)` が動作すること。
Bloom SSRのAnvilからWASMを実行してHTML文字列を受け取れること。

### 実装ステップ

1. **`forge-stdlib` に `wasmtime` を依存追加**（`Cargo.toml`）
2. **`crates/forge-stdlib/src/wasm.rs` 新規作成**
   - `struct Wasm { engine: Arc<Engine>, module: Arc<Module> }`
   - `Wasm::load(path)` → `Module::from_file`
   - `Wasm::call(fn_name, input)` → `Store` + `Instance` を毎回生成して破棄
   - JSON文字列をWASMの線形メモリ経由で渡す（ptr + len）
3. **インタープリタ登録**（`forge-stdlib/src/lib.rs`）
   - `forge/std/wasm` 名前空間に `Wasm` を登録
4. **トランスパイラ対応**（`forge-transpiler/src/codegen.rs`）
   - `Wasm::load` / `Wasm::call` のRust変換ルールを追加

### テスト方針

- `test_wasm_load_valid_binary`: 正常なWASMをロードできる
- `test_wasm_load_invalid_path_errors`: 存在しないパスでエラー
- `test_wasm_call_returns_string`: 関数呼び出しで文字列が返る
- `test_wasm_call_unknown_function_errors`: 存在しない関数でエラー

---

## Phase W-1: WasmOptions + プリセット

### 目標

`Wasm.load_with(path, opts)` が動作し、`trusted()` / `sandboxed()` / `strict()` のプリセットが使えること。

### 実装ステップ

1. **`WasmOptions` struct を Rust側に追加**
2. **`Wasm::load_with` に `WasmOptions` を渡す**
3. **プリセット3種を実装**（`trusted` / `sandboxed` / `strict`）
4. **ForgeScript向けに `WasmOptions` データ型を公開**

### テスト方針

- `test_wasm_options_trusted_has_no_limits`: trusted プリセットの値確認
- `test_wasm_options_sandboxed_has_limits`: sandboxed プリセットの値確認
- `test_wasm_options_strict_has_strict_limits`: strict プリセットの値確認
- `test_wasm_load_with_trusted`: オプション付きロードが動作する

---

## Phase W-2: セキュリティサンドボックス

### 目標

`max_instructions` / `max_memory_mb` / `timeout_ms` の制限が機能し、
超過時に適切なエラーが返ること。

### 実装ステップ

1. **wasmtime の Fuel 機能を有効化**（`max_instructions`）
2. **Store へのメモリ制限設定**（`max_memory_mb`）
3. **Epoch interruption によるタイムアウト**（`timeout_ms`）
4. **エラー型のマッピング**（`WasmFuelExhausted` / `WasmMemoryExceeded` / `WasmTimeout`）

### テスト方針

- `test_wasm_fuel_exhausted_returns_error`: 命令数超過でエラー
- `test_wasm_memory_exceeded_returns_error`: メモリ超過でエラー
- `test_wasm_timeout_returns_error`: タイムアウトでエラー
- `test_wasm_infinite_loop_blocked`: 無限ループが一定時間内にエラーになる

---

## Phase W-3: from_bytes（動的ロード）

### 目標

`Wasm.from_bytes(bytes)` でバイト列からWASMをロードできること。
AI生成コードのコンパイル結果等を動的に実行できること。

### 実装ステップ

1. **`Wasm::from_bytes` を Rust側に追加**（`Module::from_binary`）
2. **`Wasm::from_bytes_with` にも `WasmOptions` を渡せるように**
3. **ForgeScriptの `list<byte>` との型変換**

### テスト方針

- `test_wasm_from_bytes_loads_module`: バイト列からロードできる
- `test_wasm_from_bytes_invalid_errors`: 不正なバイト列でエラー
- `test_wasm_from_bytes_sandboxed`: from_bytes でもオプションが効く

---

## Phase C-0: crypto ハッシュ・HMAC

### 目標

`hash()` / `hmac()` / `hmac_verify()` が動作すること。

### 実装ステップ

1. **`forge-stdlib` に `sha2` / `hmac` を依存追加**
2. **`crates/forge-stdlib/src/crypto.rs` 新規作成**
3. **`HashAlgo` enum と `hash` / `hmac` / `hmac_verify` を実装**
4. **インタープリタ・トランスパイラ登録**

### テスト方針

- `test_hash_sha256_known_value`: SHA-256の既知ハッシュ値と一致
- `test_hash_blake3_known_value`: BLAKE3の既知ハッシュ値と一致
- `test_hmac_sha256_known_value`: HMACの既知値と一致
- `test_hmac_verify_valid`: 正しいMACで true が返る
- `test_hmac_verify_invalid`: 不正なMACで false が返る
- `test_hmac_verify_timing_safe`: 比較がタイミング攻撃耐性を持つ

---

## Phase C-1: crypto 署名・検証

### 目標

`sign()` / `verify()` / `generate_keypair()` が動作すること（Ed25519）。

### 実装ステップ

1. **`forge-stdlib` に `ring` または `ed25519-dalek` を依存追加**
2. **`generate_keypair` / `sign` / `verify` を実装**
3. **ForgeScript向けに `KeyPair` データ型を公開**

### テスト方針

- `test_generate_keypair_returns_valid_keys`: 鍵ペアが生成できる
- `test_sign_and_verify_roundtrip`: 署名→検証の往復
- `test_verify_wrong_key_returns_false`: 異なる鍵で false
- `test_verify_tampered_payload_returns_false`: 改ざんされたペイロードで false

---

## Phase Z-0: compress gzip

### 目標

`compress` / `decompress` が gzip で動作すること。

### 実装ステップ

1. **`forge-stdlib` に `flate2` を依存追加**
2. **`crates/forge-stdlib/src/compress.rs` 新規作成**
3. **`CompressAlgo::Gzip` の `compress_str` / `decompress_str` を実装**
4. **インタープリタ・トランスパイラ登録**

### テスト方針

- `test_gzip_compress_decompress_roundtrip`: 圧縮→展開の往復
- `test_gzip_compressed_smaller_than_input`: 圧縮後のサイズが小さい
- `test_gzip_decompress_invalid_errors`: 不正なバイト列でエラー

---

## Phase Z-1: compress brotli

### 目標

`CompressAlgo::Brotli` が動作すること。ブラウザ配信の標準として brotli を優先。

### 実装ステップ

1. **`forge-stdlib` に `brotli` を依存追加**
2. **`CompressAlgo::Brotli` の実装を追加**

### テスト方針

- `test_brotli_compress_decompress_roundtrip`: 圧縮→展開の往復
- `test_brotli_better_ratio_than_gzip`: 同じ入力で brotli の方が小さい

---

## 実装順序（推奨）

```
W-0 → W-1 → W-2 → W-3
              ↓
         C-0 / Z-0（並走可能）
              ↓
         C-1 / Z-1（並走可能）
```

---

## 変更ファイル一覧

```
crates/forge-stdlib/
  Cargo.toml              ← wasmtime / sha2 / hmac / ring / flate2 / brotli 追加
  src/
    lib.rs                ← wasm / crypto / compress モジュール登録
    wasm.rs               ← 新規（Phase W-0〜W-3）
    crypto.rs             ← 新規（Phase C-0〜C-1）
    compress.rs           ← 新規（Phase Z-0〜Z-1）

crates/forge-transpiler/
  src/codegen.rs          ← wasm / crypto / compress のRust変換ルール追加
```
