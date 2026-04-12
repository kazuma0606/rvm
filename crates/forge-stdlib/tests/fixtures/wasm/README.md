# WASM Fixtures

These fixtures are intended for `forge-stdlib` cross-language integration tests.

## Source Files

- `add.c`: exports `add(i32, i32) -> i32`
- `echo.c`: exports `memory`, `alloc(i32) -> i32`, `echo(ptr, len) -> packed(ptr, len)`
- `add.cpp`: C++ version of `add(i32, i32) -> i32`

## Expected ABI

`forge_stdlib::wasm::Wasm::call` currently expects this string ABI:

- exported `memory`
- exported `alloc(i32) -> i32`
- exported function `(ptr: i32, len: i32) -> i64`
- return value packs `ptr` in the high 32 bits and `len` in the low 32 bits

That means `echo.c` is directly compatible today. `add.c` and `add.cpp` are useful fixtures for future numeric-call support, but are not yet exercised by automated tests in this repository.

## Toolchains

One of the following is required to build real fixture binaries:

- `wasi-sdk`
- LLVM/Clang with a wasm target
- `emcc`
- `zig cc`

## Example Commands

### C

```powershell
clang --target=wasm32-wasi -O2 -nostdlib -Wl,--no-entry -Wl,--export-all -o add.wasm add.c
clang --target=wasm32-wasi -O2 -nostdlib -Wl,--no-entry -Wl,--export-all -o echo.wasm echo.c
```

### C++

```powershell
clang++ --target=wasm32-wasi -O2 -nostdlib -Wl,--no-entry -Wl,--export-all -o add_cpp.wasm add.cpp
```

### Zig

```powershell
$env:TMP="C:\path\to\repo\.tmp"
$env:TEMP="C:\path\to\repo\.tmp"
$env:ZIG_LOCAL_CACHE_DIR="C:\path\to\repo\.zig-cache"
$env:ZIG_GLOBAL_CACHE_DIR="C:\path\to\repo\.zig-global-cache"

zig cc --% --target=wasm32-freestanding -O2 -nostdlib -Wl,--no-entry -Wl,--export=add -o add.wasm add.c
zig c++ --% --target=wasm32-freestanding -O2 -nostdlib -Wl,--no-entry -Wl,--export=add -o add_cpp.wasm add.cpp
zig cc --% --target=wasm32-freestanding -O2 -nostdlib -Wl,--no-entry -Wl,--export-memory -Wl,--export=alloc -Wl,--export=echo -Wl,--export=__heap_base -o echo.wasm echo.c
```

## Current Status

The current workspace does not have `clang`, `clang++`, or `emcc` on `PATH`, so these source fixtures are checked in ahead of binary generation.
