// forge-stdlib: Phase 3 コレクション API テスト

use forge_vm::interpreter::eval_source;
use forge_vm::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

/// ForgeScript ソースを実行して Value を返すヘルパー
fn run(src: &str) -> Value {
    eval_source(src).unwrap_or_else(|e| panic!("実行エラー: {}", e))
}

fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(items)))
}

fn ints(ns: &[i64]) -> Value {
    list(ns.iter().map(|&n| Value::Int(n)).collect())
}

// ── Phase 3-A 単体テスト ─────────────────────────────────────────────────────

#[test]
fn test_map() {
    assert_eq!(run("[1, 2, 3].map(x => x * 2)"), ints(&[2, 4, 6]));
}

#[test]
fn test_filter() {
    assert_eq!(run("[1, 2, 3, 4].filter(x => x % 2 == 0)"), ints(&[2, 4]));
}

#[test]
fn test_fold() {
    assert_eq!(run("[1, 2, 3].fold(0, (acc, x) => acc + x)"), Value::Int(6));
}

#[test]
fn test_sum() {
    assert_eq!(run("[1, 2, 3, 4, 5].sum()"), Value::Int(15));
}

#[test]
fn test_count() {
    assert_eq!(run("[1, 2, 3].count()"), Value::Int(3));
}

#[test]
fn test_any_all() {
    // any
    assert_eq!(run("[1, 2, 3].any(x => x > 2)"), Value::Bool(true));
    assert_eq!(run("[1, 2, 3].any(x => x > 5)"), Value::Bool(false));
    // all
    assert_eq!(run("[1, 2, 3].all(x => x > 0)"), Value::Bool(true));
    assert_eq!(run("[1, 2, 3].all(x => x > 1)"), Value::Bool(false));
    // none
    assert_eq!(run("[1, 2, 3].none(x => x > 5)"), Value::Bool(true));
    assert_eq!(run("[1, 2, 3].none(x => x > 2)"), Value::Bool(false));
}

#[test]
fn test_first_last() {
    // 非空リスト
    assert_eq!(
        run("[10, 20, 30].first()"),
        Value::Option(Some(Box::new(Value::Int(10))))
    );
    assert_eq!(
        run("[10, 20, 30].last()"),
        Value::Option(Some(Box::new(Value::Int(30))))
    );
    // 空リストは none を返す
    assert_eq!(run("[].first()"), Value::Option(None));
    assert_eq!(run("[].last()"), Value::Option(None));
}

#[test]
fn test_order_by() {
    assert_eq!(
        run("[3, 1, 4, 1, 5, 2].order_by(x => x)"),
        ints(&[1, 1, 2, 3, 4, 5])
    );
    assert_eq!(
        run("[3, 1, 2].order_by_descending(x => x)"),
        ints(&[3, 2, 1])
    );
}

#[test]
fn test_take_skip() {
    // 通常
    assert_eq!(run("[1, 2, 3, 4, 5].take(3)"), ints(&[1, 2, 3]));
    assert_eq!(run("[1, 2, 3, 4, 5].skip(2)"), ints(&[3, 4, 5]));
    // 境界値: 0
    assert_eq!(run("[1, 2, 3].take(0)"), ints(&[]));
    assert_eq!(run("[1, 2, 3].skip(0)"), ints(&[1, 2, 3]));
    // 境界値: リスト長超過
    assert_eq!(run("[1, 2, 3].take(10)"), ints(&[1, 2, 3]));
    assert_eq!(run("[1, 2, 3].skip(10)"), ints(&[]));
}

#[test]
fn test_distinct() {
    assert_eq!(run("[1, 2, 1, 3, 2, 4].distinct()"), ints(&[1, 2, 3, 4]));
}

#[test]
fn test_zip() {
    // 同じ長さ
    assert_eq!(
        run("[1, 2, 3].zip([4, 5, 6])"),
        list(vec![
            list(vec![Value::Int(1), Value::Int(4)]),
            list(vec![Value::Int(2), Value::Int(5)]),
            list(vec![Value::Int(3), Value::Int(6)]),
        ])
    );
    // 長さが異なる場合は短い方に合わせる
    assert_eq!(
        run("[1, 2, 3].zip([10, 20])"),
        list(vec![
            list(vec![Value::Int(1), Value::Int(10)]),
            list(vec![Value::Int(2), Value::Int(20)]),
        ])
    );
}

#[test]
fn test_flat_map() {
    assert_eq!(
        run("[1, 2, 3].flat_map(x => [x, x * 10])"),
        ints(&[1, 10, 2, 20, 3, 30])
    );
}

#[test]
fn test_method_chain() {
    // .filter().map().fold() のチェーン
    assert_eq!(
        run("[1, 2, 3, 4, 5].filter(x => x % 2 == 0).map(x => x * 3).fold(0, (acc, x) => acc + x)"),
        Value::Int(18) // (2*3) + (4*3) = 6 + 12 = 18
    );
}
