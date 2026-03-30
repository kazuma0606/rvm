## はじめに

<!-- 2026/03/17: リポジトリURL追加 -->
**リポジトリ:** [https://github.com/kazuma0606/rvm](https://github.com/kazuma0606/rvm)

Rustはその高い安全性とパフォーマンスにより、多くの開発者に支持されるプログラミング言語ですが、所有権やライフタイムの概念は初心者にとって大きな障壁となります。また、Cargoを活用した依存関係の管理も強力である一方で、手動での設定が煩雑になりがちです。

このような課題を解決し、Rustのエコシステムをより手軽に活用できるようにするため、**ForgeScript（FS）とRust Virtual Machine（RVM）** を構想しました。

## ForgeScript + RVM とは？

ForgeScript（FS）はRustに準拠しつつ、所有権やライフタイム、mutの扱いをブラックボックス化し、より扱いやすくすることを目的としたスクリプト言語です。また、Rustのエコシステムを最大限に活用するため、**Cargoを内部的に動作させ、`use <crate>` だけでクレートを自動インストールできる仕組み** を導入します。

これを実行するプラットフォームが **Rust Virtual Machine（RVM）** です。RVMはForgeScriptのバイトコードを解釈・実行する仮想マシンであり、将来的にはJITコンパイルによる最適化やWebAssembly（WASM）対応も視野に入れています。

## ForgeScript の特徴

### 1. Rustの難しい概念をブラックボックス化
<!-- 2026/03/17: 表は従来どおり、将来の目標として記載 -->
| Rustの難しい要素 | ForgeScriptでの扱い |
|----------------|----------------|
| **所有権 (Ownership)** | **すべて参照カウント（Rc/Arc）で自動管理** |
| **ライフタイム (Lifetime)** | **明示的なライフタイム指定不要（内部的に `Box<T>` で管理）** |
| **mutの制約** | **デフォルトで可変（mutを省略可能）** |

```rust
fn main() {
    let name = "ForgeScript";  // Rc<String> で管理
    println("Hello, " + name);
}
```

### 2. Cargoを自動で管理
Rustでは依存関係を手動で `Cargo.toml` に記述する必要がありますが、ForgeScriptでは **`use <crate>` を書くだけで自動的にインストール・利用** できるようにします。

```rust
use serde

fn main() {
    let data = serde.json_parse('{"key": "value"}')
    println(data["key"])
}
```

#### Cargoの自動インストール（内部処理）
```rust
use std::process::Command;

fn install_crate(crate_name: &str) {
    let output = Command::new("cargo")
        .arg("add")
        .arg(crate_name)
        .output()
        .expect("Failed to execute Cargo");
    
    if output.status.success() {
        println!("Crate `{}` installed successfully!", crate_name);
    } else {
        eprintln!("Error installing `{}`: {}", crate_name, String::from_utf8_lossy(&output.stderr));
    }
}
```

### 3. Cargoと統一されたエラーハンドリング
Cargoの依存関係エラーが発生した際、ForgeScriptでも **Cargoと同じスタイルのエラーメッセージ** を表示します。

```bash
[FS ERROR] Failed to install `serde = "^2.0"`
Candidate versions found: 1.0.130, 1.0.129, 1.0.128
Does not match the given version requirement.
```

これにより、Rust開発者が違和感なくForgeScriptを利用できるようになります。

## RVM（Rust Virtual Machine）の概要
RVMはForgeScriptのコードを解釈・実行する仮想マシンで、将来的に **JITコンパイル（Cranelift）やAOTコンパイル、WASM対応** を目指します。

### 1. バイトコード解釈方式
まずはバイトコードを生成し、それを解釈実行するインタープリタ型VMを構築。

```rust
struct RVM {
    stack: Vec<Value>,
    memory: HashMap<String, Value>,
}
```

### 2. JITコンパイル対応（Cranelift）
頻繁に実行されるコードをJITコンパイルし、ネイティブコードとして最適化。

```rust
use cranelift::prelude::*;
fn compile_jit() -> JITModule { /* 実装 */ }
```

### 3. AOTコンパイル（事前コンパイル）
スクリプトを `.exe` や `.out` にコンパイルし、単体実行可能に。

```rust
use std::process::Command;
fn compile_to_binary(source: &str, output: &str) {
    Command::new("rustc")
        .arg(source)
        .arg("-o")
        .arg(output)
        .status()
        .expect("コンパイルに失敗しました");
}
```

---

## 実装状況（2026/03/17 時点）

<!-- 2026/03/17: MVP完了・REPL・usageドキュメントの実装状況を追加 -->
### MVP 完了

- **Phase 1〜6 完了**: Workspace、AST、Lexer、Parser、Bytecode、Compiler、RVM Core/Runtime、Host、CLI、E2E
- **テスト**: 125/125 テスト通過（12 crates）
- **対応機能**: `let` 変数宣言、四則演算、文字列連結、`print` ネイティブ関数、グローバル変数

### CLI（forge）

<!-- 2026/03/17: forge コマンドの仕様を追加 -->
```bash
forge              # REPL 起動
forge run <file>   # .fs ファイル実行
forge repl         # REPL 明示起動
forge help         # ヘルプ表示
```

### REPL（対話型インタプリタ）

<!-- 2026/03/17: Python 同様の REPL を追加 -->
`forge` を引数なしで実行すると、Python と同様に対話型インタプリタが起動します。

```bash
$ forge
ForgeScript REPL v0.1.0
>>> let x = 10
>>> let y = 20
>>> print(x + y)
30
>>> exit
Goodbye!
```

- **REPL コマンド**: `exit` / `quit`（終了）、`help`（ヘルプ）、`clear`（環境リセット）
- **変数永続**: 1行ずつ評価し、変数はセッション中保持

### マニュアル（usage/）

<!-- 2026/03/17: usage/ 配下のドキュメント構成を追加 -->
| ドキュメント | 説明 |
|-------------|------|
| quick-start.md | 5分で始めるクイックスタート |
| cli.md | forge コマンドリファレンス |
| repl.md | REPL ガイド |
| language-reference.md | 文法・型・演算子の仕様 |
| errors.md | エラーとトラブルシューティング |
| examples.md | サンプルコード集 |

---

## 今後の展望
ForgeScript + RVMは、**Rustのエコシステムをより手軽に扱える新しい言語と仮想マシンの構築を目指します**。今後のロードマップは以下のとおりです。

<!-- 2026/03/17: ロードマップを現状に合わせて更新 -->
1. ~~**ForgeScriptのパーサーとASTの構築**~~ ✅ 完了
2. ~~**RVMのインタープリタ実装（バイトコード解釈実行）**~~ ✅ 完了
3. ~~**CLI・REPL の実装**~~ ✅ 完了（2026/03/17）
4. **モジュール読み込み機構**（Phase 7）
5. **Cargoとの統合（自動クレート管理）**
6. **JITコンパイルの導入（Cranelift）**
7. **AOTコンパイルおよびWASMサポート**

## まとめ
- **ForgeScriptはRustに準拠しながらも、所有権やライフタイムをブラックボックス化し、より扱いやすくするスクリプト言語**
- **Cargoの管理を簡素化し、`use serde` だけでRustクレートを利用可能にする**
- **RVM（Rust Virtual Machine）はForgeScriptの実行環境として、インタープリタ・JIT・AOTをサポート予定**

<!-- 2026/03/17: 現状の達成を追記 -->
- **MVP 完了**: `let`、四則演算、文字列、`print` が動作
- **REPL 対応**: `forge` で対話型実行が可能
- **ドキュメント**: usage/ に詳細マニュアルを整備

ForgeScript + RVMの実装を進めながら、Rustのエコシステムを活用しやすくする新しい開発体験を提供したいと考えています。🚀
