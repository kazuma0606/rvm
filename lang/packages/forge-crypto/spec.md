# `forge-crypto` 仕様書

> バージョン: 0.1.0
> 作成: 2026-04-08

---

## 概要

ハッシュ・エンコード・HMAC を提供するパッケージ。
パスワードハッシュ・JWT・API 署名に使用する。

---

## API

```forge
use forge_crypto.*

// ハッシュ
let h = hash_sha256("hello world")          // hex string（64文字）
let h = hash_sha512("hello world")          // hex string（128文字）
let h = hash_md5("hello")                   // hex string（32文字・レガシー用途のみ）

// Base64
let enc = base64_encode("hello")            // "aGVsbG8="
let dec = base64_decode("aGVsbG8=")?        // "hello"
let enc = base64_url_encode("hello")        // URL-safe Base64（パディングなし）

// HMAC
let sig = hmac_sha256("secret-key", "message")   // hex string

// パスワードハッシュ（bcrypt）
let hash  = bcrypt_hash("my-password")?          // string!
let valid = bcrypt_verify("my-password", hash)?  // bool!

// 定数時間比較（タイミング攻撃対策）
let eq = constant_time_eq("abc", "abc")     // bool
```

### 関数一覧

| 関数 | シグネチャ | 説明 |
|---|---|---|
| `hash_sha256` | `(input: string) -> string` | SHA-256 ハッシュ（hex） |
| `hash_sha512` | `(input: string) -> string` | SHA-512 ハッシュ（hex） |
| `base64_encode` | `(input: string) -> string` | Base64 エンコード |
| `base64_decode` | `(input: string) -> string!` | Base64 デコード |
| `base64_url_encode` | `(input: string) -> string` | URL-safe Base64 エンコード |
| `hmac_sha256` | `(key: string, msg: string) -> string` | HMAC-SHA256 署名（hex） |
| `bcrypt_hash` | `(password: string) -> string!` | bcrypt ハッシュ生成 |
| `bcrypt_verify` | `(password: string, hash: string) -> bool!` | bcrypt 検証 |
| `constant_time_eq` | `(a: string, b: string) -> bool` | タイミング攻撃耐性のある等値比較 |

---

## Rust 変換

```rust
// hash_sha256    →  sha2 クレート
// hmac_sha256    →  hmac + sha2 クレート
// base64_*       →  base64 クレート
// bcrypt_*       →  bcrypt クレート
// constant_time_eq →  subtle クレート
```
