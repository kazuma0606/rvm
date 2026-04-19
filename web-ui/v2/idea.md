**方針**
Bloom 向けの最小 UI テスト戦略は、ForgeScript と Rust で大半を固めて、実ブラウザ E2E は少数に絞るのが妥当です。全部をブラウザで見るのは遅いし、全部を言語内で見るのは無理があります。

**1. ForgeScript で意味論を固定する**
`.bloom` の parser、AST、依存解析、codegen、SSR 出力は ForgeScript テストで固めます。ここは速いし、壊れたときの原因も追いやすいです。  
例:
- `{#if}` と `{#for}` の展開
- `@click` や `:class` の解釈
- `render(<Component />)` の変換
- `log(count)` が生成コードに残ること

**2. Rust で統合面を固定する**
Rust 側では build、WASM 生成、runtime bridge、SSR + hydrate の接続を見ます。ForgeScript だけでは届かない層です。  
例:
- `forge build --web` で `dist/*.wasm` と `forge.min.js` が出る
- SSR で返した HTML に対して attach できる
- DOM command が期待どおり出る
- `forge_log` import が入る

**3. 実ブラウザ E2E は少数精鋭にする**
ここは 3〜5 本程度に絞るのが良いです。全部を E2E にすると保守が重いです。  
最低限ほしいのは:
- 初期 SSR 表示が見える
- hydration 後に click で state が変わる
- console に WASM `log()` が出る
- `if/for` を含む画面で再描画が壊れない

**4. UI テスト対象を“画面”ではなく“契約”にする**
見た目全部を見るのではなく、壊れると困る契約だけ見ます。  
例:
- `data-on-click` が attach される
- `data-reactive="count"` が更新される
- `/forge.min.js` と `.wasm` が正しい URL で配信される
- SSR fallback ではなく WASM 本線に入る

**5. 手確認をチェックリスト化して減らす**
完全自動化できないものだけ手確認に残します。  
例:
- FOUC がない
- 体感的な描画崩れがない
- DevTools Console に期待ログが出る

結論として、Bloom では
- ForgeScript: 意味論
- Rust: 統合
- Browser E2E: 本物確認

の三層に分けるのが一番効率的です。  
必要なら次に、`counter_page.bloom` を題材にした具体的なテスト一覧をそのまま作れます。
