# ForgeScript モジュールシステム 実装計画

> 仕様: `forge/modules/spec.md`
> 前提: v0.1.0（struct/enum/trait/mixin/data/typestate）が完成済み

---

## フェーズ構成

```
Phase M-0: ファイル解決基盤（use ローカルモジュール・基本インポート）
Phase M-1: pub 可視性
Phase M-2: mod.forge サポート・re-export
Phase M-3: 外部クレート（use serde 等 → Cargo.toml 自動追記）
Phase M-4: 静的解析（未使用インポート・循環参照・シンボル衝突）
Phase M-5: when キーワード（条件付きコンパイル）
Phase M-6: use raw {} ブロック（生 Rust コードの埋め込み）
Phase M-7: REPL でのモジュールインポート
```

---

## Phase M-0: ファイル解決基盤

### 目標
`use ./path/to/module.symbol` でローカルの `.forge` ファイルを読み込み、
指定したシンボルを現在のスコープで使用できること。

### 実装ステップ

1. **Lexer 拡張**
   - `use` キーワードトークンを追加
   - `pub` キーワードトークンを追加
   - `as` キーワードトークンを追加（エイリアス用）
   - `when` キーワードトークンを追加

2. **AST 拡張**
   ```rust
   Stmt::UseDecl {
       path: UsePath,
       symbols: UseSymbols,
       alias: Option<String>,
   }

   enum UsePath {
       Local(String),           // "./utils/helper"
       External(String),        // "serde"
       Stdlib(String),          // "forge/std/io"
   }

   enum UseSymbols {
       Single(String),          // .add
       Multiple(Vec<(String, Option<String>)>),  // .{add, subtract as sub}
       All,                     // .*
   }
   ```

3. **パーサー拡張**
   - `use ./path/module.symbol` のパース
   - `use ./path/module.{sym1, sym2}` のパース
   - `use ./path/module.*` のパース
   - `use ./path/module.symbol as alias` のパース
   - `pub use ...` のパース

4. **モジュールローダー実装**（`forge-compiler/src/loader/` に新規作成）
   - `ModuleLoader` 構造体：ファイルシステムから `.forge` ファイルを読み込む
   - パス解決: `use ./utils/helper` → `{project_root}/src/utils/helper.forge`
   - パース済み AST のキャッシュ（同一モジュールの二重読み込み防止）
   - `use *` のワイルドカード展開

5. **インタープリタ拡張**
   - `UseDecl` の評価：モジュールローダーでファイルを読み込み・評価
   - インポートしたシンボルを現在のスコープに束縛
   - エイリアス（`as`）のサポート

6. **テスト**
   - 単純なローカルモジュールのインポート
   - 複数シンボルのインポート
   - エイリアス付きインポート

---

## Phase M-1: pub 可視性

### 目標
`pub` キーワードで公開・プライベートのアクセス制御が機能すること。
非公開シンボルを外部からインポートしようとするとエラーになること。

### 実装ステップ

1. **AST 拡張**
   - `Stmt::Let`/`Fn`/`StructDef`/`EnumDef` 等に `is_pub: bool` フラグを追加

2. **パーサー拡張**
   - `pub fn ...` / `pub let ...` / `pub struct ...` / `pub data ...` のパース

3. **モジュールローダー拡張**
   - インポート時に `is_pub` を確認
   - 非公開シンボルへのアクセスはエラー: `"symbol は非公開です"`

4. **テスト**
   - pub なシンボルのインポート成功
   - 非公開シンボルのインポートでエラー

---

## Phase M-2: mod.forge サポート・re-export

### 目標
`mod.forge` でモジュールの公開 API を絞り込める（ファサードパターン）。
`pub use` による re-export が機能すること。

### 実装ステップ

1. **モジュールローダー拡張**
   - `use ./utils` のようにディレクトリを指定した場合、`utils/mod.forge` の存在を確認
   - `mod.forge` がある → mod.forge を経由してシンボルを解決
   - `mod.forge` がない → ディレクトリ内の全 pub シンボルにアクセス可能

2. **AST 拡張**
   - `Stmt::PubUse { path, symbols }` — re-export 宣言

3. **パーサー拡張**
   - `pub use helper.{add, subtract}` のパース
   - `pub use helper.*` のパース

4. **テスト**
   - mod.forge 経由のインポート
   - re-export チェーン（A → mod.forge → B）
   - 再エクスポート深さ警告（3段階超）

---

## Phase M-3: 外部クレート

### 目標
`use serde` のように `./` なしで外部クレートをインポートできること。
`forge build` 時に `Cargo.toml` の `[dependencies]` に自動追記されること。

