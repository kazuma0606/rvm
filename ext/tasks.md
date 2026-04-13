# VS Code 拡張機能 ロードマップ

> 現状: `ext/` — シンタックスハイライトのみ（v0.1.0）
> 目標: ターミナルを一度も開かずに ForgeScript を書いて動かせる

---

## 設計思想

「慣れている人にはターミナルを開くだけ」が最大の普及障壁になる。

```
上級者:   cargo install forge → すぐ使える
初心者:   「ターミナルって何ですか」← ここで離脱
```

VS Code マーケットプレイスからインストールするだけで
ForgeScript の全機能が使えるようにする。
Cursor / Windsurf は VS Code 互換のため追加対応不要。

---

## Phase E-0: CLI 自動セットアップ（最重要）

> ターミナルなしで forge-cli を使えるようにする

- [ ] 拡張機能 `activate()` で forge-cli の存在チェック
- [ ] 未インストール時に通知バー + "インストール" ボタンを表示
- [ ] GitHub Releases からプリビルドバイナリを自動ダウンロード
      （cargo がない環境でも動く・x64/arm64/win/mac/linux 対応）
- [ ] `cargo` が利用可能な場合は `cargo install forge-cli` を選択肢として提示
- [ ] インストール完了通知 + "Hello World を作成" ボタン
- [ ] `forge.toml` を検出してプロジェクトルートを自動認識

---

## Phase E-1: コマンドパレット統合

> Ctrl+Shift+P から ForgeScript の操作をすべて実行できる

- [ ] `ForgeScript: New Project` — テンプレート選択 → プロジェクト生成
      （blank / anvil / bloom / ember / notebook）
- [ ] `ForgeScript: Run File` — 現在のファイルを `forge run` で実行
- [ ] `ForgeScript: Run Tests` — `forge test` を実行
- [ ] `ForgeScript: Build` — `forge build` を実行
- [ ] `ForgeScript: Check` — `forge check`（型チェックのみ）
- [ ] `ForgeScript: Open REPL` — インタラクティブ REPL を起動
- [ ] `ForgeScript: Open Notebook` — `.fnb` ファイルを新規作成して開く

---

## Phase E-2: Run ボタン（エディタ内実行）

> ターミナルを開かずにファイルを実行できる

- [ ] `.forge` ファイルを開くとエディタ右上に ▶ Run ボタンを表示
- [ ] 実行結果を VS Code の Output パネルに表示
- [ ] エラー時は該当行にインライン表示（Problems パネル連携）
- [ ] 実行中はスピナー表示 + キャンセルボタン

---

## Phase E-3: .bloom / .fnb ファイル対応

- [ ] `.bloom` ファイルのシンタックスハイライト追加
      （HTML + ForgeScript の混在構文）
- [ ] `.fnb` ファイルを VS Code Notebook として開く
      （`FnbSerializer` 実装）
- [ ] `.fnb` セルの実行（`FnbKernelController` 実装）
- [ ] セル出力の表示（text / html / table / plot）
- [ ] `display::plot` の WebView レンダリング（Plotly.js 統合）
- [ ] `display::math` の WebView レンダリング（KaTeX 統合）

---

## Phase E-4: 言語サーバー（LSP）

> 補完・定義ジャンプ・ホバー情報

- [ ] `forge-lsp` クレートの新規作成（`crates/forge-lsp/`）
- [ ] 変数・関数の補完（`forge-compiler` の型情報を使用）
- [ ] ホバーで型情報を表示
- [ ] 定義へジャンプ（`Go to Definition`）
- [ ] 参照の検索（`Find All References`）
- [ ] リネーム（`Rename Symbol`）
- [ ] インラインエラー表示（`forge check` の結果をリアルタイム反映）

---

## Phase E-5: MCP + AI 統合

> AI コーディングツール（Claude Code / Cursor / Copilot）との動線

- [ ] MCP サーバーの自動起動（拡張機能起動時に forge-mcp を起動）
- [ ] `.vscode/mcp.json` を自動生成（forge-mcp のエンドポイントを登録）
- [ ] コンパイルエラー時に "AI に説明を聞く" ボタンを表示
      → MCP 経由でエラーコンテキストを AI に送信
- [ ] ノートブックセル内で `@ai:` コメントを検出 → AI に質問を投げる
- [ ] Cursor / Windsurf の MCP 設定に forge-mcp を自動登録

---

## Phase E-6: forge dev（ホットリロード）統合

> Bloom 開発時の体験向上

- [ ] `forge dev` の起動・停止をコマンドパレットから操作
- [ ] 内蔵ブラウザプレビュー（Simple Browser パネル）で Bloom を表示
- [ ] ファイル保存時に自動リロード
- [ ] コンパイルエラーをブラウザオーバーレイと VS Code 両方に表示

---

## VS Code マーケットプレイス公開チェックリスト

- [ ] `publisher` ID の取得（VS Code Marketplace アカウント）
- [ ] アイコン画像の作成（128x128 PNG）
- [ ] README.md（拡張機能用）の作成
- [ ] `vsce package` でパッケージ化
- [ ] `vsce publish` でマーケットプレイスに公開
- [ ] Open VSX Registry にも公開（Cursor / Windsurf / Codium 対応）

---

## 優先度まとめ

| フェーズ | インパクト | 実装コスト | 優先度 |
|---|---|---|---|
| E-0: CLI 自動セットアップ | ◎ | 中 | **最優先** |
| E-1: コマンドパレット | ◎ | 低 | 高 |
| E-2: Run ボタン | ○ | 低 | 高 |
| E-3: .bloom / .fnb 対応 | ◎ | 高 | 中（Bloom 完成後） |
| E-4: LSP | ◎ | 高 | 中（言語が安定後） |
| E-5: MCP + AI 統合 | ◎ | 中 | 高 |
| E-6: forge dev 統合 | ○ | 中 | 低（Bloom 完成後） |
| マーケットプレイス公開 | ◎ | 低 | **E-0〜E-2 完成後すぐ** |
