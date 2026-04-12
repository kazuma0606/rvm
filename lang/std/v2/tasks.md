# `forge/std` v2 tasks

> spec: `lang/std/v2/spec.md`
> plan: `lang/std/v2/plan.md`

---

## Phase W-0: Wasm basic (load / call)

- [x] Add `wasmtime` to `crates/forge-stdlib/Cargo.toml`
- [x] Create `crates/forge-stdlib/src/wasm.rs`
- [x] Implement `Wasm::load(path)` using `Module::from_file`
- [x] Implement `Wasm::call(fn_name, input)` with per-call Store + Instance
- [x] JSON string transfer via WASM linear memory (ptr + len)
- [x] Register `forge/std/wasm` in `forge-stdlib/src/lib.rs`
- [x] Add transpiler rules for `Wasm::load` / `Wasm::call` in `codegen.rs`
- [x] `test_wasm_load_valid_binary`
- [x] `test_wasm_load_invalid_path_errors`
- [x] `test_wasm_call_returns_string`
- [x] `test_wasm_call_unknown_function_errors`

---

## Phase W-1: WasmOptions + presets

- [x] Add `WasmOptions` struct to `wasm.rs`
- [x] Implement `WasmOptions::trusted()`
- [x] Implement `WasmOptions::sandboxed()`
- [x] Implement `WasmOptions::strict()`
- [x] Implement `Wasm::load_with(path, opts)`
- [x] Expose `WasmOptions` as ForgeScript data type
- [x] `test_wasm_options_trusted_has_no_limits`
- [x] `test_wasm_options_sandboxed_has_limits`
- [x] `test_wasm_options_strict_has_strict_limits`
- [x] `test_wasm_load_with_trusted`

---

## Phase W-2: Security sandbox

- [x] Enable wasmtime Fuel for `max_instructions`
- [x] Configure Store memory limit for `max_memory_mb`
- [x] Implement Epoch interruption for `timeout_ms`
- [x] Map errors: `WasmFuelExhausted` / `WasmMemoryExceeded` / `WasmTimeout`
- [x] `test_wasm_fuel_exhausted_returns_error`
- [x] `test_wasm_memory_exceeded_returns_error`
- [x] `test_wasm_timeout_returns_error`
- [x] `test_wasm_infinite_loop_blocked`

---

## Phase W-3: from_bytes (dynamic load)

- [x] Implement `Wasm::from_bytes(bytes)` using `Module::from_binary`
- [x] Implement `Wasm::from_bytes_with(bytes, opts)`
- [x] Handle `list<byte>` to Rust `Vec<u8>` conversion
- [x] `test_wasm_from_bytes_loads_module`
- [x] `test_wasm_from_bytes_invalid_errors`
- [x] `test_wasm_from_bytes_sandboxed`

---

## Phase W-4: Cross-language integration tests

> **Prerequisite**: W-0~W-3 complete.
> Verify that `forge/std/wasm` can load WASM binaries compiled from languages other than ForgeScript,
> and that inputs/outputs match exactly. Proves the engine is truly language-agnostic.

### Test fixtures

- [x] Write `tests/fixtures/wasm/add.c` (exports `add(i32, i32) -> i32`)
- [x] Write `tests/fixtures/wasm/echo.c` (exports `echo(ptr, len) -> ptr` -- round-trips a string)
- [x] Write `tests/fixtures/wasm/add.cpp` (C++ version of add, same signature)
- [x] Compile fixtures to `.wasm` using `wasi-sdk` or `emcc` and check binaries into repo
- [x] Document required toolchain in `tests/fixtures/wasm/README.md`

### Correctness tests

