# RVM / ForgeScript Plan

## 方針

RVM は JVM に近い位置づけの実行基盤として設計するが、最初から JIT/AOT や Cargo 統合に踏み込みすぎず、まずは最小構成で動く実装を優先する。

設計思想としてはクリーンアーキテクチャの考え方を取り入れる。ただし、Web バックエンドのような典型的な層分割をそのまま持ち込むのではなく、VM 向けに以下を重視する。

- 内側のコアを安定させる
- 外部依存を境界で止める
- 言語フロントエンドと実行基盤を分離する
- ホットパスに過剰な抽象化を入れない

## 基本方針

最初に作るべきなのは VM 本体の重厚な機構ではなく、以下の確定である。

1. ForgeScript の最小仕様
2. 値モデル
3. 実行方式
4. Rust/Cargo 連携の境界

初期の実行方式は次の流れを前提にする。

```text
lexer -> parser -> AST -> bytecode -> VM
```

JIT、AOT、WASM、Cargo 自動導入は後段に置く。

## 言語仕様の最小スコープ

初期フェーズでは以下に絞る。

- `let`
- 式文
- 整数
- 文字列
- 四則演算
- 関数呼び出し
- `print`

最初の成功条件は、次のコードが動くこと。

```fs
let x = 1
let y = 2
print(x + y)
```

## 推奨 workspace / crate 構成

ForgeScript を含めた将来構成案は次の通り。

```text
rvm/
  Cargo.toml
  rust-toolchain.toml
  crates/
    fs-span/
    fs-ast/
    fs-lexer/
    fs-parser/
    fs-hir/
    fs-analyzer/
    fs-bytecode/
    fs-compiler/

    rvm-core/
    rvm-memory/
    rvm-runtime/
    rvm-host/
    rvm-stdlib/

    fs-cli/
    fs-repl/

    fs-tests/
```

## 各 crate の責務

### ForgeScript 側

- `fs-span`
  - ソース位置管理
  - `Span`, `SourceId`, `FileId`

- `fs-ast`
  - AST 定義
  - `Expr`, `Stmt`, `Module`, `Item`

- `fs-lexer`
  - 字句解析

- `fs-parser`
  - 構文解析

- `fs-hir`
  - 束縛解決後の中間表現
  - 将来の最適化や型検査の足場

- `fs-analyzer`
  - 名前解決
  - スコープ解析
  - 簡易型検査
  - 定数評価

- `fs-bytecode`
  - バイトコード定義
  - `Opcode`, `Instruction`, `Chunk`, `ConstantPool`

- `fs-compiler`
  - `AST/HIR -> bytecode`

### RVM 側

- `rvm-core`
  - 共通基本型
  - `Value`, `VmError`, `FunctionId`, `ModuleId`, `NativeFn`, `CallFrame`

- `rvm-memory`
  - ヒープオブジェクト管理
  - 文字列 intern
  - GC または参照カウント

- `rvm-runtime`
  - 命令実行器
  - スタック
  - フレーム管理
  - 関数呼び出し

- `rvm-host`
  - 外界との境界
  - ファイル読み込み
  - 標準出力
  - 時刻
  - 環境変数
  - パッケージ解決

- `rvm-stdlib`
  - 組み込み関数
  - `print`, `len`, `read_file` など

### アプリ層

- `fs-cli`
  - `forge run`, `forge check`, `forge compile`

- `fs-repl`
  - REPL

- `fs-tests`
  - 統合テスト
  - 言語仕様テスト
  - 実行テスト

## 依存関係の基本形

```text
fs-lexer    -> fs-span
fs-ast      -> fs-span
fs-parser   -> fs-lexer, fs-ast, fs-span
fs-hir      -> fs-span
fs-analyzer -> fs-ast or fs-hir, fs-span
fs-bytecode -> fs-span, rvm-core
fs-compiler -> fs-ast or fs-hir, fs-bytecode, fs-analyzer

rvm-memory  -> rvm-core
rvm-host    -> rvm-core
rvm-stdlib  -> rvm-core, rvm-host
rvm-runtime -> rvm-core, rvm-memory, rvm-host, fs-bytecode

fs-cli      -> fs-parser, fs-analyzer, fs-compiler, rvm-runtime, rvm-host, rvm-stdlib
fs-repl     -> fs-parser, fs-compiler, rvm-runtime, rvm-host, rvm-stdlib
fs-tests    -> 横断
```

重要なのは、`fs-*` を言語フロントエンド、`rvm-*` を実行基盤として分離すること。

## 初期フェーズの最小構成

最初から crate を増やしすぎない。MVP は以下で十分。

```text
crates/
  fs-ast/
  fs-lexer/
  fs-parser/
  fs-bytecode/
  fs-compiler/
  rvm-core/
  rvm-runtime/
  rvm-host/
  fs-cli/
```

この段階では以下は後回しでよい。

- `fs-hir`
- `fs-analyzer`
- `rvm-memory`
- `rvm-stdlib`
- `fs-repl`

## MVP で必要な中身

- `fs-lexer`
  - 識別子
  - 整数
  - 文字列
  - `(){} , ; = + - * /`

- `fs-parser`
  - `let`
  - 式文
  - 関数呼び出し

- `fs-bytecode`
  - `LoadConst`
  - `LoadGlobal`
  - `StoreGlobal`
  - `Add`
  - `Call`
  - `Return`
  - `Pop`

- `rvm-core`
  - `Value`
  - `VmError`

- `rvm-runtime`
  - スタックマシン

- `rvm-host`
  - 標準出力抽象

- `fs-cli`
  - `forge run file.fs`

## Host 境界の候補

Cargo 統合やモジュール解決を見据えるなら、外部依存は `rvm-host` に閉じ込める。

```rust
pub trait ModuleLoader {
    fn load(&self, path: &str) -> Result<String, HostError>;
}
```

```rust
pub trait Output {
    fn stdout(&self, text: &str);
    fn stderr(&self, text: &str);
}
```

```rust
pub trait PackageResolver {
    fn resolve(&self, name: &str, version: Option<&str>) -> Result<ResolvedPackage, HostError>;
}
```

## 実装順序

1. `fs-ast`, `fs-lexer`, `fs-parser`
2. `fs-bytecode`, `fs-compiler`
3. `rvm-core`, `rvm-runtime`
4. `rvm-host`, `fs-cli`
5. `print`, `let`, 四則演算が動くところまで完成
6. その後に `module`, `stdlib`, `package resolver`
7. 最後に `JIT`, `AOT`, `WASM`

## 設計上の注意

- Rust の所有権をそのまま言語仕様に持ち込まない
- ForgeScript 側は GC か参照カウントで単純化する
- バイトコード命令は最小限に保つ
- 例外処理や async は初期版では入れない
- Cargo 統合は言語機能ではなくホスト機能として分離する
- 将来のための抽象化を先行しすぎない

## 次の具体的作業

次に着手すべき実務は以下。

1. Cargo workspace を初期化
2. MVP 対象 crate を作成
3. `print(1 + 2)` が動く最小パイプラインを作る
4. AST と bytecode の最小データ構造を固める
5. VM の命令ディスパッチを実装する
