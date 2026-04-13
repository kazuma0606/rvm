# crates/ — Rust クレート一覧

このディレクトリはすべて単一の Cargo workspace（ルートの `Cargo.toml`）で管理される。

---

## 言語コア

| クレート | 役割 |
|---|---|
| `forge-compiler` | Lexer → Parser → AST → 型チェッカー |
| `forge-vm` | ツリーウォーキングインタープリタ（`forge run`） |
| `forge-stdlib` | 標準ライブラリ（wasm / crypto / compress / http / fs 等） |
| `forge-transpiler` | AST → Rust コード生成器（`forge build`） |
| `forge-cli` | CLI バイナリ（`forge` コマンド） |
| `forge-mcp` | MCP サーバー（AI ツール統合） |

## パッケージ Rust 実装

`packages/` 配下の ForgeScript パッケージを支える Rust クレート。

| クレート | 対応パッケージ | 役割 |
|---|---|---|
| `crucible-cli` | `packages/crucible/` | PostgreSQL クライアント CLI |
| `ember-runtime`（予定） | `packages/ember/` | wgpu + rapier ゲームエンジンランタイム |
| `bloom-runtime`（予定） | `packages/bloom/` | DOM ブリッジ + WASM ローダー |

---

## 依存関係

```
forge-cli
  └── forge-compiler
  └── forge-vm
        └── forge-compiler
        └── forge-stdlib
  └── forge-transpiler
        └── forge-compiler
  └── forge-mcp
        └── forge-vm

crucible-cli（独立。言語コアに依存しない）
ember-runtime（独立。言語コアに依存しない）
bloom-runtime（独立。言語コアに依存しない）
```

パッケージ Rust 実装は言語コアに依存しない設計にする。
将来的に個別リポジトリへの切り出しが容易になる。