- [x] `test_wasm_load_c_binary`: load `add.wasm` compiled from C
- [x] `test_wasm_c_add_returns_correct_sum`: call `add(2, 3)` returns `5`
- [x] `test_wasm_load_cpp_binary`: load `add.wasm` compiled from C++
- [x] `test_wasm_cpp_add_matches_c_output`: same input produces same output as C version
- [x] `test_wasm_echo_string_roundtrip`: pass JSON string, get identical string back
- [x] `test_wasm_output_encoding_consistent`: output bytes match between C and ForgeScript WASM

### Sandbox tests (cross-language)

- [x] `test_wasm_c_binary_respects_fuel_limit`: sandboxed C WASM is interrupted by fuel exhaustion
- [x] `test_wasm_c_binary_respects_memory_limit`: sandboxed C WASM is interrupted by memory limit

---

## Phase C-0: crypto hash / HMAC

- [x] Add `sha2` / `hmac` to `crates/forge-stdlib/Cargo.toml`
- [x] Create `crates/forge-stdlib/src/crypto.rs`
- [x] Implement `HashAlgo` enum (Sha256 / Sha512 / Blake3)
- [x] Implement `hash(input, algo) -> string`
- [x] Implement `hmac(input, secret, algo) -> string`
- [x] Implement `hmac_verify(input, mac, secret, algo) -> bool` (timing-safe)
- [x] Register `forge/std/crypto` in `forge-stdlib/src/lib.rs`
- [x] Add transpiler rules in `codegen.rs`
- [x] `test_hash_sha256_known_value`
- [x] `test_hash_blake3_known_value`
- [x] `test_hmac_sha256_known_value`
- [x] `test_hmac_verify_valid`
- [x] `test_hmac_verify_invalid`
- [x] `test_hmac_verify_timing_safe`

---

## Phase C-1: crypto sign / verify

- [x] Add `ring` or `ed25519-dalek` to `crates/forge-stdlib/Cargo.toml`
- [x] Implement `generate_keypair() -> KeyPair!`
- [x] Implement `sign(payload, private_key) -> string!`
- [x] Implement `verify(payload, signature, public_key) -> bool!`
- [x] Expose `KeyPair` as ForgeScript data type
- [x] `test_generate_keypair_returns_valid_keys`
- [x] `test_sign_and_verify_roundtrip`
- [x] `test_verify_wrong_key_returns_false`
- [x] `test_verify_tampered_payload_returns_false`

---

## Phase Z-0: compress gzip

- [x] Add `flate2` to `crates/forge-stdlib/Cargo.toml`
- [x] Create `crates/forge-stdlib/src/compress.rs`
- [x] Implement `CompressAlgo` enum (Gzip / Brotli)
- [x] Implement `compress_str(input, algo) -> list<byte>!` for Gzip
- [x] Implement `decompress_str(input, algo) -> string!` for Gzip
- [x] Register `forge/std/compress` in `forge-stdlib/src/lib.rs`
- [x] Add transpiler rules in `codegen.rs`
- [x] `test_gzip_compress_decompress_roundtrip`
- [x] `test_gzip_compressed_smaller_than_input`
- [x] `test_gzip_decompress_invalid_errors`

---

## Phase Z-1: compress brotli

- [x] Add `brotli` to `crates/forge-stdlib/Cargo.toml`
- [x] Implement `compress_str` / `decompress_str` for Brotli
- [x] `test_brotli_compress_decompress_roundtrip`
- [x] `test_brotli_better_ratio_than_gzip`

---

## Progress

| Phase | Description | Done / Total |
|---|---|---|
| W-0 | Wasm basic | 11 / 11 |
| W-1 | WasmOptions + presets | 10 / 10 |
| W-2 | Security sandbox | 8 / 8 |
| W-3 | from_bytes | 6 / 6 |
| W-4 | Cross-language integration tests | 13 / 13 |
| C-0 | crypto hash / HMAC | 14 / 14 |
| C-1 | crypto sign / verify | 9 / 9 |
| Z-0 | compress gzip | 10 / 10 |
| Z-1 | compress brotli | 4 / 4 |
| **Total** | | **85 / 85** |
