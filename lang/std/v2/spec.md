# `forge/std` 標準ライブラリ v2 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-12
> 対象: forge/std v2 追加モジュール（WASM実行・暗号・圧縮）

---

## モジュール一覧

| モジュール | 概要 | Rust依存 |
|---|---|---|
| [`forge/std/wasm`](#forgestdwasm) | サーバーサイドWASM実行 | `wasmtime` |
| [`forge/std/crypto`](#forgestdcrypto) | ハッシュ・HMAC・署名・検証 | `sha2`, `hmac`, `ring` |
| [`forge/std/compress`](#forgestdcompress) | gzip / brotli 圧縮・展開 | `flate2`, `brotli` |

---

## `forge/std/wasm`

サーバーサイドでWASMバイナリを実行するプリミティブ。
Bloom SSRのハイドレーション基盤だが、プラグインシステム・セキュアなユーザーコード実行等にも汎用的に使える。

**Rust実装**: `crates/forge-stdlib/src/wasm.rs`

### データ型

```forge
// WASMモジュール（Arc<wasmtime::Module>をラップ。スレッドセーフ）
data Wasm { }

// 実行オプション
data WasmOptions {
  max_instructions: number? = none,  // none = 無制限
  max_memory_mb:    number? = none,  // none = 無制限
  timeout_ms:       number? = none,  // none = 無制限
  allow_fs:         bool    = false,
  allow_net:        bool    = false,
  allow_env:        bool    = false,
}
```

### API

```forge
impl Wasm {
  // ファイルからWASMバイナリをロード・コンパイル（起動時1回）
  fn load(path: string) -> Wasm!
  fn load_with(path: string, opts: WasmOptions) -> Wasm!

  // バイト列からロード（動的コンパイル用）
  fn from_bytes(bytes: list<byte>) -> Wasm!
  fn from_bytes_with(bytes: list<byte>, opts: WasmOptions) -> Wasm!

  // 指定した関数を呼び出す
  // input / output ともにJSON文字列（シリアライズは呼び出し側が担う）
  fn call(self, fn_name: string, input: string) -> string!
}

impl WasmOptions {
  fn trusted()   -> WasmOptions  // 制限なし・全Capability許可
  fn sandboxed() -> WasmOptions  // 標準的な制限・Capability不可
  fn strict()    -> WasmOptions  // 厳格な制限・Capability不可
}
```

### プリセット定義

| プリセット | max_instructions | max_memory_mb | timeout_ms | allow_* |
|---|---|---|---|---|
| `trusted()` | none | none | none | true |
| `sandboxed()` | 1_000_000 | 16 | 500 | false |
| `strict()` | 100_000 | 4 | 100 | false |

### 型マッピング

WASMのプリミティブ型（i32/i64/f32/f64）とForgeScriptの型を橋渡しするため、
引数・戻り値ともに**JSONシリアライズ経由**とする。

```
ForgeScript 値 → json.stringify() → 文字列 → WASMの線形メモリ（ptr + len）
                                            ← WASM処理結果（HTML文字列等）
```

### エラー型

`Wasm.call` が返す `T!` のエラーケース：

| エラー | 原因 |
|---|---|
| `WasmLoadError` | バイナリの読み込み・コンパイル失敗 |
| `WasmCallError` | 関数が存在しない・シグネチャ不一致 |
| `WasmFuelExhausted` | max_instructions 超過 |
| `WasmMemoryExceeded` | max_memory_mb 超過 |
| `WasmTimeout` | timeout_ms 超過 |
| `WasmTrap` | WASM実行中のトラップ（ゼロ除算等） |

### ホットリロード（forge dev）

```
.bloom ファイルが変更
  → forge が再コンパイル → 新しい .wasm を生成
  → wasmtime がキャッシュしているモジュールを差し替え
  → WebSocket でブラウザに通知 → ブラウザが新しい WASM をロード
```

---

## `forge/std/crypto`

ハッシュ・HMAC・署名・検証を提供する。
WASMプラグインの署名検証・パスワードハッシュ・APIシグネチャ検証等に使う。

**Rust実装**: `crates/forge-stdlib/src/crypto.rs`

### API

```forge
// ハッシュアルゴリズム
enum HashAlgo {
  Sha256,
  Sha512,
  Blake3,
}

// ハッシュ（hex文字列を返す）
fn hash(input: string, algo: HashAlgo) -> string

// HMAC（hex文字列を返す）
fn hmac(input: string, secret: string, algo: HashAlgo) -> string

// HMAC 検証（タイミング攻撃耐性あり）
fn hmac_verify(input: string, mac: string, secret: string, algo: HashAlgo) -> bool

// 署名・検証（Ed25519）
fn sign(payload: string, private_key: string) -> string!
fn verify(payload: string, signature: string, public_key: string) -> bool!

// 鍵ペア生成
fn generate_keypair() -> KeyPair!

data KeyPair {
  public_key:  string,
  private_key: string,
}
```

### 用途例

```forge
use forge/std/crypto.{ hash, hmac_verify, HashAlgo }

// パスワードハッシュ
let digest = hash(password, HashAlgo.Sha256)

// APIシグネチャ検証
let ok = hmac_verify(req.body, req.headers.get("x-signature")?, secret, HashAlgo.Sha256)
if !ok { return Response.unauthorized() }

// WASMプラグインの署名検証
let sig     = fs.read("plugins/user.wasm.sig")?
let payload = fs.read_bytes("plugins/user.wasm")?
verify(payload, sig, trusted_public_key)?
```

---

## `forge/std/compress`

gzip・brotli 圧縮・展開を提供する。
`forge build --web` のWASMバイナリ圧縮・HTTPレスポンス圧縮に使う。

**Rust実装**: `crates/forge-stdlib/src/compress.rs`

### API

```forge
// 圧縮アルゴリズム
enum CompressAlgo {
  Gzip,
  Brotli,
}

// 圧縮
fn compress(input: list<byte>, algo: CompressAlgo) -> list<byte>!
fn compress_str(input: string, algo: CompressAlgo) -> list<byte>!

// 展開
fn decompress(input: list<byte>, algo: CompressAlgo) -> list<byte>!
fn decompress_str(input: list<byte>, algo: CompressAlgo) -> string!
```

### 用途例

```forge
use forge/std/compress.{ compress_str, CompressAlgo }

// Anvilでのレスポンス圧縮
let body       = json.stringify(data)
let compressed = compress_str(body, CompressAlgo.Brotli)?
Response.new(compressed)
  .header("Content-Encoding", "br")
  .header("Content-Type", "application/json")
```

`forge build --web` のビルドパイプラインでは内部的に自動使用される。
