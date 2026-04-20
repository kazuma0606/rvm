# Bloom Component ABI v0

Bloom の Web ランタイムは、任意の WASM を直接受け入れるのではなく、特定の export/import と command buffer 形式を満たす WASM を受け入れる。

この文書は、2026-04-19 時点の実装に対応する最小 ABI を定義する。

## 目的

Bloom Component ABI v0 の目的は次の 3 点。

- サーバーが SSR した HTML に対して、後から WASM が attach できること
- WASM が DOM を直接触らず、DOM command 列を JS ランタイムへ渡すこと
- ForgeScript 以外の言語でも、同じ ABI を満たせば Bloom ランタイムで動かせるようにすること

## 実行モデル

Bloom の実行モデルは次の 2 経路に分かれる。

1. SSR
- サーバー側で `render(<Component />)` 相当の HTML を生成する
- 必要なら WASM の `__forge_init()` を実行し、command buffer を HTML に適用する

2. Hydration
- ブラウザで `forge.min.js` が WASM をロードする
- 既存 DOM があるなら `__forge_attach()` を優先して呼ぶ
- attach がない、または失敗した場合は `__forge_init()` を呼ぶ
- command buffer を `forge.min.js` が解釈して DOM に適用する

## 必須 exports

Bloom ランタイムが期待する最低限の exports は次のとおり。

### `memory`

- 種別: WebAssembly linear memory
- 用途: JS 側が command buffer と文字列データを読む

### `__forge_init()`

- シグネチャ: `() -> ()`
- 用途: 初期状態をセットし、command buffer に DOM 命令を書き込む

### `__forge_attach()`

- シグネチャ: `() -> ()`
- 用途: SSR 済み DOM に対してイベント再接続中心の命令を書く
- 備考: ブラウザ側では存在すれば優先して呼ばれる。SSR 側（`forge-vm`）は `__forge_attach` を呼ばず `__forge_init` のみを使う

### `__forge_pull_commands_ptr()`

- シグネチャ: `() -> i32`
- 用途: command buffer の先頭ポインタを返す

### `__forge_pull_commands_len()`

- シグネチャ: `() -> i32`
- 用途: command buffer の要素数を返す
- 単位: `i32` 要素数。byte 数ではない
- 備考: `forge.min.js` は `new Int32Array(memory.buffer, ptr, len)` で読む。byte 数と混同しないこと

### `__forge_receive_events(kind, target_ptr, target_len)`

- シグネチャ: `(i32, i32, i32) -> ()`
- 用途: JS 側からイベントを WASM へ返す
- `kind`: イベント種別（ABI v0 では未定義。現状の実装は `kind` を参照せず target id のみで dispatch する）
- `target_ptr`, `target_len`: 対象 id 文字列の byte 範囲

## 必須 imports

2026-04-19 時点で、Bloom ランタイムが WASM に提供する import は次のとおり。

### `env.forge_log(ptr, len)`

- シグネチャ: `(i32, i32) -> ()`
- 用途: WASM からブラウザ console へ文字列ログを出す
- 実装:
  - browser: `forge.min.js` が `[Bloom] ...` として `console.log` する
  - SSR: `forge-vm` は no-op import を与える

## Command Buffer

Bloom ABI v0 では、WASM は DOM を直接操作しない。代わりに `i32` 配列として command buffer を構築し、JS 側がそれを解釈して DOM に適用する。

### 基本ルール

- command buffer は `i32` の連続列
- 各 command は先頭に opcode を置く
- 後続の引数は opcode ごとの固定レイアウトに従う
- 文字列は `memory` 上に UTF-8 byte 列として置き、`ptr + len` で参照する

## Opcode 一覧

2026-04-19 時点で確認できる opcode は次のとおり。

各 command の引数レイアウトは下記のとおり i32 列として並ぶ。文字列は `ptr`（memory 上の offset）と `len`（byte 長）の 2 要素で表現する。

### `1: SET_TEXT`

- 意味: 対象ノードの `textContent` を更新する
- レイアウト: `[1, id_ptr, id_len, text_ptr, text_len]`

### `2: SET_ATTR`

- 意味: 対象ノードの属性を設定する
- レイアウト: `[2, id_ptr, id_len, name_ptr, name_len, value_ptr, value_len]`

### `3: ADD_LISTENER`

- 意味: 対象ノードにイベント handler 名を紐付ける
- レイアウト: `[3, id_ptr, id_len, event_ptr, event_len, handler_ptr, handler_len]`
- 備考: hydration 時、SSR で既に `data-on-<event>` が設定済みの場合は重複登録しない