### 実装ステップ

1. **パーサー拡張**
   - `use serde` / `use reqwest.{Client}` のパース（`./` なし → External 判定）

2. **依存関係マネージャー実装**（`forge-compiler/src/deps/` に新規作成）
   - `DepsManager` 構造体：使用した外部クレート名を収集
   - `forge.toml` の `[dependencies]` への自動追記（バージョンは `"*"` でデフォルト）
   - 重複チェック（べき等に動作）

3. **トランスパイラ拡張（B-6 の一部）**
   - `use serde` → `use serde;` + Cargo.toml への追記

4. **テスト**
   - 外部クレート名の収集
   - Cargo.toml への追記（重複なし）

---

## Phase M-4: 静的解析

### 目標
コンパイル時に以下を検出できること：
- 未使用インポート（警告）
- 循環参照（エラー）
- シンボル名衝突（エラー）
- `use *` 衝突（警告）

### 実装ステップ

1. **依存グラフ構築**
   - `use` 文を収集して有向グラフを構築

2. **循環参照検出**
   - トポロジカルソートで閉路を検出
   - エラーメッセージにパスを含める

3. **未使用インポート検出**
   - インポートしたシンボルの使用状況を追跡
   - 未使用 → 警告

4. **シンボル衝突検出**
   - 同名シンボルの複数インポート → エラー
   - `use *` 衝突 → 警告

5. **テスト**
   - 循環参照のエラー検出
   - 未使用インポートの警告
   - シンボル衝突のエラー

---

## Phase M-5: when キーワード

### 目標
`when platform.linux { }` / `when feature.debug { }` で条件付きコードブロックが動作すること。

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::When {
       condition: WhenCondition,
       body: Vec<Stmt>,
   }

   enum WhenCondition {
       Platform(String),   // platform.linux
       Feature(String),    // feature.debug
       Env(String),        // env.dev
       Test,               // test
       Not(Box<WhenCondition>),
   }
   ```

2. **パーサー拡張**
   - `when platform.linux { ... }` のパース
   - `when not feature.debug { ... }` のパース
   - `when test { ... }` のパース

3. **インタープリタ拡張**
   - 実行時に `platform`/`feature`/`env` を評価
   - `when test` は `forge test` コマンドでのみ実行

4. **トランスパイラ拡張（B-8 前段）**
   - `when platform.linux { }` → `#[cfg(target_os = "linux")]`
   - `when feature.debug { }` → `#[cfg(feature = "debug")]`

5. **テスト**

---

## Phase M-6: use raw {} ブロック

### 目標
`use raw { ... }` ブロック内で生 Rust コードを埋め込めること。
`::` プレフィックスで Rust の関数と ForgeScript の関数を区別できること。

### 実装ステップ

1. **AST 拡張**
   ```rust
   Stmt::UseRaw { rust_code: String }
   ```

2. **パーサー拡張**
   - `use raw { ... }` のパース（ブロック内は生文字列として保持）
   - `::std::collections::HashMap::new()` のパース

3. **インタープリタ拡張**
   - `use raw` ブロックは `forge run` では警告＋スキップ
   - `forge build` でのみ有効

4. **トランスパイラ拡張**
   - `use raw { ... }` → ブロック内の文字列をそのまま Rust コードに出力

---

## Phase M-7: REPL でのモジュールインポート

### 目標
REPL セッション内でローカルモジュールをインポートし、対話的に使用できること。

### 実装ステップ

1. **REPL 拡張**
   - `use ./utils/helper.add` 入力 → ロード・スコープに追加
   - `:modules` コマンド → ロード済みモジュール一覧
   - `:reload utils/helper` コマンド → 再読み込み
   - `:unload utils/helper` コマンド → アンロード

2. **テスト**
   - REPL でのモジュールロード
   - `:reload` による再読み込み

---

## テスト方針

### ユニットテスト
各フェーズのローダー・パーサー・インタープリタ層を個別にテスト。

### E2E テスト
複数ファイルを使ったシナリオテスト。`forge-cli/fixtures/modules/` にディレクトリ構造を用意。

```
forge-cli/fixtures/modules/
  basic/
    main.forge
    utils/
      helper.forge
  pub_visibility/
    main.forge
    private/
      secret.forge
  mod_forge/
    main.forge
    math/
      mod.forge
      basic.forge
  circular/
    main.forge
    a.forge
    b.forge
```

### ラウンドトリップテスト
`forge run` と `forge build + 実行` の出力が一致すること（B-6 と合わせて実施）。
