# ForgeScript 言語拡張アイデア

> 他の言語にはあるが Rust にはない・弱い仕組みで、ForgeScript に取り込むと面白そうなアイデア集。
> 「Rustで冗長になる部分を改善し、他の言語の良いところを吸収する」スタンスで検討する。

---

## 1. 非同期クロージャの完成

**参考言語**: JavaScript / TypeScript, Kotlin

Rust での問題点：クロージャが `async` だと型が爆発する（`Box::pin`・`Send` 境界・ライフタイム衝突）。

ForgeScript では transpiler が `await` の存在を検出して named function を自動昇格しているので、クロージャにも同じルールを適用できる。

```forge
// 使う側は async かどうか意識しない
let handler = fn(req) { fetch(req.url).await? }

// transpiler が自動的に async クロージャに昇格
// → Rust: |req| async move { ... }
```

**実装メモ**: B-3 が「tail position 限定」で止まっているのは `spawn` 未実装が原因。`spawn` と合わせて実装すると並行処理のストーリーが完結する。

**優先度**: 高（B-3 の続き・既存タスクの完成）

---

## 2. ジェネレータ / `yield`

**参考言語**: Python, JavaScript, Kotlin (`sequence {}`)

Rust のジェネレータは長年 unstable のまま。

```forge
fn fibonacci() -> generate<number> {
    state a = 0
    state b = 1
    loop {
        yield a
        let next = a + b
        a = b
        b = next
    }
}

fibonacci()
    |> take(10)
    |> each(println)
```

コレクション API の `map` / `filter` が遅延評価になり、大きなデータストリームを省メモリで処理できる。`forge-pipeline` 素案と相性抜群。

**優先度**: 中（コレクション API と統合すると強力）

---

## 3. オプショナルチェーン `?.`

**参考言語**: Swift, Kotlin, TypeScript

Rust では `Option` のネストを `.and_then()` で繋ぐか `?` で早期リターンするしかない。

```forge
// 現状
let city = match user {
    some(u) => match u.address {
        some(a) => a.city,
        none    => "unknown",
    },
    none => "unknown",
}

// ?. があれば
let city = user?.address?.city ?? "unknown"
```

`T?` 型が言語に既にあるので `?.` は自然な拡張。`??`（null 合体演算子）とセットで導入する。

**優先度**: 高（低コスト・型システムと自然に統合）

---

## 4. 複数戻り値

**参考言語**: Go

Rust はタプルで代替するが可読性が低い。

```forge
fn divide(a: number, b: number) -> (number, number)! {
    ok(a / b, a % b)   // 商, 余り
}

let (quotient, remainder) = divide(17, 5)?
```

`(value, error)` パターンの代替として使える。多用はしないが、数学的・アルゴリズム的な関数で意図が明確になる。

**優先度**: 低（タプルで代替できるため）

---

## 5. デコレータの拡張

**参考言語**: Python, TypeScript, Java

`@derive` は既にあるが、**ユーザー定義デコレータ**まで広げると宣言的なメタプログラミングが可能になる。

```forge
@route("GET", "/users/:id")
@cache(ttl: 60)
@auth(role: "admin")
fn get_user(req: Request) -> Response! {
    // ...
}
```

Anvil のルーティングをコードとして書く代わりに宣言的に記述できる。メタプログラミングの入口にもなる。

**優先度**: 中（Anvil との相乗効果が高い）

---

## 6. 構造的型付け（ダック型付け）

**参考言語**: TypeScript, Go

Rust は `impl Trait for Type` を明示的に書く必要がある。構造的型付けを部分的に許容するとプロトタイピング速度が上がる。

```forge
// 「name フィールドと greet メソッドを持つ型」なら何でも受け付ける
fn greet(entity: { name: string, greet: fn() -> string }) {
    println(entity.greet())
}
```

`trait`（厳格・明示的）と `{ ... }` 構造型（柔軟・暗黙的）を使い分けられる。

**優先度**: 低（型システムの複雑度が上がるため慎重に）

---

## 7. ノートブック対応（`.fnb` 形式）

**参考言語**: Python（Jupyter）, Kotlin（Kotlin Notebook）, Quarto（.qmd）

採用障壁を下げ、チュートリアルをインタラクティブなドキュメントとして配布できる。

```markdown
---
title: "データ処理入門"
forge: "0.1.0"
---

# ステップ1: データ読み込み

​```forge
let rows = read_file("data.csv") |> split("\n")
println("行数: {rows.len()}")
​```

# ステップ2: フィルタリング

​```forge
let filtered = rows |> filter(fn(r) { number(r[2])? > 1000 })
println("{filtered.len()} 行")
​```
```

**フォーマット設計方針**:
- Markdown ベース（人間が直接読める・git 差分が清潔）
- 出力は `.fnb.out.json` に分離（ノートブック本体に混入しない）
- VS Code Notebook API で実装（ZeroMQ 不要・依存なし）
- 後から Jupyter 互換（`.ipynb` エクスポート）を追加可能な設計にしておく

**`display()` 組み込み関数**:
```forge
display(value)              // forge run では println に fallback
display::html("<b>bold</b>")
display::json(value)
display::table(rows)        // HTML テーブルとして描画
```

**実装コンポーネント**:
1. `.fnb` パーサー / シリアライザ（forge-cli に追加）
2. VS Code Notebook 拡張への ForgeScript kernel 登録
3. `forge notebook <file.fnb>` コマンド
4. `forge nbconvert <file.fnb> --to html` 変換ツール（後回し可）

**優先度**: 中（VS Code 拡張の延長線上・WASM より先に着手可能）

---

## 優先度まとめ

| 機能 | 実装コスト | 実用性 | 一貫性 | 優先度 |
|---|---|---|---|---|
| 非同期クロージャ完成 | 中（B-3の続き） | ◎ | ◎ | **高** |
| `?.` オプショナルチェーン | 低 | ◎ | ◎ | **高** |
| ノートブック `.fnb` | 中 | ◎ | ◎ | **中〜高** |
| yield / ジェネレータ | 高 | ◎ | ○ | 中 |
| デコレータ拡張 | 中 | ○ | ○ | 中 |
| 複数戻り値 | 低 | △ | ○ | 低 |
| 構造的型付け | 高 | ○ | △ | 低 |