### `4: SET_CLASS`

- 意味: `className` を更新する
- レイアウト: 未確定（ABI v0 では実装依存）
- 備考: 実装は `forge.min.js` 側にある

### `5: INSERT_NODE`

- 意味: 指定位置にノード断片を挿入する
- レイアウト: 未確定（ABI v0 では実装依存）

### `6: REMOVE_NODE`

- 意味: 指定ノードを削除する
- レイアウト: 未確定（ABI v0 では実装依存）

### `7: REPLACE_INNER`

- 意味: 指定ノードの `innerHTML` を置き換える
- レイアウト: 未確定（ABI v0 では実装依存）

### `9: ATTACH`

- 意味: SSR 済みノードに対して hydration attach を行う
- 備考: `forge-vm` 側でも SSR 反映時に扱われる

## 文字列とメモリ

Bloom ABI v0 の文字列は次の前提に従う。

- 文字列は UTF-8 byte 列
- JS 側は `memory.buffer.slice(ptr, ptr + len)` を読んで `TextDecoder` で復元する
- null 終端は不要

数値ログなどの一時文字列は、WASM 側で一時バッファを持って `forge_log(ptr, len)` へ渡してよい。

## イベント

ブラウザ側のイベントフローは次のとおり。

1. WASM が `ADD_LISTENER` command を出す
2. `forge.min.js` が対象ノードへ `addEventListener(...)` を登録する
3. イベント発火時に JS が `__forge_receive_events(kind, target_ptr, target_len)` を呼ぶ
4. WASM が state を更新し、次の command buffer を構築する

ABI v0 では、イベント payload は薄い。最低限の target id と event kind を返す形が前提で、DOM event 全体を渡す設計にはなっていない。

### event kind の値（ABI v0）

ABI v0 では `kind` の値は仕様として未定義であり、現状の実装は `kind` を参照せず target id のみでハンドラを dispatch する。将来的に複数イベント種別を区別する際に定義予定。

## SSR との関係

Bloom の SSR は、WASM が生成する command をサーバー側でも解釈できる前提に立っている。

そのため SSR で使う WASM も、少なくとも次を満たす必要がある。

- `memory`
- `__forge_init()`
- `__forge_pull_commands_ptr()`
- `__forge_pull_commands_len()`

加えて、import に `env.forge_log` がある場合でも SSR で instantiate できるよう、ホスト側で no-op を差せる必要がある。

## C / C++ から実装する場合の最小形

理論上、C / C++ からでも ABI v0 を満たせば Bloom で動かせる。

必要になる export/import の形は概ね次のようになる。

```cpp
extern "C" {
  void __forge_init();
  void __forge_attach();
  int __forge_pull_commands_ptr();
  int __forge_pull_commands_len();
  void __forge_receive_events(int kind, const char* target_ptr, int target_len);
  void forge_log(const char* ptr, int len);
}
```

ただし、これだけでは不十分で、実際には次も必要。

- linear memory 上に command buffer を置く
- 文字列を `ptr + len` で参照できるようにする
- Bloom の opcode レイアウトどおりに `i32` を並べる
- `__forge_attach()` / `__forge_init()` のどちらかで command を積む

## Tailwind / CSS との関係

Tailwind は ABI の一部ではない。

- WASM の役割: command buffer を出す
- HTML/CSS の役割: 見た目を決める

そのため C++ WASM であっても、`class="..."` を持つ SSR HTML や `SET_CLASS` command を正しく出せれば、Tailwind 自体は通常どおり適用される。

## 制限

ABI v0 はまだ安定版ではなく、実装依存の部分が多い。

特に次は今後変わる可能性がある。

- opcode の増減
- 各 command の厳密な引数並び
- イベント payload の形式
- host import の追加

他言語対応を本格化するなら、次の整備が必要。

- command buffer の binary layout を表形式で固定する
- conformance test を追加する
- C ABI 向けのサンプル実装を用意する
- `Bloom Component ABI v1` として互換性方針を決める

## 現状の結論

Bloom は「ForgeScript 専用の WASM ランタイム」ではなく、「Bloom Component ABI を満たす WASM ランタイム」として拡張できる余地がある。

ただし現状は ABI がまだ実装先行で、ForgeScript 以外の言語から使いやすい状態ではない。現時点では、他言語対応は可能だが、SDK と仕様固定が未整備という位置づけになる。
